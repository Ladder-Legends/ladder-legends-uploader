# Windows Code Signing Certificate Creation Script
# Run this in PowerShell as Administrator on a Windows machine

Write-Host "🔐 Creating Windows Code Signing Certificate" -ForegroundColor Cyan
Write-Host ""

# Check if running as Administrator
$isAdmin = ([Security.Principal.WindowsPrincipal] [Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $isAdmin) {
    Write-Host "❌ Error: This script must be run as Administrator" -ForegroundColor Red
    Write-Host "Right-click PowerShell and select 'Run as Administrator'" -ForegroundColor Yellow
    exit 1
}

# Prompt for password
$password = Read-Host "Choose a password for certificate export" -AsSecureString
$passwordConfirm = Read-Host "Confirm password" -AsSecureString

# Convert to plain text for comparison
$passwordPlain = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($password))
$passwordConfirmPlain = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($passwordConfirm))

if ($passwordPlain -ne $passwordConfirmPlain) {
    Write-Host "❌ Passwords don't match!" -ForegroundColor Red
    exit 1
}

Write-Host ""
Write-Host "📦 Creating self-signed certificate..." -ForegroundColor Yellow

# Create the certificate
$cert = New-SelfSignedCertificate `
    -Type CodeSigningCert `
    -Subject "CN=Ladder Legends Academy, O=Ladder Legends, C=US" `
    -KeyAlgorithm RSA `
    -KeyLength 2048 `
    -Provider "Microsoft Enhanced RSA and AES Cryptographic Provider" `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -NotAfter (Get-Date).AddYears(5)

if (-not $cert) {
    Write-Host "❌ Failed to create certificate" -ForegroundColor Red
    exit 1
}

$thumbprint = $cert.Thumbprint
Write-Host "✅ Certificate created successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "📋 Certificate Details:" -ForegroundColor Cyan
Write-Host "  Subject: $($cert.Subject)" -ForegroundColor White
Write-Host "  Thumbprint: $thumbprint" -ForegroundColor Yellow
Write-Host "  Expires: $($cert.NotAfter)" -ForegroundColor White
Write-Host ""

# Export locations
$desktopPath = [Environment]::GetFolderPath("Desktop")
$pfxPath = Join-Path $desktopPath "ladder-legends-cert.pfx"
$base64Path = Join-Path $desktopPath "ladder-legends-cert.base64.txt"

# Export certificate as PFX
Write-Host "💾 Exporting certificate..." -ForegroundColor Yellow
Export-PfxCertificate -Cert $cert -FilePath $pfxPath -Password $password | Out-Null

# Base64 encode for GitHub
$pfxBytes = [System.IO.File]::ReadAllBytes($pfxPath)
$base64String = [System.Convert]::ToBase64String($pfxBytes)
Set-Content -Path $base64Path -Value $base64String

Write-Host "✅ Certificate exported to Desktop!" -ForegroundColor Green
Write-Host ""
Write-Host "📄 Files created:" -ForegroundColor Cyan
Write-Host "  - ladder-legends-cert.pfx" -ForegroundColor White
Write-Host "  - ladder-legends-cert.base64.txt" -ForegroundColor White
Write-Host ""
Write-Host "📝 Next Steps:" -ForegroundColor Cyan
Write-Host ""
Write-Host "1. Update src-tauri/tauri.conf.json:" -ForegroundColor Yellow
Write-Host "   ""certificateThumbprint"": ""$thumbprint""" -ForegroundColor White
Write-Host ""
Write-Host "2. Add to GitHub repository secrets:" -ForegroundColor Yellow
Write-Host "   - WINDOWS_CERTIFICATE: (paste contents of ladder-legends-cert.base64.txt)" -ForegroundColor White
Write-Host "   - WINDOWS_CERTIFICATE_PASSWORD: $passwordPlain" -ForegroundColor White
Write-Host ""
Write-Host "⚠️  Security reminder:" -ForegroundColor Red
Write-Host "   - Keep these files secure" -ForegroundColor White
Write-Host "   - Delete from Desktop after adding to GitHub" -ForegroundColor White
Write-Host "   - Certificate is also stored in Windows Certificate Store" -ForegroundColor White
Write-Host ""

# Copy thumbprint to clipboard if available
if (Get-Command Set-Clipboard -ErrorAction SilentlyContinue) {
    $thumbprint | Set-Clipboard
    Write-Host "📋 Thumbprint copied to clipboard!" -ForegroundColor Green
}
