//! Configuration file utilities
//!
//! Provides helper functions for reading and writing app configuration files.
//! All config files are stored in the platform-specific config directory
//! under "ladder-legends-uploader/".

use serde::{de::DeserializeOwned, Serialize};
use std::fs;
use std::path::PathBuf;

const APP_DIR_NAME: &str = "ladder-legends-uploader";

/// Get the app's config directory path.
///
/// Returns: `~/.config/ladder-legends-uploader` (Linux)
///          `~/Library/Application Support/ladder-legends-uploader` (macOS)
///          `C:\Users\<User>\AppData\Roaming\ladder-legends-uploader` (Windows)
pub fn get_config_dir() -> Result<PathBuf, String> {
    let config_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?;
    Ok(config_dir.join(APP_DIR_NAME))
}

/// Get the directory where debug log files are written.
///
/// Returns: `~/.ladder-legends-uploader/logs`
///
/// This mirrors the path used by `DebugLogger::save_report_to_file` and must
/// stay in sync with that function if the log location ever changes.
pub fn get_logs_dir() -> Result<PathBuf, String> {
    let home_dir = dirs::home_dir()
        .ok_or_else(|| "Could not find home directory".to_string())?;
    Ok(home_dir.join(format!(".{}", APP_DIR_NAME)).join("logs"))
}

/// Get the full path to a config file.
pub fn config_file_path(filename: &str) -> Result<PathBuf, String> {
    Ok(get_config_dir()?.join(filename))
}

/// Ensure the config directory exists.
pub fn ensure_config_dir() -> Result<PathBuf, String> {
    let dir = get_config_dir()?;
    fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;
    Ok(dir)
}

/// Save data to a config file as JSON.
///
/// # Arguments
/// * `filename` - Name of the config file (e.g., "config.json")
/// * `data` - Data to serialize and save
///
/// # Returns
/// The path where the file was saved
pub fn save_config_file<T: Serialize>(filename: &str, data: &T) -> Result<PathBuf, String> {
    let config_dir = ensure_config_dir()?;
    let config_file = config_dir.join(filename);

    let json = serde_json::to_string_pretty(data)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&config_file, json)
        .map_err(|e| format!("Failed to write config file: {}", e))?;

    Ok(config_file)
}

/// Load data from a config file.
///
/// # Arguments
/// * `filename` - Name of the config file (e.g., "config.json")
///
/// # Returns
/// * `Ok(Some(data))` if file exists and was parsed successfully
/// * `Ok(None)` if file doesn't exist
/// * `Err(...)` if file exists but couldn't be read/parsed
pub fn load_config_file<T: DeserializeOwned>(filename: &str) -> Result<Option<T>, String> {
    let config_file = config_file_path(filename)?;

    if !config_file.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&config_file)
        .map_err(|e| format!("Failed to read config file: {}", e))?;

    let data = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse config file: {}", e))?;

    Ok(Some(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests use the actual config_dir() which may not work in CI.
    // In a real test setup, we'd use dependency injection for the config path.

    #[test]
    fn test_get_config_dir_returns_path() {
        let result = get_config_dir();
        assert!(result.is_ok(), "Should return a config directory");
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains(APP_DIR_NAME));
    }

    #[test]
    fn test_config_file_path_includes_filename() {
        let result = config_file_path("test.json");
        assert!(result.is_ok());
        let path = result.unwrap();
        assert!(path.to_string_lossy().contains("test.json"));
    }
}
