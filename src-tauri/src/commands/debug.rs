//! Debug and diagnostic commands.

use tauri::State;
use crate::state::AppStateManager;
use super::folders::load_folder_path;
use super::tokens::load_auth_tokens;

/// Export debug log to file
#[tauri::command]
pub async fn export_debug_log(
    state_manager: State<'_, AppStateManager>,
) -> Result<String, String> {
    // Gather current state information
    let replay_folder = load_folder_path(state_manager.clone()).await.ok().flatten();

    // Try to get Discord user ID from saved auth tokens
    let discord_user_id = load_auth_tokens(state_manager.clone())
        .await
        .ok()
        .flatten()
        .and_then(|tokens| tokens.user)
        .map(|user| user.username);

    // Try to get number of replays found
    let replays_found = if let Some(ref folder) = replay_folder {
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
pub async fn get_debug_stats(
    state_manager: State<'_, AppStateManager>,
) -> Result<serde_json::Value, String> {
    Ok(serde_json::json!({
        "error_count": state_manager.debug_logger.get_error_count(),
    }))
}
