# Windows Certificate Setup

## Important: Requires Windows Machine

The Windows code signing certificate **must be created on a Windows machine** with PowerShell.

## Quick Setup

1. **Transfer the script to a Windows machine**
   - Copy `create-windows-cert.ps1` to a Windows computer
   - Or clone this repository on Windows

2. **Run PowerShell as Administrator**
   - Right-click PowerShell
   - Select "Run as Administrator"

3. **Execute the script**
   ```powershell
   .\create-windows-cert.ps1
   ```

4. **Follow the prompts**
   - Choose a secure password
   - Script will create certificate and export files to Desktop

5. **Update tauri.conf.json**
   - Copy the thumbprint shown by the script
   - Update `src-tauri/tauri.conf.json`:
   ```json
   "windows": {
     "certificateThumbprint": "PASTE_THUMBPRINT_HERE",
     "digestAlgorithm": "sha256",
     "timestampUrl": "http://timestamp.digicert.com"
   }
   ```

6. **Add to GitHub Secrets**
   - `WINDOWS_CERTIFICATE`: Contents of `ladder-legends-cert.base64.txt`
   - `WINDOWS_CERTIFICATE_PASSWORD`: Password you chose

## Testing

To test code signing works:
```powershell
# Build the app
cargo tauri build

# Verify signature
Get-AuthenticodeSignature ".\src-tauri\target\release\bundle\msi\*.msi"
```

Should show `Status: Valid` (self-signed will show as "UnknownError" but will still be signed).

## Alternative: GitHub Actions Only

If you don't have a Windows machine, you can:
1. Skip local Windows code signing
2. Set up GitHub Actions with Windows runner
3. Create certificate in GitHub Actions during first build
4. Store in GitHub secrets for future builds

See `.github/workflows/release.yml` for GitHub Actions setup.
