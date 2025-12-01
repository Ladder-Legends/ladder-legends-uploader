use crate::debug_logger::DebugLogger;
use crate::api_contracts::{
    CheckHashesRequest, CheckHashesResponse, HashInfo,
    UploadReplayResponse, ManifestVersionResponse,
    UserSettings, UserSettingsResponse, StoredReplay,
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::fs;

/// Response from get replays endpoint (used by tests)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetReplaysResponse {
    pub replays: Vec<StoredReplay>,
}

/// Error response from API (used by tests)
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// API client for replay upload operations
pub struct ReplayUploader {
    base_url: String,
    access_token: String,
    client: reqwest::Client,
    logger: Option<Arc<DebugLogger>>,
}

impl ReplayUploader {
    /// Create a new replay uploader with access token (used by tests)
    #[allow(dead_code)]
    pub fn new(base_url: String, access_token: String) -> Self {
        Self::with_logger(base_url, access_token, None)
    }

    /// Create a new replay uploader with access token and optional logger
    pub fn with_logger(base_url: String, access_token: String, logger: Option<Arc<DebugLogger>>) -> Self {
        // Create client with 60 second timeout for replay uploads
        // (analysis can take time, so we give it more time)
        // Include version in User-Agent header for tracking
        let version = env!("CARGO_PKG_VERSION");
        let user_agent = format!("LadderLegendsUploader/{}", version);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .user_agent(&user_agent)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            base_url,
            access_token,
            client,
            logger,
        }
    }

    /// Get URL for my-replays endpoint
    fn my_replays_url(&self) -> String {
        format!("{}/api/my-replays", self.base_url)
    }

    /// Upload a replay file
    pub async fn upload_replay(
        &self,
        file_path: &Path,
        player_name: Option<&str>,
        target_build_id: Option<&str>,
        game_type: Option<&str>,
        region: Option<&str>,
    ) -> Result<StoredReplay, String> {
        // Read file contents
        let file_contents = fs::read(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let filename = file_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid filename")?
            .to_string();

        // Build URL with properly encoded query params
        // Using reqwest::Url ensures special characters (Korean, symbols, etc.) are encoded
        let mut url = reqwest::Url::parse(&self.my_replays_url())
            .map_err(|e| format!("Invalid base URL: {}", e))?;

        {
            let mut query_pairs = url.query_pairs_mut();
            if let Some(build_id) = target_build_id {
                query_pairs.append_pair("target_build_id", build_id);
            }
            if let Some(name) = player_name {
                query_pairs.append_pair("player_name", name);  // Auto-encodes special chars
            }
            if let Some(gtype) = game_type {
                query_pairs.append_pair("game_type", gtype);
            }
            if let Some(r) = region {
                query_pairs.append_pair("region", r);
            }
        }

        // Create multipart form
        let part = reqwest::multipart::Part::bytes(file_contents)
            .file_name(filename);

        let form = reqwest::multipart::Form::new()
            .part("file", part);

        // Send request
        let response = self.client
            .post(url)  // reqwest::Url is accepted directly
            .bearer_auth(&self.access_token)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Upload failed {}: {}", status, error_text));
        }

        let data: UploadReplayResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        // Handle discriminated union response
        match data.replay() {
            Some(replay) => Ok(replay.clone()),
            None => {
                if let Some(error) = data.error() {
                    Err(format!("Upload failed: {} ({})", error.message, error.code))
                } else {
                    Err("Upload failed with unknown error".to_string())
                }
            }
        }
    }

    /// Check which hashes are new on the server
    pub async fn check_hashes(
        &self,
        hashes: Vec<HashInfo>,
    ) -> Result<CheckHashesResponse, String> {
        let url = format!("{}/api/my-replays/check-hashes", self.base_url);

        let request = CheckHashesRequest { hashes };

        // Log auth debug info if logger is available
        if let Some(ref logger) = self.logger {
            let token_preview = if self.access_token.len() > 20 {
                &self.access_token[..20]
            } else {
                &self.access_token
            };
            logger.debug(format!("Using access token (first 20 chars): {}...", token_preview));
            logger.debug(format!("Sending check-hashes request to: {}", url));
        }

        let response = self.client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&request)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Server error {}: {}", status, error_text));
        }

        let data: CheckHashesResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(data)
    }

    /// Get user settings (player names, preferences)
    pub async fn get_user_settings(&self) -> Result<UserSettings, String> {
        let url = format!("{}/api/settings", self.base_url);

        let response = self.client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to fetch settings {}: {}", status, error_text));
        }

        let data: UserSettingsResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse settings response: {}", e))?;

        Ok(data.settings)
    }

    /// Get manifest version from server (lightweight check for sync detection)
    ///
    /// This endpoint is edge-cached for 24 hours to minimize compute costs.
    /// The uploader should call this frequently to detect when the server's
    /// hash manifest has been modified (e.g., bulk cleanup operations).
    ///
    /// Returns:
    /// - `manifest_version`: Integer that increments when server manifest changes
    /// - `checked_at`: ISO timestamp of when server responded
    pub async fn get_manifest_version(&self) -> Result<ManifestVersionResponse, String> {
        let url = format!("{}/api/my-replays/manifest-version", self.base_url);

        if let Some(ref logger) = self.logger {
            logger.debug(format!("Fetching manifest version from: {}", url));
        }

        let response = self.client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to fetch manifest version {}: {}", status, error_text));
        }

        let data: ManifestVersionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse manifest version response: {}", e))?;

        if let Some(ref logger) = self.logger {
            logger.info(format!("Server manifest version: {}", data.manifest_version));
        }

        Ok(data)
    }

    /// Get user's replays from server (used by integration tests)
    #[allow(dead_code)]
    pub async fn get_user_replays(&self) -> Result<Vec<StoredReplay>, String> {
        let url = self.my_replays_url();

        let response = self.client
            .get(&url)
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Failed to fetch replays {}: {}", status, error_text));
        }

        let data: GetReplaysResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse replays response: {}", e))?;

        Ok(data.replays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_replay(dir: &Path, name: &str, contents: &[u8]) -> std::path::PathBuf {
        let path = dir.join(name);
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn test_replay_uploader_new() {
        let uploader = ReplayUploader::new(
            "https://example.com".to_string(),
            "test-token".to_string(),
        );

        assert_eq!(uploader.base_url, "https://example.com");
        assert_eq!(uploader.access_token, "test-token");
    }

    #[test]
    fn test_my_replays_url() {
        let uploader = ReplayUploader::new(
            "https://example.com".to_string(),
            "test-token".to_string(),
        );

        assert_eq!(uploader.my_replays_url(), "https://example.com/api/my-replays");
    }

    #[test]
    fn test_get_replays_response_deserialize() {
        let json = r#"{
            "replays": [
                {
                    "id": "abc123",
                    "discord_user_id": "123456",
                    "uploaded_at": "2024-01-01T00:00:00Z",
                    "filename": "test.SC2Replay"
                }
            ]
        }"#;

        let response: GetReplaysResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.replays.len(), 1);
        assert_eq!(response.replays[0].id, "abc123");
        assert_eq!(response.replays[0].filename, "test.SC2Replay");
    }

    #[test]
    fn test_upload_replay_response_deserialize() {
        let json = r#"{
            "success": true,
            "replay": {
                "id": "abc123",
                "discord_user_id": "123456",
                "uploaded_at": "2024-01-01T00:00:00Z",
                "filename": "test.SC2Replay",
                "fingerprint": {"matchup": "TvZ", "race": "Terran", "result": "Win", "player_name": "Test", "all_players": []}
            }
        }"#;

        let response: UploadReplayResponse = serde_json::from_str(json).unwrap();
        assert!(response.is_success());
        let replay = response.replay().unwrap();
        assert_eq!(replay.id, "abc123");
        assert_eq!(replay.filename, "test.SC2Replay");
        assert!(replay.fingerprint.is_some());
    }

    #[test]
    fn test_error_response_deserialize() {
        let json = r#"{"error": "Unauthorized"}"#;
        let response: ErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error, "Unauthorized");
    }

    #[test]
    fn test_stored_replay_serialize() {
        let replay = StoredReplay {
            id: "test-id".to_string(),
            discord_user_id: "123456".to_string(),
            uploaded_at: "2024-01-01T00:00:00Z".to_string(),
            filename: "test.SC2Replay".to_string(),
            fingerprint: None,
        };

        let json = serde_json::to_string(&replay).unwrap();
        assert!(json.contains("test-id"));
        assert!(json.contains("test.SC2Replay"));
    }

    #[test]
    fn test_stored_replay_serialize_with_fingerprint() {
        use crate::api_contracts::ReplayFingerprint;
        let fingerprint = ReplayFingerprint {
            matchup: "TvZ".to_string(),
            race: "Terran".to_string(),
            result: "Win".to_string(),
            player_name: "Test".to_string(),
            all_players: vec![],
        };
        let replay = StoredReplay {
            id: "test-id".to_string(),
            discord_user_id: "123456".to_string(),
            uploaded_at: "2024-01-01T00:00:00Z".to_string(),
            filename: "test.SC2Replay".to_string(),
            fingerprint: Some(fingerprint),
        };

        let json = serde_json::to_string(&replay).unwrap();
        assert!(json.contains("fingerprint"));
        assert!(json.contains("matchup"));
    }

    #[test]
    fn test_url_encoding_korean_player_name() {
        // Test that Korean characters are properly URL-encoded
        let base_url = "https://example.com/api/my-replays";
        let mut url = reqwest::Url::parse(base_url).unwrap();

        {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("player_name", "홍길동");  // Korean name
        }

        let url_str = url.to_string();
        // Korean chars should be percent-encoded, not raw UTF-8
        assert!(!url_str.contains("홍길동"), "Korean chars should be encoded");
        assert!(url_str.contains("player_name="), "Should have player_name param");
        // Verify it can be decoded back
        assert!(url_str.contains("%")); // Should contain percent-encoded chars
    }

    #[test]
    fn test_url_encoding_special_characters() {
        // Test that special chars like & and = are properly encoded
        let base_url = "https://example.com/api/my-replays";
        let mut url = reqwest::Url::parse(base_url).unwrap();

        {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("player_name", "Player&Name=Test");
        }

        let url_str = url.to_string();
        // & and = in the value should be encoded, not treated as query separators
        assert!(!url_str.contains("&Name"), "& should be encoded in value");
        assert!(url_str.contains("%26"), "& should be percent-encoded");
        assert!(url_str.contains("%3D"), "= should be percent-encoded");
    }

    #[test]
    fn test_url_encoding_spaces() {
        let base_url = "https://example.com/api/my-replays";
        let mut url = reqwest::Url::parse(base_url).unwrap();

        {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("player_name", "Player Name");
        }

        let url_str = url.to_string();
        // Spaces should be encoded as + or %20
        assert!(!url_str.contains(" "), "Spaces should be encoded");
        assert!(url_str.contains("+") || url_str.contains("%20"));
    }

    // Integration tests require a running server
    #[tokio::test]
    #[ignore] // Ignore by default, run with --ignored flag
    async fn test_upload_replay_integration() {
        let temp_dir = TempDir::new().unwrap();
        let replay_path = create_test_replay(
            temp_dir.path(),
            "test.SC2Replay",
            b"fake replay content"
        );

        let uploader = ReplayUploader::new(
            std::env::var("LADDER_LEGENDS_API_HOST")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            std::env::var("TEST_ACCESS_TOKEN")
                .expect("TEST_ACCESS_TOKEN env var required for integration tests"),
        );

        let result = uploader.upload_replay(&replay_path, None, None, None, None).await;

        // Don't assert success - just verify it returns a result
        match result {
            Ok(replay) => println!("✅ Upload successful: {:?}", replay),
            Err(e) => println!("❌ Upload failed: {}", e),
        }
    }

    #[tokio::test]
    #[ignore]
    async fn test_get_user_replays_integration() {
        let uploader = ReplayUploader::new(
            std::env::var("LADDER_LEGENDS_API_HOST")
                .unwrap_or_else(|_| "http://localhost:3000".to_string()),
            std::env::var("TEST_ACCESS_TOKEN")
                .expect("TEST_ACCESS_TOKEN env var required for integration tests"),
        );

        let result = uploader.get_user_replays().await;

        match result {
            Ok(replays) => println!("✅ Fetched {} replays", replays.len()),
            Err(e) => println!("❌ Fetch failed: {}", e),
        }
    }
}
