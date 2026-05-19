# TODO — Auth removal (WIP on `chore/remove-login`)

## Status
Deleted but compile is broken. Fresh session needed.

## Already deleted
- `crates/fig_auth/`
- `crates/fig_api_client/`
- `crates/figterm/src/inline/`
- `crates/q_cli/src/cli/internal/inline_shell_completion.rs`
- `crates/fig_desktop_api/src/requests/{auth,codewhisperer,profile}.rs`
- `crates/fig_desktop/src/auth_watcher.rs`
- Workspace + per-crate `fig_auth`/`fig_api_client` deps

## Still to do

### 1. fig_telemetry (~3 files)
- `lib.rs:52,201,218,232` — remove `CodewhispererClient` field + ctor + `send_cw_telemetry_event`
- `event.rs:26,38` — remove `fig_auth::builder_id_token()` start_url tagging
- Drop `amzn-codewhisperer-streaming-client` dep if now unused

### 2. q_cli (~10 files)
- `cli/mod.rs` — `is_logged_in` import + onboarding gate; `launch_dashboard` auth check
- `cli/user.rs` — delete entirely (Login/Logout/Whoami/Profile)
- `cli/internal/mod.rs` — drop `InlineShellCompletion`/`InlineShellCompletionAccept` subcommands + `inline_shell_completion` mod
- `cli/installation.rs` — drop `login_interactive` import
- `cli/settings.rs`, `cli/init.rs`, `cli/uninstall.rs`, `cli/doctor/mod.rs`, `cli/doctor/checks/{midway,sshd_config}.rs`, `cli/debug/mod.rs`, `util/mod.rs` — strip `fig_auth::*` calls

### 3. fig_desktop (~8 files)
- `main.rs:162` — `is_logged_in` gate → always treat as logged in (or remove the gate)
- `tray.rs` — drop `is_logged_in` icon swap + `LOGIN_PATH` onboarding entry + `LOGIN_MENU_ID` arm
- `webview/mod.rs` — `LOGIN_PATH` const + `show_onboarding` flag
- `request/{user,mod}.rs` — drop `user_logged_in_callback`, `user_logout`
- `local_ipc/commands.rs` — drop `login`/`logout` handlers
- `local_ipc/mod.rs` — drop `Login`/`Logout` from imports + match arms
- `platform/macos.rs` — drop is_logged_in refs
- `event.rs` — drop ShowMessageNotification login flows if any

### 4. fig_desktop_api
- `handler.rs` — drop `Auth*Request` imports + match arms + `UserLogoutRequest`
- `requests/mod.rs` — drop `pub mod {auth,codewhisperer,profile}`

### 5. figterm
- `main.rs`, `event_handler.rs`, `message.rs` — drop inline completion calls

### 6. proto
- `fig.proto` — remove `Auth*Request/Response`, `UserLogoutRequest`, `ListAvailableProfilesRequest`, `SetProfileRequest`, `CodewhispererListCustomizationRequest`
- `local.proto` — remove `LoginCommand`, `LogoutCommand`

### 7. fig_ipc
- `local.rs` — drop `login_command`, `logout_command` + `LoginCommand`/`LogoutCommand` imports

### 8. fig_settings
- Drop state keys: `desktop.completedOnboarding`, `desktop.auth-watcher.logged-in`, `dashboard.loginTab`
- Drop migration `005_auth_table.sql`

### 9. Dashboard (TypeScript)
- `packages/dashboard-app/src/components/installs/modal/login/` — delete
- `packages/dashboard-app/src/hooks/store/useAuth.ts` — delete
- `packages/api-bindings/src/auth.ts` — delete
- Strip login modal from onboarding pages

### 10. amzn-* crates
- Verify `amzn-codewhisperer-client`, `amzn-consolas-client`, `amzn-codewhisperer-streaming-client`, `amzn-qdeveloper-streaming-client` are unused → delete

## Verification
- `cargo check --workspace --tests`
- `pnpm -r build` for dashboard
- Run `precog setup`, `precog doctor`, autocomplete UI smoke test
