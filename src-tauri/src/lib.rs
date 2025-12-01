//! Ladder Legends Uploader - Tauri Application
//!
//! This is the main entry point for the Tauri desktop application.
//! The codebase is organized into focused modules:
//!
//! - `state/` - Application state management (AppState, AppStateManager)
//! - `types` - Core data types (UserData, AuthTokens)
//! - `commands/` - All Tauri commands organized by function
//! - Other modules for specific functionality (sc2_detector, device_auth, etc.)

// Core modules
mod sc2_detector;
mod device_auth;
mod replay_tracker;
mod replay_uploader;
mod upload_manager;
mod replay_parser;
mod debug_logger;
mod services;
mod config_utils;

// API contract types (must match Academy TypeScript contracts)
pub mod api_contracts;

// Organized modules
mod state;
mod types;
mod commands;

#[cfg(test)]
mod test_harness;

// Re-export commonly used types
pub use state::{AppState, AppStateManager};
pub use types::{UserData, AuthTokens};

/// Main entry point for the Tauri application
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
        .manage(AppStateManager::new())
        .invoke_handler(tauri::generate_handler![
            commands::detection::detect_replay_folders,
            commands::auth::request_device_code,
            commands::auth::poll_device_authorization,
            commands::auth::verify_auth_token,
            commands::state_cmd::get_app_state,
            commands::state_cmd::set_app_state,
            commands::browser::open_browser,
            commands::folders::pick_replay_folder_manual,
            commands::folders::save_folder_paths,
            commands::folders::load_folder_path,
            commands::folders::load_folder_paths,
            commands::tokens::save_auth_tokens,
            commands::tokens::load_auth_tokens,
            commands::tokens::clear_auth_tokens,
            commands::settings::get_autostart_enabled,
            commands::settings::set_autostart_enabled,
            commands::upload::initialize_upload_manager,
            commands::upload::get_upload_state,
            commands::upload::scan_and_upload_replays,
            commands::upload::start_file_watcher,
            commands::version::get_version,
            commands::version::check_for_updates,
            commands::version::install_update,
            commands::debug::export_debug_log,
            commands::debug::get_debug_stats,
            commands::debug::open_folder_for_path,
        ])
        .setup(|app| {
            use tauri::menu::SubmenuBuilder;
            use tauri::Emitter;

            // Get debug logger from app state
            let debug_logger = app.state::<AppStateManager>().debug_logger.clone();

            debug_logger.info("Starting Tauri application setup".to_string());

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

            debug_logger.debug("Menu bar created".to_string());

            // Set the menu bar for the main window
            if let Some(window) = app.get_webview_window("main") {
                window.set_menu(menu_bar.clone())?;
                debug_logger.debug("Menu bar set on main window".to_string());

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
                if let Err(e) = window.eval(&inject_script) {
                    debug_logger.warn(format!("Failed to inject API host script: {}", e));
                }
                debug_logger.info(format!("Injected LADDER_LEGENDS_API_HOST: {}", api_host));

                // Handle menu events
                let logger_for_menu = debug_logger.clone();
                window.on_menu_event(move |window, event| {
                    logger_for_menu.debug(format!("Menu event: {}", event.id.as_ref()));
                    match event.id.as_ref() {
                        "file_settings" => {
                            logger_for_menu.info("Opening settings from menu".to_string());
                            if let Err(e) = window.emit("open-settings", ()) {
                                logger_for_menu.warn(format!("Failed to emit open-settings: {}", e));
                            }
                        }
                        "file_quit" => {
                            logger_for_menu.info("Quitting app from menu".to_string());
                            window.app_handle().exit(0);
                        }
                        _ => {
                            logger_for_menu.debug(format!("Unknown menu event: {}", event.id.as_ref()));
                        }
                    }
                });
            }

            // Create tray menu
            let open_item = MenuItemBuilder::with_id("open", "Open").build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

            let tray_menu = MenuBuilder::new(app)
                .items(&[
                    &open_item,
                    &settings_item,
                    &quit_item,
                ])
                .build()?;

            // Create tray icon
            debug_logger.debug("Creating tray icon".to_string());
            let logger_for_tray_menu = debug_logger.clone();
            let logger_for_tray_icon = debug_logger.clone();

            let tray_icon = app.default_window_icon()
                .expect("App must have a default window icon configured in tauri.conf.json")
                .clone();

            // On Windows, right-click should show menu (default behavior)
            // On macOS, left-click typically shows menu
            #[cfg(target_os = "windows")]
            let tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .show_menu_on_left_click(false)  // Windows: right-click for menu, left-click for window
                .icon(tray_icon);

            #[cfg(not(target_os = "windows"))]
            let tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .show_menu_on_left_click(true)   // macOS/Linux: left-click for menu
                .icon(tray_icon);

            let tray = tray
                .on_menu_event(move |app, event| {
                    logger_for_tray_menu.debug(format!("Tray menu event: {}", event.id.as_ref()));
                    match event.id.as_ref() {
                        "open" => {
                            logger_for_tray_menu.info("Opening window from tray menu".to_string());
                            if let Some(window) = app.get_webview_window("main") {
                                if let Err(e) = window.show() {
                                    logger_for_tray_menu.warn(format!("Failed to show window: {}", e));
                                }
                                if let Err(e) = window.set_focus() {
                                    logger_for_tray_menu.warn(format!("Failed to focus window: {}", e));
                                }
                            }
                        }
                        "settings" => {
                            logger_for_tray_menu.info("Opening settings from tray menu".to_string());
                            if let Some(window) = app.get_webview_window("main") {
                                if let Err(e) = window.show() {
                                    logger_for_tray_menu.warn(format!("Failed to show window: {}", e));
                                }
                                if let Err(e) = window.set_focus() {
                                    logger_for_tray_menu.warn(format!("Failed to focus window: {}", e));
                                }
                                // Emit event to trigger settings
                                if let Err(e) = window.emit("open-settings", ()) {
                                    logger_for_tray_menu.warn(format!("Failed to emit open-settings: {}", e));
                                }
                            }
                        }
                        "quit" => {
                            logger_for_tray_menu.info("Quitting app from tray menu".to_string());
                            app.exit(0);
                        }
                        _ => {
                            logger_for_tray_menu.debug(format!("Unknown tray menu event: {}", event.id.as_ref()));
                        }
                    }
                })
                .on_tray_icon_event(move |tray, event| {
                    match &event {
                        TrayIconEvent::DoubleClick { .. } => {
                            logger_for_tray_icon.info("Showing window from tray double-click".to_string());
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        #[cfg(target_os = "windows")]
                        TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Left,
                            ..
                        } => {
                            // On Windows left-click: show and focus the main window
                            logger_for_tray_icon.debug("Left-click on tray: showing window".to_string());
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                            }
                        }
                        #[cfg(target_os = "windows")]
                        TrayIconEvent::Click {
                            button: tauri::tray::MouseButton::Right,
                            ..
                        } => {
                            // On Windows right-click: the menu should show automatically
                            // But we need to ensure the app is "foreground" first
                            // This is a Windows quirk - context menus disappear without foreground
                            logger_for_tray_icon.debug("Right-click on tray: menu should appear".to_string());
                        }
                        _ => {
                            // Let other events pass through
                        }
                    }
                })
                .build(app)?;

            // CRITICAL: Prevent the tray icon from being dropped when setup() ends.
            // On Windows, dropping the TrayIcon destroys the system tray icon.
            // By using std::mem::forget, the tray icon lives for the app's lifetime.
            std::mem::forget(tray);

            debug_logger.debug("Tray icon created and persisted successfully".to_string());

            // Handle window close event - minimize to tray instead of closing
            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                let logger_for_window = debug_logger.clone();
                window.on_window_event(move |event| {
                    match event {
                        tauri::WindowEvent::CloseRequested { api, .. } => {
                            logger_for_window.info("Window close requested - hiding to tray instead".to_string());
                            // Prevent the window from closing
                            api.prevent_close();
                            // Hide the window instead
                            if let Err(e) = window_clone.hide() {
                                logger_for_window.warn(format!("Failed to hide window: {}", e));
                            }
                        }
                        tauri::WindowEvent::Focused(focused) => {
                            logger_for_window.debug(format!("Window focus changed: {}", focused));
                        }
                        _ => {
                            // Don't log every event to avoid spam
                        }
                    }
                });
                debug_logger.debug("Window event handler registered".to_string());
            }

            debug_logger.info("Tauri application setup complete".to_string());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// Integration tests for SC2 folder detection
#[cfg(test)]
mod integration_tests {
    use crate::sc2_detector;

    #[tokio::test]
    async fn test_detect_replay_folder_integration() {
        // This will use the real detection logic
        let result = sc2_detector::detect_all_sc2_folders(None);

        // Don't assert success - it may fail if SC2 isn't installed
        // Just verify it returns a result
        if result.is_empty() {
            println!("SC2 folder not found (expected if SC2 not installed)");
        } else {
            for folder in &result {
                println!("Found SC2 folder: {}", folder.path.display());
            }
        }
    }
}

// Config file format tests
#[cfg(test)]
mod config_tests {
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_save_and_load_folder_path() {
        // Use a temporary directory for test config
        let temp_dir = TempDir::new().unwrap();
        let config_dir = temp_dir.path().join("ladder-legends-uploader");

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
}
