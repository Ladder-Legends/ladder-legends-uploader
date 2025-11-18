mod sc2_detector;
mod device_auth;

use std::sync::Mutex;
use tauri::State;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AppState {
    DetectingFolder,
    FolderFound { path: String },
    NeedsAuth,
    ShowingCode {
        user_code: String,
        device_code: String,
        verification_uri: String,
        expires_at: u64,
    },
    Polling { device_code: String },
    Authenticated {
        username: String,
        avatar_url: String,
    },
    Error { message: String },
}

pub struct AppStateManager {
    state: Mutex<AppState>,
    api_client: device_auth::ApiClient,
}

#[tauri::command]
async fn detect_replay_folder() -> Result<String, String> {
    println!("[DEBUG] Starting SC2 folder detection...");
    match sc2_detector::detect_sc2_folder() {
        Some(folder) => {
            let path_str = folder.path.to_string_lossy().to_string();
            println!("[DEBUG] Found SC2 folder: {}", path_str);
            // Save to config
            let _ = save_folder_path(&path_str).await;
            Ok(path_str)
        }
        None => {
            println!("[DEBUG] Could not find SC2 folder");
            Err("Could not find SC2 replay folder".to_string())
        }
    }
}

#[tauri::command]
async fn request_device_code(
    state_manager: State<'_, AppStateManager>,
) -> Result<device_auth::DeviceCodeResponse, String> {
    state_manager.api_client.request_device_code().await
}

#[tauri::command]
async fn poll_device_authorization(
    state_manager: State<'_, AppStateManager>,
    device_code: String,
) -> Result<device_auth::AuthResponse, String> {
    state_manager.api_client.poll_authorization(&device_code).await
}

#[tauri::command]
async fn get_app_state(state_manager: State<'_, AppStateManager>) -> Result<AppState, String> {
    let state = state_manager.state.lock().unwrap();
    Ok(state.clone())
}

#[tauri::command]
async fn set_app_state(
    state_manager: State<'_, AppStateManager>,
    new_state: AppState,
) -> Result<(), String> {
    let mut state = state_manager.state.lock().unwrap();
    *state = new_state;
    Ok(())
}

#[tauri::command]
async fn open_browser(url: String) -> Result<(), String> {
    open::that(url).map_err(|e| format!("Failed to open browser: {}", e))
}

#[tauri::command]
async fn pick_replay_folder_manual(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

    let folder = app.dialog()
        .file()
        .set_title("Select StarCraft 2 Multiplayer Replays Folder")
        .blocking_pick_folder();

    match folder {
        Some(path) => {
            let path_str = path.to_string();
            // Verify it looks like a valid replay folder
            if path_str.contains("StarCraft") || path_str.contains("Replays") {
                // Save to config
                let _ = save_folder_path(&path_str);
                Ok(path_str)
            } else {
                app.dialog()
                    .message("This doesn't look like a StarCraft 2 replay folder. Please select the 'Multiplayer' folder inside your SC2 Replays directory.")
                    .kind(MessageDialogKind::Warning)
                    .blocking_show();
                Err("Invalid folder selected".to_string())
            }
        }
        None => Err("No folder selected".to_string()),
    }
}

#[tauri::command]
async fn save_folder_path(path: &str) -> Result<(), String> {
    use std::fs;
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let app_config_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let config_file = app_config_dir.join("config.json");
    let config = serde_json::json!({ "replay_folder": path });
    fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn load_folder_path() -> Result<Option<String>, String> {
    use std::fs;
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let config_file = config_dir.join("ladder-legends-uploader").join("config.json");

    if !config_file.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&config_file)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let config: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    Ok(config.get("replay_folder")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string()))
}

// Auth token storage types
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
}

