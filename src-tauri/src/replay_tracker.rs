use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Custom deserializer that handles both old (u32) and new (String) manifest_version formats
fn deserialize_manifest_version<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};

    struct ManifestVersionVisitor;

    impl<'de> Visitor<'de> for ManifestVersionVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a string or integer for manifest_version")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Old format was u32, migrate to empty string (will sync with server)
            if value == 0 {
                Ok(String::new())
            } else {
                // Non-zero old version -> force re-sync by returning empty string
                Ok(String::new())
            }
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            // Handle signed integers too
            if value == 0 {
                Ok(String::new())
            } else {
                Ok(String::new())
            }
        }
    }

    deserializer.deserialize_any(ManifestVersionVisitor)
}

/// Represents a single tracked replay file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TrackedReplay {
    /// SHA-256 hash of the replay file contents
    pub hash: String,
    /// Original filename
    pub filename: String,
    /// File size in bytes
    pub filesize: u64,
    /// When the replay was uploaded (Unix timestamp)
    pub uploaded_at: u64,
    /// Full path to the replay file
    pub filepath: String,
}

/// Manages the local cache of uploaded replays
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayTracker {
    /// Map of hash -> TrackedReplay
    replays: HashMap<String, TrackedReplay>,
    /// Total count of uploaded replays
    pub total_uploaded: usize,
    /// Last known server manifest version (ISO timestamp for sync detection)
    /// Backward compatible: old u32 values are migrated to empty string
    #[serde(default, deserialize_with = "deserialize_manifest_version")]
    pub manifest_version: String,
}

impl ReplayTracker {
    /// Create a new empty tracker
    pub fn new() -> Self {
        Self {
            replays: HashMap::new(),
            total_uploaded: 0,
            manifest_version: String::new(),
        }
    }

    /// Clear all tracked replays but preserve manifest version
    ///
    /// Used when server manifest version indicates the server's hash
    /// manifest has been reset or modified (e.g., bulk cleanup).
    pub fn clear(&mut self) {
        self.replays.clear();
        self.total_uploaded = 0;
        // Note: manifest_version is preserved - it will be updated
        // after syncing with the server
    }

    /// Get the stored manifest version
    pub fn get_manifest_version(&self) -> &str {
        &self.manifest_version
    }

    /// Update the stored manifest version
    pub fn set_manifest_version(&mut self, version: String) {
        self.manifest_version = version;
    }

    /// Calculate SHA-256 hash of a file
    pub fn calculate_hash(file_path: &Path) -> Result<String, String> {
        let contents = fs::read(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let mut hasher = Sha256::new();
        hasher.update(&contents);
        let result = hasher.finalize();

        Ok(format!("{:x}", result))
    }

    /// Check if a replay has been uploaded (by hash)
    pub fn is_uploaded(&self, hash: &str) -> bool {
        self.replays.contains_key(hash)
    }

    /// Check if a replay exists by filename and filesize (fallback check)
    pub fn exists_by_metadata(&self, filename: &str, filesize: u64) -> bool {
        self.replays.values().any(|r| r.filename == filename && r.filesize == filesize)
    }

    /// Add a replay to the tracker
    pub fn add_replay(&mut self, replay: TrackedReplay) {
        if !self.replays.contains_key(&replay.hash) {
            self.replays.insert(replay.hash.clone(), replay);
            self.total_uploaded = self.replays.len();
        }
    }

    /// Get all tracked replays (used by tests)
    #[allow(dead_code)]
    pub fn get_all(&self) -> Vec<&TrackedReplay> {
        self.replays.values().collect()
    }

    /// Get a tracked replay by hash (used by tests)
    #[allow(dead_code)]
    pub fn get_by_hash(&self, hash: &str) -> Option<&TrackedReplay> {
        self.replays.get(hash)
    }

    /// Load tracker from config file
    pub fn load() -> Result<Self, String> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?;
        let tracker_file = config_dir.join("ladder-legends-uploader").join("replays.json");
        Self::load_from_path(&tracker_file)
    }

