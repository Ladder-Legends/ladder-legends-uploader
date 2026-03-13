# Uploader Audit Fixes Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix all 21 issues found in the ladder-legends-uploader code audit, ranging from critical security/correctness bugs to low-priority cleanup.

**Architecture:** Rust/Tauri 2 desktop app with TypeScript frontend. Rust backend in `src-tauri/src/`, TypeScript in `src/`. Tests use `cargo test` for Rust and `vitest` for TypeScript.

**Tech Stack:** Rust, Tauri 2, TypeScript, notify crate (file watching), reqwest (HTTP), tokio (async), VecDeque (std), vitest

---

## CRITICAL Issues

---

### Task 1: Remove access token from debug log

**Files:**
- Modify: `src-tauri/src/replay_uploader.rs:155-163`

**Context:** The first 20 chars of the access token are written to the debug log. Anyone who receives a shared debug export gets a partial token.

**Step 1: Remove the token logging**

In `src-tauri/src/replay_uploader.rs`, replace lines 155-163:

```rust
// OLD - remove this entire block:
if let Some(ref logger) = self.logger {
    let token_preview = if self.access_token.len() > 20 {
        &self.access_token[..20]
    } else {
        &self.access_token
    };
    logger.debug(format!("Using access token (first 20 chars): {}...", token_preview));
    logger.debug(format!("Sending check-hashes request to: {}", url));
}

// REPLACE WITH:
if let Some(ref logger) = self.logger {
    let token_len = self.access_token.len();
    logger.debug(format!("Sending check-hashes request to: {} [token: {} chars]", url, token_len));
}
```

**Step 2: Verify no token preview strings remain**

```bash
grep -r "first 20 chars" src-tauri/src/
# Expected: no output
cargo test 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src-tauri/src/replay_uploader.rs
git commit -m "fix(security): remove access token preview from debug log

Token length is logged instead of the first 20 characters.
Shared debug exports no longer leak auth token data."
```

---

### Task 2: Fix watcher thread leak (add shutdown signal)

**Files:**
- Modify: `src-tauri/src/file_watcher.rs:77-93` (struct definition)
- Modify: `src-tauri/src/file_watcher.rs:109-125` (with_config)
- Modify: `src-tauri/src/file_watcher.rs:168-296` (start_native_watcher)
- Modify: `src-tauri/src/file_watcher.rs:487-492` (stop)

**Context:** The native OS thread spins in `loop { sleep(1s) }` with a comment "In a real app, you'd have a shutdown signal here". Re-initializing the upload manager leaks the old thread forever.

**Step 1: Write the failing test**

Add to the `#[cfg(test)]` block in `src-tauri/src/file_watcher.rs`:

```rust
#[tokio::test]
async fn test_watcher_stop_sets_is_running_false() {
    let temp_dir = TempDir::new().unwrap();
    let logger = Arc::new(DebugLogger::new());
    let watcher = RobustFileWatcher::new(
        vec![temp_dir.path().to_path_buf()],
        logger,
        |_path| {},
    );
    // stop() should set is_running to false (it already does)
    watcher.stop();
    assert!(!watcher.is_running.load(Ordering::SeqCst));
}
```

**Step 2: Run to verify it passes**

```bash
cargo test test_watcher_stop_sets_is_running_false -- --nocapture
```

**Step 3: Add shutdown field to struct**

Add `shutdown_tx` field to `RobustFileWatcher`:

```rust
pub struct RobustFileWatcher<F>
where
    F: Fn(PathBuf) + Send + Sync + 'static,
{
    folders: Vec<PathBuf>,
    config: WatcherConfig,
    logger: Arc<DebugLogger>,
    callback: Arc<F>,
    processed_files: Arc<tokio::sync::Mutex<HashSet<PathBuf>>>,
    last_event_time: Arc<AtomicU64>,
    is_running: Arc<AtomicBool>,
    stats: Arc<tokio::sync::Mutex<WatcherStats>>,
    // Shutdown sender for the native watcher OS thread
    shutdown_tx: Arc<std::sync::Mutex<Option<std::sync::mpsc::Sender<()>>>>,
}
```

Initialize in `with_config`:

```rust
shutdown_tx: Arc::new(std::sync::Mutex::new(None)),
```

