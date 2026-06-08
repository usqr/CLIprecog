#!/usr/bin/env bash
# =============================================================================
# create-signing-cert.sh  —  ONE-TIME setup
#
# Creates a stable self-signed code-signing identity ("Precog Dev") in your
# login keychain so locally-built Precog binaries get a CONSTANT code identity
# across rebuilds.
#
# WHY THIS EXISTS
#   Ad-hoc signing (`codesign --sign -`) mints a brand-new cdhash on every
#   build. The app's *designated requirement* is then just that cdhash, so
#   macOS sees each rebuild as a different program and INVALIDATES the
#   Accessibility (TCC) grant. Without that grant, fig_desktop's AX caret read
#   fails (kAXErrorCannotComplete / -25204) and the autocomplete popup never
#   appears.
#
#   Signing with a stable identity changes the designated requirement to:
#       identifier "dev.precog.cli" and certificate leaf = H"<cert hash>"
#   which stays identical across rebuilds — so you grant Accessibility ONCE.
#
# Idempotent: re-running is a no-op once the identity exists.
# Run once, then use ops-scripts/reinstall.sh for every rebuild.
# =============================================================================
set -euo pipefail

CERT_CN="${PRECOG_SIGN_IDENTITY:-Precog Dev}"
KEYCHAIN="$HOME/Library/Keychains/login.keychain-db"
OPENSSL=/usr/bin/openssl   # pin to system LibreSSL for security-import-compatible PKCS#12

if security find-identity -v -p codesigning 2>/dev/null | grep -q "$CERT_CN"; then
  echo "✅ Code-signing identity \"$CERT_CN\" already exists. Nothing to do."
  exit 0
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

cat > "$tmp/cert.cnf" <<CNF
[ req ]
distinguished_name = dn
x509_extensions    = v3
prompt             = no
[ dn ]
CN = $CERT_CN
[ v3 ]
basicConstraints   = critical,CA:false
keyUsage           = critical,digitalSignature
extendedKeyUsage   = critical,codeSigning
CNF

echo "🔧 Generating self-signed code-signing certificate (valid 10 years)…"
"$OPENSSL" req -x509 -newkey rsa:2048 -nodes \
  -keyout "$tmp/precog.key" -out "$tmp/precog.crt" \
  -days 3650 -config "$tmp/cert.cnf" >/dev/null 2>&1

"$OPENSSL" pkcs12 -export \
  -inkey "$tmp/precog.key" -in "$tmp/precog.crt" \
  -name "$CERT_CN" -out "$tmp/precog.p12" -passout pass:precog >/dev/null 2>&1

echo "🔧 Importing into login keychain (granting codesign permission to use it)…"
security import "$tmp/precog.p12" -k "$KEYCHAIN" -P precog \
  -T /usr/bin/codesign -T /usr/bin/security >/dev/null

echo "🔧 Trusting the certificate for Code Signing…"
echo "   (a macOS dialog may ask for your login password — allow it)"
security add-trusted-cert -p codeSign -k "$KEYCHAIN" "$tmp/precog.crt" 2>/dev/null \
  || echo "   ⚠️  Could not set trust automatically. If the check below is empty, open" \
          "Keychain Access, find \"$CERT_CN\", and set it to 'Always Trust' for Code Signing."

# Allow apple tools (codesign) to use the private key without a popup on every
# signature. Needs the login keychain password; skipping just means codesign
# shows an 'Always Allow' dialog once on first use.
read -r -s -p "🔧 Enter your macOS login password to silence codesign prompts (or press Enter to skip): " KCPW
echo
if [ -n "$KCPW" ]; then
  if security set-key-partition-list -S apple-tool:,apple: -s -k "$KCPW" "$KEYCHAIN" >/dev/null 2>&1; then
    echo "   ✅ codesign may now use the key without prompting."
  else
    echo "   ⚠️  Could not set partition list; codesign will prompt once (click 'Always Allow')."
  fi
fi

echo
echo "Result:"
if security find-identity -v -p codesigning | grep "$CERT_CN"; then
  echo "✅ Done. Next: ops-scripts/reinstall.sh --reset-tcc   (first run only; re-grants Accessibility once)"
else
  echo "❌ \"$CERT_CN\" is not listed as a valid codesigning identity."
  echo "   Open Keychain Access → find \"$CERT_CN\" → Get Info → Trust →" \
       "set 'Code Signing' to 'Always Trust', then re-run this script."
  exit 1
fi
