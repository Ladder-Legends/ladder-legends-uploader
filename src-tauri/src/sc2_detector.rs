use crate::debug_logger::DebugLogger;
use std::path::PathBuf;
use std::sync::Arc;
use std::fs;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SC2ReplayFolder {
    pub path: PathBuf,
    pub account_id: String,
    pub region: String,      // Human-readable: "NA", "EU", "KR", "CN"
    pub region_code: String, // Raw folder name: "1-S2-1-802768"
}

/// Parse region from folder name (e.g., "1-S2-1-802768" -> "NA")
fn parse_region_from_folder(folder_name: &str) -> String {
    // Region codes in SC2 folder names:
    // 1-S2-X = Americas (NA/SA)
    // 2-S2-X = Europe
    // 3-S2-X = Korea/Taiwan
    // 5-S2-X = China
    if folder_name.starts_with("1-S2-") || folder_name.starts_with("1-") {
        "NA".to_string()
    } else if folder_name.starts_with("2-S2-") || folder_name.starts_with("2-") {
        "EU".to_string()
    } else if folder_name.starts_with("3-S2-") || folder_name.starts_with("3-") {
        "KR".to_string()
    } else if folder_name.starts_with("5-S2-") || folder_name.starts_with("5-") {
        "CN".to_string()
    } else {
        "Unknown".to_string()
    }
}

