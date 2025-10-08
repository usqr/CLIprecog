use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;
use std::time::Duration;

use aws_sdk_ssooidc::config::{
    ConfigBag,
    RuntimeComponents,
};
use aws_smithy_runtime_api::client::identity::http::Token;
use aws_smithy_runtime_api::client::identity::{
    Identity,
    IdentityFuture,
    ResolveIdentity,
};
use once_cell::sync::OnceCell;
use rand::Rng;
use reqwest::Client;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::json;
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tracing::{
    debug,
    error,
    info,
    trace,
};

use crate::consts::SOCIAL_AUTH_SERVICE_ENDPOINT;
use crate::pkce::{
    PkceRegistration,
    generate_code_challenge,
    generate_code_verifier,
};
use crate::secret_store::{
    Secret,
    SecretStore,
};
use crate::{
    Error,
    Result,
};

const CALLBACK_PORTS: &[u16] = &[49153, 50153, 51153, 52153, 53153, 4649, 6588, 9091, 8008, 3128];
const DEFAULT_AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(300);
const SIGN_UP_PAUSED_MESSAGE: &str = "New signups are temporarily paused.";
const USER_AGENT: &str = "Kiro-Desktop";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SocialProvider {
    #[serde(rename = "google")]
    Google,
    #[serde(rename = "github")]
    Github,
}

impl fmt::Display for SocialProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SocialProvider::Google => write!(f, "Google"),
            SocialProvider::Github => write!(f, "GitHub"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialToken {
    pub access_token: Secret,
    #[serde(with = "time::serde::rfc3339")]
    pub expires_at: OffsetDateTime,
    pub refresh_token: Option<Secret>,
    pub provider: SocialProvider,
    pub profile_arn: Option<String>,
}

impl SocialToken {
    pub const SECRET_KEY: &'static str = "codewhisperer:social:token";

    /// Load the token from the keychain, refresh the token if it is expired and return it
    pub async fn load(secret_store: &SecretStore, force_refresh: bool) -> Result<Option<Self>> {
        if cfg!(test) {
            return Ok(Some(Self {
                access_token: Secret("test_access_token".to_string()),
                expires_at: OffsetDateTime::now_utc() + time::Duration::minutes(60),
                refresh_token: Some(Secret("test_refresh_token".to_string())),
                provider: SocialProvider::Google,
                profile_arn: None,
            }));
        }

        trace!("loading social token from secret store");
        match secret_store.get(Self::SECRET_KEY).await {
            Ok(Some(secret)) => {
                let token: Option<Self> = serde_json::from_str(&secret.0)?;
                match token {
                    Some(mut token) => {
                        // if token is expired try to refresh
                        if token.is_expired() || force_refresh {
                            trace!("social token expired, refreshing");
                            token = token.refresh_token(secret_store).await?;
                        }
                        trace!(?token, "found a valid social token");
                        Ok(Some(token))
                    },
                    None => Ok(None),
                }
            },
            Ok(None) => Ok(None),
            Err(err) => {
                error!(%err, "Error getting social token");
                Err(err)?
            },
        }
    }

    pub async fn save(&self, secret_store: &SecretStore) -> Result<()> {
        secret_store
            .set(Self::SECRET_KEY, &serde_json::to_string(self)?)
            .await?;
        Ok(())
    }

    pub async fn delete(&self, secret_store: &SecretStore) -> Result<()> {
        secret_store.delete(Self::SECRET_KEY).await?;
        Ok(())
    }

    pub fn is_expired(&self) -> bool {
        let now = OffsetDateTime::now_utc();
        (now + time::Duration::minutes(1)) > self.expires_at
    }

    pub async fn refresh_token(&self, secret_store: &SecretStore) -> Result<Self> {
        let Some(refresh_token) = &self.refresh_token else {
            error!("no refresh token found for social login");
            self.delete(secret_store).await.ok();
            return Err(Error::NoToken);
        };

        debug!("refreshing social access token");

        let client = Client::new();
        let response = client
            .post(format!("{}/refreshToken", SOCIAL_AUTH_SERVICE_ENDPOINT))
            .json(&serde_json::json!({ "refreshToken": refresh_token.0 }))
            .send()
            .await?;

        if response.status().is_success() {
            let token_response: TokenResponse = response.json().await?;
            let new_token = Self {
                access_token: Secret(token_response.access_token),
                expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
                refresh_token: Some(Secret(token_response.refresh_token)),
                provider: self.provider,
                profile_arn: token_response.profile_arn.clone().or(self.profile_arn.clone()),
            };

            new_token.save(secret_store).await?;
            Ok(new_token)
        } else {
            let status = response.status();
            error!("failed to refresh social token: {}", status);
            self.delete(secret_store).await.ok();
            Err(Error::OAuthCustomError(format!("Refresh failed: {}", status)))
        }
    }
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(rename = "accessToken")]
    access_token: String,
    #[serde(rename = "refreshToken")]
    refresh_token: String,
    #[serde(rename = "expiresIn")]
    expires_in: u64,
    #[serde(rename = "profileArn")]
    profile_arn: Option<String>,
}

