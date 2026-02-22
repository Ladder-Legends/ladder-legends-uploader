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

/// Open the folder containing a file
#[tauri::command]
pub async fn open_folder_for_path(path: String) -> Result<(), String> {
    let file_path = std::path::Path::new(&path);

    // Validate that the path is within the app's config/data directory,
    // preventing arbitrary path traversal via this Tauri command.
    let app_data_dir = crate::config_utils::get_config_dir()?;

    // Canonicalize both paths to resolve symlinks and relative components.
    let canonical_path = file_path.canonicalize()
        .map_err(|e| format!("Invalid path: {}", e))?;
    // If the app data dir doesn't exist yet, fall back to the unresolved form.
    let canonical_data_dir = app_data_dir.canonicalize()
        .unwrap_or(app_data_dir);

    if !canonical_path.starts_with(&canonical_data_dir) {
        return Err("Path must be within app data directory".to_string());
    }

    // Get the parent directory (used on Linux)
    #[allow(unused_variables)]
    let folder = file_path.parent()
        .ok_or_else(|| "Could not determine parent folder".to_string())?;

    // Open the folder in the system file explorer
    #[cfg(target_os = "windows")]
    {
        // On Windows, use explorer with /select to highlight the file
        std::process::Command::new("explorer")
            .args(["/select,", &path])
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        // On macOS, use open -R to reveal in Finder
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        // On Linux, just open the folder
        std::process::Command::new("xdg-open")
            .arg(folder)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }

    Ok(())
}
