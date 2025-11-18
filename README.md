# Ladder Legends Replay Auto-Uploader

Automatically upload your StarCraft 2 replays to Ladder Legends Academy for instant analysis.

## Features

- 🔍 **Auto-detect SC2 folder** - Finds your replay folder on Windows, macOS, and Linux
- 🔐 **Device code authentication** - Simple code-based login (like GitHub CLI)
- 📤 **Auto-upload** - Automatically uploads new replays
- 🎨 **System tray integration** - Runs in background, minimize to tray
- 🚀 **Auto-start on boot** - Optional setting to launch automatically
- 🔄 **Auto-updates** - Automatically downloads and installs updates
- 💻 **Cross-platform** - Works on Windows, macOS (Intel + Apple Silicon), and Linux
- 🎯 **Lightweight** - Minimal resource usage with Rust/Tauri

## Installation

### Prerequisites

- Rust (1.83+)
- Tauri CLI

### Build from Source

```bash
# Clone the repository
git clone https://github.com/ladder-legends/ladder-legends-uploader.git
cd ladder-legends-uploader

# Build and run
cargo tauri dev
```

### Build for Release

```bash
cargo tauri build
```

## Usage

1. **Launch the app** - The app will automatically detect your SC2 replay folder
2. **Click "Login to Ladder Legends"** - Start the authentication flow
3. **Enter the code on the website** - Visit ladderlegendsacademy.com/activate and enter the displayed code
4. **Done!** - The app will now automatically upload new replays

## Device Code Authentication Flow

This app uses a device code flow similar to GitHub CLI or Azure CLI:

```
1. App requests a device code from the server
2. User sees a code like "ABCD-1234"
3. User visits ladderlegendsacademy.com/activate
4. User enters the code on the website
5. App polls the server and receives auth tokens
6. App stores tokens securely in OS keychain
```

## Development

### Project Structure

```
ladder-legends-uploader/
├── src-tauri/           # Rust backend
│   ├── src/
│   │   ├── main.rs      # Entry point
│   │   ├── lib.rs       # Tauri commands
│   │   ├── sc2_detector.rs   # SC2 folder detection
│   │   └── device_auth.rs    # Device code auth
│   └── Cargo.toml
├── dist/                # Frontend
│   ├── index.html       # UI
│   └── main.js          # App logic
└── README.md
```

### Running Tests

```bash
cargo test
```

## SC2 Replay Locations

The app automatically detects SC2 replays in these locations:

**Windows:**
```
C:\Users\<Username>\Documents\StarCraft II\Accounts\<Account ID>\<Realm>-<Region>-<Account Number>\Replays\Multiplayer\
```

**macOS:**
```
/Users/<Username>/Library/Application Support/Blizzard/StarCraft II/Accounts/<Account ID>/<Realm>-<Region>-<Account Number>/Replays/Multiplayer/
```

**Linux (Wine/Proton):**
```
~/.wine/drive_c/users/<Username>/Documents/StarCraft II/Accounts/<Account ID>/<Realm>-<Region>-<Account Number>/Replays/Multiplayer/
```

## API Endpoints

The app communicates with these endpoints:

- `POST /api/auth/device/code` - Request device code
- `GET /api/auth/device/poll` - Poll for authorization
- `POST /api/replays/upload` - Upload replay file

## Releases and Auto-Updates

Releases are automated via GitHub Actions. The app includes built-in auto-update functionality that:

- Checks for updates when launched
- Downloads updates in the background
- Shows a dialog when an update is ready to install
- Applies the update on next restart

### Creating a Release

1. Update the version in `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`
2. Commit the changes
3. Create and push a version tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```
4. GitHub Actions will automatically build binaries for Windows and macOS
5. A draft release will be created with all binaries
6. Review and publish the release

### First Release Setup

For the first release, generate signing keys for the updater:

```bash
npm run tauri signer generate
```

Add the output secrets to your GitHub repository at Settings → Secrets and variables → Actions:
- `TAURI_SIGNING_PRIVATE_KEY`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.

## License

MIT License - see LICENSE file for details

## Links

- [Ladder Legends Academy](https://ladderlegendsacademy.com)
- [Report Issues](https://github.com/ladder-legends/ladder-legends-uploader/issues)
- [Documentation](https://github.com/ladder-legends/ladder-legends-uploader/wiki)