**Step 4: Replace the infinite loop in `start_native_watcher`**

Replace lines 219-225 (the `loop { sleep }` block):

```rust
// OLD:
loop {
    std::thread::sleep(std::time::Duration::from_secs(1));
    // Check if we should stop
    // In a real app, you'd have a shutdown signal here
}

// NEW - add shutdown channel before std::thread::spawn:
let (thread_shutdown_tx, thread_shutdown_rx) = std::sync::mpsc::channel::<()>();
// Store sender so stop() can signal the thread
if let Ok(mut holder) = self.shutdown_tx.lock() {
    *holder = Some(thread_shutdown_tx);
}
```

Then inside the thread, replace the loop with:

```rust
logger_for_watcher.debug("Native watcher thread started".to_string());
// Block until shutdown signal (channel closed or explicit send)
let _ = thread_shutdown_rx.recv();
logger_for_watcher.debug("Native watcher thread shutting down".to_string());
// watcher is dropped here, stopping OS file watching
```

**Step 5: Update `stop()` to signal the thread**

```rust
pub fn stop(&self) {
    self.logger.info("Stopping robust file watcher".to_string());
    self.is_running.store(false, Ordering::SeqCst);
    // Signal native watcher thread to exit by dropping sender
    if let Ok(mut holder) = self.shutdown_tx.lock() {
        *holder = None; // Drop sender closes channel, unblocking recv() in thread
    }
}
```

**Step 6: Run tests**

```bash
cargo test 2>&1 | tail -10
```

**Step 7: Commit**

```bash
git add src-tauri/src/file_watcher.rs
git commit -m "fix: add shutdown signal to native watcher OS thread

Previously the watcher thread looped forever with no exit path,
leaking threads and file handles on every UploadManager re-init.
Now stop() drops the mpsc sender, unblocking the thread's recv()
and allowing the watcher to be properly cleaned up."
```

---

### Task 3: Fix `hasInitialized` not set in all code paths

**Files:**
- Modify: `src/main.ts:101-193`

**Context:** `hasInitialized = true` is set in the saved-tokens and savedPaths branches, but the isInitializing flag is cleared in `finally` before hasInitialized can be set elsewhere. Use an `initSucceeded` flag pattern.

**Step 1: Refactor `init()` to use `initSucceeded` flag**

Replace the body of `init()` in `src/main.ts` with:

```typescript
async function init(): Promise<void> {
  console.log('[DEBUG] init() called');

  if (isInitializing) {
    console.log('[DEBUG] Already initializing, skipping duplicate init() call');
    return;
  }
  if (hasInitialized) {
    console.log('[DEBUG] Already initialized, skipping init() call');
    return;
  }

  isInitializing = true;
  let initSucceeded = false;

  try {
    initStateElements();

    console.log('[DEBUG] Checking for Tauri API...');
    const invoke = await initTauri();
    console.log('[DEBUG] invoke function loaded:', typeof invoke);

    const savedTokens = await invoke('load_auth_tokens') as AuthTokens | null;
    console.log('[DEBUG] Saved auth tokens:', savedTokens ? 'Found' : 'Not found');

    if (savedTokens && savedTokens.access_token) {
      const isValid = await verifySavedTokens(savedTokens);
      if (isValid) {
        initSucceeded = true;
        return;
      }
    }

    const savedPaths = await invoke('load_folder_paths') as string[];
    console.log('[DEBUG] Saved folder paths:', savedPaths?.length || 0, 'folder(s)');

    if (savedPaths && savedPaths.length > 0) {
      console.log('[DEBUG] Using', savedPaths.length, 'saved folder(s), starting device auth...');
      await startDeviceAuth();
      initSucceeded = true;
      return;
    }

    showState('detecting');
    console.log('[DEBUG] Showing detecting state');

    console.log('[DEBUG] Starting folder detection...');
    const folderPaths = await detectWithTimeout(invoke);
    console.log('[DEBUG] Detection result:', folderPaths.length, 'folder(s)');

    if (folderPaths && folderPaths.length > 0) {
      console.log('[DEBUG] Found', folderPaths.length, 'folder(s), starting device auth...');
      await startDeviceAuth();
      initSucceeded = true;
    }
  } catch (error) {
    console.error('[DEBUG] Detection error:', error);
    showManualPickerOption(error);

    setTimeout(() => {
      const manualBtn = document.getElementById('manual-pick-btn');
      if (manualBtn) {
        manualBtn.addEventListener('click', async () => {
          const folderPath = await pickFolderManually();
          if (folderPath) {
            await startDeviceAuth();
            hasInitialized = true;
          }
        });
      }
    }, 100);
  } finally {
    isInitializing = false;
    if (initSucceeded) {
      hasInitialized = true;
    }
  }

  setupRetryButton();
}
```

