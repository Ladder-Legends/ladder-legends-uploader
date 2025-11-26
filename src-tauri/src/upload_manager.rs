use crate::replay_tracker::{ReplayTracker, ReplayFileInfo};
use crate::replay_uploader::ReplayUploader;
use crate::debug_logger::DebugLogger;
use crate::services::{ReplayScanner, UploadExecutor};
use std::path::{Path, PathBuf};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use notify::{Watcher, RecursiveMode, Event};
use tauri::Emitter;

/// Check if a path is an SC2 replay file (case-insensitive extension check)
/// This is important for Windows where file extensions may have different casing.
#[inline]
pub fn is_sc2_replay(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("SC2Replay"))
        .unwrap_or(false)
}

/// Get the delay in milliseconds to wait before processing a new replay file.
/// Windows needs more time due to antivirus scanning and file locking.
#[inline]
pub const fn get_file_processing_delay_ms() -> u64 {
    #[cfg(target_os = "windows")]
    { 1000 }
    #[cfg(not(target_os = "windows"))]
    { 500 }
}

/// Represents a group of replays with the same game type and player name
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayGroup {
    pub game_type: String,
    pub player_name: String,
    pub hashes: Vec<String>,
}

/// Group replay hashes by (game_type, player_name) for batch uploading
/// Returns groups sorted by game_type then player_name
pub fn group_replays_by_type_and_player(
    hashes: &[String],
    replay_map: &HashMap<String, (ReplayFileInfo, String, String)>,
) -> Vec<ReplayGroup> {
    let mut groups: HashMap<(String, String), Vec<String>> = HashMap::new();

    for hash in hashes {
        if let Some((_, game_type_str, player_name)) = replay_map.get(hash) {
            groups.entry((game_type_str.clone(), player_name.clone()))
                .or_default()
                .push(hash.clone());
        }
    }

    // Sort groups by game_type then player_name for consistent ordering
    let mut sorted_groups: Vec<_> = groups.into_iter()
        .map(|((game_type, player_name), hashes)| ReplayGroup {
            game_type,
            player_name,
            hashes,
        })
        .collect();

    sorted_groups.sort_by(|a, b| {
        match a.game_type.cmp(&b.game_type) {
            std::cmp::Ordering::Equal => a.player_name.cmp(&b.player_name),
            other => other,
        }
    });

    sorted_groups
}

/// Player statistics for user detection
#[derive(Debug, Clone)]
struct PlayerStats {
    name: String,
    frequency: usize,
    co_occurrences: HashMap<String, usize>,
}

