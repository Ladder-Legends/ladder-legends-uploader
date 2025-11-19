use crate::replay_tracker::{ReplayTracker, TrackedReplay, ReplayFileInfo, scan_replay_folder};
use crate::replay_uploader::{ReplayUploader, HashInfo};
use crate::replay_parser;
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use notify::{Watcher, RecursiveMode, Event};
use tauri::Emitter;

/// Upload status for a single replay
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum UploadStatus {
    Pending { filename: String },
    Uploading { filename: String },
    Completed { filename: String },
    Failed { filename: String, error: String },
}

/// Current state of the upload manager
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UploadManagerState {
    pub total_uploaded: usize,
    pub current_upload: Option<UploadStatus>,
    pub pending_count: usize,
    pub is_watching: bool,
}

/// Manages replay uploads and file watching
pub struct UploadManager {
    replay_folder: PathBuf,
    tracker: Arc<Mutex<ReplayTracker>>,
    uploader: Arc<ReplayUploader>,
    state: Arc<Mutex<UploadManagerState>>,
}

impl UploadManager {
    /// Create a new upload manager
    pub fn new(
        replay_folder: PathBuf,
        base_url: String,
        access_token: String,
    ) -> Result<Self, String> {
        let tracker = ReplayTracker::load()?;

        Ok(Self {
            replay_folder,
            tracker: Arc::new(Mutex::new(tracker)),
            uploader: Arc::new(ReplayUploader::new(base_url, access_token)),
            state: Arc::new(Mutex::new(UploadManagerState {
                total_uploaded: 0,
                current_upload: None,
                pending_count: 0,
                is_watching: false,
            })),
        })
    }

    /// Get current state
    pub fn get_state(&self) -> UploadManagerState {
        let state = self.state.lock().unwrap();
        state.clone()
    }

