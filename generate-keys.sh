#!/bin/bash

# Script to generate Tauri updater signing keys
# This will be run manually by the user

cd "$(dirname "$0")"

echo "Generating Tauri updater signing keys..."
echo ""
echo "You will be prompted for a password to protect the private key."
echo "Please choose a strong password and save it securely."
echo ""

export PATH="/Users/chadfurman/.cargo/bin:$PATH"

# Generate the keys
cargo tauri signer generate -w ./updater-keys.txt

if [ $? -eq 0 ]; then
    echo ""
    echo "✓ Keys generated successfully!"
    echo ""
    echo "The keys have been saved to: updater-keys.txt"
    echo ""
    echo "IMPORTANT: Next steps:"
    echo "1. Open updater-keys.txt and copy the public key"
    echo "2. Update src-tauri/tauri.conf.json with the public key"
    echo "3. Add the private key and password as GitHub secrets:"
    echo "   - TAURI_SIGNING_PRIVATE_KEY"
    echo "   - TAURI_SIGNING_PRIVATE_KEY_PASSWORD"
    echo ""
    echo "4. NEVER commit updater-keys.txt to git!"
    echo ""
else
    echo "✗ Failed to generate keys"
    exit 1
fi
