//! Auth token storage and management commands.

use std::fs;
use tauri::State;
use crate::types::{AuthTokens, UserData};
use crate::state::AppStateManager;

/// Save authentication tokens to persistent storage
#[tauri::command]
pub async fn save_auth_tokens(
    state_manager: State<'_, AppStateManager>,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    username: Option<String>,
    avatar_url: Option<String>,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Saving auth tokens for user: {:?}", username));
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let app_config_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| {
            let error_msg = format!("Failed to create config directory: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    let config_file = app_config_dir.join("auth.json");
    let user = username.map(|un| UserData {
        username: un,
        avatar_url,
    });

    let tokens = AuthTokens {
        access_token,
        refresh_token,
        expires_at,
        user,
    };

    let tokens_json = serde_json::to_string_pretty(&tokens)
        .map_err(|e| {
            let error_msg = format!("Failed to serialize auth tokens: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    fs::write(&config_file, tokens_json)
        .map_err(|e| {
            let error_msg = format!("Failed to save auth tokens: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    state_manager.debug_logger.debug("Auth tokens saved successfully".to_string());
    Ok(())
}

/// Load authentication tokens from persistent storage
#[tauri::command]
pub async fn load_auth_tokens(state_manager: State<'_, AppStateManager>) -> Result<Option<AuthTokens>, String> {
    state_manager.debug_logger.debug("Loading auth tokens from storage".to_string());
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let config_file = config_dir.join("ladder-legends-uploader").join("auth.json");

    if !config_file.exists() {
        state_manager.debug_logger.debug("No auth tokens file exists yet".to_string());
        return Ok(None);
    }

    let contents = fs::read_to_string(&config_file)
        .map_err(|e| {
            let error_msg = format!("Failed to read auth tokens: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    let tokens: AuthTokens = serde_json::from_str(&contents)
        .map_err(|e| {
            let error_msg = format!("Failed to parse auth tokens: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    if let Some(ref user) = tokens.user {
        state_manager.debug_logger.info(format!("Loaded auth tokens for user: {}", user.username));
    } else {
        state_manager.debug_logger.debug("Loaded auth tokens (no user info)".to_string());
    }

    Ok(Some(tokens))
}

/// Clear authentication tokens from storage (logout)
#[tauri::command]
pub async fn clear_auth_tokens(state_manager: State<'_, AppStateManager>) -> Result<(), String> {
    state_manager.debug_logger.info("Clearing auth tokens".to_string());
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    let config_file = config_dir.join("ladder-legends-uploader").join("auth.json");

    if config_file.exists() {
        fs::remove_file(&config_file)
            .map_err(|e| {
                let error_msg = format!("Failed to delete auth tokens: {}", e);
                state_manager.debug_logger.error(error_msg.clone());
                error_msg
            })?;
        state_manager.debug_logger.debug("Auth tokens file deleted".to_string());
    } else {
        state_manager.debug_logger.debug("No auth tokens file to delete".to_string());
    }

    Ok(())
}