**Step 2: Build to verify no TypeScript errors**

```bash
npm run build 2>&1 | tail -10
```

**Step 3: Commit**

```bash
git add src/main.ts
git commit -m "fix: ensure hasInitialized set in all init() success paths

Used initSucceeded flag set before every return/end of try block,
then applied in finally to guarantee the guard is set consistently
regardless of which code path completes initialization."
```

---

## HIGH Issues

---

### Task 4: Atomic write for `replays.json` + recovery on corruption

**Files:**
- Modify: `src-tauri/src/replay_tracker.rs:176-207`

**Step 1: Write the failing test**

Add to `#[cfg(test)]` in `src-tauri/src/replay_tracker.rs`:

```rust
#[test]
fn test_corrupted_tracker_falls_back_to_empty() {
    let temp_dir = TempDir::new().unwrap();
    let tracker_file = temp_dir.path().join("replays.json");
    fs::write(&tracker_file, b"{\"replays\": {broken json").unwrap();

    // Should return empty tracker, not Err
    let result = ReplayTracker::load_from_path(&tracker_file);
    assert!(result.is_ok(), "Corrupted tracker should fall back to empty");
    assert_eq!(result.unwrap().total_uploaded, 0);
}

#[test]
fn test_save_to_path_writes_valid_json() {
    let temp_dir = TempDir::new().unwrap();
    let tracker_file = temp_dir.path().join("replays.json");
    let mut tracker = ReplayTracker::new();
    tracker.add_replay(TrackedReplay {
        hash: "h1".to_string(), filename: "t.SC2Replay".to_string(),
        filesize: 100, uploaded_at: 1234, filepath: "/t".to_string(),
    });
    tracker.save_to_path(&tracker_file).unwrap();
    let contents = fs::read_to_string(&tracker_file).unwrap();
    serde_json::from_str::<ReplayTracker>(&contents).expect("Should be valid JSON");
}
```

**Step 2: Run to verify first test fails**

```bash
cargo test test_corrupted_tracker_falls_back_to_empty -- --nocapture
# Expected: FAILED
```

**Step 3: Fix `load_from_path` and add `save_to_path`**

```rust
pub fn load_from_path(tracker_file: &Path) -> Result<Self, String> {
    if !tracker_file.exists() {
        return Ok(Self::new());
    }
    let contents = fs::read_to_string(tracker_file)
        .map_err(|e| format!("Failed to read tracker file: {}", e))?;

    match serde_json::from_str::<ReplayTracker>(&contents) {
        Ok(tracker) => Ok(tracker),
        Err(e) => {
            // Corrupted file (crash during write) - start fresh rather than blocking
            eprintln!("Warning: tracker file corrupted ({}), starting fresh", e);
            Ok(Self::new())
        }
    }
}

/// Save tracker atomically (write to .tmp then rename)
pub fn save_to_path(&self, tracker_file: &Path) -> Result<(), String> {
    let tmp_file = tracker_file.with_extension("json.tmp");
    let contents = serde_json::to_string_pretty(self)
        .map_err(|e| format!("Failed to serialize tracker: {}", e))?;
    fs::write(&tmp_file, &contents)
        .map_err(|e| format!("Failed to write temp tracker: {}", e))?;
    fs::rename(&tmp_file, tracker_file)
        .map_err(|e| format!("Failed to rename tracker file: {}", e))?;
    Ok(())
}

pub fn save(&self) -> Result<(), String> {
    let config_dir = dirs::config_dir().ok_or("Could not find config directory")?;
    let app_config_dir = config_dir.join("ladder-legends-uploader");
    fs::create_dir_all(&app_config_dir)
        .map_err(|e| format!("Failed to create config directory: {}", e))?;
    self.save_to_path(&app_config_dir.join("replays.json"))
}
```

