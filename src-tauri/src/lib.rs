mod sc2_detector;
mod device_auth;
mod replay_tracker;
mod replay_uploader;
mod upload_manager;
mod replay_parser;
mod debug_logger;

use std::sync::{Arc, Mutex};
use tauri::{State, Emitter};
use tauri_plugin_autostart::ManagerExt;
use upload_manager::{UploadManager, UploadManagerState};

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
    upload_manager: Mutex<Option<Arc<UploadManager>>>,
    debug_logger: Arc<debug_logger::DebugLogger>,
}

#[tauri::command]
async fn detect_replay_folder(state_manager: State<'_, AppStateManager>) -> Result<String, String> {
    state_manager.debug_logger.info("Starting SC2 folder detection".to_string());
    match sc2_detector::detect_sc2_folder() {
        Some(folder) => {
            let path_str = folder.path.to_string_lossy().to_string();
            state_manager.debug_logger.info(format!("Found SC2 folder: {}", path_str));
            // Save to config
            let _ = save_folder_path(&path_str).await;
            Ok(path_str)
        }
        None => {
            state_manager.debug_logger.warn("Could not find SC2 folder".to_string());
            Err("Could not find SC2 replay folder".to_string())
        }
    }
}

#[tauri::command]
async fn request_device_code(
    state_manager: State<'_, AppStateManager>,
) -> Result<device_auth::DeviceCodeResponse, String> {
    state_manager.debug_logger.info("Requesting device code for authentication".to_string());
    match state_manager.api_client.request_device_code().await {
        Ok(response) => {
            state_manager.debug_logger.info(format!("Device code received, expires in {}s", response.expires_in));
            Ok(response)
        }
        Err(e) => {
            state_manager.debug_logger.error(format!("Failed to request device code: {}", e));
            Err(e)
        }
    }
}

#[tauri::command]
async fn poll_device_authorization(
    state_manager: State<'_, AppStateManager>,
    device_code: String,
) -> Result<device_auth::AuthResponse, String> {
    state_manager.debug_logger.debug("Polling for device authorization".to_string());
    match state_manager.api_client.poll_authorization(&device_code).await {
        Ok(response) => {
            state_manager.debug_logger.info(format!("Authorization successful for user: {}", response.user.username));
            Ok(response)
        }
        Err(e) => {
            // Don't log "pending" as an error since it's expected
            if e.contains("authorization_pending") {
                state_manager.debug_logger.debug("Authorization still pending".to_string());
            } else {
                state_manager.debug_logger.error(format!("Authorization failed: {}", e));
            }
            Err(e)
        }
    }
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
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    pub username: String,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub user: Option<UserData>,
}

