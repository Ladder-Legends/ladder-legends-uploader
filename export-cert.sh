#!/bin/bash
# Export macOS code signing certificate for GitHub CI

set -e

echo "🔐 Exporting code signing certificate for GitHub CI"
echo ""
echo "This will create a .p12 file and base64 encode it for GitHub secrets"
echo ""

# Prompt for password
read -s -p "Choose a password for the certificate export: " CERT_PASSWORD
echo ""
read -s -p "Confirm password: " CERT_PASSWORD_CONFIRM
echo ""

if [ "$CERT_PASSWORD" != "$CERT_PASSWORD_CONFIRM" ]; then
    echo "❌ Passwords don't match!"
    exit 1
fi

CERT_NAME="Ladder Legends Academy"
OUTPUT_DIR="$HOME/Desktop"
P12_FILE="$OUTPUT_DIR/ladder-legends-cert.p12"
BASE64_FILE="$OUTPUT_DIR/ladder-legends-cert.base64.txt"

# Export certificate as .p12
echo "📦 Exporting certificate..."
security export -k ~/Library/Keychains/login.keychain-db \
  -t identities \
  -f pkcs12 \
  -o "$P12_FILE" \
  -P "$CERT_PASSWORD"

# Base64 encode it
echo "🔤 Base64 encoding..."
base64 -i "$P12_FILE" > "$BASE64_FILE"

echo ""
echo "✅ Done! Files created on your Desktop:"
echo "  - ladder-legends-cert.p12 (certificate file)"
echo "  - ladder-legends-cert.base64.txt (base64 encoded for GitHub)"
echo ""
echo "📋 Next steps:"
echo "1. Go to GitHub repo → Settings → Secrets and variables → Actions"
echo "2. Add these repository secrets:"
echo "   - APPLE_CERTIFICATE: Paste contents of ladder-legends-cert.base64.txt"
echo "   - APPLE_CERTIFICATE_PASSWORD: $CERT_PASSWORD"
echo ""
echo "⚠️  Keep these files secure and delete them after adding to GitHub!"
echo "   rm $P12_FILE"
echo "   rm $BASE64_FILE"