/// Detect StarCraft 2 replay folder on the current platform
/// Detects ALL SC2 replay folders for all accounts
pub fn detect_all_sc2_folders(logger: Option<Arc<DebugLogger>>) -> Vec<SC2ReplayFolder> {
    #[cfg(target_os = "windows")]
    {
        detect_all_windows(logger)
    }

    #[cfg(target_os = "macos")]
    {
        detect_all_macos(logger)
    }

    #[cfg(target_os = "linux")]
    {
        detect_all_linux(logger)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        let _ = logger; // suppress unused warning
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
fn detect_all_windows(logger: Option<Arc<DebugLogger>>) -> Vec<SC2ReplayFolder> {
    // Use dirs::document_dir() to handle relocated Documents folders (OneDrive, different drives, etc.)
    if let Some(documents) = dirs::document_dir() {
        let base = documents.join("StarCraft II").join("Accounts");
        if let Some(ref log) = logger {
            log.debug(format!("Windows: Checking Documents path: {}", base.display()));
        }
        find_all_multiplayer_folders(base, logger)
    } else {
        // Fallback to hardcoded path if document_dir fails
        use std::env;
        if let Ok(username) = env::var("USERNAME") {
            let base = PathBuf::from(format!("C:\\Users\\{}\\Documents\\StarCraft II\\Accounts", username));
            if let Some(ref log) = logger {
                log.debug(format!("Windows: Fallback to hardcoded path: {}", base.display()));
            }
            find_all_multiplayer_folders(base, logger)
        } else {
            if let Some(ref log) = logger {
                log.debug("Windows: Could not determine Documents folder".to_string());
            }
            Vec::new()
        }
    }
}

#[cfg(target_os = "macos")]
fn detect_all_macos(logger: Option<Arc<DebugLogger>>) -> Vec<SC2ReplayFolder> {
    if let Some(home) = dirs::home_dir() {
        let base = home.join("Library/Application Support/Blizzard/StarCraft II/Accounts");
        find_all_multiplayer_folders(base, logger)
    } else {
        Vec::new()
    }
}

#[cfg(target_os = "linux")]
fn detect_all_linux(logger: Option<Arc<DebugLogger>>) -> Vec<SC2ReplayFolder> {
    let mut all_folders = Vec::new();

    if let Some(home) = dirs::home_dir() {
        // Try Wine/Proton paths
        let wine_path = home.join(".wine/drive_c/users");
        if wine_path.exists() {
            // Find username in wine
            if let Ok(entries) = fs::read_dir(&wine_path) {
                for entry in entries.flatten() {
                    if entry.file_type().ok().map(|ft| ft.is_dir()).unwrap_or(false) {
                        let sc2_path = entry.path().join("Documents/StarCraft II/Accounts");
                        let mut folders = find_all_multiplayer_folders(sc2_path, logger.clone());
                        all_folders.append(&mut folders);
                    }
                }
            }
        }
    }

    all_folders
}

/// Find ALL Multiplayer replays folders in the Accounts directory
fn find_all_multiplayer_folders(accounts_path: PathBuf, logger: Option<Arc<DebugLogger>>) -> Vec<SC2ReplayFolder> {
    if let Some(ref log) = logger {
        log.debug(format!("Checking accounts path: {}", accounts_path.display()));
    }
    if !accounts_path.exists() {
        if let Some(ref log) = logger {
            log.debug("Accounts path does not exist".to_string());
        }
        return Vec::new();
    }

    let Ok(account_dirs) = fs::read_dir(&accounts_path) else {
        return Vec::new();
    };

    let mut found_folders = Vec::new();

    for account_dir in account_dirs.flatten() {
        if !account_dir.file_type().ok().map(|ft| ft.is_dir()).unwrap_or(false) {
            continue;
        }

        let account_id = account_dir.file_name().to_string_lossy().to_string();
        if let Some(ref log) = logger {
            log.debug(format!("Checking account: {}", account_id));
        }

        // Look for region directories like "1-S2-1-12345"
        if let Ok(region_dirs) = fs::read_dir(account_dir.path()) {
            for region_dir in region_dirs.flatten() {
                if !region_dir.file_type().ok().map(|ft| ft.is_dir()).unwrap_or(false) {
                    continue;
                }

                let region_name = region_dir.file_name().to_string_lossy().to_string();
                if let Some(ref log) = logger {
                    log.debug(format!("Checking region: {}", region_name));
                }

                let multiplayer_path = region_dir.path().join("Replays/Multiplayer");
                if let Some(ref log) = logger {
                    log.debug(format!("Checking path: {}", multiplayer_path.display()));
                }
                if multiplayer_path.exists() {
                    let region = parse_region_from_folder(&region_name);
                    if let Some(ref log) = logger {
                        log.debug(format!("Found multiplayer folder! Region: {} ({})", region, region_name));
                    }
                    found_folders.push(SC2ReplayFolder {
                        path: multiplayer_path,
                        account_id: account_id.clone(),
                        region,
                        region_code: region_name.clone(),
                    });
                }
            }
        }
    }

    if let Some(ref log) = logger {
        if found_folders.is_empty() {
            log.debug("No multiplayer folders found".to_string());
        } else {
            log.debug(format!("Found {} multiplayer folder(s):", found_folders.len()));
            for folder in &found_folders {
                log.debug(format!("  - Account {} ({} - {}): {}",
                    folder.account_id, folder.region, folder.region_code, folder.path.display()));
            }
        }
    }

    found_folders
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper function to create a fake SC2 folder structure for testing
    fn create_fake_sc2_structure() -> TempDir {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        // Create account directory (12345678)
        let account_dir = accounts_path.join("12345678");
        fs::create_dir(&account_dir).unwrap();

        // Create region directory (1-S2-1-123456)
        let region_dir = account_dir.join("1-S2-1-123456");
        fs::create_dir(&region_dir).unwrap();

        // Create Replays/Multiplayer directory
        let multiplayer_dir = region_dir.join("Replays/Multiplayer");
        fs::create_dir_all(&multiplayer_dir).unwrap();

        // Create a fake replay file
        fs::write(multiplayer_dir.join("test.SC2Replay"), b"fake replay").unwrap();

        temp_dir
    }

    #[test]
    fn test_find_all_multiplayer_folders_valid_structure() {
        let temp_dir = create_fake_sc2_structure();
        let accounts_path = temp_dir.path().join("Accounts");

        let result = find_all_multiplayer_folders(accounts_path, None);
        assert!(!result.is_empty(), "Should find multiplayer folder in valid structure");

        let folder = &result[0];
        assert_eq!(folder.account_id, "12345678");
        assert!(folder.path.to_string_lossy().contains("Multiplayer"));
        assert_eq!(folder.region, "NA", "Should parse region as NA from 1-S2-1-* folder");
        assert_eq!(folder.region_code, "1-S2-1-123456");
    }

    #[test]
    fn test_parse_region_from_folder() {
        assert_eq!(parse_region_from_folder("1-S2-1-123456"), "NA");
        assert_eq!(parse_region_from_folder("2-S2-1-123456"), "EU");
        assert_eq!(parse_region_from_folder("3-S2-1-123456"), "KR");
        assert_eq!(parse_region_from_folder("5-S2-1-123456"), "CN");
        assert_eq!(parse_region_from_folder("unknown-format"), "Unknown");
    }

    #[test]
    fn test_find_all_multiplayer_folders_missing_accounts() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("DoesNotExist");

        let result = find_all_multiplayer_folders(non_existent, None);
        assert!(result.is_empty(), "Should return empty for non-existent path");
    }

    #[test]
    fn test_find_all_multiplayer_folders_empty_accounts() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        let result = find_all_multiplayer_folders(accounts_path, None);
        assert!(result.is_empty(), "Should return empty for empty accounts directory");
    }

    #[test]
    fn test_find_all_multiplayer_folders_no_region_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        // Create account directory but no region directories
        let account_dir = accounts_path.join("12345678");
        fs::create_dir(&account_dir).unwrap();

        let result = find_all_multiplayer_folders(accounts_path, None);
        assert!(result.is_empty(), "Should return empty when no region directories exist");
    }

    #[test]
    fn test_find_all_multiplayer_folders_no_multiplayer_dir() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        let account_dir = accounts_path.join("12345678");
        fs::create_dir(&account_dir).unwrap();

        let region_dir = account_dir.join("1-S2-1-123456");
        fs::create_dir(&region_dir).unwrap();

        // Create Replays but not Multiplayer
        let replays_dir = region_dir.join("Replays");
        fs::create_dir(&replays_dir).unwrap();

        let result = find_all_multiplayer_folders(accounts_path, None);
        assert!(result.is_empty(), "Should return empty when Multiplayer directory doesn't exist");
    }

    #[test]
    fn test_find_all_multiplayer_folders_multiple_accounts() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        // Create first account
        let account1 = accounts_path.join("11111111");
        fs::create_dir(&account1).unwrap();
        let region1 = account1.join("1-S2-1-111111");
        fs::create_dir(&region1).unwrap();
        let multi1 = region1.join("Replays/Multiplayer");
        fs::create_dir_all(&multi1).unwrap();

        // Create second account
        let account2 = accounts_path.join("22222222");
        fs::create_dir(&account2).unwrap();
        let region2 = account2.join("2-S2-1-222222");
        fs::create_dir(&region2).unwrap();
        let multi2 = region2.join("Replays/Multiplayer");
        fs::create_dir_all(&multi2).unwrap();

        let result = find_all_multiplayer_folders(accounts_path, None);
        assert_eq!(result.len(), 2, "Should find both multiplayer folders");

        let account_ids: Vec<String> = result.iter().map(|f| f.account_id.clone()).collect();
        assert!(account_ids.contains(&"11111111".to_string()));
        assert!(account_ids.contains(&"22222222".to_string()));
    }

    #[test]
    fn test_find_all_multiplayer_folders() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        // Create three accounts
        let account1 = accounts_path.join("11111111");
        fs::create_dir(&account1).unwrap();
        let region1 = account1.join("1-S2-1-111111");
        fs::create_dir(&region1).unwrap();
        let multi1 = region1.join("Replays/Multiplayer");
        fs::create_dir_all(&multi1).unwrap();

        let account2 = accounts_path.join("22222222");
        fs::create_dir(&account2).unwrap();
        let region2 = account2.join("2-S2-1-222222");
        fs::create_dir(&region2).unwrap();
        let multi2 = region2.join("Replays/Multiplayer");
        fs::create_dir_all(&multi2).unwrap();

        let account3 = accounts_path.join("33333333");
        fs::create_dir(&account3).unwrap();
        let region3 = account3.join("3-S2-1-333333");
        fs::create_dir(&region3).unwrap();
        let multi3 = region3.join("Replays/Multiplayer");
        fs::create_dir_all(&multi3).unwrap();

        let result = find_all_multiplayer_folders(accounts_path, None);
        assert_eq!(result.len(), 3, "Should find all 3 multiplayer folders");

        // Check that all account IDs are present
        let account_ids: Vec<String> = result.iter().map(|f| f.account_id.clone()).collect();
        assert!(account_ids.contains(&"11111111".to_string()));
        assert!(account_ids.contains(&"22222222".to_string()));
        assert!(account_ids.contains(&"33333333".to_string()));
    }

    #[test]
    fn test_sc2_replay_folder_clone() {
        let folder = SC2ReplayFolder {
            path: PathBuf::from("/test/path"),
            account_id: "12345678".to_string(),
            region: "NA".to_string(),
            region_code: "1-S2-1-123456".to_string(),
        };

        let cloned = folder.clone();
        assert_eq!(folder.path, cloned.path);
        assert_eq!(folder.account_id, cloned.account_id);
        assert_eq!(folder.region, cloned.region);
        assert_eq!(folder.region_code, cloned.region_code);
    }

    #[test]
    fn test_sc2_replay_folder_serialize() {
        let folder = SC2ReplayFolder {
            path: PathBuf::from("/test/path"),
            account_id: "12345678".to_string(),
            region: "EU".to_string(),
            region_code: "2-S2-1-654321".to_string(),
        };

        let serialized = serde_json::to_string(&folder).unwrap();
        assert!(serialized.contains("path"));
        assert!(serialized.contains("account_id"));
        assert!(serialized.contains("12345678"));
        assert!(serialized.contains("region"));
        assert!(serialized.contains("EU"));
        assert!(serialized.contains("region_code"));
        assert!(serialized.contains("2-S2-1-654321"));
    }

    // Integration test: Real detection (platform-specific)
    #[test]
    fn test_real_detection() {
        // This test will work on systems that actually have SC2 installed
        let result = detect_all_sc2_folders(None);
        // Don't assert - just log for manual verification
        println!("Real SC2 folder detection result: {:?}", result);
    }
}