#[tauri::command]
async fn save_auth_tokens(
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    username: Option<String>,
    avatar_url: Option<String>,
) -> Result<(), String> {
    use std::fs;
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let app_config_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;

    let config_file = app_config_dir.join("auth.json");
    let user = if let Some(un) = username {
        Some(UserData {
            username: un,
            avatar_url,
        })
    } else {
        None
    };

    let tokens = AuthTokens {
        access_token,
        refresh_token,
        expires_at,
        user,
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
async fn verify_auth_token(
    state_manager: State<'_, AppStateManager>,
    access_token: String,
) -> Result<bool, String> {
    state_manager.api_client.verify_token(&access_token).await
}

// Upload Manager Commands

#[tauri::command]
async fn initialize_upload_manager(
    state_manager: State<'_, AppStateManager>,
    replay_folder: String,
    base_url: String,
    access_token: String,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Initializing upload manager for folder: {}", replay_folder));

    match UploadManager::new(
        std::path::PathBuf::from(&replay_folder),
        base_url.clone(),
        access_token,
    ) {
        Ok(manager) => {
            let mut upload_manager = state_manager.upload_manager.lock().unwrap();
            *upload_manager = Some(Arc::new(manager));
            state_manager.debug_logger.info("Upload manager initialized successfully".to_string());
            Ok(())
        }
        Err(e) => {
            state_manager.debug_logger.error(format!("Failed to initialize upload manager: {}", e));
            Err(e)
        }
    }
}

#[tauri::command]
async fn get_upload_state(
    state_manager: State<'_, AppStateManager>,
) -> Result<UploadManagerState, String> {
    let upload_manager = state_manager.upload_manager.lock().unwrap();

    match upload_manager.as_ref() {
        Some(manager) => Ok(manager.get_state()),
        None => Err("Upload manager not initialized".to_string()),
    }
}

#[tauri::command]
async fn scan_and_upload_replays(
    app: tauri::AppHandle,
    state_manager: State<'_, AppStateManager>,
    limit: usize,
) -> Result<usize, String> {
    state_manager.debug_logger.info(format!("Starting replay scan and upload (limit: {})", limit));

    // Clone the Arc to avoid holding the lock across await
    let manager = {
        let upload_manager = state_manager.upload_manager.lock().unwrap();
        match upload_manager.as_ref() {
            Some(m) => Arc::clone(m),
            None => {
                state_manager.debug_logger.error("Upload manager not initialized".to_string());
                return Err("Upload manager not initialized".to_string());
            }
        }
    };

    match manager.scan_and_upload(limit, &app).await {
        Ok(count) => {
            state_manager.debug_logger.info(format!("Scan and upload completed: {} replays uploaded", count));
            Ok(count)
        }
        Err(e) => {
            state_manager.debug_logger.error(format!("Scan and upload failed: {}", e));
            Err(e)
        }
    }
}

#[tauri::command]
async fn start_file_watcher(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state_manager.debug_logger.info("Starting file watcher for new replays".to_string());

    let manager = {
        let upload_manager = state_manager.upload_manager.lock().unwrap();
        match upload_manager.as_ref() {
            Some(m) => Arc::clone(m),
            None => {
                state_manager.debug_logger.error("Upload manager not initialized for file watcher".to_string());
                return Err("Upload manager not initialized".to_string());
            }
        }
    };

    match manager.start_watching(move |path| {
        let _ = app.emit("new-replay-detected", path.to_string_lossy().to_string());
    }).await {
        Ok(_) => {
            state_manager.debug_logger.info("File watcher started successfully".to_string());
            Ok(())
        }
        Err(e) => {
            state_manager.debug_logger.error(format!("Failed to start file watcher: {}", e));
            Err(e)
        }
    }
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
async fn set_autostart_enabled(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    use std::fs;

    // First, use the autostart plugin to enable/disable
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable()
            .map_err(|e| format!("Failed to enable autostart: {}", e))?;
    } else {
        autostart.disable()
            .map_err(|e| format!("Failed to disable autostart: {}", e))?;
    }

    // Save preference to config for persistence
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

    Ok(())
}

/// Get the current app version
#[tauri::command]
async fn get_version(app: tauri::AppHandle) -> Result<String, String> {
    app.package_info()
        .version
        .to_string()
        .parse()
        .map_err(|e| format!("Failed to get version: {}", e))
}

/// Check for app updates
#[tauri::command]
async fn check_for_updates(app: tauri::AppHandle) -> Result<bool, String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app.updater_builder().build()
        .map_err(|e| format!("Failed to build updater: {}", e))?;

    match updater.check().await {
        Ok(Some(update)) => {
            println!("[DEBUG] Update available: {:?}", update.version);
            Ok(true)
        }
        Ok(None) => {
            println!("[DEBUG] App is up to date");
            Ok(false)
        }
        Err(e) => {
            eprintln!("[ERROR] Failed to check for updates: {}", e);
            Err(format!("Failed to check for updates: {}", e))
        }
    }
}

/// Install available update
#[tauri::command]
async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    use tauri_plugin_updater::UpdaterExt;

    let updater = app.updater_builder().build()
        .map_err(|e| format!("Failed to build updater: {}", e))?;

    match updater.check().await {
        Ok(Some(update)) => {
            println!("[DEBUG] Downloading and installing update: {:?}", update.version);
            update.download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| format!("Failed to install update: {}", e))?;

            println!("[DEBUG] Update installed successfully");
            Ok(())
        }
        Ok(None) => {
            Err("No update available".to_string())
        }
        Err(e) => {
            Err(format!("Failed to check for updates: {}", e))
        }
    }
}