/// Detect likely user player names from replay data using frequency and co-occurrence analysis
///
/// Algorithm:
/// 1. Count frequency of each player across all replays
/// 2. Track co-occurrences (how often players appear together)
/// 3. Sort by frequency (descending)
/// 4. Filter out players who frequently co-occur with higher-frequency players
///    - These are likely practice partners/teammates, not the user
/// 5. Return top 1-2 players after filtering
///
/// # Arguments
/// * `replays` - List of (replay_path, players) tuples where players is Vec<(name, is_observer)>
///
/// # Returns
/// * Vec of detected user player names, sorted by confidence (highest first)
pub fn detect_user_player_names(replays: &[(String, Vec<(String, bool)>)]) -> Vec<String> {
    if replays.is_empty() {
        return Vec::new();
    }

    // Step 1: Count frequencies and co-occurrences
    let mut player_stats: HashMap<String, PlayerStats> = HashMap::new();

    for (_replay_path, players) in replays {
        // Get non-observer player names
        let active_players: Vec<String> = players.iter()
            .filter(|(_, is_observer)| !is_observer)
            .map(|(name, _)| name.clone())
            .collect();

        if active_players.is_empty() {
            continue;
        }

        // Update frequencies
        for player in &active_players {
            player_stats.entry(player.clone())
                .or_insert_with(|| PlayerStats {
                    name: player.clone(),
                    frequency: 0,
                    co_occurrences: HashMap::new(),
                })
                .frequency += 1;
        }

        // Update co-occurrences (track who appears with whom)
        for i in 0..active_players.len() {
            for j in 0..active_players.len() {
                if i != j {
                    let player = &active_players[i];
                    let other_player = &active_players[j];

                    player_stats.get_mut(player)
                        .unwrap()
                        .co_occurrences
                        .entry(other_player.clone())
                        .and_modify(|count| *count += 1)
                        .or_insert(1);
                }
            }
        }
    }

    // Step 2: Sort by frequency (descending)
    let mut sorted_players: Vec<PlayerStats> = player_stats.into_values().collect();
    sorted_players.sort_by(|a, b| b.frequency.cmp(&a.frequency));

    if sorted_players.is_empty() {
        return Vec::new();
    }

    // Filter out known AI player names
    const AI_PLAYER_NAMES: &[&str] = &["Computer", "A.I.", "AI", "Bot"];
    sorted_players.retain(|p| !AI_PLAYER_NAMES.iter().any(|ai_name| p.name.eq_ignore_ascii_case(ai_name)));

    // Step 3: Filter out players who frequently co-occur with higher-frequency players
    // Requirements for user candidates:
    // 1. Must appear in more than 1 game (frequency > 1)
    // 2. Must NOT frequently co-occur with any higher-frequency player
    //    (co-occurrence rate > 50% means they're a practice partner/teammate)
    let mut user_candidates = Vec::new();

    for (idx, player) in sorted_players.iter().enumerate() {
        // Requirement 1: Must appear more than once
        if player.frequency <= 1 {
            continue;
        }

        let mut is_user_candidate = true;

        // Requirement 2: Check if this player frequently co-occurs with any higher-frequency player
        for higher_freq_player in &sorted_players[0..idx] {
            if let Some(&co_occurrence_count) = player.co_occurrences.get(&higher_freq_player.name) {
                // If this player appears with a higher-frequency player in >50% of their games,
                // they're likely a practice partner/teammate, not the user
                let co_occurrence_rate = co_occurrence_count as f64 / player.frequency as f64;
                if co_occurrence_rate > 0.5 {
                    is_user_candidate = false;
                    break;
                }
            }
        }

        if is_user_candidate {
            user_candidates.push(player.name.clone());
        }
    }

    user_candidates
}

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
    replay_folders: Vec<PathBuf>,
    tracker: Arc<Mutex<ReplayTracker>>,
    uploader: Arc<ReplayUploader>,
    state: Arc<Mutex<UploadManagerState>>,
    logger: Arc<DebugLogger>,
}

impl UploadManager {
    /// Create a new upload manager that watches multiple folders
    pub fn new(
        replay_folders: Vec<PathBuf>,
        base_url: String,
        access_token: String,
        logger: Arc<DebugLogger>,
    ) -> Result<Self, String> {
        logger.info(format!("Loading replay tracker for {} folder(s)...", replay_folders.len()));
        for folder in &replay_folders {
            logger.debug(format!("  - {}", folder.display()));
        }
        let tracker = ReplayTracker::load()?;
        logger.info("Replay tracker loaded successfully".to_string());

        Ok(Self {
            replay_folders,
            tracker: Arc::new(Mutex::new(tracker)),
            uploader: Arc::new(ReplayUploader::with_logger(base_url, access_token, Some(logger.clone()))),
            state: Arc::new(Mutex::new(UploadManagerState {
                total_uploaded: 0,
                current_upload: None,
                pending_count: 0,
                is_watching: false,
            })),
            logger,
        })
    }

    /// Get current state
    pub fn get_state(&self) -> UploadManagerState {
        // Use unwrap_or_else to recover from poisoned mutex
        let state = self.state.lock()
            .unwrap_or_else(|e| e.into_inner());
        state.clone()
    }