    /// Load tracker from a specific file path (useful for testing).
    /// Falls back to an empty tracker if the file is corrupted or unreadable.
    pub fn load_from_path(tracker_file: &Path) -> Result<Self, String> {
        if !tracker_file.exists() {
            return Ok(Self::new());
        }

        let contents = fs::read_to_string(tracker_file)
            .map_err(|e| format!("Failed to read tracker file: {}", e))?;

        match serde_json::from_str::<ReplayTracker>(&contents) {
            Ok(tracker) => Ok(tracker),
            Err(e) => {
                eprintln!("Warning: tracker file corrupted ({}), starting fresh", e);
                Ok(Self::new())
            }
        }
    }

    /// Save tracker to a specific file path using an atomic write (temp file + rename).
    pub fn save_to_path(&self, tracker_file: &Path) -> Result<(), String> {
        let tmp_file = tracker_file.with_file_name(
            format!("{}.tmp", tracker_file.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("replays.json"))
        );
        let contents = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize tracker: {}", e))?;
        let mut tmp = fs::File::create(&tmp_file)
            .map_err(|e| format!("Failed to create temp tracker: {}", e))?;
        tmp.write_all(contents.as_bytes())
            .map_err(|e| format!("Failed to write temp tracker: {}", e))?;
        tmp.sync_all()
            .map_err(|e| format!("Failed to sync temp tracker: {}", e))?;
        drop(tmp);
        fs::rename(&tmp_file, tracker_file)
            .map_err(|e| {
                let _ = fs::remove_file(&tmp_file); // best-effort cleanup of orphaned tmp
                format!("Failed to rename tracker file: {}", e)
            })?;
        Ok(())
    }

    /// Save tracker to config file
    pub fn save(&self) -> Result<(), String> {
        let config_dir = dirs::config_dir()
            .ok_or("Could not find config directory")?;
        let app_config_dir = config_dir.join("ladder-legends-uploader");
        fs::create_dir_all(&app_config_dir)
            .map_err(|e| format!("Failed to create config directory: {}", e))?;
        self.save_to_path(&app_config_dir.join("replays.json"))
    }
}

impl Default for ReplayTracker {
    fn default() -> Self {
        Self::new()
    }
}

/// Information about a replay file found in the folder
#[derive(Debug, Clone)]
pub struct ReplayFileInfo {
    pub path: PathBuf,
    pub filename: String,
    pub filesize: u64,
    pub modified_time: SystemTime,
}

