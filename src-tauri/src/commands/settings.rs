//! Application settings commands (autostart, etc).

use std::fs;
use tauri::State;
use tauri_plugin_autostart::ManagerExt;
use crate::state::AppStateManager;

/// Check if autostart is enabled
#[tauri::command]
pub async fn get_autostart_enabled(state_manager: State<'_, AppStateManager>) -> Result<bool, String> {
    state_manager.debug_logger.debug("Getting autostart enabled status".to_string());
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let config_file = config_dir.join("ladder-legends-uploader").join("config.json");

    if !config_file.exists() {
        state_manager.debug_logger.debug("No config file, autostart defaulting to disabled".to_string());
        return Ok(false); // Default to disabled
    }

    let contents = fs::read_to_string(&config_file)
        .map_err(|e| {
            let error_msg = format!("Failed to read config: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    let config: serde_json::Value = serde_json::from_str(&contents)
        .map_err(|e| {
            let error_msg = format!("Failed to parse config: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    let enabled = config.get("autostart_enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    state_manager.debug_logger.debug(format!("Autostart enabled: {}", enabled));
    Ok(enabled)
}

/// Enable or disable autostart
#[tauri::command]
pub async fn set_autostart_enabled(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
    enabled: bool,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Setting autostart to: {}", enabled));

    // First, use the autostart plugin to enable/disable
    let autostart = app.autolaunch();
    if enabled {
        autostart.enable()
            .map_err(|e| {
                let error_msg = format!("Failed to enable autostart: {}", e);
                state_manager.debug_logger.error(error_msg.clone());
                error_msg
            })?;
        state_manager.debug_logger.debug("Autostart enabled in system".to_string());
    } else {
        autostart.disable()
            .map_err(|e| {
                let error_msg = format!("Failed to disable autostart: {}", e);
                state_manager.debug_logger.error(error_msg.clone());
                error_msg
            })?;
        state_manager.debug_logger.debug("Autostart disabled in system".to_string());
    }

    // Save preference to config for persistence
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let app_config_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| {
            let error_msg = format!("Failed to create config directory: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    let config_file = app_config_dir.join("config.json");

    // Read existing config or create new one
    let mut config: serde_json::Value = if config_file.exists() {
        let contents = fs::read_to_string(&config_file)
            .map_err(|e| {
                let error_msg = format!("Failed to read config: {}", e);
                state_manager.debug_logger.error(error_msg.clone());
                error_msg
            })?;
        serde_json::from_str(&contents)
            .map_err(|e| {
                let error_msg = format!("Failed to parse config: {}", e);
                state_manager.debug_logger.error(error_msg.clone());
                error_msg
            })?
    } else {
        serde_json::json!({})
    };

    // Update autostart_enabled field
    if let Some(obj) = config.as_object_mut() {
        obj.insert("autostart_enabled".to_string(), serde_json::Value::Bool(enabled));
    }

    let config_json = serde_json::to_string_pretty(&config)
        .map_err(|e| {
            let error_msg = format!("Failed to serialize config: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    fs::write(&config_file, config_json)
        .map_err(|e| {
            let error_msg = format!("Failed to save config: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    state_manager.debug_logger.debug("Autostart preference saved to config".to_string());
    Ok(())
}
