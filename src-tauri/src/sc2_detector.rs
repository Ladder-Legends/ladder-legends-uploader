use std::path::PathBuf;
use std::fs;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SC2ReplayFolder {
    pub path: PathBuf,
    pub account_id: String,
}

/// Detect StarCraft 2 replay folder on the current platform
/// Detects ALL SC2 replay folders for all accounts
pub fn detect_all_sc2_folders() -> Vec<SC2ReplayFolder> {
    #[cfg(target_os = "windows")]
    {
        detect_all_windows()
    }

    #[cfg(target_os = "macos")]
    {
        detect_all_macos()
    }

    #[cfg(target_os = "linux")]
    {
        detect_all_linux()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        Vec::new()
    }
}

/// Detects first SC2 replay folder (for backwards compat)
pub fn detect_sc2_folder() -> Option<SC2ReplayFolder> {
    detect_all_sc2_folders().into_iter().next()
}

#[cfg(target_os = "windows")]
fn detect_all_windows() -> Vec<SC2ReplayFolder> {
    use std::env;

    if let Ok(username) = env::var("USERNAME") {
        let base = PathBuf::from(format!("C:\\Users\\{}\\Documents\\StarCraft II\\Accounts", username));
        find_all_multiplayer_folders(base)
    } else {
        Vec::new()
    }
}

#[cfg(target_os = "windows")]
fn detect_windows() -> Option<SC2ReplayFolder> {
    detect_all_windows().into_iter().next()
}

#[cfg(target_os = "macos")]
fn detect_all_macos() -> Vec<SC2ReplayFolder> {
    if let Some(home) = dirs::home_dir() {
        let base = home.join("Library/Application Support/Blizzard/StarCraft II/Accounts");
        find_all_multiplayer_folders(base)
    } else {
        Vec::new()
    }
}

#[cfg(target_os = "macos")]
fn detect_macos() -> Option<SC2ReplayFolder> {
    detect_all_macos().into_iter().next()
}

#[cfg(target_os = "linux")]
fn detect_all_linux() -> Vec<SC2ReplayFolder> {
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
                        let mut folders = find_all_multiplayer_folders(sc2_path);
                        all_folders.append(&mut folders);
                    }
                }
            }
        }
    }

    all_folders
}

#[cfg(target_os = "linux")]
fn detect_linux() -> Option<SC2ReplayFolder> {
    detect_all_linux().into_iter().next()
}

/// Find ALL Multiplayer replays folders in the Accounts directory
fn find_all_multiplayer_folders(accounts_path: PathBuf) -> Vec<SC2ReplayFolder> {
    println!("[DEBUG] Checking accounts path: {}", accounts_path.display());
    if !accounts_path.exists() {
        println!("[DEBUG] Accounts path does not exist");
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
        println!("[DEBUG] Checking account: {}", account_id);

        // Look for region directories like "1-S2-1-12345"
        if let Ok(region_dirs) = fs::read_dir(account_dir.path()) {
            for region_dir in region_dirs.flatten() {
                if !region_dir.file_type().ok().map(|ft| ft.is_dir()).unwrap_or(false) {
                    continue;
                }

                let region_name = region_dir.file_name().to_string_lossy().to_string();
                println!("[DEBUG] Checking region: {}", region_name);

                let multiplayer_path = region_dir.path().join("Replays/Multiplayer");
                println!("[DEBUG] Checking path: {}", multiplayer_path.display());
                if multiplayer_path.exists() {
                    println!("[DEBUG] Found multiplayer folder!");
                    found_folders.push(SC2ReplayFolder {
                        path: multiplayer_path,
                        account_id: account_id.clone(),
                    });
                }
            }
        }
    }

    if found_folders.is_empty() {
        println!("[DEBUG] No multiplayer folders found");
    } else if found_folders.len() > 1 {
        println!("[DEBUG] Found {} multiplayer folders across multiple accounts:", found_folders.len());
        for folder in &found_folders {
            println!("[DEBUG]   - Account {}: {}", folder.account_id, folder.path.display());
        }
    }

    found_folders
}

/// Find first Multiplayer replays folder (for backwards compat)
fn find_multiplayer_folder(accounts_path: PathBuf) -> Option<SC2ReplayFolder> {
    find_all_multiplayer_folders(accounts_path).into_iter().next()
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
    fn test_find_multiplayer_folder_valid_structure() {
        let temp_dir = create_fake_sc2_structure();
        let accounts_path = temp_dir.path().join("Accounts");

        let result = find_multiplayer_folder(accounts_path);
        assert!(result.is_some(), "Should find multiplayer folder in valid structure");

        let folder = result.unwrap();
        assert_eq!(folder.account_id, "12345678");
        assert!(folder.path.to_string_lossy().contains("Multiplayer"));
    }

    #[test]
    fn test_find_multiplayer_folder_missing_accounts() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("DoesNotExist");

        let result = find_multiplayer_folder(non_existent);
        assert!(result.is_none(), "Should return None for non-existent path");
    }

    #[test]
    fn test_find_multiplayer_folder_empty_accounts() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        let result = find_multiplayer_folder(accounts_path);
        assert!(result.is_none(), "Should return None for empty accounts directory");
    }

    #[test]
    fn test_find_multiplayer_folder_no_region_dirs() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        // Create account directory but no region directories
        let account_dir = accounts_path.join("12345678");
        fs::create_dir(&account_dir).unwrap();

        let result = find_multiplayer_folder(accounts_path);
        assert!(result.is_none(), "Should return None when no region directories exist");
    }

    #[test]
    fn test_find_multiplayer_folder_no_multiplayer_dir() {
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

        let result = find_multiplayer_folder(accounts_path);
        assert!(result.is_none(), "Should return None when Multiplayer directory doesn't exist");
    }

    #[test]
    fn test_find_multiplayer_folder_multiple_accounts() {
        let temp_dir = TempDir::new().unwrap();
        let accounts_path = temp_dir.path().join("Accounts");
        fs::create_dir_all(&accounts_path).unwrap();

        // Create first account (should be found first)
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

        let result = find_multiplayer_folder(accounts_path);
        assert!(result.is_some(), "Should find at least one multiplayer folder");

        let folder = result.unwrap();
        // Should find one of the accounts (order may vary based on filesystem)
        assert!(
            folder.account_id == "11111111" || folder.account_id == "22222222",
            "Should find one of the valid accounts"
        );
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

        let result = find_all_multiplayer_folders(accounts_path);
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
        };

        let cloned = folder.clone();
        assert_eq!(folder.path, cloned.path);
        assert_eq!(folder.account_id, cloned.account_id);
    }

    #[test]
    fn test_sc2_replay_folder_serialize() {
        let folder = SC2ReplayFolder {
            path: PathBuf::from("/test/path"),
            account_id: "12345678".to_string(),
        };

        let serialized = serde_json::to_string(&folder).unwrap();
        assert!(serialized.contains("path"));
        assert!(serialized.contains("account_id"));
        assert!(serialized.contains("12345678"));
    }

    // Integration test: Real detection (platform-specific)
    #[test]
    fn test_real_detection() {
        // This test will work on systems that actually have SC2 installed
        let result = detect_sc2_folder();
        // Don't assert - just log for manual verification
        println!("Real SC2 folder detection result: {:?}", result);
    }
}