**Step 4: Run tests**

```bash
cargo test test_corrupted -- --nocapture
cargo test test_save_to_path -- --nocapture
cargo test 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add src-tauri/src/replay_tracker.rs
git commit -m "fix: atomic write for replays.json, graceful recovery on corruption

- save() writes to .tmp then renames (atomic on same filesystem)
- load_from_path() falls back to empty tracker on parse error
  instead of propagating an error that blocks the upload system"
```

---

### Task 5: Continue upload batch on single file failure

**Files:**
- Modify: `src-tauri/src/services/upload_executor.rs:136-141`
- Modify: `src-tauri/src/services/upload_executor.rs` (UploadResult struct)
- Modify: `src-tauri/src/upload_manager.rs` (UploadResult usage)

**Step 1: Find UploadResult definition**

```bash
grep -n "struct UploadResult\|uploaded_count" src-tauri/src/services/upload_executor.rs | head -10
```

**Step 2: Add `errors` field to UploadResult**

```rust
#[derive(Debug, Clone)]
pub struct UploadResult {
    pub uploaded_count: usize,
    pub errors: Vec<String>,  // errors collected from individual failures
}
```

**Step 3: Change `return Err(e)` to collect errors and continue**

Locate the error arm at line ~139 and replace:

```rust
// Add before the group loop:
let mut upload_errors: Vec<String> = Vec::new();

// Change the Err arm:
Err(e) => {
    self.logger.warn(format!(
        "Upload failed for {}, continuing batch: {}",
        prepared.filename, e
    ));
    upload_errors.push(format!("{}: {}", prepared.filename, e));
}
```

Update the final `Ok(...)`:

```rust
Ok(UploadResult {
    uploaded_count,
    errors: upload_errors,
})
```

**Step 4: Fix callers (add `.errors` field initialization)**

```bash
grep -rn "UploadResult {" src-tauri/src/ --include="*.rs"
# Update any other UploadResult instantiations to include errors: vec![]
```

**Step 5: Run tests**

```bash
cargo build 2>&1 | head -20
cargo test 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add src-tauri/src/services/upload_executor.rs src-tauri/src/upload_manager.rs
git commit -m "fix: continue upload batch on single file failure

Previously one failed replay aborted the entire batch and left
the frontend stuck in uploading state. Now errors are collected
and remaining replays in the batch continue uploading."
```

---

### Task 6: Remove folder name string-match validation

**Files:**
- Modify: `src-tauri/src/commands/folders.rs:22-41`

**Step 1: Replace string-match validation with existence check**

```rust
// Replace the entire Some(path) match arm body:
Some(path) => {
    let path_str = path.to_string();
    state_manager.debug_logger.debug(format!("User selected folder: {}", path_str));

    if !std::path::Path::new(&path_str).exists() {
        return Err("Selected folder does not exist".to_string());
    }

    if let Err(e) = save_folder_path(state_manager.clone(), &path_str).await {
        state_manager.debug_logger.warn(format!("Failed to save folder path: {}", e));
    }
    state_manager.debug_logger.info(format!("Saved folder path: {}", path_str));
    Ok(path_str)
}
```

Also remove the `use tauri_plugin_dialog::MessageDialogKind;` import if it was only used for the warning dialog. Check by building:

```bash
cargo build 2>&1 | grep "unused import\|error"
```

**Step 2: Run tests**

```bash
cargo test 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src-tauri/src/commands/folders.rs
git commit -m "fix: allow any folder path in manual picker

Removed string-match validation that rejected paths not containing
'StarCraft' or 'Replays'. This blocked non-English installations and
custom replay directories. Now only checks folder existence."
```

---

### Task 7: Preserve auth tokens on network error

**Files:**
- Modify: `src/modules/auth.ts:260-266`

**Step 1: Fix the catch block**

