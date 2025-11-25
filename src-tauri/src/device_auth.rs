use serde::{Deserialize, Serialize};
use std::env;

/// API client for Ladder Legends Academy
pub struct ApiClient {
    base_url: String,
    client: reqwest::Client,
}

impl ApiClient {
    /// Create a new API client
    pub fn new() -> Self {
        // Priority: runtime env var > compile-time env var > production default
        // This allows maximum flexibility:
        // - Runtime: LADDER_LEGENDS_API_HOST=http://localhost:3000 ./app
        // - Compile: LADDER_LEGENDS_API_HOST=http://localhost:3000 cargo build
        // - Default: https://www.ladderlegendsacademy.com
        let base_url = env::var("LADDER_LEGENDS_API_HOST")
            .ok()
            .or_else(|| option_env!("LADDER_LEGENDS_API_HOST").map(String::from))
            .unwrap_or_else(|| "https://www.ladderlegendsacademy.com".to_string());

        Self {
            base_url,
            client: reqwest::Client::new(),
        }
    }

    /// Get the device auth base URL
    fn device_auth_url(&self, path: &str) -> String {
        format!("{}/api/auth/device/{}", self.base_url, path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserData {
    pub id: String,
    pub username: String,
    pub avatar_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user: UserData,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: Option<String>,
}

impl ApiClient {
    /// Request a device code from the server
    pub async fn request_device_code(&self) -> Result<DeviceCodeResponse, String> {
        let response = self.client
            .post(self.device_auth_url("code"))
            .json(&serde_json::json!({
                "client_id": "ladder-legends-uploader"
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            return Err(format!("Server error: {}", response.status()));
        }

        let device_code: DeviceCodeResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(device_code)
    }

    /// Poll for authorization status (single check, no automatic retry)
    pub async fn poll_authorization(&self, device_code: &str) -> Result<AuthResponse, String> {
        let response = self.client
            .get(self.device_auth_url("poll"))
            .query(&[("device_code", device_code)])
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        match response.status().as_u16() {
            200 => {
                let auth: AuthResponse = response
                    .json()
                    .await
                    .map_err(|e| format!("Failed to parse response: {}", e))?;
                Ok(auth)
            }
            428 => Err("pending".to_string()),
            410 => Err("expired".to_string()),
            403 => Err("denied".to_string()),
            _ => {
                Err(format!("Server error: {}", response.status()))
            }
        }
    }

    /// Refresh an access token
    #[allow(dead_code)]
    pub async fn refresh_token(&self, refresh_token: &str) -> Result<String, String> {
        let response = self.client
            .post(self.device_auth_url("refresh"))
            .json(&serde_json::json!({
                "refresh_token": refresh_token
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            return Err("Failed to refresh token".to_string());
        }

        #[derive(Deserialize)]
        struct RefreshResponse {
            access_token: String,
        }

        let refresh_resp: RefreshResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(refresh_resp.access_token)
    }

    /// Verify an access token
    pub async fn verify_token(&self, access_token: &str) -> Result<bool, String> {
        let response = self.client
            .post(self.device_auth_url("verify"))
            .json(&serde_json::json!({
                "access_token": access_token
            }))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        #[derive(Deserialize)]
        struct VerifyResponse {
            valid: bool,
        }

        let verify_resp: VerifyResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(verify_resp.valid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_new_default_url() {
        let client = ApiClient::new();
        // When LADDER_LEGENDS_API_HOST is not set, should use production URL
        assert!(
            client.base_url == "https://www.ladderlegendsacademy.com" ||
            client.base_url.starts_with("http://localhost") ||
            client.base_url.starts_with("http://127.0.0.1"),
            "Should use production URL or localhost if env var is set"
        );
    }

    #[test]
    fn test_device_auth_url_formatting() {
        let client = ApiClient {
            base_url: "https://example.com".to_string(),
            client: reqwest::Client::new(),
        };

        assert_eq!(
            client.device_auth_url("code"),
            "https://example.com/api/auth/device/code"
        );

        assert_eq!(
            client.device_auth_url("poll"),
            "https://example.com/api/auth/device/poll"
        );

        assert_eq!(
            client.device_auth_url("refresh"),
            "https://example.com/api/auth/device/refresh"
        );
    }

    #[test]
    fn test_device_code_response_serialize() {
        let response = DeviceCodeResponse {
            device_code: "test-device-code".to_string(),
            user_code: "ABCD-1234".to_string(),
            verification_uri: "https://example.com/activate?code=ABCD-1234".to_string(),
            expires_in: 900,
            interval: 5,
        };

        let serialized = serde_json::to_string(&response).unwrap();
        assert!(serialized.contains("test-device-code"));
        assert!(serialized.contains("ABCD-1234"));
        assert!(serialized.contains("900"));
        assert!(serialized.contains("5"));
    }

    #[test]
    fn test_device_code_response_deserialize() {
        let json = r#"{
            "device_code": "test-device-code",
            "user_code": "ABCD-1234",
            "verification_uri": "https://example.com/activate",
            "expires_in": 900,
            "interval": 5
        }"#;

        let response: DeviceCodeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.device_code, "test-device-code");
        assert_eq!(response.user_code, "ABCD-1234");
        assert_eq!(response.expires_in, 900);
        assert_eq!(response.interval, 5);
    }

    #[test]
    fn test_user_data_serialize() {
        let user = UserData {
            id: "123456".to_string(),
            username: "TestUser".to_string(),
            avatar_url: "https://example.com/avatar.png".to_string(),
        };

        let serialized = serde_json::to_string(&user).unwrap();
        assert!(serialized.contains("123456"));
        assert!(serialized.contains("TestUser"));
        assert!(serialized.contains("avatar.png"));
    }

    #[test]
    fn test_auth_response_deserialize() {
        let json = r#"{
            "access_token": "test-access-token",
            "refresh_token": "test-refresh-token",
            "token_type": "Bearer",
            "expires_in": 3600,
            "user": {
                "id": "123456",
                "username": "TestUser",
                "avatar_url": "https://example.com/avatar.png"
            }
        }"#;

        let response: AuthResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.access_token, "test-access-token");
        assert_eq!(response.refresh_token, "test-refresh-token");
        assert_eq!(response.token_type, "Bearer");
        assert_eq!(response.expires_in, 3600);
        assert_eq!(response.user.id, "123456");
        assert_eq!(response.user.username, "TestUser");
    }

    #[test]
    fn test_error_response_deserialize() {
        let json = r#"{
            "error": "invalid_code",
            "message": "The code is invalid or expired"
        }"#;

        let response: ErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error, "invalid_code");
        assert_eq!(response.message, Some("The code is invalid or expired".to_string()));
    }

    #[test]
    fn test_error_response_deserialize_no_message() {
        let json = r#"{
            "error": "invalid_code"
        }"#;

        let response: ErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error, "invalid_code");
        assert_eq!(response.message, None);
    }

    #[test]
    fn test_device_code_response_clone() {
        let response = DeviceCodeResponse {
            device_code: "test-code".to_string(),
            user_code: "ABCD-1234".to_string(),
            verification_uri: "https://example.com".to_string(),
            expires_in: 900,
            interval: 5,
        };

        let cloned = response.clone();
        assert_eq!(response.device_code, cloned.device_code);
        assert_eq!(response.user_code, cloned.user_code);
        assert_eq!(response.expires_in, cloned.expires_in);
    }

    #[test]
    fn test_user_data_clone() {
        let user = UserData {
            id: "123".to_string(),
            username: "Test".to_string(),
            avatar_url: "https://example.com".to_string(),
        };

        let cloned = user.clone();
        assert_eq!(user.id, cloned.id);
        assert_eq!(user.username, cloned.username);
        assert_eq!(user.avatar_url, cloned.avatar_url);
    }

    #[test]
    fn test_auth_response_clone() {
        let response = AuthResponse {
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: 3600,
            user: UserData {
                id: "123".to_string(),
                username: "Test".to_string(),
                avatar_url: "https://example.com".to_string(),
            },
        };

        let cloned = response.clone();
        assert_eq!(response.access_token, cloned.access_token);
        assert_eq!(response.user.id, cloned.user.id);
    }

    #[test]
    fn test_verify_response_deserialize_valid() {
        let json = r#"{"valid": true, "userId": "123", "expires_at": 1234567890}"#;

        #[derive(Deserialize)]
        struct VerifyResponse {
            valid: bool,
        }

        let response: VerifyResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.valid, true);
    }

    #[test]
    fn test_verify_response_deserialize_invalid() {
        let json = r#"{"valid": false, "error": "token_expired"}"#;

        #[derive(Deserialize)]
        struct VerifyResponse {
            valid: bool,
        }

        let response: VerifyResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.valid, false);
    }
}
