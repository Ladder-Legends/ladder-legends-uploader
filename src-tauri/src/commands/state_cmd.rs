//! Application state management commands.

use tauri::State;
use crate::state::{AppState, AppStateManager};

/// Get the current application state
#[tauri::command]
pub async fn get_app_state(state_manager: State<'_, AppStateManager>) -> Result<AppState, String> {
    state_manager.debug_logger.debug("Getting app state".to_string());
    let state = state_manager.state.lock()
        .map_err(|_| "State mutex poisoned")?;
    Ok(state.clone())
}

/// Set the application state
#[tauri::command]
pub async fn set_app_state(
    state_manager: State<'_, AppStateManager>,
    new_state: AppState,
) -> Result<(), String> {
    state_manager.debug_logger.debug(format!("Setting app state to: {:?}", new_state));
    let mut state = state_manager.state.lock()
        .map_err(|_| "State mutex poisoned")?;
    *state = new_state;
    Ok(())
}
