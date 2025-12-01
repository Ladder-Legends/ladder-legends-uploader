/**
 * API Contract Types for Ladder Legends Uploader
 *
 * These types define the exact structure of API requests/responses between
 * the Desktop Uploader (Rust) and Academy API (TypeScript/Next.js).
 *
 * IMPORTANT: These must stay in sync with Academy's TypeScript types.
 * Location in Academy: src/lib/contracts/uploader-contracts.ts
 *
 * Principles:
 * - Use explicit Option<T> instead of omitting fields
 * - Use serde attributes to match JSON format exactly
 * - Add validation where possible (e.g., non-empty strings)
 */

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =============================================================================
// Check Hashes Endpoint
// =============================================================================

/// Request to check which replay hashes are new
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckHashesRequest {
    pub hashes: Vec<HashInfo>,
}

/// Hash information for a single replay
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HashInfo {
    pub hash: String,       // SHA-256 hash (64 hex chars)
    pub filename: String,   // Original filename
    pub filesize: u64,      // File size in bytes
}

/// Response from check-hashes endpoint
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckHashesResponse {
    pub new_hashes: Vec<String>,     // Hashes that need uploading
    pub existing_count: usize,        // Count of already-uploaded replays
    pub total_submitted: usize,       // Total hashes in request
    pub manifest_version: String,     // Current manifest timestamp
}

// =============================================================================
// Upload Replay Endpoint
// =============================================================================

/// Successful replay upload response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UploadReplaySuccess {
    pub success: bool,  // Always true for this variant
    pub replay: StoredReplay,
}

/// Failed replay upload response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UploadReplayError {
    pub success: bool,  // Always false for this variant
    pub error: UploadError,
}

/// Error details from failed upload
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UploadError {
    pub code: String,       // Error code (e.g., "REPLAY_DUPLICATE")
    pub message: String,    // Human-readable error message
    pub retryable: bool,    // Whether the client should retry
}

/// Discriminated union for upload response
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum UploadReplayResponse {
    Success(UploadReplaySuccess),
    Error(UploadReplayError),
}

impl UploadReplayResponse {
    /// Check if upload was successful
    pub fn is_success(&self) -> bool {
        matches!(self, UploadReplayResponse::Success(_))
    }

    /// Get the replay if successful, None otherwise
    pub fn replay(&self) -> Option<&StoredReplay> {
        match self {
            UploadReplayResponse::Success(s) => Some(&s.replay),
            UploadReplayResponse::Error(_) => None,
        }
    }

    /// Get the error if failed, None otherwise
    pub fn error(&self) -> Option<&UploadError> {
        match self {
            UploadReplayResponse::Success(_) => None,
            UploadReplayResponse::Error(e) => Some(&e.error),
        }
    }
}

/// Stored replay data from API
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StoredReplay {
    pub id: String,
    pub discord_user_id: String,
    pub uploaded_at: String,        // ISO 8601 timestamp
    pub filename: String,
    pub fingerprint: Option<ReplayFingerprint>,
}

/// Replay fingerprint (minimal subset - full type in Academy)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReplayFingerprint {
    pub matchup: String,            // e.g., "TvZ"
    pub race: String,               // Player's race
    pub result: String,             // "Win" | "Loss"
    pub player_name: String,
    pub all_players: Vec<PlayerInfo>,
}

/// Player information in replay
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayerInfo {
    pub name: String,
    pub race: String,
    pub result: String,
}

// =============================================================================
// Manifest Version Endpoint
// =============================================================================

/// Response from /api/my-replays/manifest-version
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManifestVersionResponse {
    pub manifest_version: String,   // Timestamp of last index rebuild
    pub checked_at: String,          // ISO 8601 timestamp
}

// =============================================================================
// Device OAuth Flow
// =============================================================================

/// Request to initiate device authorization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceAuthRequest {
    pub client_id: String,
}

/// Response from device auth initiation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DeviceAuthResponse {
    pub device_code: String,         // Unique code for this device
    pub user_code: String,           // Code user enters in browser
    pub verification_uri: String,    // URL user visits
    pub expires_in: u32,             // Seconds until device_code expires
    pub interval: u32,               // Seconds between poll requests
}

