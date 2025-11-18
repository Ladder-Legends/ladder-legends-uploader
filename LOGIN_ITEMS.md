# Login Items / Autostart Behavior

## Current Implementation

The app uses Tauri's `tauri-plugin-autostart` with `MacosLauncher::LaunchAgent` mode.

### How LaunchAgents Work (macOS)

**Location:** `~/Library/LaunchAgents/Ladder Legends Uploader.plist`

**Mechanism:**
- Creates a property list file that tells macOS to launch the app on login
- Uses the legacy LaunchAgent API (pre-macOS 13)
- **Does NOT appear** in System Settings > Login Items (modern UI)
- Can be viewed with: `launchctl list | grep ladder`
- Works with unsigned apps (no code signing required)

**The Path Problem:**
When you enable "Launch at Login" in the app:
1. App captures its current path (where it's running from)
2. Creates plist with that path
3. **Problem:** If you later move the app (e.g., dev build → /Applications), the plist still points to the old location
4. LaunchAgent fails silently because the old path doesn't exist anymore

## Current Workaround

The `install.sh` script now:
1. Unloads the old LaunchAgent (if exists)
2. Deletes the plist file
3. Lets the app recreate it with the correct path when user enables autostart again

**User must:** Re-enable "Launch at Login" in Settings after running `install.sh`

## Limitations

### Why doesn't it appear in System Settings?

**System Settings > General > Login Items** shows two types:
1. **"Open at Login"** - Modern apps using SMAppService (macOS 13+)
2. **"Allow in the Background"** - Background items

LaunchAgents are neither - they're a legacy mechanism that works but doesn't integrate with the modern UI.

### Can users see it anywhere?

Yes, in older macOS versions:
- System Preferences > Users & Groups > Login Items (macOS 12 and earlier)

In all macOS versions via Terminal:
```bash
launchctl list | grep ladder
ls -la ~/Library/LaunchAgents/ | grep "Ladder Legends"
```

## Long-Term Solutions

### Option 1: Use SMAppService via smappservice-rs ⭐ RECOMMENDED

**Note:** Tauri's autostart plugin does NOT support SMAppService. We need to use a separate crate.

**Add dependency:**
```toml
# Cargo.toml
smappservice-rs = "0.1"
```

**Implementation:**
```rust
use smappservice::{AppService, ServiceType, ServiceStatus};

#[tauri::command]
async fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    let app_service = AppService::new(ServiceType::MainApp);

    if enabled {
        app_service.register()
            .map_err(|e| format!("Failed to register: {}", e))?;

        // Check if user approval is needed
        let status = app_service.status();
        if status == ServiceStatus::RequiresApproval {
            // Open System Settings for user to approve
            AppService::open_system_settings_login_items();
            return Err("User approval required - please allow in System Settings".to_string());
        }
    } else {
        app_service.unregister()
            .map_err(|e| format!("Failed to unregister: {}", e))?;
    }

    Ok(())
}
```

**Pros:**
- ✅ Appears in System Settings > Login Items properly
- ✅ Modern macOS-native integration (SMAppService)
- ✅ Single notification to user
- ✅ Better user experience
- ✅ No path issues (uses bundle identifier)
- ✅ Works with unsigned apps (user just needs to approve in System Settings)

**Cons:**
- ❌ **Requires macOS 13+ (Ventura)** as minimum version
- ❌ Need to remove tauri-plugin-autostart dependency for macOS
- ❌ Platform-specific code (need to keep LaunchAgent or alternative for older macOS)

### Option 2: Hybrid Approach (SMAppService + LaunchAgent)

Use SMAppService on macOS 13+ and fall back to LaunchAgent on older versions.

**Implementation:**
```rust
#[tauri::command]
async fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    // Detect macOS version
    let os_version = /* get macOS version */;

    if os_version >= 13.0 {
        // Use SMAppService on Ventura+
        use smappservice::{AppService, ServiceType};
        let app_service = AppService::new(ServiceType::MainApp);
        if enabled {
            app_service.register()?;
        } else {
            app_service.unregister()?;
        }
    } else {
        // Use LaunchAgent on older macOS
        use auto_launch::AutoLaunch;
        // ... existing LaunchAgent code
    }

    Ok(())
}
```

**Pros:**
- ✅ Best of both worlds
- ✅ Backwards compatible with older macOS
- ✅ Modern UX on current macOS versions

**Cons:**
- ❌ More complex code
- ❌ Need to test on multiple macOS versions
- ❌ Still need to handle path issues for LaunchAgent fallback

### Option 3: Fix Path Detection (Keep LaunchAgent)

Make the app detect when it's moved and automatically update the LaunchAgent path.

**Implementation:**
```rust
// On app startup, check if LaunchAgent path matches current path
// If not, re-register with correct path
```

**Pros:**
- Works with unsigned apps
- Compatible with all macOS versions
- No entitlements needed

**Cons:**
- Still doesn't show in modern System Settings UI
- More code complexity
- Need to handle permissions for writing to LaunchAgents

## Recommendation

### For MVP / Current Development

**Keep current LaunchAgent approach** because:
- ✅ Works without code signing
- ✅ Compatible with all macOS versions (10.13+)
- ✅ The path fix in `install.sh` is sufficient for testing
- ✅ Already implemented and working

**Trade-off:** Doesn't appear in System Settings > Login Items (but works)

### For Production v1.0

**Switch to SMAppService (smappservice-rs)** with either:

**Option A - Modern Only (Recommended):**
- Use `smappservice-rs` exclusively
- Set minimum macOS to 13.0 (Ventura, Oct 2022)
- 95% of active Macs are on Ventura+ as of 2024
- Professional integration with System Settings
- Code signing optional but recommended

**Option B - Backwards Compatible:**
- Use `smappservice-rs` on macOS 13+
- Fall back to LaunchAgent on macOS 12 and earlier
- Support wider range of users
- More complex code to maintain

### Why SMAppService for v1.0?

1. **Native Integration:** Shows up in System Settings > Login Items
2. **No Path Issues:** Uses bundle identifier instead of file paths
3. **Better UX:** Single notification, user controls in System Settings
4. **Works Unsigned:** User approval required but no code signing needed
5. **Future-proof:** Apple's modern API, LaunchAgent is legacy

### Code Signing Note

SMAppService **does not require code signing**, but you get a better experience with it:
- **Unsigned:** User sees prompt, must approve in System Settings
- **Signed:** Cleaner notification, less friction

Either way, the app will work. Get signing when you're ready to eliminate all security warnings.

## Testing Login Items

### After running install.sh:

1. Open the app from /Applications
2. Go to Settings
3. Enable "Launch at Login"
4. Verify plist created:
   ```bash
   cat ~/Library/LaunchAgents/Ladder\ Legends\ Uploader.plist
   ```
5. Check path points to /Applications:
   ```bash
   grep "/Applications" ~/Library/LaunchAgents/Ladder\ Legends\ Uploader.plist
   ```
6. Verify it's loaded:
   ```bash
   launchctl list | grep ladder
   ```
7. Test: Log out and log back in - app should launch

### Common Issues:

**App doesn't launch at login:**
- Check plist path is correct (should be /Applications)
- Check LaunchAgent is loaded: `launchctl list | grep ladder`
- Check Console.app for errors during login

**App launches but from wrong location:**
- Old plist still has build path
- Run `install.sh` again to clear it
- Re-enable "Launch at Login" in app settings

**App doesn't show in System Settings:**
- Expected behavior with LaunchAgent approach
- This is a limitation, not a bug
- Will be fixed when switching to modern LoginItem API

## Code Signing & Notarization

Currently the app is **unsigned**. This affects:

**Gatekeeper:**
- Users see "App is from an unidentified developer" warning
- Must right-click → Open first time

**Login Items:**
- LaunchAgent works fine without signing
- LoginItem (modern API) works better with signing
- Notarization not required for Login Items to work

**When to get code signing:**
- When you're ready to distribute to users (not just testing)
- When you want to switch to modern Login Items API
- When you want to eliminate security warnings

**See:** `CODE_SIGNING.md` for details on getting Apple Developer certificate
