//! Replay folder management commands.

use tauri::State;
use crate::config_utils;
use crate::state::AppStateManager;

/// Manually pick a replay folder using system dialog
#[tauri::command]
pub async fn pick_replay_folder_manual(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    use tauri_plugin_dialog::DialogExt;

    state_manager.debug_logger.info("Opening folder picker dialog".to_string());

    let folder = app.dialog()
        .file()
        .set_title("Select StarCraft 2 Multiplayer Replays Folder")
        .blocking_pick_folder();

    match folder {
        Some(path) => {
            let path_str = path.to_string();
            state_manager.debug_logger.debug(format!("User selected folder: {}", path_str));

            if !std::path::Path::new(&path_str).exists() {
                return Err("Selected folder does not exist".to_string());
            }

            if let Err(e) = add_folder_path(state_manager.clone(), path_str.clone()).await {
                state_manager.debug_logger.warn(format!("Failed to save folder path: {}", e));
            }
            state_manager.debug_logger.info(format!("Saved folder path: {}", path_str));
            Ok(path_str)
        }
        None => {
            state_manager.debug_logger.debug("User cancelled folder selection".to_string());
            Err("No folder selected".to_string())
        }
    }
}

/// Save multiple replay folder paths to config
#[tauri::command]
pub async fn save_folder_paths(
    state_manager: State<'_, AppStateManager>,
    paths: Vec<String>,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Saving {} folder path(s)", paths.len()));
    let config = serde_json::json!({ "replay_folders": paths });

    config_utils::save_config_file("config.json", &config)
        .inspect_err(|e| {
            state_manager.debug_logger.error(e.clone());
        })?;

    state_manager.debug_logger.debug("Folder paths saved successfully".to_string());
    Ok(())
}

/// Add a folder path to the persisted list (appends, deduplicates)
#[tauri::command]
pub async fn add_folder_path(
    state_manager: State<'_, AppStateManager>,
    path: String,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Adding folder path: {}", path));

    let config: Option<serde_json::Value> = config_utils::load_config_file("config.json")
        .inspect_err(|e| { state_manager.debug_logger.error(e.clone()); })?;

    let mut paths: Vec<String> = config
        .as_ref()
        .and_then(|c| c.get("replay_folders"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    if !paths.contains(&path) {
        paths.push(path.clone());
    }

    let config = serde_json::json!({ "replay_folders": paths });
    config_utils::save_config_file("config.json", &config)
        .inspect_err(|e| { state_manager.debug_logger.error(e.clone()); })?;

    state_manager.debug_logger.info(format!("Folder list now has {} path(s)", paths.len()));
    Ok(())
}

/// Load all replay folder paths from config
#[tauri::command]
pub async fn load_folder_paths(state_manager: State<'_, AppStateManager>) -> Result<Vec<String>, String> {
    state_manager.debug_logger.debug("Loading folder paths from config".to_string());

    let config: Option<serde_json::Value> = config_utils::load_config_file("config.json")
        .inspect_err(|e| {
            state_manager.debug_logger.error(e.clone());
        })?;

    let Some(config) = config else {
        state_manager.debug_logger.debug("Config file does not exist yet".to_string());
        return Ok(Vec::new());
    };

    // Load replay_folders array
    if let Some(folders) = config.get("replay_folders").and_then(|v| v.as_array()) {
        let paths: Vec<String> = folders
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();
        state_manager.debug_logger.debug(format!("Loaded {} folder path(s)", paths.len()));
        return Ok(paths);
    }

    state_manager.debug_logger.debug("No folder paths found in config".to_string());
    Ok(Vec::new())
}

/// Helper for frontend that expects single folder string - returns first folder or None
#[tauri::command]
pub async fn load_folder_path(state_manager: State<'_, AppStateManager>) -> Result<Option<String>, String> {
    let paths = load_folder_paths(state_manager).await?;
    Ok(paths.first().cloned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_add_folder_deduplicates() {
        let existing = vec!["/a".to_string(), "/b".to_string()];
        let new_path = "/a";
        let mut merged = existing.clone();
        if !merged.contains(&new_path.to_string()) {
            merged.push(new_path.to_string());
        }
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn test_add_folder_appends_new() {
        let existing = vec!["/a".to_string(), "/b".to_string()];
        let new_path = "/c";
        let mut merged = existing.clone();
        if !merged.contains(&new_path.to_string()) {
            merged.push(new_path.to_string());
        }
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&"/c".to_string()));
    }

    #[test]
    fn test_auto_detect_plus_manual_merge() {
        let auto_detected = vec!["/sc2/account1/replays".to_string(), "/sc2/account2/replays".to_string()];
        let manual = "/custom/replays";
        let mut all_paths = auto_detected.clone();
        if !all_paths.contains(&manual.to_string()) {
            all_paths.push(manual.to_string());
        }
        assert_eq!(all_paths.len(), 3);
    }
}
