//! Application state management.
//!
//! This module contains the state types and manager for tracking
//! the application's lifecycle state.

mod app_state;
mod manager;

pub use app_state::AppState;
pub use manager::AppStateManager;
