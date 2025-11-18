# Build & Distribution Guide

## Quick Reference

| Purpose | Command | API URL | Output |
|---------|---------|---------|--------|
| **Local Development** | `cargo tauri dev` | localhost:3000 | Debug build, hot reload |
| **Test Installed Version** | `./install.sh` | localhost:3000 | Installed to /Applications |
| **Production Release** | `./build-release.sh` | ladderlegendsacademy.com | DMG for distribution |

---

## Development Workflows

### 1. Day-to-Day Development

```bash
# Start dev server with hot reload
LADDER_LEGENDS_API_HOST=http://localhost:3000 cargo tauri dev
```

**What this does:**
- Compiles in debug mode (fast)
- Hot reloads on code changes
- Connects to localhost:3000 backend
- Lives in `target/debug/`

### 2. Testing the "Real" Installed App

```bash
# Build and install to /Applications
./install.sh
```

**What this does:**
- **Clears all app data and settings** (fresh start for testing)
- Stops any running instances
- Removes cached data, preferences, Login Items
- Builds optimized release version
- **Uses localhost:3000 API** (dev mode)
- Creates .app bundle
- Copies to `/Applications/`
- Now findable in Spotlight
- Tests Login Items, icon, etc.

**Use when testing:**
- Spotlight integration
- Login Items / autostart
- App icon display
- Any macOS-specific features
- Fresh installation behavior

**⚠️ Important:** This script clears ALL app data for clean testing. This is intentional for development but would NOT happen with DMG/MSI installers (which preserve user data).

### 3. Creating Production Release

```bash
# Build for distribution
./build-release.sh
```

**What this does:**
- Builds optimized release version
- **Uses production API** (ladderlegendsacademy.com)
- Creates DMG installer
- Output: `src-tauri/target/release/bundle/dmg/Ladder Legends Uploader_0.1.0_x64.dmg`

**Upload the DMG to:**
- GitHub Releases
- Your website
- Anywhere users will download from

---

## Bundle Formats Explained

### macOS

**What we build:**
- ✅ `.app` bundle - The actual application (18.5 MB)
- ✅ `.dmg` disk image - **Distribute this!** (6.1 MB compressed)

**Why DMG?**
- Professional macOS installer
- Drag-to-Applications UI
- Smaller file size (compressed)
- Industry standard

### Windows (via GitHub Actions CI/CD)

**Recommended:**
- ✅ `.msi` - Microsoft Installer format
  - Official format
  - Trusted by enterprises
  - Integrates with Windows Update

**Alternative:**
- `.nsis` - Nullsoft installer
  - More customizable
  - Good for custom UX
  - Common in open-source

**Our choice: MSI** (standard, professional, expected by users)

---

## Environment Variable Behavior

The app supports BOTH runtime and compile-time configuration for maximum flexibility:

```rust
// Priority: runtime > compile-time > default
let base_url = env::var("LADDER_LEGENDS_API_HOST")           // 1. Runtime
    .ok()
    .or_else(|| option_env!("LADDER_LEGENDS_API_HOST")      // 2. Compile-time
        .map(String::from))
    .unwrap_or_else(|| "https://ladderlegendsacademy.com"   // 3. Production default
        .to_string());
```

