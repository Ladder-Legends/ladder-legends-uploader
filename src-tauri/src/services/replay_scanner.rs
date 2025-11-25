//! Replay scanning and preparation service
//!
//! Handles scanning replay folders, filtering, hashing, and preparing
//! replays for upload. Extracted from the monolithic scan_and_upload function.

use crate::replay_tracker::{ReplayTracker, ReplayFileInfo, scan_replay_folder};
use crate::replay_uploader::{ReplayUploader, HashInfo};
use crate::replay_parser;
use crate::debug_logger::DebugLogger;
use crate::upload_manager::detect_user_player_names;
use std::path::PathBuf;
use std::collections::HashMap;
use std::sync::Arc;

/// A replay that has been scanned, filtered, and is ready for upload
#[derive(Debug, Clone)]
pub struct PreparedReplay {
    pub hash: String,
    pub file_info: ReplayFileInfo,
    pub game_type: String,
    pub player_name: String,
}

/// Result of scanning replay folders
#[derive(Debug)]
pub struct ScanResult {
    /// Replays ready for upload, grouped by (game_type, player_name)
    pub prepared_replays: Vec<PreparedReplay>,
    /// Total replays found across all folders
    pub total_found: usize,
    /// Replays already in local tracker
    pub local_duplicate_count: usize,
    /// Replays already on server
    pub server_duplicate_count: usize,
}

/// Service for scanning and preparing replays for upload
pub struct ReplayScanner {
    replay_folders: Vec<PathBuf>,
    logger: Arc<DebugLogger>,
}

impl ReplayScanner {
    pub fn new(replay_folders: Vec<PathBuf>, logger: Arc<DebugLogger>) -> Self {
        Self {
            replay_folders,
            logger,
        }
    }

    /// Scan all folders and prepare replays for upload
    ///
    /// This performs:
    /// 1. Folder scanning
    /// 2. Game type filtering (only competitive games)
    /// 3. Player filtering (only games where user is active)
    /// 4. Local tracker deduplication
    /// 5. Hash computation
    /// 6. Server deduplication
    pub async fn scan_and_prepare(
        &self,
        tracker: &ReplayTracker,
        uploader: &ReplayUploader,
        player_names: Vec<String>,
        limit: usize,
    ) -> Result<ScanResult, String> {
        // Step 1: Scan all folders for replays
        let all_replays = self.scan_all_folders()?;
        let total_found = all_replays.len();

        if all_replays.is_empty() {
            self.logger.info("No replays found in any folder".to_string());
            return Ok(ScanResult {
                prepared_replays: Vec::new(),
                total_found: 0,
                local_duplicate_count: 0,
                server_duplicate_count: 0,
            });
        }

        // Sort by modified time (newest first) and take recent subset
        let recent_replays = self.get_recent_replays(all_replays, limit * 2);
        self.logger.info(format!(
            "Found {} replays across {} folder(s) (total: {})",
            recent_replays.len(),
            self.replay_folders.len(),
            total_found
        ));

        // Step 2: Detect player names if not provided
        let player_names = if player_names.is_empty() {
            self.detect_players_from_replays(&recent_replays)
        } else {
            player_names
        };

        // Step 3: Filter and compute hashes
        let filter_result = self.filter_and_hash_replays(
            recent_replays,
            tracker,
            &player_names,
        )?;

        if filter_result.hash_infos.is_empty() {
            self.logger.info("All replays already uploaded (per local tracker)".to_string());
            return Ok(ScanResult {
                prepared_replays: Vec::new(),
                total_found,
                local_duplicate_count: filter_result.local_duplicate_count,
                server_duplicate_count: 0,
            });
        }

        // Step 4: Check with server for new hashes
        self.logger.info(format!("Checking {} hashes with server...", filter_result.hash_infos.len()));
        let check_result = uploader.check_hashes(filter_result.hash_infos).await?;

        self.logger.info(format!(
            "Server check complete: {} new, {} existing",
            check_result.new_hashes.len(),
            check_result.existing_count
        ));

        // Step 5: Build prepared replays list (limited)
        let prepared_replays: Vec<PreparedReplay> = check_result
            .new_hashes
            .into_iter()
            .take(limit)
            .filter_map(|hash| {
                filter_result.replay_map.get(&hash).map(|(file_info, game_type, player_name)| {
                    PreparedReplay {
                        hash,
                        file_info: file_info.clone(),
                        game_type: game_type.clone(),
                        player_name: player_name.clone(),
                    }
                })
            })
            .collect();

        Ok(ScanResult {
            prepared_replays,
            total_found,
            local_duplicate_count: filter_result.local_duplicate_count,
            server_duplicate_count: check_result.existing_count,
        })
    }

