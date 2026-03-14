//! SC2 replay folder detection commands.

use tauri::State;
use crate::sc2_detector;
use crate::state::AppStateManager;
use super::folders::save_folder_paths;

/// Detect all StarCraft 2 replay folders on the system.
///
/// Supports DEV_REPLAY_FOLDER env var for development without SC2 installed.
/// Set DEV_REPLAY_FOLDER to a directory containing .SC2Replay files.
#[tauri::command]
pub async fn detect_replay_folders(state_manager: State<'_, AppStateManager>) -> Result<Vec<String>, String> {
    state_manager.debug_logger.info("Starting SC2 folder detection".to_string());

    // Dev mode: use DEV_REPLAY_FOLDER env var if set
    if let Ok(dev_folder) = std::env::var("DEV_REPLAY_FOLDER") {
        let dev_path = std::path::PathBuf::from(&dev_folder);
        if dev_path.exists() {
            state_manager.debug_logger.info(format!("DEV MODE: Using dev replay folder: {}", dev_folder));
            let paths = vec![dev_folder];
            if let Err(e) = save_folder_paths(state_manager.clone(), paths.clone()).await {
                state_manager.debug_logger.warn(format!("Failed to save folder paths: {}", e));
            }
            return Ok(paths);
        } else {
            state_manager.debug_logger.warn(format!("DEV MODE: DEV_REPLAY_FOLDER does not exist: {}", dev_folder));
        }
    }

    let folders = sc2_detector::detect_all_sc2_folders(Some(state_manager.debug_logger.clone()));

    if folders.is_empty() {
        state_manager.debug_logger.warn("Could not find any SC2 folders".to_string());
        return Err("Could not find SC2 replay folders".to_string());
    }

    let paths: Vec<String> = folders.iter()
        .map(|f| f.path.to_string_lossy().to_string())
        .collect();

    state_manager.debug_logger.info(format!("Found {} SC2 folder(s)", paths.len()));
    for path in &paths {
        state_manager.debug_logger.debug(format!("  - {}", path));
    }

    // Save all folders to config
    if let Err(e) = save_folder_paths(state_manager.clone(), paths.clone()).await {
        state_manager.debug_logger.warn(format!("Failed to save folder paths: {}", e));
    }
    Ok(paths)
}
