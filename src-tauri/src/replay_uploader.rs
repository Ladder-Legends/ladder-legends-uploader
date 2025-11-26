use crate::debug_logger::DebugLogger;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
use std::fs;

/// Response from /api/my-replays GET endpoint
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetReplaysResponse {
    pub replays: Vec<UserReplay>,
}

/// User replay data from API
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserReplay {
    pub id: String,
    pub discord_user_id: String,
    pub uploaded_at: String,
    pub filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<serde_json::Value>,
}

/// Response from /api/my-replays POST endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadReplayResponse {
    pub success: bool,
    pub replay: UserReplay,
}

/// Error response from API
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// Hash info for checking with server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashInfo {
    pub hash: String,
    pub filename: String,
    pub filesize: u64,
}

/// Request for checking hashes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckHashesRequest {
    pub hashes: Vec<HashInfo>,
}

/// Response from check-hashes endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckHashesResponse {
    pub new_hashes: Vec<String>,
    pub existing_count: usize,
    pub total_submitted: usize,
}

/// User settings response from /api/settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettingsResponse {
    pub settings: UserSettings,
}

/// User settings data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub discord_user_id: String,
    pub default_race: Option<String>,
    pub favorite_builds: Vec<String>,
    pub confirmed_player_names: Vec<String>,
    pub possible_player_names: std::collections::HashMap<String, u32>,
    pub created_at: String,
    pub updated_at: String,
}

/// Response from /api/my-replays/manifest-version endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestVersionResponse {
    pub manifest_version: u32,
    pub checked_at: String,
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

    /// Fetch all replays for the current user
    #[allow(dead_code)]
    pub async fn get_user_replays(&self) -> Result<Vec<UserReplay>, String> {
        let response = self.client
            .get(self.my_replays_url())
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Server error {}: {}", status, error_text));
        }

        let data: GetReplaysResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(data.replays)
    }

    /// Upload a replay file
    pub async fn upload_replay(
        &self,
        file_path: &Path,
        player_name: Option<&str>,
        target_build_id: Option<&str>,
        game_type: Option<&str>,
        region: Option<&str>,
    ) -> Result<UserReplay, String> {
        // Read file contents
        let file_contents = fs::read(file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        let filename = file_path.file_name()
            .and_then(|n| n.to_str())
            .ok_or("Invalid filename")?
            .to_string();

        // Build URL with query params
        let mut url = self.my_replays_url();
        let mut has_params = false;

        if let Some(build_id) = target_build_id {
            url.push_str(&format!("?target_build_id={}", build_id));
            has_params = true;
        }

        if let Some(name) = player_name {
            let separator = if has_params { "&" } else { "?" };
            url.push_str(&format!("{}player_name={}", separator, name));
            has_params = true;
        }

        if let Some(gtype) = game_type {
            let separator = if has_params { "&" } else { "?" };
            url.push_str(&format!("{}game_type={}", separator, gtype));
            has_params = true;
        }

        if let Some(r) = region {
            let separator = if has_params { "&" } else { "?" };
            url.push_str(&format!("{}region={}", separator, r));
        }

        // Create multipart form
        let part = reqwest::multipart::Part::bytes(file_contents)
            .file_name(filename);

        let form = reqwest::multipart::Form::new()
            .part("file", part);

        // Send request
        let response = self.client
            .post(&url)
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

        Ok(data.replay)
    }

    /// Check if a replay with given filename exists in user's replays
    #[allow(dead_code)]
    pub async fn replay_exists(&self, filename: &str) -> Result<bool, String> {
        let replays = self.get_user_replays().await?;
        Ok(replays.iter().any(|r| r.filename == filename))
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
                "fingerprint": {"some": "data"}
            }
        }"#;

        let response: UploadReplayResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.replay.id, "abc123");
        assert_eq!(response.replay.filename, "test.SC2Replay");
        assert!(response.replay.fingerprint.is_some());
    }

    #[test]
    fn test_error_response_deserialize() {
        let json = r#"{"error": "Unauthorized"}"#;
        let response: ErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error, "Unauthorized");
    }

    #[test]
    fn test_user_replay_serialize() {
        let replay = UserReplay {
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
    fn test_user_replay_serialize_with_fingerprint() {
        let fingerprint = serde_json::json!({"key": "value"});
        let replay = UserReplay {
            id: "test-id".to_string(),
            discord_user_id: "123456".to_string(),
            uploaded_at: "2024-01-01T00:00:00Z".to_string(),
            filename: "test.SC2Replay".to_string(),
            fingerprint: Some(fingerprint),
        };

        let json = serde_json::to_string(&replay).unwrap();
        assert!(json.contains("fingerprint"));
        assert!(json.contains("key"));
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