```typescript
// BEFORE:
  } catch (error) {
    // Verification failed (network error, etc.), clear tokens
    console.error('[DEBUG] Token verification failed:', error);
    const invoke = getInvoke();
    await invoke('clear_auth_tokens');
    return false;
  }

// AFTER:
  } catch (error) {
    // Network error (offline, DNS, server down) - preserve tokens, allow retry on next launch
    // Only clear tokens on explicit invalid-token responses (handled in the try block above)
    console.error('[DEBUG] Token verification network error, preserving tokens for retry:', error);
    return false;
  }
```

**Step 2: Build to verify no TypeScript errors**

```bash
npm run build 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src/modules/auth.ts
git commit -m "fix: preserve auth tokens on network error during verification

Previously any exception (including offline/DNS failures) cleared saved
tokens and forced full re-authentication. Now only explicit invalid-token
responses clear tokens."
```

---

### Task 8: Validate path in `open_folder_for_path`

**Files:**
- Modify: `src-tauri/src/commands/debug.rs:54-91`

**Step 1: Add path validation**

```rust
#[tauri::command]
pub async fn open_folder_for_path(path: String) -> Result<(), String> {
    let file_path = std::path::Path::new(&path);

    // Validate path is within the app's data directory
    let app_data_dir = dirs::config_dir()
        .ok_or("Could not find config directory")?
        .join("ladder-legends-uploader");

    // Canonicalize to resolve symlinks and relative segments
    let canonical_path = file_path.canonicalize()
        .map_err(|e| format!("Invalid path: {}", e))?;
    let canonical_data_dir = app_data_dir.canonicalize()
        .unwrap_or(app_data_dir);

    if !canonical_path.starts_with(&canonical_data_dir) {
        return Err(format!(
            "Path must be within app data directory"
        ));
    }

    // ... rest of platform-specific OS commands unchanged
```

Add `dirs` to the imports at the top if not already present (check `Cargo.toml`):

```bash
grep "dirs" src-tauri/Cargo.toml
```

**Step 2: Run tests**

```bash
cargo build 2>&1 | head -10
cargo test 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src-tauri/src/commands/debug.rs
git commit -m "fix(security): validate path scope in open_folder_for_path

Only paths within the app config directory are accepted,
preventing arbitrary path traversal via the Tauri command."
```

---

### Task 9: Wire file watcher to trigger upload scan

**Files:**
- Modify: `src-tauri/src/commands/upload.rs:110-134`

**Context:** The watcher detects new replays and emits `new-replay-detected` but never calls `scan_and_upload_replays`. Auto-upload on detect is not wired up.

**Step 1: Check `scan_and_upload_replays` signature**

```bash
grep -n "pub async fn scan_and_upload_replays" src-tauri/src/upload_manager.rs
```

**Step 2: Update `start_file_watcher` callback**

```rust
    let manager_for_watcher = Arc::clone(&manager);
    let app_for_upload = app.clone();

    match manager.start_watching(move |path| {
        let path_str = path.to_string_lossy().to_string();

        // Notify frontend of detected replay
        let _ = app.emit("new-replay-detected", path_str);

        // Trigger upload scan for the new replay
        let manager_clone = Arc::clone(&manager_for_watcher);
        let app_clone = app_for_upload.clone();
        tokio::spawn(async move {
            match manager_clone.scan_and_upload_replays(Some(5), &app_clone).await {
                Ok(count) => {
                    if count > 0 {
                        manager_clone.logger.info(format!(
                            "Auto-upload triggered by watcher: {} replay(s) uploaded", count
                        ));
                    }
                }
                Err(e) => {
                    manager_clone.logger.warn(format!(
                        "Auto-upload scan failed after watcher detection: {}", e
                    ));
                }
            }
        });
    }).await {
```

Note: `manager_clone.logger` — check actual field name in `UploadManager`:

```bash
grep -n "pub logger\|pub debug_logger" src-tauri/src/upload_manager.rs | head -5
```

Adjust the logger field name accordingly.

**Step 3: Build and fix type errors**

```bash
cargo build 2>&1 | head -30
```

**Step 4: Run tests**

```bash
cargo test 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add src-tauri/src/commands/upload.rs
git commit -m "fix: wire file watcher to trigger upload scan on new replay

Previously the watcher detected new replays and emitted a frontend
notification but never triggered an upload. Now a scan_and_upload_replays
call is spawned asynchronously on each new replay detection."
```

---

## MEDIUM Issues