    /// Scan for new replays and upload them (up to limit)
    /// Uses two-layer deduplication: local tracker + server check
    pub async fn scan_and_upload(&self, limit: usize, app: &tauri::AppHandle) -> Result<usize, String> {
        println!("🔍 [UPLOAD] Starting scan and upload (limit: {})", limit);

        // Emit start event
        let _ = app.emit("upload-start", serde_json::json!({
            "limit": limit
        }));

        let tracker = self.tracker.lock().unwrap().clone();

        // Step 1: Scan folder for replays (get more than limit for server check)
        let all_replays = scan_replay_folder(&self.replay_folder)?;
        let recent_replays: Vec<_> = all_replays.into_iter().take(limit * 2).collect();

        println!("📁 [UPLOAD] Found {} replays in folder", recent_replays.len());

        if recent_replays.is_empty() {
            println!("ℹ️  [UPLOAD] No replays found in folder");
            return Ok(0);
        }

        // Step 2: Calculate hashes for all replays and filter by local tracker
        let mut hash_infos = Vec::new();
        // Store both replay_info and game_type string together
        let mut replay_map: HashMap<String, (ReplayFileInfo, String)> = HashMap::new();
        let mut non_1v1_count = 0;

        for replay_info in recent_replays {
            // Extract game type and check if should upload
            let game_type = match replay_parser::get_game_type(&replay_info.path) {
                Ok(gtype) => gtype,
                Err(e) => {
                    println!("⚠️  [UPLOAD] Could not parse {} ({}), skipping", replay_info.filename, e);
                    continue;
                }
            };

            // Check if this game type should be uploaded
            if !game_type.should_upload() {
                non_1v1_count += 1;
                println!("⏭️  [UPLOAD] Skipping {} (game type: {})", replay_info.filename, game_type.as_str());
                continue;
            }

            // Quick check: skip if we know we uploaded it
            if tracker.exists_by_metadata(&replay_info.filename, replay_info.filesize) {
                println!("⏭️  [UPLOAD] Skipping {} (in local tracker by metadata)", replay_info.filename);
                continue;
            }

            // Calculate hash
            let hash = ReplayTracker::calculate_hash(&replay_info.path)?;

            // Check if hash is in local tracker
            if tracker.is_uploaded(&hash) {
                println!("⏭️  [UPLOAD] Skipping {} (in local tracker by hash)", replay_info.filename);
                continue;
            }

            hash_infos.push(HashInfo {
                hash: hash.clone(),
                filename: replay_info.filename.clone(),
                filesize: replay_info.filesize,
            });

            // Store both replay info and game type string for upload
            replay_map.insert(hash, (replay_info, game_type.as_str().to_string()));
        }

        if non_1v1_count > 0 {
            println!("🎮 [UPLOAD] Filtered out {} non-1v1 replays", non_1v1_count);
        }

        println!("🔍 [UPLOAD] {} replays not in local tracker", hash_infos.len());

        if hash_infos.is_empty() {
            println!("✅ [UPLOAD] All replays already uploaded (per local tracker)");
            return Ok(0);
        }

        // Step 3: Check with server which hashes are new
        println!("🌐 [UPLOAD] Checking {} hashes with server...", hash_infos.len());
        let _ = app.emit("upload-checking", serde_json::json!({
            "count": hash_infos.len()
        }));

        let check_result = self.uploader.check_hashes(hash_infos).await?;

        println!(
            "✅ [UPLOAD] Server check complete: {} new, {} existing",
            check_result.new_hashes.len(),
            check_result.existing_count
        );

        let _ = app.emit("upload-check-complete", serde_json::json!({
            "new_count": check_result.new_hashes.len(),
            "existing_count": check_result.existing_count
        }));

        // Step 4: Upload only the new replays (up to limit)
        let to_upload: Vec<_> = check_result.new_hashes
            .into_iter()
            .take(limit)
            .collect();

        println!("⬆️  [UPLOAD] Uploading {} replay(s)...", to_upload.len());

        {
            let mut state = self.state.lock().unwrap();
            state.pending_count = to_upload.len();
        }

        let mut uploaded_count = 0;

        for (index, hash) in to_upload.iter().enumerate() {
            let (replay_info, game_type_str) = match replay_map.get(hash) {
                Some((info, gtype)) => (info, gtype),
                None => {
                    println!("⚠️  [UPLOAD] Hash {} not found in replay map, skipping", hash);
                    continue;
                }
            };

            println!("⬆️  [UPLOAD] [{}/{}] Uploading {} ({})...", index + 1, to_upload.len(), replay_info.filename, game_type_str);

            // Update status to uploading
            {
                let mut state = self.state.lock().unwrap();
                state.current_upload = Some(UploadStatus::Uploading {
                    filename: replay_info.filename.clone(),
                });
            }

            // Emit progress event
            let _ = app.emit("upload-progress", serde_json::json!({
                "current": index + 1,
                "total": to_upload.len(),
                "filename": replay_info.filename
            }));

            // Perform upload with game type
            match self.uploader.upload_replay(&replay_info.path, None, None, Some(game_type_str.as_str())).await {
                Ok(_) => {
                    let tracked_replay = TrackedReplay {
                        hash: hash.clone(),
                        filename: replay_info.filename.clone(),
                        filesize: replay_info.filesize,
                        uploaded_at: SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap()
                            .as_secs(),
                        filepath: replay_info.path.to_string_lossy().to_string(),
                    };

                    // Add to tracker and save
                    {
                        let mut tracker = self.tracker.lock().unwrap();
                        tracker.add_replay(tracked_replay);
                        tracker.save()?;
                    }

                    // Update state
                    {
                        let mut state = self.state.lock().unwrap();
                        let tracker = self.tracker.lock().unwrap();
                        state.total_uploaded = tracker.total_uploaded;
                        state.current_upload = Some(UploadStatus::Completed {
                            filename: replay_info.filename.clone(),
                        });
                        state.pending_count = state.pending_count.saturating_sub(1);
                    }

                    uploaded_count += 1;
                    println!("✅ [UPLOAD] Successfully uploaded {}", replay_info.filename);
                }
                Err(e) => {
                    println!("❌ [UPLOAD] Failed to upload {}: {}", replay_info.filename, e);

                    let mut state = self.state.lock().unwrap();
                    state.current_upload = Some(UploadStatus::Failed {
                        filename: replay_info.filename.clone(),
                        error: e.clone(),
                    });
                    state.pending_count = state.pending_count.saturating_sub(1);

                    return Err(format!("Failed to upload {}: {}", replay_info.filename, e));
                }
            }
        }

        // Clear current upload status
        {
            let mut state = self.state.lock().unwrap();
            state.current_upload = None;
        }

        println!("🎉 [UPLOAD] Scan and upload complete: {} replays uploaded", uploaded_count);

        // Emit completion event
        let _ = app.emit("upload-complete", serde_json::json!({
            "count": uploaded_count
        }));

        Ok(uploaded_count)
    }

    /// Start watching the replay folder for new files
    pub async fn start_watching<F>(
        &self,
        on_new_file: F,
    ) -> Result<(), String>
    where
        F: Fn(PathBuf) + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel(100);

        let folder = self.replay_folder.clone();