#[derive(serde::Deserialize)]
struct ErrBody {
    message: Option<String>,
}

struct Pending {
    listener: TcpListener,
    state: String,
    verifier: String,
    redirect_uri: String,
    provider: SocialProvider,
}

static PENDING: OnceCell<Mutex<HashMap<String, Pending>>> = OnceCell::new();
fn pending_map() -> &'static Mutex<HashMap<String, Pending>> {
    PENDING.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn bind_allowed_port(ports: &[u16]) -> Result<TcpListener> {
    for port in ports {
        match TcpListener::bind(("127.0.0.1", *port)).await {
            Ok(listener) => return Ok(listener),
            Err(e) => debug!("failed to bind to port {}: {}", port, e),
        }
    }
    Err(std::io::Error::new(std::io::ErrorKind::AddrInUse, "Failed to bind to any port").into())
}

pub async fn start_social_authorization(
    provider: SocialProvider,
    _secret_store: SecretStore,
) -> Result<(String, String)> {
    info!("Starting social login with {}", provider);

    // PKCE & state
    let verifier = generate_code_verifier();
    let challenge = generate_code_challenge(&verifier);
    let state = rand::rng()
        .sample_iter(rand::distr::Alphanumeric)
        .take(10)
        .collect::<Vec<_>>();
    let state = String::from_utf8(state).unwrap_or_else(|_| "state".to_string());

    let listener = bind_allowed_port(CALLBACK_PORTS).await?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://localhost:{}/oauth/callback", port);
    info!("OAuth callback server listening on {}", redirect_uri);

    let idp = match provider {
        SocialProvider::Google => "Google",
        SocialProvider::Github => "Github",
    };
    let login_url = format!(
        "{}/login?idp={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}",
        SOCIAL_AUTH_SERVICE_ENDPOINT,
        idp,
        urlencoding::encode(&redirect_uri),
        challenge,
        state
    );

    let auth_request_id = uuid::Uuid::new_v4().to_string();
    pending_map()
        .lock()
        .expect("pending map poisoned")
        .insert(auth_request_id.clone(), Pending {
            listener,
            state,
            verifier,
            redirect_uri,
            provider,
        });

    Ok((auth_request_id, login_url))
}