---

### Task 10: Replace `Vec::remove(0)` with `VecDeque` in logger

**Files:**
- Modify: `src-tauri/src/debug_logger.rs`

**Step 1: Update imports and field type**

```rust
// Add to imports at top:
use std::collections::VecDeque;

// Find the struct field:
// logs: Arc<Mutex<Vec<DebugLogEntry>>>
// Change to:
// logs: Arc<Mutex<VecDeque<DebugLogEntry>>>
```

**Step 2: Update initialization**

```rust
// Find: logs: Arc::new(Mutex::new(Vec::new())),
// Change to:
logs: Arc::new(Mutex::new(VecDeque::new())),
```

**Step 3: Update the rotation code (lines 84-90)**

```rust
if let Ok(mut logs) = self.logs.lock() {
    if logs.len() >= 1000 {
        logs.pop_front();  // O(1) vs Vec::remove(0) which is O(n)
    }
    logs.push_back(entry);
}
```

**Step 4: Update any iteration code**

```bash
grep -n "\.logs\." src-tauri/src/debug_logger.rs
# VecDeque supports .iter(), .into_iter() same as Vec
# If there's a .to_vec() call update to: logs.iter().cloned().collect::<Vec<_>>()
```

**Step 5: Run tests**

```bash
cargo test 2>&1 | tail -5
```

**Step 6: Commit**

```bash
git add src-tauri/src/debug_logger.rs
git commit -m "perf: use VecDeque for debug log buffer (O(1) rotation)

Vec::remove(0) is O(n) and runs under a Mutex lock.
VecDeque::pop_front() is O(1) with no element shifting."
```

---

### Task 11: Make `scan_replay_folder` recursive and case-insensitive

**Files:**
- Modify: `src-tauri/src/replay_tracker.rs:226-268`

**Step 1: Write the failing tests**

Add to `#[cfg(test)]` in `replay_tracker.rs`:

```rust
#[test]
fn test_scan_finds_subdirectory_replays() {
    let temp_dir = TempDir::new().unwrap();
    let sub_dir = temp_dir.path().join("Season-3");
    fs::create_dir(&sub_dir).unwrap();
    create_test_replay_file(&sub_dir, "nested.SC2Replay", b"content");
    create_test_replay_file(temp_dir.path(), "top.SC2Replay", b"content");

    let replays = scan_replay_folder(temp_dir.path()).unwrap();
    assert_eq!(replays.len(), 2, "Must find replays in subdirectories");
}

#[test]
fn test_scan_case_insensitive_extension() {
    let temp_dir = TempDir::new().unwrap();
    create_test_replay_file(temp_dir.path(), "a.sc2replay", b"content");
    create_test_replay_file(temp_dir.path(), "b.SC2REPLAY", b"content");
    let replays = scan_replay_folder(temp_dir.path()).unwrap();
    assert_eq!(replays.len(), 2, "Must find replays regardless of extension case");
}
```

**Step 2: Run to verify tests fail**

```bash
cargo test test_scan_finds_subdirectory -- --nocapture
# Expected: FAILED
```

**Step 3: Rewrite `scan_replay_folder` to recurse and use `is_sc2_replay`**

```rust
use crate::file_watcher::is_sc2_replay;

pub fn scan_replay_folder(folder_path: &Path) -> Result<Vec<ReplayFileInfo>, String> {
    if !folder_path.exists() {
        return Err(format!("Folder does not exist: {}", folder_path.display()));
    }
    let mut replays = Vec::new();
    scan_folder_recursive(folder_path, &mut replays)?;
    replays.sort_by(|a, b| b.modified_time.cmp(&a.modified_time));
    Ok(replays)
}

fn scan_folder_recursive(dir: &Path, replays: &mut Vec<ReplayFileInfo>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let _ = scan_folder_recursive(&path, replays);
        } else if path.is_file() && is_sc2_replay(&path) {
            let filename = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string();
            let metadata = entry.metadata()
                .map_err(|e| format!("Failed to get metadata: {}", e))?;
            replays.push(ReplayFileInfo {
                path,
                filename,
                filesize: metadata.len(),
                modified_time: metadata.modified()
                    .map_err(|e| format!("Failed to get modified time: {}", e))?,
            });
        }
    }
    Ok(())
}
```

