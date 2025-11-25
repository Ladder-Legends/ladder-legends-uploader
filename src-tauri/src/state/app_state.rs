//! Application state enum representing different phases of the app lifecycle.

/// The current state of the application
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum AppState {
    /// Initial state - detecting SC2 replay folders
    DetectingFolder,
    /// SC2 replay folder found
    FolderFound { path: String },
    /// User needs to authenticate via Discord
    NeedsAuth,
    /// Showing device code for user to enter
    ShowingCode {
        user_code: String,
        device_code: String,
        verification_uri: String,
        expires_at: u64,
    },
    /// Polling for device authorization completion
    Polling { device_code: String },
    /// User is authenticated
    Authenticated {
        username: String,
        avatar_url: String,
    },
    /// An error occurred
    Error { message: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_detecting_folder_serialize() {
        let state = AppState::DetectingFolder;
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("DetectingFolder"));
    }

    #[test]
    fn test_app_state_folder_found_serialize() {
        let state = AppState::FolderFound {
            path: "/test/path".to_string(),
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("FolderFound"));
        assert!(serialized.contains("/test/path"));
    }

    #[test]
    fn test_app_state_showing_code_serialize() {
        let state = AppState::ShowingCode {
            user_code: "ABCD-1234".to_string(),
            device_code: "test-device-code".to_string(),
            verification_uri: "https://example.com".to_string(),
            expires_at: 1234567890,
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("ShowingCode"));
        assert!(serialized.contains("ABCD-1234"));
        assert!(serialized.contains("test-device-code"));
    }

    #[test]
    fn test_app_state_authenticated_serialize() {
        let state = AppState::Authenticated {
            username: "TestUser".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("Authenticated"));
        assert!(serialized.contains("TestUser"));
    }

    #[test]
    fn test_app_state_error_serialize() {
        let state = AppState::Error {
            message: "Test error message".to_string(),
        };
        let serialized = serde_json::to_string(&state).unwrap();
        assert!(serialized.contains("Error"));
        assert!(serialized.contains("Test error message"));
    }

    #[test]
    fn test_app_state_clone() {
        let state1 = AppState::DetectingFolder;
        let state2 = state1.clone();

        let serialized1 = serde_json::to_string(&state1).unwrap();
        let serialized2 = serde_json::to_string(&state2).unwrap();
        assert_eq!(serialized1, serialized2);
    }
}