/// Scan a directory for .SC2Replay files and return file information
pub fn scan_replay_folder(folder_path: &Path) -> Result<Vec<ReplayFileInfo>, String> {
    if !folder_path.exists() {
        return Err(format!("Folder does not exist: {}", folder_path.display()));
    }

    let entries = fs::read_dir(folder_path)
        .map_err(|e| format!("Failed to read directory: {}", e))?;

    let mut replays = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();

        // Only process .SC2Replay files
        if !path.is_file() || path.extension().is_none_or(|ext| ext != "SC2Replay") {
            continue;
        }

        let filename = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();

        let metadata = entry.metadata()
            .map_err(|e| format!("Failed to get file metadata: {}", e))?;

        let filesize = metadata.len();
        let modified_time = metadata.modified()
            .map_err(|e| format!("Failed to get modified time: {}", e))?;

        replays.push(ReplayFileInfo {
            path,
            filename,
            filesize,
            modified_time,
        });
    }

    // Sort by modified time (newest first)
    replays.sort_by(|a, b| b.modified_time.cmp(&a.modified_time));

    Ok(replays)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_replay_file(dir: &Path, name: &str, contents: &[u8]) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn test_tracker_new() {
        let tracker = ReplayTracker::new();
        assert_eq!(tracker.total_uploaded, 0);
        assert!(tracker.get_all().is_empty());
    }

    #[test]
    fn test_calculate_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = create_test_replay_file(temp_dir.path(), "test.SC2Replay", b"test content");

        let hash = ReplayTracker::calculate_hash(&file_path).unwrap();
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters

        // Same content should produce same hash
        let hash2 = ReplayTracker::calculate_hash(&file_path).unwrap();
        assert_eq!(hash, hash2);
    }

    #[test]
    fn test_calculate_hash_different_content() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = create_test_replay_file(temp_dir.path(), "test1.SC2Replay", b"content1");
        let file2 = create_test_replay_file(temp_dir.path(), "test2.SC2Replay", b"content2");

        let hash1 = ReplayTracker::calculate_hash(&file1).unwrap();
        let hash2 = ReplayTracker::calculate_hash(&file2).unwrap();

        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_is_uploaded() {
        let mut tracker = ReplayTracker::new();
        let hash = "abc123";

        assert!(!tracker.is_uploaded(hash));

        tracker.add_replay(TrackedReplay {
            hash: hash.to_string(),
            filename: "test.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/path".to_string(),
        });

        assert!(tracker.is_uploaded(hash));
    }

    #[test]
    fn test_exists_by_metadata() {
        let mut tracker = ReplayTracker::new();

        tracker.add_replay(TrackedReplay {
            hash: "hash1".to_string(),
            filename: "replay1.SC2Replay".to_string(),
            filesize: 5000,
            uploaded_at: 123456789,
            filepath: "/test/replay1.SC2Replay".to_string(),
        });

        assert!(tracker.exists_by_metadata("replay1.SC2Replay", 5000));
        assert!(!tracker.exists_by_metadata("replay1.SC2Replay", 6000)); // Different size
        assert!(!tracker.exists_by_metadata("replay2.SC2Replay", 5000)); // Different name
    }

    #[test]
    fn test_add_replay() {
        let mut tracker = ReplayTracker::new();

        let replay = TrackedReplay {
            hash: "hash1".to_string(),
            filename: "test.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/path".to_string(),
        };

        tracker.add_replay(replay.clone());
        assert_eq!(tracker.total_uploaded, 1);
        assert_eq!(tracker.get_by_hash("hash1"), Some(&replay));

        // Adding same hash again should not increase count
        tracker.add_replay(replay.clone());
        assert_eq!(tracker.total_uploaded, 1);
    }

    #[test]
    fn test_get_all() {
        let mut tracker = ReplayTracker::new();

        tracker.add_replay(TrackedReplay {
            hash: "hash1".to_string(),
            filename: "replay1.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/replay1".to_string(),
        });

        tracker.add_replay(TrackedReplay {
            hash: "hash2".to_string(),
            filename: "replay2.SC2Replay".to_string(),
            filesize: 2000,
            uploaded_at: 123456790,
            filepath: "/test/replay2".to_string(),
        });

        let all = tracker.get_all();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_serialize_deserialize() {
        let mut tracker = ReplayTracker::new();

        tracker.add_replay(TrackedReplay {
            hash: "hash1".to_string(),
            filename: "test.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/path".to_string(),
        });

        let json = serde_json::to_string(&tracker).unwrap();
        let deserialized: ReplayTracker = serde_json::from_str(&json).unwrap();

        assert_eq!(tracker.total_uploaded, deserialized.total_uploaded);
        assert_eq!(tracker.get_all().len(), deserialized.get_all().len());
    }

    #[test]
    fn test_scan_replay_folder() {
        let temp_dir = TempDir::new().unwrap();

        // Create some replay files
        create_test_replay_file(temp_dir.path(), "replay1.SC2Replay", b"content1");
        create_test_replay_file(temp_dir.path(), "replay2.SC2Replay", b"content2");
        create_test_replay_file(temp_dir.path(), "notareplay.txt", b"text file");

        let replays = scan_replay_folder(temp_dir.path()).unwrap();

        // Should only find .SC2Replay files
        assert_eq!(replays.len(), 2);
        assert!(replays.iter().all(|r| r.filename.ends_with(".SC2Replay")));
    }

    #[test]
    fn test_scan_replay_folder_sorted_by_time() {
        let temp_dir = TempDir::new().unwrap();

        // Create files with slight delays to ensure different timestamps
        create_test_replay_file(temp_dir.path(), "old.SC2Replay", b"old");
        std::thread::sleep(std::time::Duration::from_millis(10));
        create_test_replay_file(temp_dir.path(), "new.SC2Replay", b"new");

        let replays = scan_replay_folder(temp_dir.path()).unwrap();

        // Newest should be first
        assert_eq!(replays[0].filename, "new.SC2Replay");
        assert_eq!(replays[1].filename, "old.SC2Replay");
    }

    #[test]
    fn test_scan_nonexistent_folder() {
        let result = scan_replay_folder(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_tracked_replay_equality() {
        let replay1 = TrackedReplay {
            hash: "hash1".to_string(),
            filename: "test.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/path".to_string(),
        };

        let replay2 = TrackedReplay {
            hash: "hash1".to_string(),
            filename: "test.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/path".to_string(),
        };

        assert_eq!(replay1, replay2);
    }

    #[test]
    fn test_load_nonexistent_tracker() {
        // Should return empty tracker if file doesn't exist
        // Use a temp directory to avoid loading real user data
        let temp_dir = TempDir::new().unwrap();
        let nonexistent_file = temp_dir.path().join("replays.json");

        let tracker = ReplayTracker::load_from_path(&nonexistent_file);
        assert!(tracker.is_ok());
        let tracker = tracker.unwrap();
        assert_eq!(tracker.total_uploaded, 0);
    }

    #[test]
    fn test_load_from_path_with_existing_file() {
        let temp_dir = TempDir::new().unwrap();
        let tracker_file = temp_dir.path().join("replays.json");

        // Create a tracker file with some data
        let json_content = r#"{
            "replays": {
                "hash1": {
                    "hash": "hash1",
                    "filename": "test.SC2Replay",
                    "filesize": 1000,
                    "uploaded_at": 123456789,
                    "filepath": "/test/path"
                }
            },
            "total_uploaded": 1
        }"#;
        fs::write(&tracker_file, json_content).unwrap();

        let tracker = ReplayTracker::load_from_path(&tracker_file);
        assert!(tracker.is_ok());
        let tracker = tracker.unwrap();
        assert_eq!(tracker.total_uploaded, 1);
        assert!(tracker.is_uploaded("hash1"));
    }

    // Tests for manifest version functionality

    #[test]
    fn test_manifest_version_default() {
        let tracker = ReplayTracker::new();
        assert_eq!(tracker.get_manifest_version(), "", "New tracker should have empty version");
    }

    #[test]
    fn test_manifest_version_set_get() {
        let mut tracker = ReplayTracker::new();
        tracker.set_manifest_version("2025-11-30T12:00:00.000Z".to_string());
        assert_eq!(tracker.get_manifest_version(), "2025-11-30T12:00:00.000Z", "Should be able to set and get version");
    }

    #[test]
    fn test_clear_preserves_manifest_version() {
        let mut tracker = ReplayTracker::new();

        // Add some replays and set version
        tracker.add_replay(TrackedReplay {
            hash: "hash1".to_string(),
            filename: "test.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/path".to_string(),
        });
        tracker.set_manifest_version("2025-11-30T12:00:00.000Z".to_string());

        assert_eq!(tracker.total_uploaded, 1);
        assert_eq!(tracker.get_manifest_version(), "2025-11-30T12:00:00.000Z");

        // Clear should remove replays but not change version
        tracker.clear();

        assert_eq!(tracker.total_uploaded, 0, "Should have no replays after clear");
        assert!(!tracker.is_uploaded("hash1"), "Should not find cleared replay");
        // Note: version is preserved but will be updated by sync logic
        assert_eq!(tracker.get_manifest_version(), "2025-11-30T12:00:00.000Z", "Version should be preserved");
    }

    #[test]
    fn test_manifest_version_serialization() {
        let mut tracker = ReplayTracker::new();
        tracker.set_manifest_version("2025-11-30T12:00:00.000Z".to_string());
        tracker.add_replay(TrackedReplay {
            hash: "hash1".to_string(),
            filename: "test.SC2Replay".to_string(),
            filesize: 1000,
            uploaded_at: 123456789,
            filepath: "/test/path".to_string(),
        });

        let json = serde_json::to_string(&tracker).unwrap();
        assert!(json.contains("manifest_version"), "JSON should contain manifest_version");
        assert!(json.contains("2025-11-30T12:00:00.000Z"), "JSON should contain the version value");

        let deserialized: ReplayTracker = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.get_manifest_version(), "2025-11-30T12:00:00.000Z", "Deserialized version should match");
        assert_eq!(deserialized.total_uploaded, 1, "Deserialized should have 1 replay");
    }

    #[test]
    fn test_manifest_version_backward_compatibility() {
        // Test that we can load old tracker files without manifest_version
        let temp_dir = TempDir::new().unwrap();
        let tracker_file = temp_dir.path().join("replays.json");

        // Create old-format tracker file (no manifest_version)
        let json_content = r#"{
            "replays": {
                "hash1": {
                    "hash": "hash1",
                    "filename": "test.SC2Replay",
                    "filesize": 1000,
                    "uploaded_at": 123456789,
                    "filepath": "/test/path"
                }
            },
            "total_uploaded": 1
        }"#;
        fs::write(&tracker_file, json_content).unwrap();

        let tracker = ReplayTracker::load_from_path(&tracker_file).unwrap();
        assert_eq!(tracker.get_manifest_version(), "", "Old format should default to empty version");
        assert_eq!(tracker.total_uploaded, 1, "Should still load replays");
    }

    #[test]
    fn test_corrupted_tracker_falls_back_to_empty() {
        let temp_dir = TempDir::new().unwrap();
        let tracker_file = temp_dir.path().join("replays.json");
        std::fs::write(&tracker_file, b"{\"replays\": {broken json").unwrap();

        // Should return empty tracker, not Err
        let result = ReplayTracker::load_from_path(&tracker_file);
        assert!(result.is_ok(), "Corrupted tracker should fall back to empty");
        assert_eq!(result.unwrap().total_uploaded, 0);
    }

    #[test]
    fn test_save_to_path_writes_valid_json() {
        let temp_dir = TempDir::new().unwrap();
        let tracker_file = temp_dir.path().join("replays.json");
        let tmp_file = temp_dir.path().join("replays.json.tmp");

        let replay = TrackedReplay {
            hash: "abc123".to_string(),
            filename: "game.SC2Replay".to_string(),
            filesize: 2048,
            uploaded_at: 1700000000,
            filepath: "/replays/game.SC2Replay".to_string(),
        };

        let mut tracker = ReplayTracker::new();
        tracker.set_manifest_version("2025-11-30T12:00:00.000Z".to_string());
        tracker.add_replay(replay.clone());

        tracker.save_to_path(&tracker_file).unwrap();

        // Existing assertion: file contains valid JSON
        let contents = std::fs::read_to_string(&tracker_file).unwrap();
        serde_json::from_str::<serde_json::Value>(&contents).expect("Should be valid JSON");

        // Round-trip: loaded tracker must match what was saved
        let loaded = ReplayTracker::load_from_path(&tracker_file).unwrap();
        assert_eq!(loaded.total_uploaded, 1, "Round-trip total_uploaded should match");
        assert_eq!(loaded.get_manifest_version(), "2025-11-30T12:00:00.000Z",
            "Round-trip manifest_version should match");
        let loaded_replay = loaded.get_by_hash("abc123")
            .expect("Round-trip replay should be present by hash");
        assert_eq!(*loaded_replay, replay, "Round-trip replay data should match exactly");

        // Tmp file must be cleaned up after a successful save
        assert!(!tmp_file.exists(), "Tmp file should not exist after successful save");
    }
}