    /// Scan for new replays and upload them (up to limit)
    /// Uses two-layer deduplication: local tracker + server check
    ///
    /// This method delegates to ReplayScanner and UploadExecutor services
    /// for better separation of concerns and testability.
    pub async fn scan_and_upload(&self, limit: usize, app: &tauri::AppHandle) -> Result<usize, String> {
        self.logger.info(format!("Starting scan and upload (limit: {})", limit));

        // Emit start event
        if let Err(e) = app.emit("upload-start", serde_json::json!({
            "limit": limit
        })) {
            self.logger.warn(format!("Failed to emit upload-start: {}", e));
        }

        // Step 1: Fetch player names from user settings
        let player_names = self.fetch_player_names().await;

        // Step 2: Clone tracker for scanning (avoid holding lock across await)
        let tracker = self.tracker.lock()
            .map_err(|_| "Failed to lock tracker")?
            .clone();

        // Step 3: Use ReplayScanner to prepare replays
        let scanner = ReplayScanner::new(self.replay_folders.clone(), Arc::clone(&self.logger));
        let scan_result = scanner.scan_and_prepare(
            &tracker,
            &self.uploader,
            player_names,
            limit,
        ).await?;

        // Emit check events
        if let Err(e) = app.emit("upload-checking", serde_json::json!({
            "count": scan_result.total_found
        })) {
            self.logger.warn(format!("Failed to emit upload-checking: {}", e));
        }

        let new_count = scan_result.prepared_replays.len();
        if let Err(e) = app.emit("upload-check-complete", serde_json::json!({
            "new_count": new_count,
            "existing_count": scan_result.server_duplicate_count + scan_result.local_duplicate_count
        })) {
            self.logger.warn(format!("Failed to emit upload-check-complete: {}", e));
        }

        if scan_result.prepared_replays.is_empty() {
            self.logger.info("No new replays to upload".to_string());
            if let Err(e) = app.emit("upload-complete", serde_json::json!({
                "count": 0
            })) {
                self.logger.warn(format!("Failed to emit upload-complete: {}", e));
            }
            return Ok(0);
        }

        // Step 4: Use UploadExecutor to execute uploads
        let executor = UploadExecutor::new(
            Arc::clone(&self.uploader),
            Arc::clone(&self.tracker),
            Arc::clone(&self.state),
            Arc::clone(&self.logger),
        );

        let upload_result = executor.execute(scan_result.prepared_replays, app).await?;

        self.logger.info(format!(
            "Scan and upload complete: {} replays uploaded",
            upload_result.uploaded_count
        ));

        // Emit completion event
        if let Err(e) = app.emit("upload-complete", serde_json::json!({
            "count": upload_result.uploaded_count
        })) {
            self.logger.warn(format!("Failed to emit upload-complete: {}", e));
        }

        Ok(upload_result.uploaded_count)
    }

    /// Fetch player names from user settings API
    async fn fetch_player_names(&self) -> Vec<String> {
        self.logger.info("Fetching user settings for player name filtering".to_string());

        match self.uploader.get_user_settings().await {
            Ok(settings) => {
                let mut names = settings.confirmed_player_names.clone();
                names.extend(settings.possible_player_names.keys().cloned());

                if names.is_empty() {
                    self.logger.info("No player names configured yet - will detect from replays".to_string());
                } else {
                    self.logger.info(format!(
                        "Filtering for {} player name(s): {}",
                        names.len(),
                        names.join(", ")
                    ));
                }
                names
            }
            Err(e) => {
                self.logger.warn(format!(
                    "Could not fetch user settings: {}, will detect from replays",
                    e
                ));
                Vec::new()
            }
        }
    }

    /// Start watching all replay folders for new files
    pub async fn start_watching<F>(
        &self,
        on_new_file: F,
    ) -> Result<(), String>
    where
        F: Fn(PathBuf) + Send + 'static,
    {
        let (tx, mut rx) = mpsc::channel(100);

        let folders = self.replay_folders.clone();
        let logger = self.logger.clone();
        let logger_for_watcher = self.logger.clone();

        // Create file watcher
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            match res {
                Ok(event) => {
                    logger_for_watcher.debug(format!("File event detected: {:?}", event.kind));

                    // Only care about create and modify events
                    if matches!(event.kind, notify::EventKind::Create(_) | notify::EventKind::Modify(_)) {
                        for path in event.paths {
                            if is_sc2_replay(&path) {
                                logger_for_watcher.info(format!("New replay file detected: {}", path.display()));
                                if let Err(e) = tx.blocking_send(path.clone()) {
                                    logger_for_watcher.warn(format!("Failed to queue replay: {} - {}", path.display(), e));
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    logger_for_watcher.error(format!("File watcher error: {}", e));
                }
            }
        })
        .map_err(|e| format!("Failed to create watcher: {}", e))?;

        // Watch ALL folders recursively (important for Windows where SC2 may have nested folders)
        for folder in &folders {
            watcher.watch(folder, RecursiveMode::Recursive)
                .map_err(|e| format!("Failed to watch folder {}: {}", folder.display(), e))?;
            logger.info(format!("Started watching replay folder (recursive): {}", folder.display()));
        }

        logger.info(format!("Watching {} replay folder(s) recursively for new files", folders.len()));

        // Update state (recover from poisoned mutex if needed)
        {
            let mut state = self.state.lock()
                .unwrap_or_else(|e| e.into_inner());
            state.is_watching = true;
        }

        // Spawn task to handle events
        // CRITICAL: Move watcher into the task to keep it alive for the app's lifetime
        let logger_for_task = logger.clone();
        tokio::spawn(async move {
            // Keep watcher alive by moving it into this long-running task
            let _watcher = watcher;

            while let Some(path) = rx.recv().await {
                // Add delay to ensure file is fully written
                let delay_ms = get_file_processing_delay_ms();
                logger_for_task.debug(format!("Waiting {}ms before processing: {}", delay_ms, path.display()));
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms)).await;
                logger_for_task.info(format!("Processing new replay file: {}", path.display()));
                on_new_file(path);
            }

            // This point is only reached if the channel is closed (which shouldn't happen)
            logger_for_task.warn("File watcher channel closed unexpectedly".to_string());
        });

        Ok(())
    }

