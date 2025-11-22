use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use sysinfo::{System, Disks};
use chrono::Utc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugLogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub context: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub os: String,
    pub os_version: String,
    pub kernel_version: Option<String>,
    pub hostname: Option<String>,
    pub cpu_count: usize,
    pub cpu_brand: String,
    pub total_memory_gb: f64,
    pub used_memory_gb: f64,
    pub available_memory_gb: f64,
    pub total_swap_gb: f64,
    pub used_swap_gb: f64,
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_space_gb: f64,
    pub available_space_gb: f64,
    pub is_removable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DebugReport {
    pub generated_at: String,
    pub app_version: String,
    pub system_info: SystemInfo,
    pub replay_folder: Option<String>,
    pub replays_found: Option<usize>,
    pub discord_user_id: Option<String>,
    pub error_count: usize,
    pub log_entries: Vec<DebugLogEntry>,
}

pub struct DebugLogger {
    logs: Arc<Mutex<Vec<DebugLogEntry>>>,
    error_count: Arc<Mutex<usize>>,
}

impl DebugLogger {
    pub fn new() -> Self {
        Self {
            logs: Arc::new(Mutex::new(Vec::new())),
            error_count: Arc::new(Mutex::new(0)),
        }
    }

    pub fn log(&self, level: &str, message: String, context: Option<serde_json::Value>) {
        // Print to console first
        eprintln!("[{}] {}", level, message);

        let entry = DebugLogEntry {
            timestamp: Utc::now().to_rfc3339(),
            level: level.to_string(),
            message,
            context,
        };

        if level == "ERROR" || level == "FATAL" {
            if let Ok(mut count) = self.error_count.lock() {
                *count += 1;
            }
        }

        if let Ok(mut logs) = self.logs.lock() {
            // Keep last 1000 entries to avoid memory issues
            if logs.len() >= 1000 {
                logs.remove(0);
            }
            logs.push(entry);
        }
    }

    pub fn info(&self, message: String) {
        self.log("INFO", message, None);
    }

    pub fn warn(&self, message: String) {
        self.log("WARN", message, None);
    }

    pub fn error(&self, message: String) {
        self.log("ERROR", message, None);
    }

    pub fn debug(&self, message: String) {
        self.log("DEBUG", message, None);
    }

    pub fn get_error_count(&self) -> usize {
        self.error_count.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn gather_system_info() -> SystemInfo {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu_brand = sys.cpus()
            .first()
            .map(|cpu| cpu.brand().to_string())
            .unwrap_or_else(|| "Unknown CPU".to_string());

        let total_memory_gb = sys.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let used_memory_gb = sys.used_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let available_memory_gb = sys.available_memory() as f64 / 1024.0 / 1024.0 / 1024.0;
        let total_swap_gb = sys.total_swap() as f64 / 1024.0 / 1024.0 / 1024.0;
        let used_swap_gb = sys.used_swap() as f64 / 1024.0 / 1024.0 / 1024.0;

        let disks_obj = Disks::new_with_refreshed_list();
        let disks: Vec<DiskInfo> = disks_obj
            .iter()
            .map(|disk| {
                let total_space_gb = disk.total_space() as f64 / 1024.0 / 1024.0 / 1024.0;
                let available_space_gb = disk.available_space() as f64 / 1024.0 / 1024.0 / 1024.0;

                DiskInfo {
                    name: disk.name().to_string_lossy().to_string(),
                    mount_point: disk.mount_point().to_string_lossy().to_string(),
                    total_space_gb,
                    available_space_gb,
                    is_removable: disk.is_removable(),
                }
            })
            .collect();

        SystemInfo {
            os: System::name().unwrap_or_else(|| "Unknown".to_string()),
            os_version: System::os_version().unwrap_or_else(|| "Unknown".to_string()),
            kernel_version: System::kernel_version(),
            hostname: System::host_name(),
            cpu_count: sys.cpus().len(),
            cpu_brand,
            total_memory_gb,
            used_memory_gb,
            available_memory_gb,
            total_swap_gb,
            used_swap_gb,
            disks,
        }
    }

    pub fn generate_report(
        &self,
        replay_folder: Option<String>,
        replays_found: Option<usize>,
        discord_user_id: Option<String>,
    ) -> DebugReport {
        let logs = self.logs.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let error_count = self.get_error_count();

        DebugReport {
            generated_at: Utc::now().to_rfc3339(),
            app_version: env!("CARGO_PKG_VERSION").to_string(),
            system_info: Self::gather_system_info(),
            replay_folder,
            replays_found,
            discord_user_id,
            error_count,
            log_entries: logs,
        }
    }

    pub fn save_report_to_file(
        &self,
        replay_folder: Option<String>,
        replays_found: Option<usize>,
        discord_user_id: Option<String>,
    ) -> Result<PathBuf, String> {
        let report = self.generate_report(replay_folder, replays_found, discord_user_id);

        // Get user's home directory
        let home_dir = dirs::home_dir()
            .ok_or_else(|| "Could not find home directory".to_string())?;

        // Create logs directory in user's home
        let logs_dir = home_dir.join(".ladder-legends-uploader").join("logs");
        fs::create_dir_all(&logs_dir)
            .map_err(|e| format!("Failed to create logs directory: {}", e))?;

        // Generate filename with timestamp
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let filename = format!("debug_log_{}.json", timestamp);
        let log_path = logs_dir.join(&filename);

        // Write report as JSON
        let json = serde_json::to_string_pretty(&report)
            .map_err(|e| format!("Failed to serialize report: {}", e))?;

        let mut file = File::create(&log_path)
            .map_err(|e| format!("Failed to create log file: {}", e))?;

        file.write_all(json.as_bytes())
            .map_err(|e| format!("Failed to write to log file: {}", e))?;

        Ok(log_path)
    }
}

impl Default for DebugLogger {
    fn default() -> Self {
        Self::new()
    }
}