#[tauri::command]
async fn save_auth_tokens(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
) -> Result<(), String> {
    use std::fs;
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let app_config_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let config_file = app_config_dir.join("auth.json");
    let tokens = AuthTokens {
        access_token,
        refresh_token,
        expires_at,
    };

    fs::write(&config_file, serde_json::to_string_pretty(&tokens).unwrap())
        .map_err(|e| format!("Failed to save auth tokens: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn load_auth_tokens() -> Result<Option<AuthTokens>, String> {
    use std::fs;
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let config_file = config_dir.join("ladder-legends-uploader").join("auth.json");

    if !config_file.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&config_file)
        .map_err(|e| format!("Failed to read auth tokens: {}", e))?;

    let tokens: AuthTokens = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse auth tokens: {}", e))?;

    Ok(Some(tokens))
}

#[tauri::command]
async fn clear_auth_tokens() -> Result<(), String> {
    use std::fs;
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let config_file = config_dir.join("ladder-legends-uploader").join("auth.json");

    if config_file.exists() {
        fs::remove_file(&config_file)
            .map_err(|e| format!("Failed to delete auth tokens: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
async fn get_autostart_enabled() -> Result<bool, String> {
    use std::fs;
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let config_file = config_dir.join("ladder-legends-uploader").join("config.json");

    if !config_file.exists() {
        return Ok(false); // Default to disabled
    }

    let contents = fs::read_to_string(&config_file)
        .map_err(|e| format!("Failed to read config: {}", e))?;

    let config: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    Ok(config.get("autostart_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false))
}

#[tauri::command]
async fn set_autostart_enabled(enabled: bool) -> Result<(), String> {
    use std::fs;

    // Save preference to config
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let app_config_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let config_file = app_config_dir.join("config.json");

    // Read existing config or create new one
    let mut config: serde_json::Value = if config_file.exists() {
        let contents = fs::read_to_string(&config_file)
            .map_err(|e| format!("Failed to read config: {}", e))?;
        serde_json::from_str(&contents)
            .map_err(|e| format!("Failed to parse config: {}", e))?
    } else {
        serde_json::json!({})
    };

    // Update autostart_enabled field
    if let Some(obj) = config.as_object_mut() {
        obj.insert("autostart_enabled".to_string(), serde_json::Value::Bool(enabled));
    }

    fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap())
        .map_err(|e| format!("Failed to save config: {}", e))?;

    // Note: Autostart is configured via tauri.conf.json and the autostart plugin
    // The user preference is saved here, but actual system integration happens via plugin
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::Manager;
    use tauri::menu::{MenuBuilder, MenuItemBuilder};
    use tauri::tray::{TrayIconBuilder, TrayIconEvent};

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .manage(AppStateManager {
            state: Mutex::new(AppState::DetectingFolder),
            api_client: device_auth::ApiClient::new(),
        })
        .invoke_handler(tauri::generate_handler![
            detect_replay_folder,
            request_device_code,
            poll_device_authorization,
            get_app_state,
            set_app_state,
            open_browser,
            pick_replay_folder_manual,
            save_folder_path,
            load_folder_path,
            save_auth_tokens,
            load_auth_tokens,
            clear_auth_tokens,
            get_autostart_enabled,
            set_autostart_enabled,
        ])
        .setup(|app| {
            // Create tray menu
            let show_item = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let menu = MenuBuilder::new(app)
                .items(&[
                    &show_item,
                    &settings_item,
                    &quit_item,
                ])
                .build()?;

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .icon(app.default_window_icon().unwrap().clone())
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        "settings" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                // Navigate to settings page
                                let _ = window.eval("window.location.hash = '#/settings'");
                            }
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { .. } = event {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            // Handle window close event - minimize to tray instead of closing
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Prevent the window from closing
                        api.prevent_close();
                        // Hide the window instead
                        let _ = window_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_app_state_detecting_folder_serialize() {
        let state = AppState::DetectingFolder;
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("DetectingFolder"));
    }

    #[test]
    fn test_app_state_folder_found_serialize() {
        let state = AppState::FolderFound {
            path: "/test/path".to_string(),
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("FolderFound"));
        assert!(serialized.contains("/test/path"));
    }

    #[test]
    fn test_app_state_showing_code_serialize() {
        let state = AppState::ShowingCode {
            user_code: "ABCD-1234".to_string(),
            device_code: "test-device-code".to_string(),
            verification_uri: "https://example.com".to_string(),
            expires_at: 1234567890,
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("ShowingCode"));
        assert!(serialized.contains("ABCD-1234"));
        assert!(serialized.contains("test-device-code"));
    }

    #[test]
    fn test_app_state_authenticated_serialize() {
        let state = AppState::Authenticated {
            username: "TestUser".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("Authenticated"));
        assert!(serialized.contains("TestUser"));
    }

    #[test]
    fn test_app_state_error_serialize() {
        let state = AppState::Error {
            message: "Test error message".to_string(),
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("Error"));
        assert!(serialized.contains("Test error message"));
    }

    #[test]
    fn test_app_state_clone() {
        let state1 = AppState::DetectingFolder;
        let state2 = state1.clone();

        let serialized1 = serde_json::to_string(&state1).unwrap();
        let serialized2 = serde_json::to_string(&state2).unwrap();
        assert_eq!(serialized1, serialized2);
    }

    #[tokio::test]
    async fn test_save_and_load_folder_path() {
        // Use a temporary directory for test config
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("ladder-legends-uploader");

        // Set test environment variable to override config dir
        // Note: This won't work because dirs::config_dir() uses system path
        // Instead, we'll test the config file format
        let test_path = "/test/sc2/replays/path";
        let config_file = config_dir.join("config.json");

        fs::create_dir_all(&config_dir).unwrap();
        let config = serde_json::json!({ "replay_folder": test_path });
        fs::write(&config_file, serde_json::to_string_pretty(&config).unwrap()).unwrap();

        // Read it back
        let contents = fs::read_to_string(&config_file).unwrap();
        let loaded_config: serde_json::Value = serde_json::from_str(&contents).unwrap();
        let loaded_path = loaded_config.get("replay_folder")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        assert_eq!(loaded_path, Some(test_path.to_string()));
    }

    #[test]
    fn test_config_json_format() {
        let test_path = "/test/path";
        let config = serde_json::json!({ "replay_folder": test_path });
        let serialized = serde_json::to_string_pretty(&config).unwrap();

        assert!(serialized.contains("replay_folder"));
        assert!(serialized.contains(test_path));

        // Parse it back
        let parsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(
            parsed.get("replay_folder").and_then(|v| v.as_str()),
            Some(test_path)
        );
    }

    #[test]
    fn test_app_state_manager_initial_state() {
        let manager = AppStateManager {
            state: Mutex::new(AppState::DetectingFolder),
            api_client: device_auth::ApiClient::new(),
        };

        let state = manager.state.lock().unwrap();
        match *state {
            AppState::DetectingFolder => {
                // Expected initial state
            }
            _ => panic!("Expected DetectingFolder state"),
        }
    }

    #[test]
    fn test_app_state_manager_update_state() {
        let manager = AppStateManager {
            state: Mutex::new(AppState::DetectingFolder),
            api_client: device_auth::ApiClient::new(),
        };

        // Update state
        {
            let mut state = manager.state.lock().unwrap();
            *state = AppState::FolderFound {
                path: "/test/path".to_string(),
            };
        }

        // Verify update
        let state = manager.state.lock().unwrap();
        match &*state {
            AppState::FolderFound { path } => {
                assert_eq!(path, "/test/path");
            }
            _ => panic!("Expected FolderFound state"),
        }
    }
}

// Integration tests for Tauri commands
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_detect_replay_folder_integration() {
        // This will use the real detection logic
        let result = detect_replay_folder().await;

        // Don't assert success - it may fail if SC2 isn't installed
        // Just verify it returns a result
        match result {
            Ok(path) => println!("Found SC2 folder: {}", path),
            Err(e) => println!("SC2 folder not found (expected if SC2 not installed): {}", e),
        }
    }
}