**Step 4: Run tests**

```bash
cargo test test_scan -- --nocapture
cargo test 2>&1 | tail -5
```

**Step 5: Commit**

```bash
git add src-tauri/src/replay_tracker.rs
git commit -m "fix: scan_replay_folder now recurses subdirs and is case-insensitive

Reuses is_sc2_replay() from file_watcher.rs for consistent behavior.
Previously only scanned top-level, missing replays in season subdirs."
```

---

### Task 12: Remove dead code in `classify_game_type`

**Files:**
- Modify: `src-tauri/src/replay_parser.rs:159-176`

**Step 1: Remove the unreachable `if ai_count > 0` block**

In `replay_parser.rs`, find:

```rust
    if team_sizes.len() == 2 && team_sizes[0] == 1 && team_sizes[1] == 1 {
        if observers > 0 {
            return GameType::Obs1v1;
        }
        if ai_count > 0 {         // DEAD CODE: remove this block
            return GameType::VsAI1v1;
        }
        if amm { ... }
        return GameType::Private1v1;
    }
```

Remove the `if ai_count > 0` block entirely.

**Step 2: Run tests**

```bash
cargo test 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src-tauri/src/replay_parser.rs
git commit -m "fix: remove dead code in classify_game_type

The ai_count check inside the two-human-team branch was unreachable:
two teams of exactly 1 human each cannot also contain AI players."
```

---

### Task 13: Fix `poll_device_authorization` pending string check

**Files:**
- Modify: `src-tauri/src/commands/auth.rs:39`

**Context:** `poll_authorization()` returns `Err("pending")` but the handler checks `e.contains("authorization_pending")` which never matches. Pending polls are incorrectly logged as errors.

**Step 1: Fix the check**

```rust
// BEFORE:
if e.contains("authorization_pending") {

// AFTER:
if e == "pending" {
```

**Step 2: Run tests**

```bash
cargo test 2>&1 | tail -5
```

**Step 3: Commit**

```bash
git add src-tauri/src/commands/auth.rs
git commit -m "fix: correct pending auth check in poll_device_authorization

poll_authorization() returns Err(\"pending\") but the handler was
checking contains(\"authorization_pending\") which never matched,
causing pending polls to be incorrectly logged as errors."
```

---

### Task 14: Show upload init failure to user

**Files:**
- Modify: `src/modules/upload.ts:97-100`
- Modify: `src/index.html` (add error element to authenticated state)

**Step 1: Add error element to index.html**

Find the authenticated state `<div>` and add inside it:

```html
<div id="upload-init-error" class="hidden" style="color:#e74c3c;margin-top:8px;font-size:12px;"></div>
```

**Step 2: Update the catch block in upload.ts**

```typescript
  } catch (error) {
    console.error('[DEBUG] Failed to initialize upload system:', error);
    const errorEl = document.getElementById('upload-init-error');
    if (errorEl) {
      errorEl.textContent = `Upload failed to start: ${error}. Please restart the app.`;
      errorEl.classList.remove('hidden');
    }
  }
```

**Step 3: Build**

```bash
npm run build 2>&1 | tail -5
```

**Step 4: Commit**

```bash
git add src/modules/upload.ts src/index.html
git commit -m "fix: show upload init failure message to user

Previously upload system failures were silently console.error'd.
Now a visible error message is shown in the authenticated view."
```

---

## LOW Issues

---

### Task 15: Remove useless `.parse()` in `get_version`

**Files:**
- Modify: `src-tauri/src/commands/version.rs:15-23`

**Step 1: Simplify**

```rust
// BEFORE:
    let version = app.package_info()
        .version
        .to_string()
        .parse()
        .map_err(|e| {
            let error_msg = format!("Failed to get version: {}", e);
            state_manager.debug_logger.error(error_msg.clone());
            error_msg
        })?;

// AFTER:
    let version = app.package_info().version.to_string();
```

**Step 2: Run tests and commit**

```bash
cargo test 2>&1 | tail -5
git add src-tauri/src/commands/version.rs
git commit -m "fix: remove useless .parse() call in get_version

.to_string().parse::<String>() always succeeds; the map_err
branch was unreachable dead code."
```

