//! Upload manager commands for scanning and uploading replays.

use std::sync::Arc;
use tauri::{State, Emitter};
use crate::upload_manager::{UploadManager, UploadManagerState};
use crate::state::AppStateManager;

/// Initialize the upload manager with folder paths and authentication
#[tauri::command]
pub async fn initialize_upload_manager(
    state_manager: State<'_, AppStateManager>,
    replay_folders: Vec<String>,
    base_url: String,
    access_token: String,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Initializing upload manager for {} folder(s)", replay_folders.len()));
    for folder in &replay_folders {
        state_manager.debug_logger.debug(format!("  - {}", folder));
    }

    let paths: Vec<std::path::PathBuf> = replay_folders.iter()
        .map(std::path::PathBuf::from)
        .collect();

    match UploadManager::new(
        paths,
        base_url.clone(),
        access_token,
        Arc::clone(&state_manager.debug_logger),
    ) {
        Ok(manager) => {
            let mut upload_manager = state_manager.upload_manager.lock()
                .map_err(|_| "Upload manager mutex poisoned")?;
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

/// Get the current upload manager state
#[tauri::command]
pub async fn get_upload_state(
    state_manager: State<'_, AppStateManager>,
) -> Result<UploadManagerState, String> {
    state_manager.debug_logger.debug("Getting upload manager state".to_string());
    let upload_manager = state_manager.upload_manager.lock()
        .map_err(|_| "Upload manager mutex poisoned")?;

    match upload_manager.as_ref() {
        Some(manager) => {
            let state = manager.get_state();
            state_manager.debug_logger.debug(format!("Upload state - watching: {}, uploaded: {}, pending: {}",
                state.is_watching, state.total_uploaded, state.pending_count));
            Ok(state)
        }
        None => {
            state_manager.debug_logger.error("Upload manager not initialized".to_string());
            Err("Upload manager not initialized".to_string())
        }
    }
}

/// Scan for and upload replay files
#[tauri::command]
pub async fn scan_and_upload_replays(
    app: tauri::AppHandle,
    state_manager: State<'_, AppStateManager>,
    limit: usize,
) -> Result<usize, String> {
    state_manager.debug_logger.info(format!("Starting replay scan and upload (limit: {})", limit));

    // Clone the Arc to avoid holding the lock across await
    let manager = {
        let upload_manager = state_manager.upload_manager.lock()
            .map_err(|_| "Upload manager mutex poisoned")?;
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

/// Start watching replay folders for new files
#[tauri::command]
pub async fn start_file_watcher(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state_manager.debug_logger.info("Starting file watcher for new replays".to_string());

    let manager = {
        let upload_manager = state_manager.upload_manager.lock()
            .map_err(|_| "Upload manager mutex poisoned")?;
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
