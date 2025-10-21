// crates/fig_auth/src/portal.rs
// Build portal URL, listen for callback, and fan out to Social (exchange) or Internal (return PKCE
// params).

use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;
use fig_util::system_info::is_mwinit_available;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::Service;
use hyper::{
    Request,
    Response,
};
use hyper_util::rt::TokioIo;
use rand::Rng;
use tokio::net::TcpListener;
use tracing::{
    debug,
    info,
};

use crate::pkce::{
    generate_code_challenge,
    generate_code_verifier,
};
use crate::secret_store::SecretStore;
use crate::social::{
    CALLBACK_PORTS,
    SocialProvider,
    exchange_social_token,
};
use crate::{
    Error,
    Result,
};

const AUTH_PORTAL_URL: &str = "https://gamma.app.kiro.aws.dev/signin";
const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(600);

/// Final outcome of portal flow.
pub enum PortalResult {
    Social(SocialProvider),
    Internal { issuer_uri: String, idc_region: String },
}

/// Handle returned by init; caller opens the URL, then calls `finish_unified_portal`.
pub struct PortalInit {
    pub auth_url: String,
    pub state: String,
    pub verifier: String,
    pub listener: TcpListener,
    pub port: u16,
}

/// Step 1: prepare URL + bind a loopback callback port from the allowlist.
pub async fn init_unified_portal() -> Result<PortalInit> {
    let is_internal = is_mwinit_available();
    // PKCE params for the portal & social exchange
    let verifier = generate_code_verifier();
    let challenge = generate_code_challenge(&verifier);

    let state: String = rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    // Bind one of the pre-allowed ports (must match Cognito/IdP allowlist)
    let listener = bind_allowed_port(CALLBACK_PORTS).await?;
    let port = listener.local_addr()?.port();
    let redirect_base = format!("http://localhost:{port}");
    info!(%port, %redirect_base, "Unified auth portal listening for callback");

    // Compose the portal URL (whether to include internal hint is decided by caller)
    let internal = if is_internal { "&from_amazon_internal=true" } else { "" };
    let auth_url = format!(
        "{base}?state={state}&code_challenge={challenge}&code_challenge_method=S256&redirect_uri={redirect}{internal}&redirect_from=kirocli",
        base = AUTH_PORTAL_URL,
        state = state,
        challenge = challenge,
        redirect = urlencoding::encode(&redirect_base),
        internal = internal,
    );

    Ok(PortalInit {
        auth_url,
        state,
        verifier,
        listener,
        port,
    })
}

/// Step 2: wait for a single callback, then either exchange social tokens or return PKCE params.
pub async fn finish_unified_portal(init: PortalInit, secret_store: &SecretStore) -> Result<PortalResult> {
    let callback = wait_for_auth_callback(init.listener, init.state).await?;

    match callback.login_option.as_str() {
        "google" | "github" => {
            // Social: exchange tokens via shared auth service and persist them
            let provider = if callback.login_option == "google" {
                SocialProvider::Google
            } else {
                SocialProvider::Github
            };

            let code = match callback.code {
                Some(c) => c,
                None => return Err(Error::OAuthCustomError("Missing authorization code".to_string())),
            };

            let redirect_uri = format!(
                "http://localhost:{port}{path}?login_option={opt}",
                port = init.port,
                path = callback.path,
                opt = urlencoding::encode(&callback.login_option),
            );

            exchange_social_token(secret_store, provider, &init.verifier, &code, &redirect_uri).await?;
            Ok(PortalResult::Social(provider))
        },
        "internal" => {
            // Internal (IdC): pass issuer_uri + idc_region to caller for PKCE start+finish
            let issuer_uri = match callback.issuer_uri {
                Some(v) => v,
                None => {
                    return Err(Error::OAuthCustomError(
                        "Missing issuer_uri for internal auth".to_string(),
                    ));
                },
            };
            let idc_region = match callback.sso_region {
                Some(v) => v,
                None => {
                    return Err(Error::OAuthCustomError(
                        "Missing idc_region for internal auth".to_string(),
                    ));
                },
            };
            Ok(PortalResult::Internal { issuer_uri, idc_region })
        },
        other => Err(Error::OAuthCustomError(format!("Unknown login_option: {other}"))),
    }
}

async fn bind_allowed_port(ports: &[u16]) -> Result<TcpListener> {
    for port in ports {
        match TcpListener::bind(("127.0.0.1", *port)).await {
            Ok(l) => return Ok(l),
            Err(e) => debug!("Failed to bind to port {port}: {e}"),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "Failed to bind to any port").into())
}

struct AuthPortalCallback {
    login_option: String,
    code: Option<String>,
    issuer_uri: Option<String>,
    sso_region: Option<String>,
    state: String,
    path: String,
}

async fn wait_for_auth_callback(listener: TcpListener, expected_state: String) -> Result<AuthPortalCallback> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<AuthPortalCallback>(1);

    let server_handle = tokio::spawn(async move {
        if let Ok((stream, _)) = listener.accept().await {
            let io = TokioIo::new(stream);
            let svc = AuthCallbackService { tx: tx.clone() };
            let _ = http1::Builder::new().serve_connection(io, svc).await;
        }
    });

    let result = tokio::select! {
        v = rx.recv() => v.ok_or_else(|| Error::OAuthCustomError("Failed to receive callback".to_string())),
        _ = tokio::time::sleep(DEFAULT_AUTHORIZATION_TIMEOUT) => Err(Error::OAuthCustomError("Auth portal timed out".to_string())),
    }?;

    server_handle.abort();

    if result.state != expected_state {
        return Err(Error::OAuthCustomError(format!(
            "OAuth state mismatch: expected={expected_state}, actual={}",
            result.state
        )));
    }

    Ok(result)
}

#[derive(Clone)]
struct AuthCallbackService {
    tx: tokio::sync::mpsc::Sender<AuthPortalCallback>,
}

impl Service<Request<Incoming>> for AuthCallbackService {
    type Error = Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = Result<Self::Response>> + Send>>;
    type Response = Response<Full<Bytes>>;

    fn call(&self, req: Request<Incoming>) -> Self::Future {
        let tx = self.tx.clone();
        Box::pin(async move {
            let uri = req.uri();
            let path = uri.path();

            if path == "/oauth/callback" || path == "/signin/callback" {
                let params: HashMap<String, String> = uri
                    .query()
                    .map(|q| {
                        q.split('&')
                            .filter_map(|kv| kv.split_once('='))
                            .map(|(k, v)| (k.to_string(), urlencoding::decode(v).unwrap_or_default().to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                let cb = AuthPortalCallback {
                    login_option: params.get("login_option").cloned().unwrap_or_default(),
                    code: params.get("code").cloned(),
                    issuer_uri: params.get("issuer_uri").cloned(),
                    sso_region: params.get("idc_region").cloned(),
                    state: params.get("state").cloned().unwrap_or_default(),
                    path: path.to_string(),
                };
                let _ = tx.send(cb).await;

                let resp = Response::builder()
                    .status(302)
                    .header("Location", AUTH_PORTAL_URL)
                    .header("Cache-Control", "no-store")
                    .body(Full::new(Bytes::from_static(b"")))
                    .map_err(|e| Error::OAuthCustomError(e.to_string()))?;
                Ok(resp)
            } else {
                let resp = Response::builder()
                    .status(404)
                    .body(Full::new(Bytes::from_static(b"")))
                    .map_err(|e| Error::OAuthCustomError(e.to_string()))?;
                Ok(resp)
            }
        })
    }
}
