//! Upload execution service
//!
//! Handles the actual upload of prepared replays with progress tracking,
//! grouping by game type/player, and event emission.

use crate::replay_tracker::{ReplayTracker, ReplayFileInfo, TrackedReplay};
use crate::replay_uploader::ReplayUploader;
use crate::debug_logger::DebugLogger;
use crate::upload_manager::{group_replays_by_type_and_player, UploadStatus, UploadManagerState};
use crate::services::replay_scanner::PreparedReplay;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;

/// Result of executing uploads
#[derive(Debug)]
pub struct UploadResult {
    /// Number of replays successfully uploaded
    pub uploaded_count: usize,
}

/// Service for executing replay uploads
pub struct UploadExecutor {
    uploader: Arc<ReplayUploader>,
    tracker: Arc<Mutex<ReplayTracker>>,
    state: Arc<Mutex<UploadManagerState>>,
    logger: Arc<DebugLogger>,
}

impl UploadExecutor {
    pub fn new(
        uploader: Arc<ReplayUploader>,
        tracker: Arc<Mutex<ReplayTracker>>,
        state: Arc<Mutex<UploadManagerState>>,
        logger: Arc<DebugLogger>,
    ) -> Self {
        Self {
            uploader,
            tracker,
            state,
            logger,
        }
    }

    /// Execute uploads for prepared replays
    ///
    /// Groups replays by (game_type, player_name) and uploads in batches,
    /// emitting progress events along the way.
    pub async fn execute(
        &self,
        prepared_replays: Vec<PreparedReplay>,
        app: &tauri::AppHandle,
    ) -> Result<UploadResult, String> {
        if prepared_replays.is_empty() {
            return Ok(UploadResult {
                uploaded_count: 0,
            });
        }

        // Build hash list and replay maps for grouping
        let hashes: Vec<String> = prepared_replays.iter().map(|r| r.hash.clone()).collect();

        // Map for group_replays_by_type_and_player (needs tuple format)
        let tuple_map: HashMap<String, (ReplayFileInfo, String, String)> = prepared_replays
            .iter()
            .map(|r| (
                r.hash.clone(),
                (r.file_info.clone(), r.game_type.clone(), r.player_name.clone()),
            ))
            .collect();

        // Map for quick lookup during upload
        let replay_map: HashMap<String, &PreparedReplay> = prepared_replays
            .iter()
            .map(|r| (r.hash.clone(), r))
            .collect();

        // Group by (game_type, player_name)
        let groups = group_replays_by_type_and_player(&hashes, &tuple_map);
        let total_count = prepared_replays.len();

        self.logger.info(format!(
            "Uploading {} replay(s) in {} group(s)...",
            total_count,
            groups.len()
        ));

        // Update pending count
        self.update_pending_count(total_count);

        let mut uploaded_count = 0;
        let mut global_index = 0;

        // Upload each group
        for group in groups {
            self.logger.info(format!(
                "Uploading {} {} replays for {}...",
                group.hashes.len(),
                group.game_type,
                group.player_name
            ));

            // Emit batch start
            if let Err(e) = app.emit("upload-batch-start", serde_json::json!({
                "game_type": group.game_type,
                "player_name": group.player_name,
                "count": group.hashes.len()
            })) {
                self.logger.warn(format!("Failed to emit upload-batch-start: {}", e));
            }

            for hash in &group.hashes {
                let prepared = match replay_map.get(hash) {
                    Some(p) => p,
                    None => {
                        self.logger.warn(format!("Hash {} not found in replay map, skipping", hash));
                        continue;
                    }
                };

                global_index += 1;

                match self.upload_single_replay(
                    prepared,
                    hash,
                    global_index,
                    total_count,
                    &group.game_type,
                    &group.player_name,
                    app,
                ).await {
                    Ok(()) => {
                        uploaded_count += 1;
                    }
                    Err(e) => {
                        // Return error on first failure (current behavior)
                        // Could be changed to continue on failure in the future
                        return Err(e);
                    }
                }
            }

            // Emit batch complete
            if let Err(e) = app.emit("upload-batch-complete", serde_json::json!({
                "game_type": group.game_type,
                "player_name": group.player_name,
                "count": group.hashes.len()
            })) {
                self.logger.warn(format!("Failed to emit upload-batch-complete: {}", e));
            }
        }

        // Clear current upload status
        self.clear_current_upload();

        self.logger.info(format!(
            "Upload execution complete: {} uploaded",
            uploaded_count
        ));

        Ok(UploadResult {
            uploaded_count,
        })
    }

