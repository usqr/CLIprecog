import Lockup from "@/components/svg/logo";
import { Button } from "@/components/ui/button";
import {
  Auth,
  Internal,
  Native,
} from "@aws/amazon-q-developer-cli-api-bindings";
import { useEffect, useState } from "react";
import Tab, { ProfileTab } from "./tabs";
import { useLocalStateZodDefault } from "@/hooks/store/useState";
import { z } from "zod";
import { Link } from "@/components/ui/link";
import { Q_MIGRATION_URL } from "@/lib/constants";
import { useAuth, useAuthRequest, useRefreshAuth } from "@/hooks/store/useAuth";

export default function LoginModal({ next }: { next: () => void }) {
  const midway = window?.fig?.constants?.midway ?? false;

  const waitlistUrl: string | undefined = window?.fig?.constants?.waitlistUrl;

  const [loginState, setLoginState] = useState<
    "not started" | "loading" | "logged in"
  >("not started");
  const [tab, setTab] = useLocalStateZodDefault<"builderId" | "iam">(
    "dashboard.loginTab",
    z.enum(["builderId", "iam"]),
    midway ? "iam" : "builderId",
  );

  // Since PKCE requires the ability to open a browser, we also support falling
  // back to device code in case of an error.
  const [loginMethod, setLoginMethod] = useState<"pkce" | "deviceCode">("pkce");

  // used for pkce
  const [currAuthRequestId, setAuthRequestId] = useAuthRequest();
  const [pkceTimedOut, setPkceTimedOut] = useState(false);
  useEffect(() => {
    if (pkceTimedOut) return;
    if (loginMethod === "pkce" && loginState === "loading") {
      const timer = setTimeout(() => setPkceTimedOut(true), 4000);
      return () => clearTimeout(timer);
    }
  }, [loginMethod, loginState, pkceTimedOut]);

  // used for device code
  const [loginCode, setLoginCode] = useState<string | null>(null);
  const [loginUrl, setLoginUrl] = useState<string | null>(null);
  const [copyToClipboardText, setCopyToClipboardText] = useState<
    "Copy to clipboard" | "Copied!"
  >("Copy to clipboard");
  const [error, setError] = useState<string | null>(null);
  const [completedOnboarding] = useLocalStateZodDefault(
    "desktop.completedOnboarding",
    z.boolean(),
    false,
  );
  const [showProfileTab, setShowProfileTab] = useState(false);
  const auth = useAuth();
  const refreshAuth = useRefreshAuth();

  // Social login specific states
  const [socialAuthRequestId, setSocialAuthRequestId] = useState<string | null>(
    null,
  );
  const [pendingProvider, setPendingProvider] = useState<
    "Google" | "Github" | null
  >(null);
  const [needInvitation, setNeedInvitation] = useState(false);
  const [invitationCode, setInvitationCode] = useState("");

  useEffect(() => {
    // Reset the auth request id so that we don't present the "OAuth cancelled" error
    // to the user.
    setAuthRequestId("");
  }, [loginMethod, setAuthRequestId]);

  async function handleLogin(startUrl?: string, region?: string) {
    if (loginMethod === "pkce") {
      handlePkceAuth(startUrl, region);
    } else {
      handleDeviceCodeAuth(startUrl, region);
    }
  }

  async function handlePkceAuth(issuerUrl?: string, region?: string) {
    setLoginState("loading");
    setPkceTimedOut(false);
    setError(null);
    // We need to reset the auth request state before attempting, otherwise
    // an expected auth request cancellation will be presented to the user
    // as an error.
    setAuthRequestId(undefined);
    const init = await Auth.startPkceAuthorization({
      issuerUrl,
      region,
    }).catch((err) => {
      setLoginState("not started");
      setError(err.message);
      console.error(err);
    });

    if (!init) return;
    setAuthRequestId(init.authRequestId);

    Native.open(init.url).catch((err) => {
      console.error(err);
      setError(
        "Failed to open the browser. As an alternative, try logging in with device code.",
      );
    });

    await Auth.finishPkceAuthorization({
      authRequestId: init.authRequestId,
    })
      .then(() => {
        Internal.sendWindowFocusRequest({});
        if (tab == "iam") {
          setShowProfileTab(true);
        } else {
          refreshAuth();
          next();
        }
      })
      .catch((err) => {
        // If this promise was originally for some older request attempt,
        // then we should just ignore the error.
        if (currAuthRequestId() === init.authRequestId) {
          setLoginState("not started");
          setError(err.message);
          console.error(err);
        }
      });
  }

  async function handleDeviceCodeAuth(startUrl?: string, region?: string) {
    setLoginState("loading");
    setError(null);
    setLoginUrl(null);
    setCopyToClipboardText("Copy to clipboard");
    await Auth.cancelPkceAuthorization().catch((err) => {
      console.error(err);
    });
    const init = await Auth.builderIdStartDeviceAuthorization({
      startUrl,
      region,
    }).catch((err) => {
      setLoginState("not started");
      setLoginCode(null);
      setError(err.message);
      console.error(err);
    });

    if (!init) return;

    setLoginCode(init.code);
    setLoginUrl(init.url);

    await Auth.builderIdPollCreateToken(init)
      .then(() => {
        setLoginState("logged in");
        Internal.sendWindowFocusRequest({});
        refreshAuth();
        next();
      })
      .catch((err) => {
        setLoginState("not started");
        setLoginCode(null);
        setError(err.message);
        console.error(err);
      });
  }

  function isSignUpBlocked(e: unknown) {
    const msg = String((e as any)?.message ?? e ?? "");
    return msg.includes("SIGN_IN_BLOCKED");
  }

  function normalizeAuthError(e: unknown) {
    const raw = String((e as any)?.message ?? e ?? "");

    const msg = raw.replace(/^OAuth error:\s*/i, "");

    if (msg.includes("access_denied")) {
      return "Authentication failed: The identity provider denied access to Kiro. Please ensure you grant all required permissions.";
    }

    return msg;
  }

  async function handleSocialLogin(provider: "Google" | "Github") {
    setError(null);
    setNeedInvitation(false);
    setInvitationCode("");
    setPendingProvider(provider);
    setLoginState("loading");

    try {
      // 1) start social authorization
      const init = await Auth.startSocialAuthorization({ provider });
      setSocialAuthRequestId(init.authRequestId);

      // 2) open browser to login
      await Native.open(init.url);

      // 3) first finish attempt (no invitation code)
      await Auth.finishSocialAuthorization({
        authRequestId: init.authRequestId,
      })
        .then(() => {
          Internal.sendWindowFocusRequest({});
          refreshAuth();
          setLoginState("logged in");
          next();
        })
        .catch((err: Error) => {
          if (isSignUpBlocked(err)) {
            setNeedInvitation(true);
            setLoginState("not started");
            setError(null);
            setPendingProvider(provider);
          } else {
            setError(normalizeAuthError(err));
            setLoginState("not started");
          }
        });
    } catch (e: any) {
      setError(e?.message ?? String(e));
      setLoginState("not started");
    }
  }

  async function submitInvitationCode() {
    if (!pendingProvider || !invitationCode) return;

    setError(null);
    setLoginState("loading");

    try {
      const init = await Auth.startSocialAuthorization({
        provider: pendingProvider,
      });
      await Native.open(init.url);
      await Auth.finishSocialAuthorization({
        authRequestId: init.authRequestId,
        invitationCode,
      });

      Internal.sendWindowFocusRequest({});
      refreshAuth();
      setLoginState("logged in");
      setNeedInvitation(false);
      setInvitationCode("");
      setSocialAuthRequestId(null);
      next();
    } catch (e: any) {
      console.log("[social login] validate error:", e, e?.message);
      const msg = String(e?.message ?? e ?? "");
      if (msg.includes("SIGN_IN_BLOCKED")) {
        setError("Invalid access code. Please try again.");
      } else {
        setError(msg);
      }
      setLoginState("not started");
    }
  }

  function resetInvitationFlow() {
    setNeedInvitation(false);
    setInvitationCode("");
    setPendingProvider(null);
    setSocialAuthRequestId(null);
    setError(null);
  }

  useEffect(() => {
    setLoginState(auth.authed ? "logged in" : "not started");
  }, [auth]);

  useEffect(() => {
    if (loginState !== "logged in" || showProfileTab) return;
    next();
  }, [loginState, showProfileTab, next]);

  if (needInvitation) {
    return (
      <div className="relative -m-10 rounded-lg overflow-hidden text-white">
        <div className="absolute inset-0 gradient-q-secondary-light" />

        <div className="relative p-6 pt-10 flex flex-col items-center gap-6">
          {/* Back */}
          <div className="w-full max-w-md">
            <Button
              variant="ghost"
              className="p-0 text-white/80 hover:text-white text-sm"
              onClick={resetInvitationFlow}
              aria-label="Go back"
              title="Go back"
            >
              ← Back
            </Button>
          </div>

          {/* Title */}
          <div className="w-full max-w-md flex flex-col gap-2">
            <h2 className="text-xl font-semibold leading-none font-ember tracking-tight">
              Looks like you're new here...
            </h2>
            <p className="text-sm text-white/90">
              Kiro CLI currently requires an access code to use. If you've
              received a code via email, enter it below.
            </p>
          </div>

          {/* Card */}
          <div className="w-full max-w-md bg-white/10 border border-white/20 rounded-xl p-5">
            <label className="text-sm font-medium block mb-3">
              Kiro Access Code
            </label>

            <div className="flex items-center gap-2">
              <input
                className="flex-1 min-w-0 h-10 rounded-lg px-3 text-black placeholder:text-gray-500"
                placeholder="KIRO-XXXX-XXXX"
                value={invitationCode}
                onChange={(e) => setInvitationCode(e.target.value)}
                onKeyDown={(e) => {
                  if (
                    e.key === "Enter" &&
                    invitationCode &&
                    loginState !== "loading"
                  ) {
                    submitInvitationCode();
                  }
                }}
              />
              <Button
                type="button"
                variant="glass"
                className="h-10 px-4 shrink-0 focus:outline-none focus:ring-2 focus:ring-white/30 focus-visible:ring-2 border border-white/40 rounded-lg hover:bg-white/10 transition-colors"
                onClick={submitInvitationCode}
                disabled={!invitationCode || loginState === "loading"}
              >
                {loginState === "loading" ? (
                  <span className="text-sm">Validating...</span>
                ) : (
                  "Validate"
                )}
              </Button>
            </div>

            {waitlistUrl && (
              <div className="mt-3 text-sm text-white/80">
                If you don't have a code please {" "}
                <Link
                  href={waitlistUrl}
                  rel="noreferrer"
                  className="underline underline-offset-2 hover:text-white transition-colors"
                >
                  join our waitlist
                </Link>
                .
              </div>
            )}
          </div>

          {error && (
            <div className="w-full max-w-md bg-red-500/20 backdrop-blur-sm border border-red-400/50 rounded-lg py-3 px-4">
              <p className="text-white text-sm">{error}</p>
            </div>
          )}
        </div>
      </div>
    );
  }

  return showProfileTab ? (
    <ProfileTab
      next={() => {
        refreshAuth();
        setLoginState("logged in");
        setShowProfileTab(false);
      }}
      back={() => {
        setLoginState("not started");
        setShowProfileTab(false);
      }}
    />
  ) : (
    <div className="flex flex-col items-center gap-8 gradient-q-secondary-light -m-10 pt-10 p-4 rounded-lg text-white">
      <div className="flex flex-col items-center gap-8">
        <Lockup />
        {!completedOnboarding && (
          <h2 className="text-xl text-white font-semibold select-none leading-none font-ember tracking-tight">
            Sign in to get started
          </h2>
        )}
        {completedOnboarding && tab == "builderId" && (
          <div className="text-center flex flex-col">
            <div className="font-ember font-bold">
              CodeWhisperer is now Amazon Q
            </div>
            <Link href={Q_MIGRATION_URL} className="text-sm">
              Read the announcement blog post
            </Link>
          </div>
        )}
      </div>
      {error && (
        <div className="flex flex-col items-center gap-2 w-full bg-red-200 border border-red-600 rounded py-2 px-2">
          <p className="text-black dark:text-white font-semibold text-center">
            Failed to login
          </p>
          <p className="text-black dark:text-white text-center">{error}</p>
          {loginMethod === "pkce" && loginState === "loading" && (
            <Button
              variant="ghost"
              className="self-center mx-auto text-black hover:bg-white/40"
              onClick={() => {
                setLoginMethod("deviceCode");
                setLoginState("not started");
                setError(null);
              }}
            >
              Login with Device Code
            </Button>
          )}
        </div>
      )}

      <div className="flex flex-col items-center gap-4 text-white text-sm">
        {loginState === "loading" && loginMethod === "pkce" ? (
          <>
            <p className="text-center w-80">
              Waiting for authentication in the browser to complete...
            </p>
            {pkceTimedOut && (
              <div className="text-center w-80">
                <p>Browser not opening?</p>
                <Button
                  variant="ghost"
                  className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white italic"
                  onClick={() => {
                    setLoginMethod("deviceCode");
                    setLoginState("not started");
                  }}
                >
                  Try authenticating with device code
                </Button>
              </div>
            )}
            <Button
              variant="glass"
              className="self-center w-32"
              onClick={() => {
                setLoginState("not started");
              }}
            >
              Back
            </Button>
          </>
        ) : loginState === "loading" &&
          loginMethod === "deviceCode" &&
          loginCode &&
          loginUrl ? (
          <>
            <p className="text-center w-80">
              Confirm code <span className="font-bold">{loginCode}</span> in the
              login page at the following link:
            </p>
            <p className="text-center">{loginUrl}</p>
            <Button
              variant="ghost"
              className="h-auto p-1 px-2 hover:bg-white/20 hover:text-white"
              onClick={() => {
                navigator.clipboard.writeText(loginUrl);
                setCopyToClipboardText("Copied!");
              }}
            >
              {copyToClipboardText}
            </Button>
            <Button
              variant="glass"
              className="self-center w-32"
              onClick={() => {
                setLoginState("not started");
                setLoginCode(null);
              }}
            >
              Back
            </Button>
          </>
        ) : (
          // Default tabs. Hide when invitation code UI is active.
          !needInvitation && (
            <Tab
              tab={tab}
              handleLogin={handleLogin}
              handleSocialLogin={handleSocialLogin as any}
              toggleTab={
                tab === "builderId"
                  ? () => setTab("iam")
                  : () => setTab("builderId")
              }
              signInText={completedOnboarding ? "Log back in" : "Sign in"}
            />
          )
        )}
      </div>
    </div>
  );
}
