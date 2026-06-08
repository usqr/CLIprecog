#!/usr/bin/env bash
# =============================================================================
# reinstall.sh  —  reinstall locally-built Precog into /Applications/Precog.app
#
# Installs the freshly-built binaries + webview assets + specs, then signs the
# binaries we own with the STABLE "Precog Dev" identity (see
# create-signing-cert.sh) instead of ad-hoc. A stable identity keeps the macOS
# Accessibility (TCC) grant valid across rebuilds, so the autocomplete popup
# keeps working without re-granting permission every time.
#
# Prereqs (build first):
#   cargo build --release -p q_cli -p figterm -p fig_input_method -p fig_desktop
#   pnpm build
# One-time before the first use:
#   ops-scripts/create-signing-cert.sh
#
# Usage:
#   ops-scripts/reinstall.sh              # normal rebuild reinstall
#   ops-scripts/reinstall.sh --reset-tcc  # also reset+re-grant Accessibility
#                                         # (use on the FIRST run after switching
#                                         #  from ad-hoc to the stable identity)
# =============================================================================
set -euo pipefail

CERT_CN="${PRECOG_SIGN_IDENTITY:-Precog Dev}"
APP="/Applications/Precog.app"
R="$APP/Contents/Resources"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESET_TCC=false
[ "${1:-}" = "--reset-tcc" ] && RESET_TCC=true

cd "$REPO"

if ! security find-identity -v -p codesigning 2>/dev/null | grep -q "$CERT_CN"; then
  echo "❌ Signing identity \"$CERT_CN\" not found." >&2
  echo "   Run ops-scripts/create-signing-cert.sh first." >&2
  exit 1
fi

for f in target/release/q_cli target/release/fig_desktop target/release/figterm; do
  [ -f "$f" ] || { echo "❌ Missing $f — build first (cargo build --release …)." >&2; exit 1; }
done

echo "🔧 Stopping running Precog…"
precog quit 2>/dev/null || true

echo "🔧 Installing binaries…"
install -m 755 target/release/q_cli       "$APP/Contents/MacOS/precog"
ln -sf "$APP/Contents/MacOS/precog" "$HOME/.local/bin/precog"
install -m 755 target/release/fig_desktop "$APP/Contents/MacOS/precog_desktop"
for n in precogterm "bash (precogterm)" "zsh (precogterm)" "fish (precogterm)" "nu (precogterm)"; do
  install -m 755 target/release/figterm "$HOME/.local/bin/$n"
done

echo "🔧 Refreshing dashboard + autocomplete webviews and compiled specs…"
rm -rf "$R/dashboard" "$R/autocomplete"
cp -R packages/dashboard-app/dist    "$R/dashboard"
cp -R packages/autocomplete-app/dist "$R/autocomplete"
cp -R packages/autocomplete-specs/build/. "$R/autocomplete-specs/build/"

echo "🔧 Signing with stable identity \"$CERT_CN\"…"
# Sign the secondary CLI binary, then SEAL THE WHOLE BUNDLE LAST. This is the
# critical step: the .app carries a CodeResources seal hashing every nested
# file, so replacing inner binaries without re-sealing leaves the bundle
# signature invalid ("nested code is modified") — and macOS then DENIES the
# Accessibility grant with kAXErrorAPIDisabled (-25211), no matter how many
# times you toggle the permission. Signing the bundle (NOT --deep) signs the
# main executable (precog_desktop) and seals nested code by reference, so the
# root-owned Contents/Helpers/* don't need re-signing.
codesign --force --sign "$CERT_CN" "$APP/Contents/MacOS/precog" 2>/dev/null || true
codesign --force --sign "$CERT_CN" "$APP/Contents/MacOS/precog_desktop"
codesign --force --sign "$CERT_CN" "$APP"

# An invalid signature => Accessibility denied. Fail loudly rather than ship it.
if codesign --verify --strict "$APP" 2>/dev/null; then
  echo "   ✅ bundle signature valid (Accessibility grant will be honored)."
else
  echo "❌ Bundle signature does NOT validate — Accessibility would be denied (-25211). Aborting." >&2
  codesign --verify --strict --verbose=2 "$APP" 2>&1 | tail -6 >&2
  exit 1
fi

if [ "$RESET_TCC" = true ]; then
  echo "🔧 Resetting Accessibility grant — you will re-grant ONCE after launch…"
  tccutil reset Accessibility dev.precog.cli 2>/dev/null || true
fi

echo "🔧 Launching…"
precog launch 2>/dev/null || true

echo
echo "Designated requirement of precog_desktop (must reference 'certificate leaf', not a bare cdhash):"
codesign -d -r- "$APP/Contents/MacOS/precog_desktop" 2>&1 | sed -n 's/^# designated => /   /p'

cat <<'NOTE'

Next:
  • If this was the first run with the stable identity (or you passed
    --reset-tcc): open System Settings → Privacy & Security → Accessibility,
    enable "Precog" (remove any stale entry first), then FULLY quit and reopen
    iTerm (not just a new tab).
  • Verify: type a command with a spec (e.g. `git che`). The popup should appear.
    L="$TMPDIR/precoglog/fig_desktop.log"
    grep -E "Sending caret update|Prevents flashing" "$L" | sed 's/\x1b\[[0-9;]*m//g' | tail
    (you want "Sending caret update" lines, and NO "Prevents flashing")
NOTE
