//! Authentication-related commands for device code flow.

use tauri::State;
use crate::device_auth;
use crate::state::AppStateManager;

/// Request a new device code for authentication
#[tauri::command]
pub async fn request_device_code(
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

/// Poll for device authorization completion
#[tauri::command]
pub async fn poll_device_authorization(
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

/// Verify if an auth token is still valid
#[tauri::command]
pub async fn verify_auth_token(
    state_manager: State<'_, AppStateManager>,
    access_token: String,
) -> Result<bool, String> {
    state_manager.debug_logger.debug("Verifying auth token".to_string());
    match state_manager.api_client.verify_token(&access_token).await {
        Ok(valid) => {
            if valid {
                state_manager.debug_logger.info("Auth token verified successfully".to_string());
            } else {
                state_manager.debug_logger.warn("Auth token is invalid".to_string());
            }
            Ok(valid)
        }
        Err(e) => {
            state_manager.debug_logger.error(format!("Failed to verify auth token: {}", e));
            Err(e)
        }
    }
}