    /// Update pending count in state
    fn update_pending_count(&self, count: usize) {
        if let Ok(mut state) = self.state.lock() {
            state.pending_count = count;
        } else {
            self.logger.error("Failed to lock state for pending count update".to_string());
        }
    }

    /// Clear current upload status
    fn clear_current_upload(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.current_upload = None;
        } else {
            self.logger.error("Failed to lock state for clearing current upload".to_string());
        }
    }

    /// Upload a single replay with progress tracking
    #[allow(clippy::too_many_arguments)]
    async fn upload_single_replay(
        &self,
        prepared: &PreparedReplay,
        hash: &str,
        index: usize,
        total: usize,
        game_type: &str,
        player_name: &str,
        app: &tauri::AppHandle,
    ) -> Result<(), String> {
        self.logger.info(format!(
            "[{}/{}] Uploading {} ({} for {})...",
            index, total, prepared.file_info.filename, game_type, player_name
        ));

        // Update status to uploading
        self.set_upload_status(UploadStatus::Uploading {
            filename: prepared.file_info.filename.clone(),
        });

        // Emit progress event
        if let Err(e) = app.emit("upload-progress", serde_json::json!({
            "current": index,
            "total": total,
            "filename": prepared.file_info.filename,
            "game_type": game_type,
            "player_name": player_name
        })) {
            self.logger.warn(format!("Failed to emit upload-progress: {}", e));
        }

        // Extract region from path
        let region = extract_region_from_path(&prepared.file_info.path);

        // Perform upload
        match self.uploader.upload_replay(
            &prepared.file_info.path,
            Some(player_name),
            None, // target_build_id
            Some(game_type),
            region.as_deref(),
        ).await {
            Ok(_) => {
                self.handle_upload_success(prepared, hash)?;
                self.logger.info(format!("Successfully uploaded {}", prepared.file_info.filename));
                Ok(())
            }
            Err(e) => {
                // Check if this is a 409 Conflict (duplicate) - treat as success
                // This can happen in race conditions where the same replay is uploaded twice
                if e.contains("409") || e.contains("REPLAY_DUPLICATE") || e.contains("already been uploaded") {
                    self.logger.info(format!(
                        "Replay {} already exists on server (treating as success)",
                        prepared.file_info.filename
                    ));
                    // Still mark as success in local tracker to prevent re-upload attempts
                    self.handle_upload_success(prepared, hash)?;
                    Ok(())
                } else {
                    self.handle_upload_failure(&prepared.file_info.filename, &e, app);
                    Err(format!("Failed to upload {}: {}", prepared.file_info.filename, e))
                }
            }
        }
    }

    /// Handle successful upload - update tracker and state
    fn handle_upload_success(&self, prepared: &PreparedReplay, hash: &str) -> Result<(), String> {
        let tracked_replay = TrackedReplay {
            hash: hash.to_string(),
            filename: prepared.file_info.filename.clone(),
            filesize: prepared.file_info.filesize,
            uploaded_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0), // Fallback to 0 if clock is weird
            filepath: prepared.file_info.path.to_string_lossy().to_string(),
        };

        // Add to tracker and save
        {
            let mut tracker = self.tracker.lock()
                .map_err(|_| "Failed to lock tracker")?;
            tracker.add_replay(tracked_replay);
            tracker.save()?;
        }

        // Update state
        {
            let tracker = self.tracker.lock()
                .map_err(|_| "Failed to lock tracker for state update")?;
            let mut state = self.state.lock()
                .map_err(|_| "Failed to lock state")?;
            state.total_uploaded = tracker.total_uploaded;
            state.current_upload = Some(UploadStatus::Completed {
                filename: prepared.file_info.filename.clone(),
            });
            state.pending_count = state.pending_count.saturating_sub(1);
        }

        Ok(())
    }

    /// Handle failed upload - update state and emit error event
    fn handle_upload_failure(&self, filename: &str, error: &str, app: &tauri::AppHandle) {
        if let Ok(mut state) = self.state.lock() {
            state.current_upload = Some(UploadStatus::Failed {
                filename: filename.to_string(),
                error: error.to_string(),
            });
            state.pending_count = state.pending_count.saturating_sub(1);
        } else {
            self.logger.error("Failed to lock state for failure update".to_string());
        }

        // Emit error event so frontend can display it
        if let Err(e) = app.emit("upload-error", serde_json::json!({
            "filename": filename,
            "error": error
        })) {
            self.logger.warn(format!("Failed to emit upload-error: {}", e));
        }
    }

    /// Set current upload status
    fn set_upload_status(&self, status: UploadStatus) {
        if let Ok(mut state) = self.state.lock() {
            state.current_upload = Some(status);
        } else {
            self.logger.error("Failed to lock state for status update".to_string());
        }
    }
}

