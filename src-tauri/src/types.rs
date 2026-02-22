//! Core types for authentication and user data.
//!
//! This module contains the data structures used for storing
//! authentication tokens and user profile information.

/// User profile data from Discord
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub username: String,
    pub avatar_url: Option<String>,
}

/// Authentication tokens and associated user data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AuthTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<u64>,
    pub user: Option<UserData>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_user_data_serialize() {
        let user_data = UserData {
            id: None,
            username: "TestUser".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };

        let serialized = serde_json::to_string(&user_data).unwrap();
        assert!(serialized.contains("TestUser"));
        assert!(serialized.contains("avatar.png"));
    }

    #[test]
    fn test_user_data_deserialize() {
        let json = r#"{"username":"TestUser","avatar_url":"https://example.com/avatar.png"}"#;
        let user_data: UserData = serde_json::from_str(json).unwrap();

        assert_eq!(user_data.username, "TestUser");
        assert_eq!(user_data.avatar_url, Some("https://example.com/avatar.png".to_string()));
    }

    #[test]
    fn test_user_data_deserialize_no_avatar() {
        let json = r#"{"username":"TestUser","avatar_url":null}"#;
        let user_data: UserData = serde_json::from_str(json).unwrap();

        assert_eq!(user_data.username, "TestUser");
        assert_eq!(user_data.avatar_url, None);
    }

    #[test]
    fn test_auth_tokens_serialize() {
        let auth_tokens = AuthTokens {
            access_token: "test-access-token".to_string(),
            refresh_token: Some("test-refresh-token".to_string()),
            expires_at: Some(1234567890),
            user: Some(UserData {
                id: None,
                username: "TestUser".to_string(),
                avatar_url: Some("https://example.com/avatar.png".to_string()),
            }),
        };

        let serialized = serde_json::to_string(&auth_tokens).unwrap();
        assert!(serialized.contains("test-access-token"));
        assert!(serialized.contains("test-refresh-token"));
        assert!(serialized.contains("TestUser"));
        assert!(serialized.contains("1234567890"));
    }

    #[test]
    fn test_auth_tokens_deserialize() {
        let json = r#"{
            "access_token": "test-access-token",
            "refresh_token": "test-refresh-token",
            "expires_at": 1234567890,
            "user": {
                "username": "TestUser",
                "avatar_url": "https://example.com/avatar.png"
            }
        }"#;

        let auth_tokens: AuthTokens = serde_json::from_str(json).unwrap();
        assert_eq!(auth_tokens.access_token, "test-access-token");
        assert_eq!(auth_tokens.refresh_token, Some("test-refresh-token".to_string()));
        assert_eq!(auth_tokens.expires_at, Some(1234567890));
        assert!(auth_tokens.user.is_some());

        let user = auth_tokens.user.unwrap();
        assert_eq!(user.username, "TestUser");
        assert_eq!(user.avatar_url, Some("https://example.com/avatar.png".to_string()));
    }

    #[test]
    fn test_auth_tokens_deserialize_minimal() {
        let json = r#"{
            "access_token": "test-access-token",
            "refresh_token": null,
            "expires_at": null,
            "user": null
        }"#;

        let auth_tokens: AuthTokens = serde_json::from_str(json).unwrap();
        assert_eq!(auth_tokens.access_token, "test-access-token");
        assert_eq!(auth_tokens.refresh_token, None);
        assert_eq!(auth_tokens.expires_at, None);
        assert_eq!(auth_tokens.user, None);
    }

    #[test]
    fn test_auth_tokens_clone() {
        let auth_tokens = AuthTokens {
            access_token: "test-access-token".to_string(),
            refresh_token: Some("test-refresh-token".to_string()),
            expires_at: Some(1234567890),
            user: Some(UserData {
                id: None,
                username: "TestUser".to_string(),
                avatar_url: Some("https://example.com/avatar.png".to_string()),
            }),
        };

        let cloned = auth_tokens.clone();
        assert_eq!(auth_tokens.access_token, cloned.access_token);
        assert_eq!(auth_tokens.refresh_token, cloned.refresh_token);
        assert_eq!(auth_tokens.expires_at, cloned.expires_at);
        assert_eq!(auth_tokens.user.as_ref().unwrap().username, cloned.user.as_ref().unwrap().username);
    }

    #[test]
    fn test_user_data_clone() {
        let user_data = UserData {
            id: None,
            username: "TestUser".to_string(),
            avatar_url: Some("https://example.com/avatar.png".to_string()),
        };

        let cloned = user_data.clone();
        assert_eq!(user_data.username, cloned.username);
        assert_eq!(user_data.avatar_url, cloned.avatar_url);
    }
}