    /// Scan all configured replay folders
    fn scan_all_folders(&self) -> Result<Vec<ReplayFileInfo>, String> {
        let mut all_replays = Vec::new();

        for folder in &self.replay_folders {
            match scan_replay_folder(folder) {
                Ok(replays) => {
                    self.logger.debug(format!(
                        "Found {} replays in {}",
                        replays.len(),
                        folder.display()
                    ));
                    all_replays.extend(replays);
                }
                Err(e) => {
                    self.logger.warn(format!(
                        "Error scanning {}: {}",
                        folder.display(),
                        e
                    ));
                }
            }
        }

        Ok(all_replays)
    }

    /// Get most recent replays sorted by modified time
    fn get_recent_replays(
        &self,
        mut replays: Vec<ReplayFileInfo>,
        limit: usize,
    ) -> Vec<ReplayFileInfo> {
        replays.sort_by(|a, b| b.modified_time.cmp(&a.modified_time));
        replays.into_iter().take(limit).collect()
    }

    /// Detect player names from replay files
    fn detect_players_from_replays(&self, replays: &[ReplayFileInfo]) -> Vec<String> {
        self.logger.info("No player names from API, scanning replays to detect user".to_string());

        let mut replay_player_data = Vec::new();
        for replay_info in replays {
            if let Ok(players) = replay_parser::get_players(&replay_info.path) {
                let player_list: Vec<(String, bool)> = players
                    .iter()
                    .map(|p| (p.name.clone(), p.is_observer))
                    .collect();
                replay_player_data.push((
                    replay_info.path.to_string_lossy().to_string(),
                    player_list,
                ));
            }
        }

        let detected_names = detect_user_player_names(&replay_player_data);
        if !detected_names.is_empty() {
            self.logger.info(format!(
                "Detected {} player name(s): {}",
                detected_names.len(),
                detected_names.join(", ")
            ));
        } else {
            self.logger.warn("Could not detect player names from replays".to_string());
        }
        detected_names
    }

    /// Filter replays and compute hashes
    fn filter_and_hash_replays(
        &self,
        replays: Vec<ReplayFileInfo>,
        tracker: &ReplayTracker,
        player_names: &[String],
    ) -> Result<FilterResult, String> {
        let mut hash_infos = Vec::new();
        let mut replay_map: HashMap<String, (ReplayFileInfo, String, String)> = HashMap::new();
        let mut non_competitive_count = 0;
        let mut observer_game_count = 0;
        let mut local_duplicate_count = 0;

        for replay_info in replays {
            // Filter 1: Game type (only competitive games)
            let game_type = match replay_parser::get_game_type(&replay_info.path) {
                Ok(gtype) => gtype,
                Err(e) => {
                    self.logger.warn(format!(
                        "Could not parse {} ({}), skipping",
                        replay_info.filename, e
                    ));
                    continue;
                }
            };

            if !game_type.should_upload() {
                non_competitive_count += 1;
                self.logger.info(format!(
                    "Skipping {} - not a competitive game (type: {})",
                    replay_info.filename,
                    game_type.as_str()
                ));
                continue;
            }

            // Filter 2: Player presence (user must be active player)
            let players = match replay_parser::get_players(&replay_info.path) {
                Ok(p) => p,
                Err(e) => {
                    self.logger.warn(format!(
                        "Could not extract players from {} ({}), skipping",
                        replay_info.filename, e
                    ));
                    continue;
                }
            };

            let player_name_in_replay = self.find_user_in_game(&players, player_names);
            let player_name_in_replay = match player_name_in_replay {
                Some(name) => name,
                None => {
                    observer_game_count += 1;
                    self.logger.debug(format!(
                        "Skipping {} (player not active in game)",
                        replay_info.filename
                    ));
                    continue;
                }
            };

            // Filter 3: Local tracker (skip if already uploaded)
            if tracker.exists_by_metadata(&replay_info.filename, replay_info.filesize) {
                local_duplicate_count += 1;
                self.logger.debug(format!(
                    "Skipping {} (in local tracker by metadata)",
                    replay_info.filename
                ));
                continue;
            }

            // Compute hash
            let hash = ReplayTracker::calculate_hash(&replay_info.path)?;

            // Filter 4: Local tracker by hash
            if tracker.is_uploaded(&hash) {
                local_duplicate_count += 1;
                self.logger.debug(format!(
                    "Skipping {} (in local tracker by hash)",
                    replay_info.filename
                ));
                continue;
            }

            hash_infos.push(HashInfo {
                hash: hash.clone(),
                filename: replay_info.filename.clone(),
                filesize: replay_info.filesize,
            });

            replay_map.insert(
                hash,
                (replay_info, game_type.as_str().to_string(), player_name_in_replay),
            );
        }

        // Log filter stats
        if non_competitive_count > 0 {
            self.logger.info(format!("Filtered out {} non-competitive replays", non_competitive_count));
        }
        if observer_game_count > 0 {
            self.logger.info(format!("Filtered out {} observer/non-player games", observer_game_count));
        }

        self.logger.info(format!("{} replays not in local tracker", hash_infos.len()));

        Ok(FilterResult {
            hash_infos,
            replay_map,
            local_duplicate_count,
        })
    }