/// Extract region from replay path by looking at folder structure
/// Looks for patterns like "1-S2-1-802768" in the path
/// Returns: "NA", "EU", "KR", "CN", or None
fn extract_region_from_path(path: &std::path::Path) -> Option<String> {
    for component in path.components() {
        if let std::path::Component::Normal(folder_name) = component {
            if let Some(name) = folder_name.to_str() {
                if name.starts_with("1-S2-") || name.starts_with("1-") {
                    return Some("NA".to_string());
                } else if name.starts_with("2-S2-") || name.starts_with("2-") {
                    return Some("EU".to_string());
                } else if name.starts_with("3-S2-") || name.starts_with("3-") {
                    return Some("KR".to_string());
                } else if name.starts_with("5-S2-") || name.starts_with("5-") {
                    return Some("CN".to_string());
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extract_region_na() {
        let path = PathBuf::from("/Users/test/StarCraft II/Accounts/123/1-S2-1-802768/Replays/test.SC2Replay");
        assert_eq!(extract_region_from_path(&path), Some("NA".to_string()));

        let path2 = PathBuf::from("/Users/test/1-S2-2-123456/replay.SC2Replay");
        assert_eq!(extract_region_from_path(&path2), Some("NA".to_string()));
    }

    #[test]
    fn test_extract_region_eu() {
        let path = PathBuf::from("/Users/test/StarCraft II/Accounts/123/2-S2-1-802768/Replays/test.SC2Replay");
        assert_eq!(extract_region_from_path(&path), Some("EU".to_string()));
    }

    #[test]
    fn test_extract_region_kr() {
        let path = PathBuf::from("/Users/test/StarCraft II/Accounts/123/3-S2-1-802768/Replays/test.SC2Replay");
        assert_eq!(extract_region_from_path(&path), Some("KR".to_string()));
    }

    #[test]
    fn test_extract_region_cn() {
        let path = PathBuf::from("/Users/test/StarCraft II/Accounts/123/5-S2-1-802768/Replays/test.SC2Replay");
        assert_eq!(extract_region_from_path(&path), Some("CN".to_string()));
    }

    #[test]
    fn test_extract_region_none() {
        let path = PathBuf::from("/Users/test/Documents/replays/test.SC2Replay");
        assert_eq!(extract_region_from_path(&path), None);
    }

    #[test]
    fn test_upload_result() {
        let result = UploadResult {
            uploaded_count: 5,
        };
        assert_eq!(result.uploaded_count, 5);
    }
}
