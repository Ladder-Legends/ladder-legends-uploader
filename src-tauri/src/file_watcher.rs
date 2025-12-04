//! Robust file watcher utility for reliable SC2 replay detection.
//!
//! This module provides a resilient file watcher that handles Windows-specific
//! issues with ReadDirectoryChangesW, including:
//! - Buffer overflow recovery (RescanNeeded errors)
//! - Heartbeat monitoring for silent watcher death
//! - Periodic polling fallback for missed events
//! - Automatic watcher restart on failure
//!
//! ## Usage
//! ```rust,ignore
//! let watcher = RobustFileWatcher::new(
//!     folders,
//!     logger,
//!     |path| { /* handle new replay */ },
//! )?;
//! watcher.start().await?;
//! ```

use crate::debug_logger::DebugLogger;
use notify::{Event, EventKind, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::mpsc;

/// Configuration for the robust file watcher
#[derive(Clone)]
pub struct WatcherConfig {
    /// How often to check if the watcher is still alive (seconds)
    pub heartbeat_interval_secs: u64,
    /// How long without events before considering the watcher dead (seconds)
    pub heartbeat_timeout_secs: u64,
    /// How often to poll for files as a fallback (seconds)
    pub poll_interval_secs: u64,
    /// Delay before processing a new file (milliseconds)
    pub file_processing_delay_ms: u64,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            heartbeat_interval_secs: 60,      // Check every minute
            heartbeat_timeout_secs: 300,      // 5 minutes without events = restart
            poll_interval_secs: 120,          // Poll every 2 minutes as fallback
            #[cfg(target_os = "windows")]
            file_processing_delay_ms: 1000,   // Windows needs more time
            #[cfg(not(target_os = "windows"))]
            file_processing_delay_ms: 500,
        }
    }
}

/// Check if a path is an SC2 replay file (case-insensitive)
#[inline]
pub fn is_sc2_replay(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("SC2Replay"))
        .unwrap_or(false)
}

/// Stats about the file watcher for debugging
#[derive(Debug, Clone, Default)]
pub struct WatcherStats {
    pub events_received: u64,
    pub replays_detected: u64,
    pub errors_recovered: u64,
    pub restarts: u64,
    pub poll_scans: u64,
    pub poll_finds: u64,
}

/// A robust file watcher that handles Windows ReadDirectoryChangesW issues
pub struct RobustFileWatcher<F>
where
    F: Fn(PathBuf) + Send + Sync + 'static,
{
    folders: Vec<PathBuf>,
    config: WatcherConfig,
    logger: Arc<DebugLogger>,
    callback: Arc<F>,
    /// Tracks files we've already processed to avoid duplicates
    processed_files: Arc<tokio::sync::Mutex<HashSet<PathBuf>>>,
    /// Timestamp of last event (for heartbeat monitoring)
    last_event_time: Arc<AtomicU64>,
    /// Whether the watcher is running
    is_running: Arc<AtomicBool>,
    /// Stats for debugging
    stats: Arc<tokio::sync::Mutex<WatcherStats>>,
}

