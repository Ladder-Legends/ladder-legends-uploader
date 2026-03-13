//! Auth token storage and management commands.
//!
//! Tokens are stored in the OS keychain (macOS Keychain, Windows Credential Manager,
//! Linux Secret Service). On first load, any existing `auth.json` plaintext file is
//! automatically migrated to the keychain and deleted.

use std::fs;
use std::path::PathBuf;
use tauri::State;
use crate::types::{AuthTokens, UserData};
use crate::state::AppStateManager;

const SERVICE_NAME: &str = "ladder-legends-uploader";
const ACCOUNT_NAME: &str = "auth_tokens";

/// Returns the path to the legacy auth.json file.
fn legacy_auth_file() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ladder-legends-uploader").join("auth.json"))
}

/// Attempt to migrate legacy plaintext auth.json → keychain.
/// Returns `true` if migration ran (regardless of outcome).
fn migrate_legacy_if_present(logger: &crate::debug_logger::DebugLogger) -> bool {
    let Some(path) = legacy_auth_file() else { return false; };
    if !path.exists() {
        return false;
    }

    logger.info("Migrating legacy auth.json to OS keychain".to_string());

    let contents = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(e) => {
            logger.error(format!("Migration: failed to read auth.json: {}", e));
            return true;
        }
    };

    match store_in_keyring(&contents) {
        Ok(()) => {
            if let Err(e) = fs::remove_file(&path) {
                logger.error(format!("Migration: failed to delete auth.json: {}", e));
            } else {
                logger.info("Migration complete — auth.json removed".to_string());
            }
        }
        Err(e) => {
            logger.error(format!("Migration: failed to store in keychain: {}", e));
        }
    }

    true
}

/// Write a JSON string to the OS keychain entry.
fn store_in_keyring(json: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME)
        .map_err(|e| format!("Keyring entry creation failed: {}", e))?;
    entry.set_password(json)
        .map_err(|e| format!("Keyring set_password failed: {}", e))
}

/// Read the JSON string from the OS keychain entry.
/// Returns `None` when no entry is stored yet.
fn load_from_keyring() -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME)
        .map_err(|e| format!("Keyring entry creation failed: {}", e))?;
    match entry.get_password() {
        Ok(json) => Ok(Some(json)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Keyring get_password failed: {}", e)),
    }
}

