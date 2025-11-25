//! Tauri command modules.
//!
//! This module organizes all Tauri commands into focused submodules
//! for better maintainability and code organization.
//!
//! Commands are accessed via their full module path (e.g., `commands::auth::request_device_code`)
//! in the `generate_handler!` macro in lib.rs.

pub mod auth;
pub mod browser;
pub mod debug;
pub mod detection;
pub mod folders;
pub mod settings;
pub mod state_cmd;
pub mod tokens;
pub mod upload;
pub mod version;