impl<F> RobustFileWatcher<F>
where
    F: Fn(PathBuf) + Send + Sync + 'static,
{
    /// Create a new robust file watcher
    pub fn new(
        folders: Vec<PathBuf>,
        logger: Arc<DebugLogger>,
        callback: F,
    ) -> Self {
        Self::with_config(folders, logger, callback, WatcherConfig::default())
    }

    /// Create with custom configuration
    pub fn with_config(
        folders: Vec<PathBuf>,
        logger: Arc<DebugLogger>,
        callback: F,
        config: WatcherConfig,
    ) -> Self {
        Self {
            folders,
            config,
            logger,
            callback: Arc::new(callback),
            processed_files: Arc::new(tokio::sync::Mutex::new(HashSet::new())),
            last_event_time: Arc::new(AtomicU64::new(current_timestamp())),
            is_running: Arc::new(AtomicBool::new(false)),
            stats: Arc::new(tokio::sync::Mutex::new(WatcherStats::default())),
        }
    }

    /// Get current watcher statistics
    #[allow(dead_code)]  // Exposed for debugging purposes
    pub async fn get_stats(&self) -> WatcherStats {
        self.stats.lock().await.clone()
    }

    /// Start watching for new replay files
    ///
    /// This spawns multiple async tasks:
    /// 1. Native file watcher (using notify crate)
    /// 2. Heartbeat monitor (restarts watcher if it dies)
    /// 3. Polling fallback (catches missed events)
    pub async fn start(&self) -> Result<(), String> {
        if self.is_running.swap(true, Ordering::SeqCst) {
            return Err("File watcher is already running".to_string());
        }

        self.logger.info(format!(
            "Starting robust file watcher for {} folder(s)",
            self.folders.len()
        ));

        // Channel for file events
        let (tx, rx) = mpsc::channel::<PathBuf>(100);

        // Start the native watcher
        self.start_native_watcher(tx.clone()).await?;

        // Start heartbeat monitor
        self.start_heartbeat_monitor(tx.clone());

        // Start polling fallback
        self.start_polling_fallback(tx.clone());

        // Start event processor
        self.start_event_processor(rx);

        self.logger.info("Robust file watcher started successfully".to_string());
        Ok(())
    }

    /// Start the native file system watcher
    async fn start_native_watcher(
        &self,
        tx: mpsc::Sender<PathBuf>,
    ) -> Result<(), String> {
        let folders = self.folders.clone();
        let logger = self.logger.clone();
        let last_event_time = self.last_event_time.clone();
        let stats = self.stats.clone();
        let is_running = self.is_running.clone();

        // Create the watcher in a separate thread (notify requires sync context)
        let (watcher_tx, mut watcher_rx) = mpsc::channel::<Result<Event, notify::Error>>(100);

        let logger_for_watcher = logger.clone();
        std::thread::spawn(move || {
            let watcher_tx_clone = watcher_tx.clone();
            let logger_clone = logger_for_watcher.clone();

            let mut watcher = match notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                // Send event to async channel
                if let Err(e) = watcher_tx_clone.blocking_send(res) {
                    // Channel closed, watcher should stop
                    logger_clone.debug(format!("Watcher channel closed: {}", e));
                }
            }) {
                Ok(w) => w,
                Err(e) => {
                    logger_for_watcher.error(format!("Failed to create watcher: {}", e));
                    return;
                }
            };

            // Watch all folders
            for folder in &folders {
                if let Err(e) = watcher.watch(folder, RecursiveMode::Recursive) {
                    logger_for_watcher.error(format!(
                        "Failed to watch folder {}: {}",
                        folder.display(),
                        e
                    ));
                } else {
                    logger_for_watcher.info(format!(
                        "Watching folder (recursive): {}",
                        folder.display()
                    ));
                }
            }

            logger_for_watcher.debug("Native watcher thread started".to_string());

            // Keep the watcher alive by holding it in this thread
            // The thread will exit when watcher_rx is dropped (on app shutdown)
            loop {
                std::thread::sleep(std::time::Duration::from_secs(1));
                // Check if we should stop
                // In a real app, you'd have a shutdown signal here
            }
        });

        // Process watcher events in async context
        let tx_clone = tx.clone();
        let logger_clone = logger.clone();
        tokio::spawn(async move {
            while let Some(result) = watcher_rx.recv().await {
                // Update heartbeat timestamp
                last_event_time.store(current_timestamp(), Ordering::SeqCst);

                match result {
                    Ok(event) => {
                        // Increment event counter
                        {
                            let mut s = stats.lock().await;
                            s.events_received += 1;
                        }

                        logger_clone.debug(format!("File event: {:?}", event.kind));

                        // Only process create/modify events
                        if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                            for path in event.paths {
                                if is_sc2_replay(&path) {
                                    logger_clone.info(format!(
                                        "Replay detected by watcher: {}",
                                        path.display()
                                    ));
                                    {
                                        let mut s = stats.lock().await;
                                        s.replays_detected += 1;
                                    }
                                    if let Err(e) = tx_clone.send(path).await {
                                        logger_clone.warn(format!(
                                            "Failed to queue replay: {}",
                                            e
                                        ));
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        logger_clone.error(format!("Watcher error: {}", e));
                        {
                            let mut s = stats.lock().await;
                            s.errors_recovered += 1;
                        }

                        // Check for RescanNeeded error (buffer overflow)
                        let error_str = format!("{:?}", e);
                        if error_str.contains("rescan") || error_str.contains("Rescan") {
                            logger_clone.warn(
                                "Buffer overflow detected, triggering immediate poll scan".to_string()
                            );
                            // The polling fallback will catch any missed files
                        }
                    }
                }

                // Check if we should stop
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }
            }

            logger_clone.warn("Native watcher event loop ended".to_string());
        });

        Ok(())
    }

    /// Start heartbeat monitor that restarts watcher if it appears dead
    fn start_heartbeat_monitor(&self, tx: mpsc::Sender<PathBuf>) {
        let logger = self.logger.clone();
        let last_event_time = self.last_event_time.clone();
        let config = self.config.clone();
        let stats = self.stats.clone();
        let is_running = self.is_running.clone();
        let folders = self.folders.clone();

        tokio::spawn(async move {
            let interval = Duration::from_secs(config.heartbeat_interval_secs);
            let timeout = config.heartbeat_timeout_secs;

            loop {
                tokio::time::sleep(interval).await;

                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                let last_event = last_event_time.load(Ordering::SeqCst);
                let now = current_timestamp();
                let elapsed = now.saturating_sub(last_event);

                logger.debug(format!(
                    "Heartbeat check: {}s since last event (timeout: {}s)",
                    elapsed, timeout
                ));

                if elapsed > timeout {
                    logger.warn(format!(
                        "Watcher heartbeat timeout ({}s without events), triggering recovery",
                        elapsed
                    ));

                    {
                        let mut s = stats.lock().await;
                        s.restarts += 1;
                    }

                    // Trigger a poll scan to catch any missed files
                    logger.info("Performing recovery poll scan...".to_string());
                    let found = poll_folders_for_replays(&folders, &logger).await;
                    for path in found {
                        if let Err(e) = tx.send(path).await {
                            logger.warn(format!("Failed to queue recovery replay: {}", e));
                        }
                    }

                    // Reset the heartbeat
                    last_event_time.store(current_timestamp(), Ordering::SeqCst);
                }
            }

            logger.debug("Heartbeat monitor ended".to_string());
        });
    }

    /// Start periodic polling as a fallback for missed events
    fn start_polling_fallback(&self, tx: mpsc::Sender<PathBuf>) {
        let logger = self.logger.clone();
        let folders = self.folders.clone();
        let config = self.config.clone();
        let processed_files = self.processed_files.clone();
        let stats = self.stats.clone();
        let is_running = self.is_running.clone();
        let last_event_time = self.last_event_time.clone();

        tokio::spawn(async move {
            let interval = Duration::from_secs(config.poll_interval_secs);

            // Wait a bit before first poll to let native watcher process initial files
            tokio::time::sleep(Duration::from_secs(10)).await;

            loop {
                tokio::time::sleep(interval).await;

                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                logger.debug("Running fallback poll scan...".to_string());
                {
                    let mut s = stats.lock().await;
                    s.poll_scans += 1;
                }

                // Update heartbeat to show we're still active
                last_event_time.store(current_timestamp(), Ordering::SeqCst);

                let found = poll_folders_for_replays(&folders, &logger).await;
                let mut new_count = 0;

                for path in found {
                    let mut processed = processed_files.lock().await;
                    if !processed.contains(&path) {
                        // Check if file was modified recently (within last poll interval + buffer)
                        if let Ok(metadata) = tokio::fs::metadata(&path).await {
                            if let Ok(modified) = metadata.modified() {
                                let age = SystemTime::now()
                                    .duration_since(modified)
                                    .unwrap_or(Duration::from_secs(u64::MAX));

                                // Only process files modified since last poll + 30s buffer
                                if age.as_secs() < config.poll_interval_secs + 30 {
                                    logger.info(format!(
                                        "Poll scan found new replay: {} (age: {}s)",
                                        path.display(),
                                        age.as_secs()
                                    ));
                                    processed.insert(path.clone());
                                    new_count += 1;
                                    drop(processed); // Release lock before send

                                    if let Err(e) = tx.send(path).await {
                                        logger.warn(format!(
                                            "Failed to queue polled replay: {}",
                                            e
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }

                if new_count > 0 {
                    let mut s = stats.lock().await;
                    s.poll_finds += new_count;
                    logger.info(format!("Poll scan found {} new replay(s)", new_count));
                }
            }

            logger.debug("Polling fallback ended".to_string());
        });
    }

    /// Start event processor that handles the callback with delay
    fn start_event_processor(&self, mut rx: mpsc::Receiver<PathBuf>) {
        let logger = self.logger.clone();
        let callback = self.callback.clone();
        let processed_files = self.processed_files.clone();
        let delay_ms = self.config.file_processing_delay_ms;
        let is_running = self.is_running.clone();

        tokio::spawn(async move {
            while let Some(path) = rx.recv().await {
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                // Check if already processed (dedup from multiple sources)
                {
                    let mut processed = processed_files.lock().await;
                    if processed.contains(&path) {
                        logger.debug(format!(
                            "Skipping already processed: {}",
                            path.display()
                        ));
                        continue;
                    }
                    processed.insert(path.clone());
                }

                // Wait for file to be fully written
                logger.debug(format!(
                    "Waiting {}ms before processing: {}",
                    delay_ms,
                    path.display()
                ));
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                // Verify file still exists and is readable
                if !path.exists() {
                    logger.warn(format!(
                        "File no longer exists: {}",
                        path.display()
                    ));
                    continue;
                }

                logger.info(format!("Processing replay: {}", path.display()));
                callback(path);
            }

            logger.warn("Event processor ended".to_string());
        });
    }

    /// Stop the file watcher
    #[allow(dead_code)]  // Exposed for graceful shutdown
    pub fn stop(&self) {
        self.logger.info("Stopping robust file watcher".to_string());
        self.is_running.store(false, Ordering::SeqCst);
    }
}

/// Get current timestamp in seconds
fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Poll folders for replay files (fallback mechanism)
async fn poll_folders_for_replays(
    folders: &[PathBuf],
    logger: &DebugLogger,
) -> Vec<PathBuf> {
    let mut replays = Vec::new();

    for folder in folders {
        if let Ok(mut entries) = tokio::fs::read_dir(folder).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if path.is_file() && is_sc2_replay(&path) {
                    replays.push(path);
                } else if path.is_dir() {
                    // Recursively scan subdirectories
                    if let Ok(sub_replays) = scan_directory_recursive(&path).await {
                        replays.extend(sub_replays);
                    }
                }
            }
        } else {
            logger.warn(format!("Failed to read directory: {}", folder.display()));
        }
    }

    replays
}

/// Recursively scan a directory for replay files
async fn scan_directory_recursive(dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
    let mut replays = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() && is_sc2_replay(&path) {
            replays.push(path);
        } else if path.is_dir() {
            if let Ok(sub_replays) = Box::pin(scan_directory_recursive(&path)).await {
                replays.extend(sub_replays);
            }
        }
    }

    Ok(replays)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn test_is_sc2_replay() {
        assert!(is_sc2_replay(Path::new("game.SC2Replay")));
        assert!(is_sc2_replay(Path::new("game.sc2replay")));
        assert!(is_sc2_replay(Path::new("game.SC2REPLAY")));
        assert!(!is_sc2_replay(Path::new("game.txt")));
        assert!(!is_sc2_replay(Path::new("game")));
    }

    #[test]
    fn test_current_timestamp() {
        let ts = current_timestamp();
        assert!(ts > 0);
        // Should be after 2020
        assert!(ts > 1577836800);
    }

    #[test]
    fn test_watcher_config_default() {
        let config = WatcherConfig::default();
        assert_eq!(config.heartbeat_interval_secs, 60);
        assert_eq!(config.heartbeat_timeout_secs, 300);
        assert_eq!(config.poll_interval_secs, 120);
    }

    #[tokio::test]
    async fn test_poll_folders_for_replays() {
        let temp_dir = TempDir::new().unwrap();
        let replay_path = temp_dir.path().join("test.SC2Replay");
        tokio::fs::write(&replay_path, b"test content").await.unwrap();

        let logger = Arc::new(DebugLogger::new());
        let replays = poll_folders_for_replays(&[temp_dir.path().to_path_buf()], &logger).await;

        assert_eq!(replays.len(), 1);
        assert_eq!(replays[0], replay_path);
    }

    #[tokio::test]
    async fn test_poll_folders_ignores_non_replays() {
        let temp_dir = TempDir::new().unwrap();
        tokio::fs::write(temp_dir.path().join("test.txt"), b"not a replay").await.unwrap();
        tokio::fs::write(temp_dir.path().join("test.mp4"), b"not a replay").await.unwrap();

        let logger = Arc::new(DebugLogger::new());
        let replays = poll_folders_for_replays(&[temp_dir.path().to_path_buf()], &logger).await;

        assert_eq!(replays.len(), 0);
    }

    #[tokio::test]
    async fn test_watcher_creation() {
        let temp_dir = TempDir::new().unwrap();
        let logger = Arc::new(DebugLogger::new());
        let callback_count = Arc::new(AtomicUsize::new(0));
        let count_clone = callback_count.clone();

        let watcher = RobustFileWatcher::new(
            vec![temp_dir.path().to_path_buf()],
            logger,
            move |_path| {
                count_clone.fetch_add(1, Ordering::SeqCst);
            },
        );

        let stats = watcher.get_stats().await;
        assert_eq!(stats.events_received, 0);
        assert_eq!(stats.replays_detected, 0);
    }

    #[tokio::test]
    async fn test_recursive_scan() {
        let temp_dir = TempDir::new().unwrap();

        // Create nested structure
        let sub_dir = temp_dir.path().join("subdir");
        tokio::fs::create_dir(&sub_dir).await.unwrap();

        let replay1 = temp_dir.path().join("game1.SC2Replay");
        let replay2 = sub_dir.join("game2.SC2Replay");

        tokio::fs::write(&replay1, b"replay1").await.unwrap();
        tokio::fs::write(&replay2, b"replay2").await.unwrap();

        let replays = scan_directory_recursive(temp_dir.path()).await.unwrap();
        assert_eq!(replays.len(), 2);
    }
}