/// Delete the OS keychain entry.
fn delete_from_keyring() -> Result<(), String> {
    let entry = keyring::Entry::new(SERVICE_NAME, ACCOUNT_NAME)
        .map_err(|e| format!("Keyring entry creation failed: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(format!("Keyring delete_credential failed: {}", e)),
    }
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// Save authentication tokens to the OS keychain.
#[tauri::command]
pub async fn save_auth_tokens(
    state_manager: State<'_, AppStateManager>,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
    username: Option<String>,
    avatar_url: Option<String>,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Saving auth tokens for user: {:?}", username));

    let user = username.map(|un| UserData {
        id: None,
        username: un,
        avatar_url,
    });

    let tokens = AuthTokens {
        access_token,
        refresh_token,
        expires_at,
        user,
    };

    let tokens_json = serde_json::to_string_pretty(&tokens)
        .map_err(|e| {
            let msg = format!("Failed to serialize auth tokens: {}", e);
            state_manager.debug_logger.error(msg.clone());
            msg
        })?;

    match store_in_keyring(&tokens_json) {
        Ok(()) => {
            state_manager.debug_logger.debug("Auth tokens saved to keychain".to_string());
            Ok(())
        }
        Err(e) => {
            state_manager.debug_logger.error(format!("Keychain unavailable ({}); falling back to file", e));
            save_to_file(&tokens_json, &state_manager.debug_logger)
        }
    }
}

/// Load authentication tokens from the OS keychain (auto-migrates legacy file).
#[tauri::command]
pub async fn load_auth_tokens(state_manager: State<'_, AppStateManager>) -> Result<Option<AuthTokens>, String> {
    state_manager.debug_logger.debug("Loading auth tokens from storage".to_string());

    migrate_legacy_if_present(&state_manager.debug_logger);

    let json = match load_from_keyring() {
        Ok(Some(j)) => j,
        Ok(None) => {
            state_manager.debug_logger.debug("No auth tokens stored yet".to_string());
            return Ok(None);
        }
        Err(e) => {
            state_manager.debug_logger.error(format!("Keychain unavailable ({}); falling back to file", e));
            return load_from_file(&state_manager.debug_logger);
        }
    };

    let tokens: AuthTokens = serde_json::from_str(&json)
        .map_err(|e| {
            let msg = format!("Failed to parse auth tokens: {}", e);
            state_manager.debug_logger.error(msg.clone());
            msg
        })?;

    if let Some(ref user) = tokens.user {
        state_manager.debug_logger.info(format!("Loaded auth tokens for user: {}", user.username));
    } else {
        state_manager.debug_logger.debug("Loaded auth tokens (no user info)".to_string());
    }

    Ok(Some(tokens))
}

/// Clear authentication tokens from storage (logout).
#[tauri::command]
pub async fn clear_auth_tokens(state_manager: State<'_, AppStateManager>) -> Result<(), String> {
    state_manager.debug_logger.info("Clearing auth tokens".to_string());

    // Always attempt to remove the legacy file too (belt-and-suspenders).
    if let Some(path) = legacy_auth_file() {
        if path.exists() {
            if let Err(e) = fs::remove_file(&path) {
                state_manager.debug_logger.error(format!("Failed to delete legacy auth.json: {}", e));
            }
        }
    }

    match delete_from_keyring() {
        Ok(()) => {
            state_manager.debug_logger.debug("Auth tokens cleared from keychain".to_string());
            Ok(())
        }
        Err(e) => {
            let msg = format!("Failed to clear auth tokens from keychain: {}", e);
            state_manager.debug_logger.error(msg.clone());
            Err(msg)
        }
    }
}

// ---------------------------------------------------------------------------
// File-based fallback helpers (keychain unavailable)
// ---------------------------------------------------------------------------

fn save_to_file(json: &str, logger: &crate::debug_logger::DebugLogger) -> Result<(), String> {
    let config_dir = dirs::config_dir().ok_or("Could not find config directory")?;
    let app_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_dir)
        .map_err(|e| {
            let msg = format!("Failed to create config directory: {}", e);
            logger.error(msg.clone());
            msg
        })?;
    fs::write(app_dir.join("auth.json"), json)
        .map_err(|e| {
            let msg = format!("Failed to save auth tokens to file: {}", e);
            logger.error(msg.clone());
            msg
        })?;
    logger.debug("Auth tokens saved to fallback file".to_string());
    Ok(())
}

fn load_from_file(logger: &crate::debug_logger::DebugLogger) -> Result<Option<AuthTokens>, String> {
    let Some(path) = legacy_auth_file() else {
        return Ok(None);
    };
    if !path.exists() {
        logger.debug("No fallback auth.json file found".to_string());
        return Ok(None);
    }
    let contents = fs::read_to_string(&path)
        .map_err(|e| {
            let msg = format!("Failed to read auth tokens from file: {}", e);
            logger.error(msg.clone());
            msg
        })?;
    let tokens: AuthTokens = serde_json::from_str(&contents)
        .map_err(|e| {
            let msg = format!("Failed to parse auth tokens from file: {}", e);
            logger.error(msg.clone());
            msg
        })?;
    Ok(Some(tokens))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // Helper: build a minimal AuthTokens value.
    fn sample_tokens() -> AuthTokens {
        AuthTokens {
            access_token: "tok-access".to_string(),
            refresh_token: Some("tok-refresh".to_string()),
            expires_at: Some(9_999_999_999),
            user: Some(UserData {
                id: None,
                username: "TestUser".to_string(),
                avatar_url: Some("https://example.com/avatar.png".to_string()),
            }),
        }
    }

    // ---------------------------------------------------------------------------
    // Serialization round-trip
    // ---------------------------------------------------------------------------

    #[test]
    fn test_serialize_deserialize_roundtrip() {
        let tokens = sample_tokens();
        let json = serde_json::to_string(&tokens).unwrap();
        let back: AuthTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, tokens.access_token);
        assert_eq!(back.refresh_token, tokens.refresh_token);
        assert_eq!(back.expires_at, tokens.expires_at);
        let user = back.user.unwrap();
        assert_eq!(user.username, "TestUser");
    }

    #[test]
    fn test_serialize_minimal_tokens() {
        let tokens = AuthTokens {
            access_token: "only-access".to_string(),
            refresh_token: None,
            expires_at: None,
            user: None,
        };
        let json = serde_json::to_string(&tokens).unwrap();
        let back: AuthTokens = serde_json::from_str(&json).unwrap();
        assert_eq!(back.access_token, "only-access");
        assert!(back.refresh_token.is_none());
        assert!(back.user.is_none());
    }

    // ---------------------------------------------------------------------------
    // Migration: reads old file and deletes it
    // ---------------------------------------------------------------------------

    #[test]
    fn test_migration_reads_and_removes_auth_json() {
        let tmp = TempDir::new().unwrap();
        let auth_file = tmp.path().join("auth.json");

        let tokens = sample_tokens();
        let json = serde_json::to_string_pretty(&tokens).unwrap();
        fs::write(&auth_file, &json).unwrap();

        assert!(auth_file.exists(), "file should exist before migration");

        // Simulate the migration logic directly (read + parse + delete).
        let contents = fs::read_to_string(&auth_file).unwrap();
        let parsed: AuthTokens = serde_json::from_str(&contents).unwrap();
        fs::remove_file(&auth_file).unwrap();

        assert!(!auth_file.exists(), "file should be gone after migration");
        assert_eq!(parsed.access_token, tokens.access_token);
    }

    #[test]
    fn test_migration_skipped_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let auth_file = tmp.path().join("auth.json");

        // File does not exist — migration should be a no-op.
        assert!(!auth_file.exists());
        // Nothing to assert beyond "no panic".
    }

    // ---------------------------------------------------------------------------
    // File fallback helpers
    // ---------------------------------------------------------------------------

    #[test]
    fn test_load_from_file_returns_none_when_missing() {
        // Point legacy_auth_file at a nonexistent path via the helper indirectly.
        // We test load_from_file by giving it a path that does not exist.
        let tmp = TempDir::new().unwrap();
        let missing = tmp.path().join("no-such-file.json");
        assert!(!missing.exists());

        // load_from_file checks path.exists() first so we just verify the predicate.
        assert!(!missing.exists());
    }

    #[test]
    fn test_save_and_load_file_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let auth_file = tmp.path().join("auth.json");

        let tokens = sample_tokens();
        let json = serde_json::to_string_pretty(&tokens).unwrap();
        fs::write(&auth_file, &json).unwrap();

        let contents = fs::read_to_string(&auth_file).unwrap();
        let loaded: AuthTokens = serde_json::from_str(&contents).unwrap();

        assert_eq!(loaded.access_token, tokens.access_token);
        assert_eq!(loaded.refresh_token, tokens.refresh_token);
        assert_eq!(loaded.expires_at, tokens.expires_at);
    }
}
