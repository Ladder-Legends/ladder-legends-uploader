//! Integration test harness for mock API servers
//!
//! This module provides utilities for running integration tests against mock servers
//! instead of requiring real infrastructure to be running.

use mockito::{Mock, Server, ServerGuard};
use serde_json::json;

/// A test harness that sets up mock API servers for integration testing
pub struct TestHarness {
    pub server: ServerGuard,
}

impl TestHarness {
    /// Create a new test harness with a mock server
    pub async fn new() -> Self {
        let server = Server::new_async().await;
        Self { server }
    }

    /// Get the mock server URL
    pub fn url(&self) -> String {
        self.server.url()
    }

    /// Mock the /api/my-replays POST endpoint for upload
    pub fn mock_upload_success(&mut self, replay_id: &str, filename: &str) -> Mock {
        self.server.mock("POST", "/api/my-replays")
            .match_header("authorization", mockito::Matcher::Regex(r"Bearer .+".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "success": true,
                "replay": {
                    "id": replay_id,
                    "discord_user_id": "test-user-123",
                    "uploaded_at": "2025-01-01T00:00:00Z",
                    "filename": filename,
                    "fingerprint": {
                        "player_name": "TestPlayer",
                        "matchup": "TvZ",
                        "race": "Terran"
                    }
                }
            }).to_string())
            .create()
    }

    /// Mock the /api/my-replays POST endpoint for failure
    pub fn mock_upload_failure(&mut self, status: usize, error_message: &str) -> Mock {
        self.server.mock("POST", "/api/my-replays")
            .with_status(status)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "error": error_message
            }).to_string())
            .create()
    }

    /// Mock the /api/my-replays GET endpoint
    pub fn mock_get_replays(&mut self, replays: Vec<serde_json::Value>) -> Mock {
        self.server.mock("GET", "/api/my-replays")
            .match_header("authorization", mockito::Matcher::Regex(r"Bearer .+".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "replays": replays
            }).to_string())
            .create()
    }

    /// Mock the /api/my-replays/check-hashes POST endpoint
    pub fn mock_check_hashes(&mut self, new_hashes: Vec<&str>, existing_count: usize) -> Mock {
        self.server.mock("POST", "/api/my-replays/check-hashes")
            .match_header("authorization", mockito::Matcher::Regex(r"Bearer .+".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "new_hashes": new_hashes,
                "existing_count": existing_count,
                "total_submitted": new_hashes.len() + existing_count
            }).to_string())
            .create()
    }

    /// Mock the /api/settings GET endpoint
    pub fn mock_get_settings(&mut self, confirmed_names: Vec<&str>, possible_names: Vec<(&str, u32)>) -> Mock {
        let possible: std::collections::HashMap<&str, u32> = possible_names.into_iter().collect();
        self.server.mock("GET", "/api/settings")
            .match_header("authorization", mockito::Matcher::Regex(r"Bearer .+".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "settings": {
                    "discord_user_id": "test-user-123",
                    "default_race": null,
                    "favorite_builds": [],
                    "confirmed_player_names": confirmed_names,
                    "possible_player_names": possible,
                    "created_at": "2025-01-01T00:00:00Z",
                    "updated_at": "2025-01-01T00:00:00Z"
                }
            }).to_string())
            .create()
    }

    /// Mock an unauthorized response (401)
    pub fn mock_unauthorized(&mut self, path: &str) -> Mock {
        self.server.mock("POST", path)
            .with_status(401)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "error": "Unauthorized"
            }).to_string())
            .create()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_uploader::ReplayUploader;
    use std::path::Path;
    use tempfile::TempDir;
    use std::fs;

    fn create_test_replay(dir: &Path, name: &str, contents: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[tokio::test]
    async fn test_upload_replay_with_mock_server() {
        let mut harness = TestHarness::new().await;
        let _mock = harness.mock_upload_success("replay-123", "test.SC2Replay");

        let temp_dir = TempDir::new().unwrap();
        let replay_path = create_test_replay(
            temp_dir.path(),
            "test.SC2Replay",
            b"fake replay content"
        );

        let uploader = ReplayUploader::new(
            harness.url(),
            "test-access-token".to_string(),
        );

        let result = uploader.upload_replay(&replay_path, None, None, None, None).await;

        assert!(result.is_ok());
        let replay = result.unwrap();
        assert_eq!(replay.id, "replay-123");
        assert_eq!(replay.filename, "test.SC2Replay");
    }

    #[tokio::test]
    async fn test_upload_replay_with_game_type_and_player() {
        let mut harness = TestHarness::new().await;
        // Mock with path matcher that accepts query parameters
        let _mock = harness.server.mock("POST", mockito::Matcher::Regex(r"/api/my-replays\?.*".to_string()))
            .match_header("authorization", mockito::Matcher::Regex(r"Bearer .+".to_string()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(json!({
                "success": true,
                "replay": {
                    "id": "replay-456",
                    "discord_user_id": "test-user-123",
                    "uploaded_at": "2025-01-01T00:00:00Z",
                    "filename": "match.SC2Replay",
                    "fingerprint": {
                        "player_name": "Lotus",
                        "matchup": "TvZ",
                        "race": "Terran"
                    }
                }
            }).to_string())
            .create();

        let temp_dir = TempDir::new().unwrap();
        let replay_path = create_test_replay(
            temp_dir.path(),
            "match.SC2Replay",
            b"ladder game content"
        );

        let uploader = ReplayUploader::new(
            harness.url(),
            "test-access-token".to_string(),
        );

        let result = uploader.upload_replay(
            &replay_path,
            Some("Lotus"),
            None,
            Some("1v1-ladder"),
            Some("NA"),
        ).await;

        assert!(result.is_ok());
        let replay = result.unwrap();
        assert_eq!(replay.id, "replay-456");
    }

    #[tokio::test]
    async fn test_upload_replay_unauthorized() {
        let mut harness = TestHarness::new().await;
        let _mock = harness.mock_unauthorized("/api/my-replays");

        let temp_dir = TempDir::new().unwrap();
        let replay_path = create_test_replay(
            temp_dir.path(),
            "test.SC2Replay",
            b"content"
        );

        let uploader = ReplayUploader::new(
            harness.url(),
            "invalid-token".to_string(),
        );

        let result = uploader.upload_replay(&replay_path, None, None, None, None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("401"));
    }

    #[tokio::test]
    async fn test_upload_replay_server_error() {
        let mut harness = TestHarness::new().await;
        let _mock = harness.mock_upload_failure(500, "Internal server error");

        let temp_dir = TempDir::new().unwrap();
        let replay_path = create_test_replay(
            temp_dir.path(),
            "test.SC2Replay",
            b"content"
        );

        let uploader = ReplayUploader::new(
            harness.url(),
            "test-token".to_string(),
        );

        let result = uploader.upload_replay(&replay_path, None, None, None, None).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("500"));
    }

    #[tokio::test]
    async fn test_get_user_replays_with_mock() {
        let mut harness = TestHarness::new().await;
        let _mock = harness.mock_get_replays(vec![
            json!({
                "id": "replay-1",
                "discord_user_id": "user-123",
                "uploaded_at": "2025-01-01T00:00:00Z",
                "filename": "game1.SC2Replay"
            }),
            json!({
                "id": "replay-2",
                "discord_user_id": "user-123",
                "uploaded_at": "2025-01-02T00:00:00Z",
                "filename": "game2.SC2Replay"
            }),
        ]);

        let uploader = ReplayUploader::new(
            harness.url(),
            "test-token".to_string(),
        );

        let result = uploader.get_user_replays().await;

        assert!(result.is_ok());
        let replays = result.unwrap();
        assert_eq!(replays.len(), 2);
        assert_eq!(replays[0].id, "replay-1");
        assert_eq!(replays[1].id, "replay-2");
    }

    #[tokio::test]
    async fn test_check_hashes_with_mock() {
        let mut harness = TestHarness::new().await;
        let _mock = harness.mock_check_hashes(
            vec!["hash1", "hash2"],
            3,
        );

        let uploader = ReplayUploader::new(
            harness.url(),
            "test-token".to_string(),
        );

        let hashes = vec![
            crate::replay_uploader::HashInfo {
                hash: "hash1".to_string(),
                filename: "game1.SC2Replay".to_string(),
                filesize: 1000,
            },
            crate::replay_uploader::HashInfo {
                hash: "hash2".to_string(),
                filename: "game2.SC2Replay".to_string(),
                filesize: 2000,
            },
        ];

        let result = uploader.check_hashes(hashes).await;

        assert!(result.is_ok());
        let response = result.unwrap();
        assert_eq!(response.new_hashes.len(), 2);
        assert_eq!(response.existing_count, 3);
    }

    #[tokio::test]
    async fn test_get_user_settings_with_mock() {
        let mut harness = TestHarness::new().await;
        let _mock = harness.mock_get_settings(
            vec!["Lotus", "LotusAlt"],
            vec![("NewPlayer", 5)],
        );

        let uploader = ReplayUploader::new(
            harness.url(),
            "test-token".to_string(),
        );

        let result = uploader.get_user_settings().await;

        assert!(result.is_ok());
        let settings = result.unwrap();
        assert_eq!(settings.confirmed_player_names.len(), 2);
        assert!(settings.confirmed_player_names.contains(&"Lotus".to_string()));
    }
}
