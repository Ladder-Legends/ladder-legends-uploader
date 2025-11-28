//! Version and update management commands.

use std::sync::Arc;
use tauri::State;
use tauri_plugin_updater::UpdaterExt;
use crate::state::AppStateManager;

/// Get the current app version
#[tauri::command]
pub async fn get_version(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
) -> Result<String, String> {
    state_manager.debug_logger.debug("Getting app version".to_string());
    let version = app.package_info()
        .version
        .to_string()
        .parse()
        .map_err(|e| {
            let error_msg = format!("Failed to get version: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;
    state_manager.debug_logger.debug(format!("App version: {}", version));
    Ok(version)
}

/// Check for app updates
#[tauri::command]
pub async fn check_for_updates(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
) -> Result<bool, String> {
    state_manager.debug_logger.info("Checking for app updates".to_string());

    let updater = app.updater_builder().build()
        .map_err(|e| {
            let error_msg = format!("Failed to build updater: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    match updater.check().await {
        Ok(Some(update)) => {
            state_manager.debug_logger.info(format!("Update available: {}", update.version));
            state_manager.debug_logger.debug(format!("Update details - current_version: {}, date: {:?}",
                update.current_version, update.date));
            Ok(true)
        }
        Ok(None) => {
            state_manager.debug_logger.info("App is up to date, no updates available".to_string());
            Ok(false)
        }
        Err(e) => {
            let error_msg = format!("Failed to check for updates: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            Err(error_msg)
        }
    }
}

/// Install available update
#[tauri::command]
pub async fn install_update(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state_manager.debug_logger.info("Starting update installation process".to_string());

    let updater = app.updater_builder().build()
        .map_err(|e| {
            let error_msg = format!("Failed to build updater: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

    state_manager.debug_logger.debug("Checking for available updates".to_string());
    match updater.check().await {
        Ok(Some(update)) => {
            state_manager.debug_logger.info(format!("Downloading and installing update: {}", update.version));

            // Clone logger for progress callback
            let logger_for_progress = Arc::clone(&state_manager.debug_logger);
            let logger_for_complete = Arc::clone(&state_manager.debug_logger);

            update.download_and_install(
                move |chunk_length, content_length| {
                    if let Some(total) = content_length {
                        logger_for_progress.debug(format!("Download progress: {}/{} bytes", chunk_length, total));
                    } else {
                        logger_for_progress.debug(format!("Downloaded {} bytes", chunk_length));
                    }
                },
                move || {
                    logger_for_complete.debug("Download complete, installing...".to_string());
                }
            )
            .await
            .map_err(|e| {
                let error_msg = format!("Failed to install update: {}", e);
                state_manager.debug_logger.error(error_msg.clone());
                error_msg
            })?;

            state_manager.debug_logger.info("Update installed successfully, restarting app...".to_string());

            // Explicitly restart the app to apply the update
            // Tauri v2 updater may not auto-restart depending on platform
            // Note: restart() never returns, so no code after this executes
            app.restart()
        }
        Ok(None) => {
            let error_msg = "No update available to install".to_string();
            state_manager.debug_logger.warn(error_msg.clone());
            Err(error_msg)
        }
        Err(e) => {
            let error_msg = format!("Failed to check for updates: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            Err(error_msg)
        }
    }
}