    /// Find which of the user's names appears in this game as an active player
    fn find_user_in_game(
        &self,
        players: &[replay_parser::PlayerInfo],
        user_names: &[String],
    ) -> Option<String> {
        for player in players {
            if !player.is_observer && user_names.contains(&player.name) {
                return Some(player.name.clone());
            }
        }
        None
    }
}

/// Internal result of filtering and hashing
struct FilterResult {
    hash_infos: Vec<HashInfo>,
    replay_map: HashMap<String, (ReplayFileInfo, String, String)>,
    local_duplicate_count: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_replay(dir: &std::path::Path, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn test_scan_all_folders_empty() {
        let temp_dir = TempDir::new().unwrap();
        let logger = Arc::new(DebugLogger::new());
        let scanner = ReplayScanner::new(vec![temp_dir.path().to_path_buf()], logger);

        let result = scanner.scan_all_folders().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_scan_all_folders_with_replays() {
        let temp_dir = TempDir::new().unwrap();
        create_test_replay(temp_dir.path(), "test1.SC2Replay", b"replay1");
        create_test_replay(temp_dir.path(), "test2.SC2Replay", b"replay2");

        let logger = Arc::new(DebugLogger::new());
        let scanner = ReplayScanner::new(vec![temp_dir.path().to_path_buf()], logger);

        let result = scanner.scan_all_folders().unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_get_recent_replays_ordering() {
        let temp_dir = TempDir::new().unwrap();
        let logger = Arc::new(DebugLogger::new());
        let scanner = ReplayScanner::new(vec![temp_dir.path().to_path_buf()], logger);

        // Create replays with different sizes to simulate different files
        let replays = vec![
            ReplayFileInfo {
                path: temp_dir.path().join("old.SC2Replay"),
                filename: "old.SC2Replay".to_string(),
                filesize: 100,
                modified_time: std::time::SystemTime::UNIX_EPOCH,
            },
            ReplayFileInfo {
                path: temp_dir.path().join("new.SC2Replay"),
                filename: "new.SC2Replay".to_string(),
                filesize: 200,
                modified_time: std::time::SystemTime::now(),
            },
        ];

        let recent = scanner.get_recent_replays(replays, 10);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].filename, "new.SC2Replay"); // Newest first
    }

    #[test]
    fn test_get_recent_replays_limit() {
        let temp_dir = TempDir::new().unwrap();
        let logger = Arc::new(DebugLogger::new());
        let scanner = ReplayScanner::new(vec![temp_dir.path().to_path_buf()], logger);

        let replays = (0..10)
            .map(|i| ReplayFileInfo {
                path: temp_dir.path().join(format!("replay{}.SC2Replay", i)),
                filename: format!("replay{}.SC2Replay", i),
                filesize: i as u64,
                modified_time: std::time::SystemTime::UNIX_EPOCH,
            })
            .collect();

        let recent = scanner.get_recent_replays(replays, 3);
        assert_eq!(recent.len(), 3);
    }
}
