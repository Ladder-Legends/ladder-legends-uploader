//! Browser-related commands.

use tauri::State;
use crate::state::AppStateManager;

/// Open a URL in the system's default browser
#[tauri::command]
pub async fn open_browser(
    state_manager: State<'_, AppStateManager>,
    url: String,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Opening browser to: {}", url));
    match open::that(&url) {
        Ok(_) => {
            state_manager.debug_logger.debug("Browser opened successfully".to_string());
            Ok(())
        }
        Err(e) => {
            let error_msg = format!("Failed to open browser: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            Err(error_msg)
        }
    }
}
