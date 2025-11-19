# Code Signing Setup Guide

## macOS Code Signing

### Prerequisites:
1. **Apple Developer Program** membership ($99/year)
   - Sign up at: https://developer.apple.com/programs/

2. **Developer ID Application Certificate**
   - In Xcode: Preferences → Accounts → Manage Certificates
   - Or use: https://developer.apple.com/account/resources/certificates/list

### Setup Steps:

1. **Generate Certificate Signing Request (CSR)**
   ```bash
   # This creates a .certSigningRequest file
   # Upload it to developer.apple.com
   ```

2. **Download Developer ID Certificate**
   - Type: "Developer ID Application"
   - Install in Keychain Access

3. **Configure Tauri**
   
   In `tauri.conf.json`:
   ```json
   {
     "bundle": {
       "macOS": {
         "minimumSystemVersion": "10.13",
         "signingIdentity": "Developer ID Application: Your Name (TEAM_ID)",
         "entitlements": null
       }
     }
   }
   ```

4. **Build and Sign**
   ```bash
   # Tauri will automatically sign during build
   cargo tauri build
   
   # Verify signature
   codesign --verify --deep --strict --verbose=2 \
     "src-tauri/target/release/bundle/macos/Ladder Legends Uploader.app"
   ```

5. **Notarize** (Required for macOS 10.15+)
   ```bash
   # Create app-specific password at appleid.apple.com
   
   # Notarize the DMG
   xcrun notarytool submit \
     "Ladder Legends Uploader_0.1.0_x64.dmg" \
     --apple-id "your@email.com" \
     --team-id "YOUR_TEAM_ID" \
     --password "app-specific-password" \
     --wait
   
   # Staple the notarization
   xcrun stapler staple "Ladder Legends Uploader_0.1.0_x64.dmg"
   ```

---

## Windows Code Signing

### Prerequisites:
1. **Code Signing Certificate** from a Certificate Authority:
   - **DigiCert** (recommended): ~$300/year
   - **Sectigo/Comodo**: ~$200/year
   - **SSL.com**: ~$200/year
   
   Get an "EV Code Signing Certificate" (preferred) or "Standard Code Signing Certificate"

2. **USB Token** (for EV certificates)
   - EV certificates are stored on hardware token
   - More trusted by Windows SmartScreen

### Setup Steps:

1. **Purchase Certificate**
   - Choose: "Code Signing Certificate for Windows"
   - Provide business documents (EV requires business validation)
   - Receive USB token (EV) or .pfx file (Standard)

2. **Configure Tauri**
   
   In `tauri.conf.json`:
   ```json
   {
     "bundle": {
       "windows": {
         "certificateThumbprint": "YOUR_CERT_THUMBPRINT",
         "digestAlgorithm": "sha256",
         "timestampUrl": "http://timestamp.digicert.com"
       }
     }
   }
   ```

3. **Sign with SignTool** (if using .pfx)
   ```powershell
   # Install Windows SDK for SignTool
   
   # Sign the MSI
   signtool sign /f certificate.pfx /p password /fd SHA256 \
     /t http://timestamp.digicert.com \
     "Ladder Legends Uploader_0.1.0_x64.msi"
   ```

4. **Verify Signature**
   ```powershell
   signtool verify /pa "Ladder Legends Uploader_0.1.0_x64.msi"
   ```

---

## GitHub Actions CI/CD with Code Signing

Store secrets in GitHub repository settings:

### macOS Secrets:
- `APPLE_CERTIFICATE` - Base64 encoded .p12 certificate
- `APPLE_CERTIFICATE_PASSWORD` - Certificate password
- `APPLE_ID` - Your Apple ID email
- `APPLE_TEAM_ID` - 10-character team ID
- `APPLE_APP_SPECIFIC_PASSWORD` - Generated at appleid.apple.com

### Windows Secrets:
- `WINDOWS_CERTIFICATE` - Base64 encoded .pfx file
- `WINDOWS_CERTIFICATE_PASSWORD` - Certificate password

### Example Workflow (simplified):
```yaml
- name: Import macOS certificate
  if: matrix.platform == 'macos-latest'
  run: |
    echo ${{ secrets.APPLE_CERTIFICATE }} | base64 --decode > certificate.p12
    security create-keychain -p actions temp.keychain
    security import certificate.p12 -k temp.keychain -P ${{ secrets.APPLE_CERTIFICATE_PASSWORD }}

- name: Build and sign
  uses: tauri-apps/tauri-action@v0
  env:
    APPLE_ID: ${{ secrets.APPLE_ID }}
    APPLE_PASSWORD: ${{ secrets.APPLE_APP_SPECIFIC_PASSWORD }}
    APPLE_TEAM_ID: ${{ secrets.APPLE_TEAM_ID }}
```

---

## Cost Summary

| Platform | Type | Cost/Year | Notes |
|----------|------|-----------|-------|
| macOS | Apple Developer Program | $99 | Required |
| Windows | Standard Code Signing | ~$200 | Good for indie |
| Windows | EV Code Signing | ~$300 | Better SmartScreen reputation |

**Total for both platforms:** ~$300-400/year

---

## Without Code Signing

If you don't sign:
- **macOS:** Users see "Unidentified Developer" → Must right-click → Open
- **Windows:** SmartScreen warning → "Windows protected your PC"
- **Auto-updater:** Won't work (requires signing)

For MVP/testing, you can ship unsigned and document the installation process.
For production/serious distribution, code signing is essential.