---

### Task 16: Fix `decodeHTMLEntities` to use DOMParser

**Files:**
- Modify: `src/modules/upload-progress.ts:10-14`

**Context:** Using `textarea.innerHTML` to decode entities is fragile. Replace with `DOMParser` which is safe and purpose-built.

**Step 1: Replace the decoder**

```typescript
// BEFORE:
function decodeHTMLEntities(text: string): string {
  const textarea = document.createElement('textarea');
  textarea.innerHTML = text;
  return textarea.value;
}

// AFTER:
function decodeHTMLEntities(text: string): string {
  const doc = new DOMParser().parseFromString(text, 'text/html');
  return doc.documentElement.textContent ?? text;
}
```

**Step 2: Build and commit**

```bash
npm run build 2>&1 | tail -5
git add src/modules/upload-progress.ts
git commit -m "fix: use DOMParser instead of innerHTML for HTML entity decoding

DOMParser safely decodes HTML entities without the fragility
of setting innerHTML on a DOM element."
```

---

### Task 17: Consolidate duplicate `UserData` structs

**Files:**
- Modify: `src-tauri/src/types.rs:7-11`
- Modify: `src-tauri/src/device_auth.rs:44-49`

**Step 1: Check all usages**

```bash
grep -rn "UserData\|\.id\b\|avatar_url" src-tauri/src/ --include="*.rs" | grep -v "#\[" | grep -v test
```

**Step 2: Update `types.rs` UserData to be a superset**

```rust
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UserData {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,        // Present in auth response
    pub username: String,
    pub avatar_url: Option<String>,
}
```

**Step 3: Remove `UserData` from `device_auth.rs` and import from `types`**

```rust
// At top of device_auth.rs, add:
use crate::types::UserData;
// Remove the local struct definition (lines 44-49)
```

**Step 4: Fix compile errors (id field is now Option)**

```bash
cargo build 2>&1 | head -30
# Fix any field access: user.id -> user.id.as_deref().unwrap_or("")
```

**Step 5: Run tests and commit**

```bash
cargo test 2>&1 | tail -5
git add src-tauri/src/types.rs src-tauri/src/device_auth.rs
git commit -m "refactor: consolidate duplicate UserData structs

Two incompatible UserData definitions existed in device_auth.rs and
types.rs. Merged into a single definition in types.rs."
```

---

### Task 18: Final verification

**Step 1: Run full Rust test suite**

```bash
cd /Users/chadfurman/projects/lla/ladder-legends-uploader/src-tauri
cargo test 2>&1 | tail -20
```

**Step 2: Run TypeScript tests**

```bash
cd /Users/chadfurman/projects/lla/ladder-legends-uploader
npm test 2>&1 | tail -10
```

**Step 3: Full build**

```bash
npm run build && cargo build
```

**Step 4: Audit checklist**

- [x] Task 1: Token logging removed (CRITICAL)
- [x] Task 2: Watcher thread shutdown added (CRITICAL)
- [x] Task 3: hasInitialized set in all paths (CRITICAL)
- [x] Task 4: Atomic replays.json + corruption recovery (HIGH)
- [x] Task 5: Batch continues on failure (HIGH)
- [x] Task 6: Folder validation removed (HIGH)
- [x] Task 7: Tokens preserved on network error (HIGH)
- [x] Task 8: Path validation in open_folder_for_path (HIGH)
- [x] Task 9: File watcher triggers upload (HIGH)
- [x] Task 10: VecDeque in logger (MEDIUM)
- [x] Task 11: scan_replay_folder recursive + case-insensitive (MEDIUM)
- [x] Task 12: Dead code removed (MEDIUM)
- [x] Task 13: Pending auth check fixed (MEDIUM)
- [x] Task 14: Upload init failure shown to user (MEDIUM)
- [x] Task 15: Useless .parse() removed (LOW)
- [x] Task 16: DOMParser replaces innerHTML (LOW)
- [x] Task 17: UserData consolidated (LOW)

Items accepted as design decisions (not changed):
- processed_files HashSet growth: acceptable for typical SC2 usage volume
- Heartbeat/poll interval invariant: documented as constants
- In-memory tracker vs disk divergence: intentional, managed by manifest_version sync
