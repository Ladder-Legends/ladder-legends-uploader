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
                .or_insert_with(Vec::new)
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

        // Step 0: Fetch user settings for player name filtering (minimize API calls)
        println!("🔍 [UPLOAD] Fetching user settings for player name filtering...");
        let player_names = match self.uploader.get_user_settings().await {
            Ok(settings) => {
                // Combine confirmed names + possible names (any count) for filtering
                let mut names = settings.confirmed_player_names.clone();
                names.extend(settings.possible_player_names.keys().cloned());

                if names.is_empty() {
                    println!("ℹ️  [UPLOAD] No player names configured yet - will upload all replays");
                } else {
                    println!("🎮 [UPLOAD] Filtering for {} player name(s): {}",
                        names.len(),
                        names.join(", ")
                    );
                }
                names
            },
            Err(e) => {
                println!("⚠️  [UPLOAD] Could not fetch user settings ({}), will upload all replays", e);
                Vec::new() // Empty list = no filtering
            }
        };

        // Step 1: Scan folder for replays (get more than limit for server check)
        let all_replays = scan_replay_folder(&self.replay_folder)?;
        let recent_replays: Vec<_> = all_replays.into_iter().take(limit * 2).collect();

        println!("📁 [UPLOAD] Found {} replays in folder", recent_replays.len());

        if recent_replays.is_empty() {
            println!("ℹ️  [UPLOAD] No replays found in folder");
            return Ok(0);
        }

        // Step 1.5: If no player names from API, detect them from replays
        let player_names = if player_names.is_empty() {
            println!("🔍 [UPLOAD] No player names from API, scanning replays to detect user...");

            // Collect player data from all replays for detection
            let mut replay_player_data = Vec::new();
            for replay_info in &recent_replays {
                if let Ok(players) = replay_parser::get_players(&replay_info.path) {
                    let player_list: Vec<(String, bool)> = players.iter()
                        .map(|p| (p.name.clone(), p.is_observer))
                        .collect();
                    replay_player_data.push((replay_info.path.to_string_lossy().to_string(), player_list));
                }
            }

            let detected_names = detect_user_player_names(&replay_player_data);
            if !detected_names.is_empty() {
                println!("✅ [UPLOAD] Detected {} player name(s): {}",
                    detected_names.len(),
                    detected_names.join(", ")
                );
            } else {
                println!("⚠️  [UPLOAD] Could not detect player names from replays");
            }
            detected_names
        } else {
            player_names
        };

        // Step 2: Calculate hashes for all replays and filter by local tracker
        let mut hash_infos = Vec::new();
        // Store replay_info, game_type, and player_name together
        let mut replay_map: HashMap<String, (ReplayFileInfo, String, String)> = HashMap::new();
        let mut non_1v1_count = 0;
        let mut observer_game_count = 0;

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

            // Extract players from replay to find the user's player name
            let players = match replay_parser::get_players(&replay_info.path) {
                Ok(p) => p,
                Err(e) => {
                    println!("⚠️  [UPLOAD] Could not extract players from {} ({}), skipping", replay_info.filename, e);
                    continue;
                }
            };

            // Find which of the user's names appears in this replay (as an active player)
            let player_name_in_replay = {
                let mut found_name = None;
                for player in &players {
                    if !player.is_observer && player_names.contains(&player.name) {
                        found_name = Some(player.name.clone());
                        break;
                    }
                }

                match found_name {
                    Some(name) => name,
                    None => {
                        // User is not an active player in this game
                        observer_game_count += 1;
                        println!("⏭️  [UPLOAD] Skipping {} (player not active in game)", replay_info.filename);
                        continue;
                    }
                }
            };

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

            // Store replay info, game type, and player name for upload
            replay_map.insert(hash, (replay_info, game_type.as_str().to_string(), player_name_in_replay));
        }

        if non_1v1_count > 0 {
            println!("🎮 [UPLOAD] Filtered out {} non-1v1 replays", non_1v1_count);
        }

        if observer_game_count > 0 {
            println!("👁️  [UPLOAD] Filtered out {} observer/non-player games", observer_game_count);
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

        // Step 4: Group replays by (game_type, player_name) for batch uploading
        let to_upload: Vec<_> = check_result.new_hashes
            .into_iter()
            .take(limit)
            .collect();

        let groups = group_replays_by_type_and_player(&to_upload, &replay_map);

        println!("⬆️  [UPLOAD] Uploading {} replay(s) in {} group(s)...", to_upload.len(), groups.len());

        {
            let mut state = self.state.lock().unwrap();
            state.pending_count = to_upload.len();
        }

        let mut uploaded_count = 0;
        let mut global_index = 0;

        // Upload each group
        for group in groups {
            println!("🎮 [UPLOAD] Uploading {} {} replays for {}...", group.hashes.len(), group.game_type, group.player_name);

            // Emit batch start event
            let _ = app.emit("upload-batch-start", serde_json::json!({
                "game_type": group.game_type,
                "player_name": group.player_name,
                "count": group.hashes.len()
            }));

            for hash in &group.hashes {
                let (replay_info, game_type_str_inner, player_name_inner) = match replay_map.get(hash) {
                    Some((info, gtype, pname)) => (info, gtype, pname),
                    None => {
                        println!("⚠️  [UPLOAD] Hash {} not found in replay map, skipping", hash);
                        continue;
                    }
                };

                global_index += 1;
                println!("⬆️  [UPLOAD] [{}/{}] Uploading {} ({} for {})...",
                    global_index, to_upload.len(), replay_info.filename, game_type_str_inner, player_name_inner);

                // Update status to uploading
                {
                    let mut state = self.state.lock().unwrap();
                    state.current_upload = Some(UploadStatus::Uploading {
                        filename: replay_info.filename.clone(),
                    });
                }

                // Emit progress event with batch info
                let _ = app.emit("upload-progress", serde_json::json!({
                    "current": global_index,
                    "total": to_upload.len(),
                    "filename": replay_info.filename,
                    "game_type": group.game_type,
                    "player_name": group.player_name
                }));

                // Perform upload with game type and player name
                match self.uploader.upload_replay(&replay_info.path, Some(&group.player_name), None, Some(game_type_str_inner.as_str())).await {
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

            // Emit batch complete event
            let _ = app.emit("upload-batch-complete", serde_json::json!({
                "game_type": group.game_type,
                "player_name": group.player_name,
                "count": group.hashes.len()
            }));
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
}
