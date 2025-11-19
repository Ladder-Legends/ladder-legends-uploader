use std::path::Path;
use std::process::Command;

/// Check if a replay is a 1v1 game using bundled Python script
pub fn is_1v1_replay(file_path: &Path) -> Result<bool, String> {
    // Get the path to the bundled Python script
    // In development, it's in src-tauri/
    // In production, it will be in the resources directory
    let script_path = if cfg!(debug_assertions) {
        // Development mode - use src-tauri/ directory
        std::env::current_dir()
            .map_err(|e| format!("Failed to get current dir: {}", e))?
            .join("src-tauri")
            .join("check_replay_type.py")
    } else {
        // Production mode - use bundled resource
        // TODO: Update this path for production builds
        std::env::current_dir()
            .map_err(|e| format!("Failed to get current dir: {}", e))?
            .join("check_replay_type.py")
    };

    // Call the Python script
    let output = Command::new("python3")
        .arg(script_path)
        .arg(file_path)
        .output()
        .map_err(|e| format!("Failed to execute Python script: {}", e))?;

    // Check exit code:
    // 0 = 1v1 game
    // 1 = not a 1v1 game
    // 2 = error parsing replay
    match output.status.code() {
        Some(0) => Ok(true),
        Some(1) => Ok(false),
        _ => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            Err(format!("Failed to parse replay: {}", stderr.trim()))
        }
    }
}
