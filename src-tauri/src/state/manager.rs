//! Application state manager providing thread-safe access to app state.

use std::sync::{Arc, Mutex};
use crate::device_auth;
use crate::debug_logger;
use crate::upload_manager::UploadManager;
use super::AppState;

/// Manages the application state and provides access to shared resources
pub struct AppStateManager {
    /// Current application state (thread-safe)
    pub state: Mutex<AppState>,
    /// API client for device authentication
    pub api_client: device_auth::ApiClient,
    /// Upload manager instance (initialized after authentication)
    pub upload_manager: Mutex<Option<Arc<UploadManager>>>,
    /// Debug logger for capturing application events
    pub debug_logger: Arc<debug_logger::DebugLogger>,
}

impl AppStateManager {
    /// Create a new AppStateManager with default initial state
    pub fn new() -> Self {
        Self {
            state: Mutex::new(AppState::DetectingFolder),
            api_client: device_auth::ApiClient::new(),
            upload_manager: Mutex::new(None),
            debug_logger: Arc::new(debug_logger::DebugLogger::new()),
        }
    }
}

impl Default for AppStateManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_state_manager_initial_state() {
        let manager = AppStateManager::new();

        let state = manager.state.lock().unwrap();
        match *state {
            AppState::DetectingFolder => {
                // Expected initial state
            }
            _ => panic!("Expected DetectingFolder state"),
        }
    }

    #[test]
    fn test_app_state_manager_update_state() {
        let manager = AppStateManager::new();

        // Update state
        {
            let mut state = manager.state.lock().unwrap();
            *state = AppState::FolderFound {
                path: "/test/path".to_string(),
            };
        }

        // Verify update
        let state = manager.state.lock().unwrap();
        match &*state {
            AppState::FolderFound { path } => {
                assert_eq!(path, "/test/path");
            }
            _ => panic!("Expected FolderFound state"),
        }
    }
}