**Priority Order:**
1. **Runtime** - Environment variable when running the app
2. **Compile-time** - Environment variable during build
3. **Default** - Production URL (https://ladderlegendsacademy.com)

**Usage Examples:**
```bash
# Runtime override (for testing installed app)
LADDER_LEGENDS_API_HOST=http://localhost:3000 "/Applications/Ladder Legends Uploader.app/Contents/MacOS/ladder-legends-uploader"

# Compile-time override (baked into binary)
LADDER_LEGENDS_API_HOST=http://localhost:3000 cargo tauri build

# Production build (no override)
cargo tauri build
```

**Our setup:**
- `cargo tauri dev` - Uses compile-time env var
- `./install.sh` - Builds with localhost baked in + clears all app data
- `./build-release.sh` - Builds with production URL (no env var)
- GitHub Actions - Builds with production URL (no env var)

---

## App Data & Settings

### What Data Does the App Store?

The uploader stores data in standard macOS locations:

```
~/Library/Application Support/com.ladderlegends.uploader/
  - Authentication tokens
  - User preferences
  - Upload history

~/Library/Caches/com.ladderlegends.uploader/
  - Temporary files
  - Cache data

~/Library/WebKit/com.ladderlegends.uploader/
  - localStorage data
  - Cookies
  - IndexedDB

~/Library/Preferences/com.ladderlegends.uploader.plist
  - macOS system preferences
  - Window position, etc.

Login Items
  - macOS system setting for auto-start
```

### When is Data Cleared?

| Installation Method | Clears Data? | Why? |
|-------------------|--------------|------|
| `./install.sh` | ✅ Yes | Development testing - ensures fresh state |
| DMG installer | ❌ No | Production - preserves user data |
| MSI installer | ❌ No | Production - preserves user data |
| Manual reinstall | ❌ No | User controls data deletion |

**Why clear data in development?**
- Tests fresh installation experience
- Prevents stale auth tokens
- Ensures Login Items register correctly
- Catches bugs that only appear on first run
- Makes testing reproducible

**To manually clear data:**
```bash
# Stop the app first
pkill -f "Ladder Legends Uploader"

# Clear all data
rm -rf ~/Library/Application\ Support/com.ladderlegends.uploader
rm -rf ~/Library/Caches/com.ladderlegends.uploader
rm -rf ~/Library/WebKit/com.ladderlegends.uploader
rm -f ~/Library/Preferences/com.ladderlegends.uploader.plist
```

---

## File Structure

```
ladder-legends-uploader/
├── install.sh              # Local dev: build + install with localhost API
├── build-release.sh        # Production: build DMG with prod API
├── CODE_SIGNING.md         # Guide for code signing setup
├── BUILD.md               # This file
│
├── src-tauri/
│   ├── tauri.conf.json    # App config, bundle settings, icons
│   ├── Cargo.toml         # Rust dependencies
│   ├── src/
│   │   ├── lib.rs         # Main Tauri commands
│   │   ├── device_auth.rs # API client (reads env var here)
│   │   └── ...
│   │
│   └── target/release/bundle/
│       ├── macos/
│       │   └── Ladder Legends Uploader.app
│       └── dmg/
│           └── Ladder Legends Uploader_0.1.0_x64.dmg  ← Distribute this!
│
└── dist/                  # Frontend HTML/JS
    ├── index.html
    └── main.js
```

---

## Distribution Checklist

### For MVP / Testing (Unsigned)

- [x] Build with `./build-release.sh`
- [x] Test DMG installer locally
- [ ] Upload DMG to GitHub Releases
- [ ] Document installation for users:
  - macOS: Right-click → Open (bypass Gatekeeper)
  - Windows: Click "More info" → Run anyway

### For Production (Signed)

**macOS:**
- [ ] Sign up for Apple Developer Program ($99/year)
- [ ] Get Developer ID Application certificate
- [ ] Configure in `tauri.conf.json`
- [ ] Build and sign
- [ ] Notarize with Apple
- [ ] Test on fresh Mac

**Windows:**
- [ ] Purchase code signing certificate (~$200-300/year)
- [ ] Configure in `tauri.conf.json`
- [ ] Build on Windows machine or GitHub Actions
- [ ] Sign MSI
- [ ] Test on fresh Windows PC

**See CODE_SIGNING.md for detailed steps**

---

## GitHub Actions Setup (Optional)

For automatic builds on every release:

1. Create `.github/workflows/release.yml`
2. Configure build matrix (macOS, Windows, Linux)
3. Store code signing secrets
4. Tag a release: `git tag v0.1.0 && git push --tags`
5. Action builds and uploads installers automatically

This lets you build Windows installers from your Mac!

---

## Quick Commands

```bash
# Development
cargo tauri dev                     # Dev mode with hot reload
./install.sh                        # Install dev version to /Applications

# Production
./build-release.sh                  # Build production DMG
open src-tauri/target/release/bundle/dmg/  # View DMG

# Clean builds
cargo clean                         # Remove all build artifacts
rm -rf target/                      # Nuclear option
```

---

## Troubleshooting

**"App is damaged and can't be opened"**
- macOS Gatekeeper blocking unsigned app
- Fix: `xattr -cr "/Applications/Ladder Legends Uploader.app"`
- Or: Right-click → Open (first time only)

**Can't find app in Spotlight**
- Rebuild search index: `sudo mdutil -E /Applications`
- Wait a few minutes for reindexing

**Login Items not working**
- App must be in `/Applications` or `~/Applications`
- Try: `./install.sh` to reinstall properly

**Wrong API URL in installed app**
- Check which script you used:
  - `./install.sh` = localhost
  - `./build-release.sh` = production
- Reinstall with correct script