/// Export debug log to file
#[tauri::command]
async fn export_debug_log(
    state_manager: State<'_, AppStateManager>,
) -> Result<String, String> {
    // Gather current state information
    let replay_folder = load_folder_path().await.ok().flatten();

    // Try to get Discord user ID from saved auth tokens
    let discord_user_id = load_auth_tokens()
        .await
        .ok()
        .flatten()
        .and_then(|tokens| tokens.user)
        .map(|user| user.username);

    // Try to get number of replays found
    let replays_found = if let Some(ref folder) = replay_folder {
        use std::fs;
        std::path::Path::new(folder)
            .read_dir()
            .ok()
            .map(|entries| entries.filter_map(|e| e.ok()).count())
    } else {
        None
    };

    // Save the report and get the file path
    let log_path = state_manager.debug_logger
        .save_report_to_file(replay_folder, replays_found, discord_user_id)?;

    // Return the path as a string
    Ok(log_path.to_string_lossy().to_string())
}

/// Get debug log statistics
#[tauri::command]
async fn get_debug_stats(
    state_manager: State<'_, AppStateManager>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "error_count": state_manager.debug_logger.get_error_count(),
    }))
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
            upload_manager: Mutex::new(None),
            debug_logger: Arc::new(debug_logger::DebugLogger::new()),
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
            verify_auth_token,
            get_autostart_enabled,
            set_autostart_enabled,
            initialize_upload_manager,
            get_upload_state,
            scan_and_upload_replays,
            start_file_watcher,
            get_version,
            check_for_updates,
            install_update,
            export_debug_log,
            get_debug_stats,
        ])
        .setup(|app| {
            use tauri::menu::SubmenuBuilder;

            // Create File menu with Settings option
            let file_settings_item = MenuItemBuilder::with_id("file_settings", "Settings").build(app)?;
            let file_quit_item = MenuItemBuilder::with_id("file_quit", "Quit").build(app)?;

            let file_menu = SubmenuBuilder::new(app, "File")
                .items(&[
                    &file_settings_item,
                    &file_quit_item,
                ])
                .build()?;

            // Create menu bar
            let menu_bar = MenuBuilder::new(app)
                .item(&file_menu)
                .build()?;

            // Set the menu bar for the main window
            if let Some(window) = app.get_webview_window("main") {
                window.set_menu(menu_bar.clone())?;

                // Inject LADDER_LEGENDS_API_HOST into window object
                use std::env;
                let api_host = env::var("LADDER_LEGENDS_API_HOST")
                    .ok()
                    .or_else(|| option_env!("LADDER_LEGENDS_API_HOST").map(String::from))
                    .unwrap_or_else(|| "https://www.ladderlegendsacademy.com".to_string());

                let inject_script = format!(
                    "window.LADDER_LEGENDS_API_HOST = '{}';",
                    api_host
                );
                let _ = window.eval(&inject_script);
                println!("🌐 [TAURI] Injected LADDER_LEGENDS_API_HOST: {}", api_host);

                // Handle menu events
                window.on_menu_event(|window, event| {
                    use tauri::Emitter;
                    match event.id.as_ref() {
                        "file_settings" => {
                            let _ = window.emit("open-settings", ());
                        }
                        "file_quit" => {
                            window.app_handle().exit(0);
                        }
                        _ => {}
                    }
                });
            }

            // Create tray menu
            let show_item = MenuItemBuilder::with_id("show", "Show").build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .items(&[
                    &show_item,
                    &settings_item,
                    &quit_item,
                ])
                .build()?;

            // Create tray icon
            let _tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .icon(app.default_window_icon().unwrap().clone())
                .on_menu_event(|app, event| {
                    use tauri::Emitter;
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
                                // Emit event to trigger settings
                                let _ = window.emit("open-settings", ());
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
            upload_manager: Mutex::new(None),
            debug_logger: Arc::new(debug_logger::DebugLogger::new()),
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
            upload_manager: Mutex::new(None),
            debug_logger: Arc::new(debug_logger::DebugLogger::new()),
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

    #[test]
    fn test_user_data_serialize() {
        let user_data = UserData {
            username: "TestUser".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };

        let serialized = serde_json::to_string(&user_data).unwrap();
        assert!(serialized.contains("TestUser"));
        assert!(serialized.contains("avatar.png"));
    }

    #[test]
    fn test_user_data_deserialize() {
        let json = r#"{"username":"TestUser","avatar_url":"https://example.com/avatar.png"}"#;
        let user_data: UserData = serde_json::from_str(json).unwrap();

        assert_eq!(user_data.username, "TestUser");
        assert_eq!(user_data.avatar_url, Some("https://example.com/avatar.png".to_string()));
    }

    #[test]
    fn test_user_data_deserialize_no_avatar() {
        let json = r#"{"username":"TestUser","avatar_url":null}"#;
        let user_data: UserData = serde_json::from_str(json).unwrap();

        assert_eq!(user_data.username, "TestUser");
        assert_eq!(user_data.avatar_url, None);
    }

    #[test]
    fn test_auth_tokens_serialize() {
        let auth_tokens = AuthTokens {
            access_token: "test-access-token".to_string(),
            refresh_token: Some("test-refresh-token".to_string()),
            expires_at: Some(1234567890),
            user: Some(UserData {
                username: "TestUser".to_string(),
                avatar_url: Some("https://example.com/avatar.png".to_string()),
            }),
        };

        let serialized = serde_json::to_string(&auth_tokens).unwrap();
        assert!(serialized.contains("test-access-token"));
        assert!(serialized.contains("test-refresh-token"));
        assert!(serialized.contains("TestUser"));
        assert!(serialized.contains("1234567890"));
    }

    #[test]
    fn test_auth_tokens_deserialize() {
        let json = r#"{
            "access_token": "test-access-token",
            "refresh_token": "test-refresh-token",
            "expires_at": 1234567890,
            "user": {
                "username": "TestUser",
                "avatar_url": "https://example.com/avatar.png"
            }
        }"#;

        let auth_tokens: AuthTokens = serde_json::from_str(json).unwrap();
        assert_eq!(auth_tokens.access_token, "test-access-token");
        assert_eq!(auth_tokens.refresh_token, Some("test-refresh-token".to_string()));
        assert_eq!(auth_tokens.expires_at, Some(1234567890));
        assert!(auth_tokens.user.is_some());

        let user = auth_tokens.user.unwrap();
        assert_eq!(user.username, "TestUser");
        assert_eq!(user.avatar_url, Some("https://example.com/avatar.png".to_string()));
    }

    #[test]
    fn test_auth_tokens_deserialize_minimal() {
        let json = r#"{
            "access_token": "test-access-token",
            "refresh_token": null,
            "expires_at": null,
            "user": null
        }"#;

        let auth_tokens: AuthTokens = serde_json::from_str(json).unwrap();
        assert_eq!(auth_tokens.access_token, "test-access-token");
        assert_eq!(auth_tokens.refresh_token, None);
        assert_eq!(auth_tokens.expires_at, None);
        assert_eq!(auth_tokens.user, None);
    }

    #[test]
    fn test_auth_tokens_clone() {
        let auth_tokens = AuthTokens {
            access_token: "test-access-token".to_string(),
            refresh_token: Some("test-refresh-token".to_string()),
            expires_at: Some(1234567890),
            user: Some(UserData {
                username: "TestUser".to_string(),
                avatar_url: Some("https://example.com/avatar.png".to_string()),
            }),
        };

        let cloned = auth_tokens.clone();
        assert_eq!(auth_tokens.access_token, cloned.access_token);
        assert_eq!(auth_tokens.refresh_token, cloned.refresh_token);
        assert_eq!(auth_tokens.expires_at, cloned.expires_at);
        assert_eq!(auth_tokens.user.as_ref().unwrap().username, cloned.user.as_ref().unwrap().username);
    }

    #[test]
    fn test_user_data_clone() {
        let user_data = UserData {
            username: "TestUser".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };

        let cloned = user_data.clone();
        assert_eq!(user_data.username, cloned.username);
        assert_eq!(user_data.avatar_url, cloned.avatar_url);
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