    /// Stop watching (not implemented - watcher lives for app lifetime)
    #[allow(dead_code)]
    pub fn stop_watching(&self) {
        let mut state = self.state.lock()
            .unwrap_or_else(|e| e.into_inner());
        state.is_watching = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_tracker::ReplayFileInfo;
    use tempfile::TempDir;
    use std::fs;
    use std::path::Path;
    use std::time::SystemTime;

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
        let logger = Arc::new(DebugLogger::new());

        let manager = UploadManager::new(
            vec![temp_dir.path().to_path_buf()],
            "https://example.com".to_string(),
            "test-token".to_string(),
            logger,
        );

        assert!(manager.is_ok());
        let manager = manager.unwrap();

        let state = manager.get_state();
        assert_eq!(state.total_uploaded, 0);
        assert_eq!(state.pending_count, 0);
        assert!(!state.is_watching);
    }

    #[tokio::test]
    async fn test_upload_manager_multiple_folders() {
        let temp_dir1 = TempDir::new().unwrap();
        let temp_dir2 = TempDir::new().unwrap();
        let logger = Arc::new(DebugLogger::new());

        let manager = UploadManager::new(
            vec![temp_dir1.path().to_path_buf(), temp_dir2.path().to_path_buf()],
            "https://example.com".to_string(),
            "test-token".to_string(),
            logger,
        );

        assert!(manager.is_ok(), "Should accept multiple folders");
    }