/// User info from OAuth
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub avatar_url: Option<String>,
}

/// Discriminated union for device poll response
/// Uses internally tagged enum with "status" field
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum DevicePollResponse {
    Pending,
    Expired,
    Denied,
    #[serde(rename_all = "snake_case")]
    Success {
        access_token: String,
        refresh_token: String,
        token_type: String,
        expires_in: u32,
        user: Option<UserInfo>,
    },
}

impl DevicePollResponse {
    /// Check if authorization is complete
    pub fn is_success(&self) -> bool {
        matches!(self, DevicePollResponse::Success { .. })
    }

    /// Get tokens if successful
    pub fn tokens(&self) -> Option<(&str, &str)> {
        match self {
            DevicePollResponse::Success { access_token, refresh_token, .. } => {
                Some((access_token.as_str(), refresh_token.as_str()))
            }
            _ => None,
        }
    }
}

// =============================================================================
// User Settings Endpoint
// =============================================================================

/// Response from /api/settings
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserSettingsResponse {
    pub settings: UserSettings,
}

/// User settings data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserSettings {
    pub discord_user_id: String,
    pub default_race: Option<String>,
    pub favorite_builds: Vec<String>,
    pub confirmed_player_names: Vec<String>,
    pub possible_player_names: HashMap<String, u32>,
    pub created_at: String,          // ISO 8601 timestamp
    pub updated_at: String,          // ISO 8601 timestamp
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_hashes_response_deserialization() {
        let json = r#"{
            "new_hashes": ["abc123", "def456"],
            "existing_count": 5,
            "total_submitted": 7,
            "manifest_version": "2025-12-01T00:00:00Z"
        }"#;

        let response: CheckHashesResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.new_hashes.len(), 2);
        assert_eq!(response.existing_count, 5);
        assert_eq!(response.total_submitted, 7);
    }

    #[test]
    fn test_upload_success_deserialization() {
        let json = r#"{
            "success": true,
            "replay": {
                "id": "replay-123",
                "discord_user_id": "user-456",
                "uploaded_at": "2025-12-01T00:00:00Z",
                "filename": "test.SC2Replay",
                "fingerprint": null
            }
        }"#;

        let response: UploadReplayResponse = serde_json::from_str(json).unwrap();
        assert!(response.is_success());
        assert!(response.replay().is_some());
    }

    #[test]
    fn test_upload_error_deserialization() {
        let json = r#"{
            "success": false,
            "error": {
                "code": "REPLAY_DUPLICATE",
                "message": "This replay has already been uploaded",
                "retryable": false
            }
        }"#;

        let response: UploadReplayResponse = serde_json::from_str(json).unwrap();
        assert!(!response.is_success());
        assert!(response.error().is_some());
        assert_eq!(response.error().unwrap().code, "REPLAY_DUPLICATE");
    }

    #[test]
    fn test_device_poll_pending() {
        let json = r#"{"status": "pending"}"#;
        let response: DevicePollResponse = serde_json::from_str(json).unwrap();
        assert!(!response.is_success());
    }

    #[test]
    fn test_device_poll_success() {
        let json = r#"{
            "status": "success",
            "access_token": "token123",
            "refresh_token": "refresh456",
            "token_type": "Bearer",
            "expires_in": 3600,
            "user": {
                "id": "123",
                "username": "testuser",
                "avatar_url": null
            }
        }"#;

        let response: DevicePollResponse = serde_json::from_str(json).unwrap();
        assert!(response.is_success());
        let (access, refresh) = response.tokens().unwrap();
        assert_eq!(access, "token123");
        assert_eq!(refresh, "refresh456");
    }

    #[test]
    fn test_manifest_version_response() {
        let json = r#"{
            "manifest_version": "2025-12-01T15:14:56.505Z",
            "checked_at": "2025-12-01T15:15:00.000Z"
        }"#;

        let response: ManifestVersionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.manifest_version, "2025-12-01T15:14:56.505Z");
    }
}