/// finish authorization：wait for code → exchange token → save the token
/// If backend pause sign up: return Error::OAuthCustomError("SIGN_IN_BLOCKED")
pub async fn finish_social_authorization(
    auth_request_id: String,
    invitation_code: Option<String>,
    _ctx: &impl ?Sized,
) -> Result<()> {
    let pending = pending_map()
        .lock()
        .expect("pending map poisoned")
        .remove(&auth_request_id)
        .ok_or(Error::OAuthCustomError("Invalid auth request id".into()))?;

    // wait for code
    let code_fut = PkceRegistration::recv_code_with_extra_accepts(pending.listener, pending.state.clone(), 2);
    let code = tokio::time::timeout(DEFAULT_AUTHORIZATION_TIMEOUT, code_fut)
        .await
        .map_err(|_| Error::OAuthTimeout)??;

    debug!("Received authorization code");

    // exchange token
    let client = Client::new();
    let mut token_request = serde_json::json!({
        "code": code,
        "code_verifier": pending.verifier,
        "redirect_uri": pending.redirect_uri,
    });

    let had_invitation_code = invitation_code.is_some();
    if let Some(inv) = invitation_code {
        token_request["invitationCode"] = serde_json::Value::String(inv);
        debug!("Including invitationCode in token exchange");
    }

    let response = client
        .post(format!("{}/oauth/token", SOCIAL_AUTH_SERVICE_ENDPOINT))
        .header("Content-Type", "application/json")
        .header("User-Agent", USER_AGENT)
        .json(&token_request)
        .send()
        .await?;

    if response.status().is_success() {
        let token_response: TokenResponse = response.json().await?;
        let token = SocialToken {
            access_token: Secret(token_response.access_token),
            expires_at: OffsetDateTime::now_utc() + time::Duration::seconds(token_response.expires_in as i64),
            refresh_token: Some(Secret(token_response.refresh_token)),
            provider: pending.provider,
            profile_arn: token_response.profile_arn,
        };

        let secret_store = SecretStore::new().await?;
        token.save(&secret_store).await?;
        info!("Successfully logged in with {}", token.provider);

        if let Some(arn) = token.profile_arn.as_deref() {
            let profile_value = json!({
                "arn": arn,
                "profile_name": "Social_default_Profile"
            });

            if let Err(err) = fig_settings::state::set_value("api.codewhisperer.profile", profile_value) {
                error!(?err, profile_arn=%arn, "failed to set profile from social token");
            }
        }

        let _ = fig_settings::state::remove_value("api.selectedCustomization");

        tokio::time::sleep(Duration::from_millis(200)).await;
        Ok(())
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());

        let error_text = serde_json::from_str::<ErrBody>(&body)
            .ok()
            .and_then(|e| e.message)
            .unwrap_or_else(|| body.clone());

        let signups_paused = error_text == SIGN_UP_PAUSED_MESSAGE;

        if (status.as_u16() == 401 || status.as_u16() == 403) && signups_paused && !had_invitation_code {
            // provide frontend with invitation code
            return Err(Error::OAuthCustomError("SIGN_IN_BLOCKED".into()));
        }

        if status.as_u16() == 401 && signups_paused && had_invitation_code {
            return Err(Error::OAuthCustomError("Invalid invitation code".into()));
        }

        error!("Failed to exchange code for token: {} - {}", status, error_text);
        Err(Error::OAuthCustomError(format!(
            "Token exchange failed: {}",
            error_text
        )))
    }
}

#[derive(Debug, Clone)]
pub struct SocialBearerResolver;

impl ResolveIdentity for SocialBearerResolver {
    fn resolve_identity<'a>(
        &'a self,
        _runtime_components: &'a RuntimeComponents,
        _config_bag: &'a ConfigBag,
    ) -> IdentityFuture<'a> {
        IdentityFuture::new_boxed(Box::pin(async {
            let secret_store = SecretStore::new().await?;
            match SocialToken::load(&secret_store, false).await? {
                Some(token) => Ok(Identity::new(
                    Token::new(token.access_token.0.clone(), Some(token.expires_at.into())),
                    Some(token.expires_at.into()),
                )),
                None => Err(Error::NoToken.into()),
            }
        }))
    }
}

pub async fn is_social_logged_in(secret_store: &SecretStore) -> bool {
    matches!(SocialToken::load(secret_store, false).await, Ok(Some(_)))
}

pub async fn social_token(secret_store: &SecretStore) -> Result<Option<SocialToken>> {
    SocialToken::load(secret_store, false).await
}

pub async fn logout_social(secret_store: &SecretStore) -> Result<()> {
    secret_store.delete(SocialToken::SECRET_KEY).await?;
    Ok(())
}