    #[tokio::test]
    async fn test_get_state() {
        let temp_dir = TempDir::new().unwrap();
        let logger = Arc::new(DebugLogger::new());

        let manager = UploadManager::new(
            vec![temp_dir.path().to_path_buf()],
            "https://example.com".to_string(),
            "test-token".to_string(),
            logger,
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
        let logger = Arc::new(DebugLogger::new());

        let manager = UploadManager::new(
            vec![temp_dir.path().to_path_buf()],
            "https://example.com".to_string(),
            "test-token".to_string(),
            logger,
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

    // Tests for group_replays_by_type_and_player function
    #[test]
    fn test_group_replays_by_type_and_player_empty() {
        let hashes: Vec<String> = vec![];
        let replay_map: HashMap<String, (ReplayFileInfo, String, String)> = HashMap::new();

        let groups = group_replays_by_type_and_player(&hashes, &replay_map);

        assert_eq!(groups.len(), 0, "Empty input should produce no groups");
    }

    #[test]
    fn test_group_replays_by_type_and_player_single_group() {
        let temp_dir = TempDir::new().unwrap();
        let replay_path1 = create_test_replay(temp_dir.path(), "replay1.SC2Replay", b"test1");
        let replay_path2 = create_test_replay(temp_dir.path(), "replay2.SC2Replay", b"test2");

        let hashes = vec!["hash1".to_string(), "hash2".to_string()];
        let mut replay_map = HashMap::new();
        replay_map.insert("hash1".to_string(), (
            ReplayFileInfo {
                path: replay_path1,
                filename: "replay1.SC2Replay".to_string(),
                filesize: 5,
                modified_time: SystemTime::UNIX_EPOCH,
            },
            "1v1-ladder".to_string(),
            "lotus".to_string(),
        ));
        replay_map.insert("hash2".to_string(), (
            ReplayFileInfo {
                path: replay_path2,
                filename: "replay2.SC2Replay".to_string(),
                filesize: 5,
                modified_time: SystemTime::UNIX_EPOCH,
            },
            "1v1-ladder".to_string(),
            "lotus".to_string(),
        ));

        let groups = group_replays_by_type_and_player(&hashes, &replay_map);

        assert_eq!(groups.len(), 1, "Should have one group for same type/player");
        assert_eq!(groups[0].game_type, "1v1-ladder");
        assert_eq!(groups[0].player_name, "lotus");
        assert_eq!(groups[0].hashes.len(), 2);
        assert!(groups[0].hashes.contains(&"hash1".to_string()));
        assert!(groups[0].hashes.contains(&"hash2".to_string()));
    }

    #[test]
    fn test_group_replays_by_type_and_player_multiple_players() {
        let temp_dir = TempDir::new().unwrap();
        let replay_path1 = create_test_replay(temp_dir.path(), "replay1.SC2Replay", b"test1");
        let replay_path2 = create_test_replay(temp_dir.path(), "replay2.SC2Replay", b"test2");

        let hashes = vec!["hash1".to_string(), "hash2".to_string()];
        let mut replay_map = HashMap::new();
        replay_map.insert("hash1".to_string(), (
            ReplayFileInfo {
                path: replay_path1,
                filename: "replay1.SC2Replay".to_string(),
                filesize: 5,
                modified_time: SystemTime::UNIX_EPOCH,
            },
            "1v1-ladder".to_string(),
            "lotus".to_string(),
        ));
        replay_map.insert("hash2".to_string(), (
            ReplayFileInfo {
                path: replay_path2,
                filename: "replay2.SC2Replay".to_string(),
                filesize: 5,
                modified_time: SystemTime::UNIX_EPOCH,
            },
            "1v1-ladder".to_string(),
            "lotusAlt".to_string(),
        ));

        let groups = group_replays_by_type_and_player(&hashes, &replay_map);

        assert_eq!(groups.len(), 2, "Should have two groups for different players");
        // Groups should be sorted by player name
        assert_eq!(groups[0].player_name, "lotus");
        assert_eq!(groups[1].player_name, "lotusAlt");
        assert_eq!(groups[0].hashes.len(), 1);
        assert_eq!(groups[1].hashes.len(), 1);
    }

    #[test]
    fn test_group_replays_by_type_and_player_multiple_types() {
        let temp_dir = TempDir::new().unwrap();
        let replay_path1 = create_test_replay(temp_dir.path(), "replay1.SC2Replay", b"test1");
        let replay_path2 = create_test_replay(temp_dir.path(), "replay2.SC2Replay", b"test2");

        let hashes = vec!["hash1".to_string(), "hash2".to_string()];
        let mut replay_map = HashMap::new();
        replay_map.insert("hash1".to_string(), (
            ReplayFileInfo {
                path: replay_path1,
                filename: "replay1.SC2Replay".to_string(),
                filesize: 5,
                modified_time: SystemTime::UNIX_EPOCH,
            },
            "1v1-ladder".to_string(),
            "lotus".to_string(),
        ));
        replay_map.insert("hash2".to_string(), (
            ReplayFileInfo {
                path: replay_path2,
                filename: "replay2.SC2Replay".to_string(),
                filesize: 5,
                modified_time: SystemTime::UNIX_EPOCH,
            },
            "2v2-ladder".to_string(),
            "lotus".to_string(),
        ));

        let groups = group_replays_by_type_and_player(&hashes, &replay_map);

        assert_eq!(groups.len(), 2, "Should have two groups for different types");
        // Groups should be sorted by game type
        assert_eq!(groups[0].game_type, "1v1-ladder");
        assert_eq!(groups[1].game_type, "2v2-ladder");
        assert_eq!(groups[0].hashes.len(), 1);
        assert_eq!(groups[1].hashes.len(), 1);
    }

    #[test]
    fn test_group_replays_by_type_and_player_complex() {
        let temp_dir = TempDir::new().unwrap();
        let replay_path1 = create_test_replay(temp_dir.path(), "replay1.SC2Replay", b"test1");
        let replay_path2 = create_test_replay(temp_dir.path(), "replay2.SC2Replay", b"test2");
        let replay_path3 = create_test_replay(temp_dir.path(), "replay3.SC2Replay", b"test3");
        let replay_path4 = create_test_replay(temp_dir.path(), "replay4.SC2Replay", b"test4");

        let hashes = vec!["hash1".to_string(), "hash2".to_string(), "hash3".to_string(), "hash4".to_string()];
        let mut replay_map = HashMap::new();
        replay_map.insert("hash1".to_string(), (
            ReplayFileInfo { path: replay_path1, filename: "replay1.SC2Replay".to_string(), filesize: 5, modified_time: SystemTime::UNIX_EPOCH },
            "1v1-ladder".to_string(), "lotus".to_string(),
        ));
        replay_map.insert("hash2".to_string(), (
            ReplayFileInfo { path: replay_path2, filename: "replay2.SC2Replay".to_string(), filesize: 5, modified_time: SystemTime::UNIX_EPOCH },
            "1v1-ladder".to_string(), "lotusAlt".to_string(),
        ));
        replay_map.insert("hash3".to_string(), (
            ReplayFileInfo { path: replay_path3, filename: "replay3.SC2Replay".to_string(), filesize: 5, modified_time: SystemTime::UNIX_EPOCH },
            "2v2-ladder".to_string(), "lotus".to_string(),
        ));
        replay_map.insert("hash4".to_string(), (
            ReplayFileInfo { path: replay_path4, filename: "replay4.SC2Replay".to_string(), filesize: 5, modified_time: SystemTime::UNIX_EPOCH },
            "2v2-ladder".to_string(), "lotusAlt".to_string(),
        ));

        let groups = group_replays_by_type_and_player(&hashes, &replay_map);

        assert_eq!(groups.len(), 4, "Should have four groups (2 types × 2 players)");

        // Verify sorting: 1v1-ladder < 2v2-ladder, then lotus < lotusAlt
        assert_eq!(groups[0].game_type, "1v1-ladder");
        assert_eq!(groups[0].player_name, "lotus");
        assert_eq!(groups[1].game_type, "1v1-ladder");
        assert_eq!(groups[1].player_name, "lotusAlt");
        assert_eq!(groups[2].game_type, "2v2-ladder");
        assert_eq!(groups[2].player_name, "lotus");
        assert_eq!(groups[3].game_type, "2v2-ladder");
        assert_eq!(groups[3].player_name, "lotusAlt");
    }

    #[test]
    fn test_group_replays_by_type_and_player_missing_hash() {
        let hashes = vec!["hash1".to_string(), "hash_missing".to_string()];
        let mut replay_map = HashMap::new();
        let temp_dir = TempDir::new().unwrap();
        let replay_path = create_test_replay(temp_dir.path(), "replay1.SC2Replay", b"test1");

        replay_map.insert("hash1".to_string(), (
            ReplayFileInfo {
                path: replay_path,
                filename: "replay1.SC2Replay".to_string(),
                filesize: 5,
                modified_time: SystemTime::UNIX_EPOCH,
            },
            "1v1-ladder".to_string(),
            "lotus".to_string(),
        ));

        let groups = group_replays_by_type_and_player(&hashes, &replay_map);

        assert_eq!(groups.len(), 1, "Should skip missing hash and create one group");
        assert_eq!(groups[0].hashes.len(), 1);
        assert_eq!(groups[0].hashes[0], "hash1");
    }

    // Player detection tests

    #[test]
    fn test_detect_user_player_names_empty() {
        let replays = vec![];
        let detected = detect_user_player_names(&replays);
        assert_eq!(detected.len(), 0, "Should return empty vec for no replays");
    }

    #[test]
    fn test_detect_user_player_names_single_player_1v1() {
        // User plays 1v1 against different opponents
        let replays = vec![
            ("replay1".to_string(), vec![("Lotus".to_string(), false), ("Opponent1".to_string(), false)]),
            ("replay2".to_string(), vec![("Lotus".to_string(), false), ("Opponent2".to_string(), false)]),
            ("replay3".to_string(), vec![("Lotus".to_string(), false), ("Opponent3".to_string(), false)]),
            ("replay4".to_string(), vec![("Lotus".to_string(), false), ("Opponent4".to_string(), false)]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 1, "Should detect one user");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as the user");
    }

    #[test]
    fn test_detect_user_player_names_filters_practice_partner() {
        // User plays 1v1, but has a frequent practice partner
        let replays = vec![
            ("replay1".to_string(), vec![("Lotus".to_string(), false), ("PracticePartner".to_string(), false)]),
            ("replay2".to_string(), vec![("Lotus".to_string(), false), ("PracticePartner".to_string(), false)]),
            ("replay3".to_string(), vec![("Lotus".to_string(), false), ("PracticePartner".to_string(), false)]),
            ("replay4".to_string(), vec![("Lotus".to_string(), false), ("Opponent1".to_string(), false)]),
            ("replay5".to_string(), vec![("Lotus".to_string(), false), ("Opponent2".to_string(), false)]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 1, "Should detect one user");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as the user, not practice partner");
        assert!(!detected.contains(&"PracticePartner".to_string()), "Should filter out practice partner");
    }

    #[test]
    fn test_detect_user_player_names_2v2_filters_teammate() {
        // User plays 2v2 with a frequent teammate
        let replays = vec![
            ("replay1".to_string(), vec![
                ("Lotus".to_string(), false),
                ("FrequentTeammate".to_string(), false),
                ("Enemy1".to_string(), false),
                ("Enemy2".to_string(), false),
            ]),
            ("replay2".to_string(), vec![
                ("Lotus".to_string(), false),
                ("FrequentTeammate".to_string(), false),
                ("Enemy3".to_string(), false),
                ("Enemy4".to_string(), false),
            ]),
            ("replay3".to_string(), vec![
                ("Lotus".to_string(), false),
                ("FrequentTeammate".to_string(), false),
                ("Enemy5".to_string(), false),
                ("Enemy6".to_string(), false),
            ]),
            ("replay4".to_string(), vec![
                ("Lotus".to_string(), false),
                ("RandomTeammate".to_string(), false),
                ("Enemy7".to_string(), false),
                ("Enemy8".to_string(), false),
            ]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 1, "Should detect one user");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as the user");
        assert!(!detected.contains(&"FrequentTeammate".to_string()), "Should filter out frequent teammate");
    }

    #[test]
    fn test_detect_user_player_names_multiple_smurfs() {
        // User has multiple accounts (smurfs)
        let replays = vec![
            ("replay1".to_string(), vec![("Lotus".to_string(), false), ("Opponent1".to_string(), false)]),
            ("replay2".to_string(), vec![("Lotus".to_string(), false), ("Opponent2".to_string(), false)]),
            ("replay3".to_string(), vec![("Lotus".to_string(), false), ("Opponent3".to_string(), false)]),
            ("replay4".to_string(), vec![("LotusAlt".to_string(), false), ("Opponent4".to_string(), false)]),
            ("replay5".to_string(), vec![("LotusAlt".to_string(), false), ("Opponent5".to_string(), false)]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 2, "Should detect two user accounts");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as primary account (highest frequency)");
        assert_eq!(detected[1], "LotusAlt", "Should detect 'LotusAlt' as secondary account");
    }

    #[test]
    fn test_detect_user_player_names_ignores_observers() {
        // Some replays have observers, should ignore them
        let replays = vec![
            ("replay1".to_string(), vec![
                ("Lotus".to_string(), false),
                ("Opponent1".to_string(), false),
                ("Observer1".to_string(), true),
            ]),
            ("replay2".to_string(), vec![
                ("Lotus".to_string(), false),
                ("Opponent2".to_string(), false),
                ("Observer2".to_string(), true),
                ("Observer3".to_string(), true),
            ]),
            ("replay3".to_string(), vec![
                ("Lotus".to_string(), false),
                ("Opponent3".to_string(), false),
            ]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 1, "Should detect one user");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as the user");
        assert!(!detected.contains(&"Observer1".to_string()), "Should not detect observers");
        assert!(!detected.contains(&"Observer2".to_string()), "Should not detect observers");
    }

    #[test]
    fn test_detect_user_player_names_complex_scenario() {
        // Mix of 1v1 and 2v2 games with multiple accounts
        let replays = vec![
            // 1v1 games on main account
            ("1v1_1".to_string(), vec![("Lotus".to_string(), false), ("Opponent1".to_string(), false)]),
            ("1v1_2".to_string(), vec![("Lotus".to_string(), false), ("Opponent2".to_string(), false)]),
            ("1v1_3".to_string(), vec![("Lotus".to_string(), false), ("Opponent3".to_string(), false)]),
            // 2v2 games on main account with frequent teammate
            ("2v2_1".to_string(), vec![
                ("Lotus".to_string(), false),
                ("FrequentTeammate".to_string(), false),
                ("Enemy1".to_string(), false),
                ("Enemy2".to_string(), false),
            ]),
            ("2v2_2".to_string(), vec![
                ("Lotus".to_string(), false),
                ("FrequentTeammate".to_string(), false),
                ("Enemy3".to_string(), false),
                ("Enemy4".to_string(), false),
            ]),
            // 1v1 games on alt account
            ("1v1_alt_1".to_string(), vec![("LotusAlt".to_string(), false), ("Opponent4".to_string(), false)]),
            ("1v1_alt_2".to_string(), vec![("LotusAlt".to_string(), false), ("Opponent5".to_string(), false)]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 2, "Should detect two user accounts");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as primary");
        assert_eq!(detected[1], "LotusAlt", "Should detect 'LotusAlt' as secondary");
        assert!(!detected.contains(&"FrequentTeammate".to_string()), "Should filter out frequent teammate");
    }

    #[test]
    fn test_detect_user_player_names_all_observers() {
        // Edge case: all players are observers
        let replays = vec![
            ("replay1".to_string(), vec![
                ("Observer1".to_string(), true),
                ("Observer2".to_string(), true),
            ]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 0, "Should return empty for all-observer games");
    }

    #[test]
    fn test_detect_user_player_names_filters_single_occurrence() {
        // Players who appear only once should be filtered out
        let replays = vec![
            ("replay1".to_string(), vec![("Lotus".to_string(), false), ("Opponent1".to_string(), false)]),
            ("replay2".to_string(), vec![("Lotus".to_string(), false), ("Opponent2".to_string(), false)]),
            ("replay3".to_string(), vec![("Lotus".to_string(), false), ("Opponent3".to_string(), false)]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 1, "Should detect one user");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as the user");
        assert!(!detected.contains(&"Opponent1".to_string()), "Should filter out single-occurrence players");
        assert!(!detected.contains(&"Opponent2".to_string()), "Should filter out single-occurrence players");
        assert!(!detected.contains(&"Opponent3".to_string()), "Should filter out single-occurrence players");
    }

    #[test]
    fn test_detect_user_player_names_filters_ai_players() {
        // AI player names should be filtered out
        let replays = vec![
            ("ai1".to_string(), vec![("Lotus".to_string(), false), ("Computer".to_string(), false)]),
            ("ai2".to_string(), vec![("Lotus".to_string(), false), ("Computer".to_string(), false)]),
            ("ai3".to_string(), vec![("Lotus".to_string(), false), ("Computer".to_string(), false)]),
            ("ai4".to_string(), vec![("Lotus".to_string(), false), ("A.I.".to_string(), false)]),
            ("ai5".to_string(), vec![("Lotus".to_string(), false), ("Bot".to_string(), false)]),
        ];

        let detected = detect_user_player_names(&replays);

        assert_eq!(detected.len(), 1, "Should detect one user");
        assert_eq!(detected[0], "Lotus", "Should detect 'Lotus' as the user");
        assert!(!detected.contains(&"Computer".to_string()), "Should filter out 'Computer' AI name");
        assert!(!detected.contains(&"A.I.".to_string()), "Should filter out 'A.I.' AI name");
        assert!(!detected.contains(&"Bot".to_string()), "Should filter out 'Bot' AI name");
    }

    // Tests for is_sc2_replay helper function

    #[test]
    fn test_is_sc2_replay_standard_extension() {
        use std::path::Path;
        assert!(is_sc2_replay(Path::new("game.SC2Replay")));
        assert!(is_sc2_replay(Path::new("/path/to/game.SC2Replay")));
        assert!(is_sc2_replay(Path::new("C:\\Users\\Player\\Replays\\game.SC2Replay")));
    }

    #[test]
    fn test_is_sc2_replay_case_insensitive() {
        use std::path::Path;
        // Windows may have different casing
        assert!(is_sc2_replay(Path::new("game.sc2replay")));
        assert!(is_sc2_replay(Path::new("game.SC2REPLAY")));
        assert!(is_sc2_replay(Path::new("game.Sc2Replay")));
        assert!(is_sc2_replay(Path::new("game.sC2rEpLaY")));
    }

    #[test]
    fn test_is_sc2_replay_non_replay_files() {
        use std::path::Path;
        assert!(!is_sc2_replay(Path::new("game.txt")));
        assert!(!is_sc2_replay(Path::new("game.mp4")));
        assert!(!is_sc2_replay(Path::new("SC2Replay.txt")));
        assert!(!is_sc2_replay(Path::new("game")));
        assert!(!is_sc2_replay(Path::new("/path/to/folder/")));
    }

    #[test]
    fn test_is_sc2_replay_edge_cases() {
        use std::path::Path;
        assert!(!is_sc2_replay(Path::new("")));
        assert!(!is_sc2_replay(Path::new(".")));
        assert!(!is_sc2_replay(Path::new(".SC2Replay")));  // Hidden file, no name
        assert!(is_sc2_replay(Path::new("a.SC2Replay")));  // Minimal valid name
    }

    #[test]
    fn test_get_file_processing_delay_ms() {
        let delay = get_file_processing_delay_ms();
        // On Windows, delay should be 1000ms; on other platforms, 500ms
        #[cfg(target_os = "windows")]
        assert_eq!(delay, 1000, "Windows should have 1000ms delay");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(delay, 500, "Non-Windows should have 500ms delay");
    }
}