        // Create file watcher
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                // Only care about create and modify events
                if matches!(event.kind, notify::EventKind::Create(_) | notify::EventKind::Modify(_)) {
                    for path in event.paths {
                        if path.extension().map_or(false, |ext| ext == "SC2Replay") {
                            let _ = tx.blocking_send(path);
                        }
                    }
                }
            }
        })
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        watcher.watch(&folder, RecursiveMode::NonRecursive)
            .map_err(|e| format!("Failed to watch folder: {}", e))?;

        // Update state
        {
            let mut state = self.state.lock().unwrap();
            state.is_watching = true;
        }

        // Spawn task to handle events
        tokio::spawn(async move {
            while let Some(path) = rx.recv().await {
                on_new_file(path);
            }

            // Keep watcher alive
            drop(watcher);
        });

        Ok(())
    }

    /// Stop watching (not implemented - watcher lives for app lifetime)
    #[allow(dead_code)]
    pub fn stop_watching(&self) {
        let mut state = self.state.lock().unwrap();
        state.is_watching = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;
    use std::path::Path;

    fn create_test_replay(dir: &Path, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn test_upload_status_serialize() {
        let pending = UploadStatus::Pending {
            filename: "test.SC2Replay".to_string(),
        };
        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains("pending"));
        assert!(json.contains("test.SC2Replay"));

        let uploading = UploadStatus::Uploading {
            filename: "test.SC2Replay".to_string(),
        };
        let json = serde_json::to_string(&uploading).unwrap();
        assert!(json.contains("uploading"));

        let completed = UploadStatus::Completed {
            filename: "test.SC2Replay".to_string(),
        };
        let json = serde_json::to_string(&completed).unwrap();
        assert!(json.contains("completed"));

        let failed = UploadStatus::Failed {
            filename: "test.SC2Replay".to_string(),
            error: "Network error".to_string(),
        };
        let json = serde_json::to_string(&failed).unwrap();
        assert!(json.contains("failed"));
        assert!(json.contains("Network error"));
    }

    #[test]
    fn test_upload_manager_state_serialize() {
        let state = UploadManagerState {
            total_uploaded: 5,
            current_upload: Some(UploadStatus::Uploading {
                filename: "test.SC2Replay".to_string(),
            }),
            pending_count: 3,
            is_watching: true,
        };

        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains("\"total_uploaded\":5"));
        assert!(json.contains("\"pending_count\":3"));
        assert!(json.contains("\"is_watching\":true"));
    }

    #[test]
    fn test_upload_manager_state_deserialize() {
        let json = r#"{
            "total_uploaded": 10,
            "current_upload": {
                "status": "completed",
                "filename": "test.SC2Replay"
            },
            "pending_count": 0,
            "is_watching": false
        }"#;

        let state: UploadManagerState = serde_json::from_str(json).unwrap();
        assert_eq!(state.total_uploaded, 10);
        assert_eq!(state.pending_count, 0);
        assert!(!state.is_watching);
        assert!(matches!(state.current_upload, Some(UploadStatus::Completed { .. })));
    }

    // Mock tests without actual API calls
    #[tokio::test]
    async fn test_upload_manager_creation() {
        let temp_dir = TempDir::new().unwrap();

        let manager = UploadManager::new(
            temp_dir.path().to_path_buf(),
            "https://example.com".to_string(),
            "test-token".to_string(),
        );

        assert!(manager.is_ok());
        let manager = manager.unwrap();

        let state = manager.get_state();
        assert_eq!(state.total_uploaded, 0);
        assert_eq!(state.pending_count, 0);
        assert!(!state.is_watching);
    }

    #[tokio::test]
    async fn test_get_state() {
        let temp_dir = TempDir::new().unwrap();

        let manager = UploadManager::new(
            temp_dir.path().to_path_buf(),
            "https://example.com".to_string(),
            "test-token".to_string(),
        ).unwrap();

        let state = manager.get_state();
        assert_eq!(state.total_uploaded, 0);
        assert!(state.current_upload.is_none());
    }

    // Integration test with file watcher
    #[tokio::test]
    #[ignore] // Requires filesystem events
    async fn test_file_watcher_integration() {
        let temp_dir = TempDir::new().unwrap();

        let manager = UploadManager::new(
            temp_dir.path().to_path_buf(),
            "https://example.com".to_string(),
            "test-token".to_string(),
        ).unwrap();

        let detected_files = Arc::new(Mutex::new(Vec::new()));
        let detected_clone = detected_files.clone();

        manager.start_watching(move |path| {
            let mut files = detected_clone.lock().unwrap();
            files.push(path);
        }).await.unwrap();

        // Give watcher time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Create a new replay file
        create_test_replay(temp_dir.path(), "new.SC2Replay", b"test content");

        // Wait for event
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        let files = detected_files.lock().unwrap();
        assert!(!files.is_empty(), "File watcher should detect new replay");
    }
}
