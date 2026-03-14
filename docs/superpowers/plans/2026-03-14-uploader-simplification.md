# Uploader Simplification Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Simplify the uploader to a dumb pipe — find competitive replays, dedup, upload. Remove player name filtering, replace broken file watcher with polling, allow adding additional folders.

**Architecture:** Remove the player name filter gate from `ReplayScanner`, delete `file_watcher.rs` and replace with a poll loop calling `scan_if_available()`, and add an `add_folder_path` command that appends to the persisted folder list.

**Tech Stack:** Rust (Tauri backend), TypeScript (frontend), tokio (async runtime)

**Spec:** `docs/superpowers/specs/2026-03-14-uploader-simplification-design.md`

---

## File Structure

**Delete:**
- `src-tauri/src/file_watcher.rs` — native watcher, heartbeat, event processor (replaced by polling)

**Modify:**
- `src-tauri/src/services/replay_scanner.rs` — remove player filtering, simplify `scan_and_prepare` signature
- `src-tauri/src/upload_manager.rs` — update `scan_and_upload` to pass player name as hint (not filter), update grouping function, remove `start_watching`, add `start_polling`
- `src-tauri/src/services/upload_executor.rs` — update import from `group_replays_by_type_and_player` to `group_replays_by_type`
- `src-tauri/src/commands/upload.rs` — replace `start_file_watcher` command with `start_polling`
- `src-tauri/src/commands/folders.rs` — add `add_folder_path` command, fix `pick_replay_folder_manual` to append
- `src-tauri/src/lib.rs` — remove `file_watcher` module, register `add_folder_path` command, replace `start_file_watcher` with `start_polling`
- `src/modules/upload.ts` — call `start_polling` instead of `start_file_watcher`
- `src/modules/detection.ts` — already patched (double-click-handler fix)

---

## Chunk 1: Remove Player Name Filtering

### Task 1: Remove player filtering from `ReplayScanner`

**Files:**
- Modify: `src-tauri/src/services/replay_scanner.rs`

- [ ] **Step 1: Update `scan_and_prepare` signature — remove `player_names` param**

Change:
```rust
pub async fn scan_and_prepare(
    &self,
    tracker: &ReplayTracker,
    uploader: &ReplayUploader,
    player_names: Vec<String>,
    limit: usize,
) -> Result<ScanResult, String> {
```

To:
```rust
pub async fn scan_and_prepare(
    &self,
    tracker: &ReplayTracker,
    uploader: &ReplayUploader,
    player_name_hint: &str,
    limit: usize,
) -> Result<ScanResult, String> {
```

`player_name_hint` is the best-guess name used for the upload payload — NOT for filtering.

- [ ] **Step 2: Remove player detection and filtering from `scan_and_prepare` body**

Remove the player detection block (lines 90-95):
```rust
// Step 2: Detect player names if not provided — scan ALL replays for accuracy
let player_names = if player_names.is_empty() {
    self.detect_players_from_replays(&all_sorted_replays)
} else {
    player_names
};
```

Update the `filter_and_hash_replays` call (lines 97-102) to remove `player_names`:
```rust
let filter_result = self.filter_and_hash_replays(
    all_sorted_replays,
    tracker,
    player_name_hint,
)?;
```

- [ ] **Step 3: Simplify `filter_and_hash_replays` — remove Filter 2 (player name check)**

Change signature to accept `player_name_hint: &str` instead of `player_names: &[String]`.

Remove the entire Filter 2 block (lines 253-276 — player extraction and name matching).

Remove `observer_game_count` variable and its logging.

Update the `replay_map.insert` to use `player_name_hint` instead of `player_name_in_replay`:
```rust
replay_map.insert(
    hash,
    (replay_info, game_type.as_str().to_string(), player_name_hint.to_string()),
);
```

- [ ] **Step 4: Delete `find_user_in_game` and `detect_players_from_replays` methods**

Remove `find_user_in_game` (lines 330-342) and `detect_players_from_replays` (lines 186-215) entirely.

Remove the import of `detect_user_player_names` from line 11:
```rust
use crate::upload_manager::detect_user_player_names;
```

- [ ] **Step 5: Update `PreparedReplay` and `ScanResult` doc comments**

Update the `ScanResult` doc comment (line 28) from:
```rust
/// Replays ready for upload, grouped by (game_type, player_name)
```
To:
```rust
/// Replays ready for upload
```

Update `scan_and_prepare` doc comment (lines 52-58) to remove mention of player filtering.

- [ ] **Step 6: Run `cargo test` to verify scanner tests pass**

Run: `cd src-tauri && cargo test replay_scanner`
Expected: All existing scanner tests pass (they don't test player filtering directly)

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/services/replay_scanner.rs
git commit -m "refactor: remove player name filtering from ReplayScanner

uploads all competitive replays regardless of player name.
player_name_hint is passed through for the upload payload
but no longer used for filtering."
```

### Task 2: Update `UploadManager` and `UploadExecutor` for player name hint

**Files:**
- Modify: `src-tauri/src/upload_manager.rs`
- Modify: `src-tauri/src/services/upload_executor.rs`

- [ ] **Step 1: Update `scan_and_upload` to resolve player name hint before calling scanner**

In `scan_and_upload()`, the current code calls `self.fetch_player_names()` then passes the result to `scanner.scan_and_prepare(... player_names ...)`. Change this to:

1. Call `self.fetch_player_names()` to get names
2. If empty, detect from replays (move detection logic here)
3. Pick the first name as `player_name_hint` (or `""` if none)
4. Pass `&player_name_hint` to `scanner.scan_and_prepare()`

Find the `scan_and_upload` method and locate where it calls `scanner.scan_and_prepare`. Update the call:

```rust
let player_names = self.fetch_player_names().await.unwrap_or_default();
let player_name_hint = player_names.first().cloned().unwrap_or_default();

let scan_result = scanner.scan_and_prepare(
    &tracker_guard,
    &self.uploader,
    &player_name_hint,
    limit,
).await?;
```

Note: If player names are empty, the scanner scans all replays and `detect_user_player_names` can still be called here in `upload_manager.rs` to get a hint. The key change is this is only a hint, not a filter.

- [ ] **Step 2: Update `ReplayGroup` and grouping function**

Rename `group_replays_by_type_and_player` to `group_replays_by_type`. Simplify `ReplayGroup`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayGroup {
    pub game_type: String,
    pub player_name: String,  // hint for upload, same for all replays
    pub hashes: Vec<String>,
}

pub fn group_replays_by_type(
    hashes: &[String],
    replay_map: &HashMap<String, (ReplayFileInfo, String, String)>,
) -> Vec<ReplayGroup> {
    let mut groups: HashMap<String, Vec<String>> = HashMap::new();

    for hash in hashes {
        if let Some((_, game_type_str, _)) = replay_map.get(hash) {
            groups.entry(game_type_str.clone())
                .or_default()
                .push(hash.clone());
        }
    }

    let mut sorted_groups: Vec<_> = groups.into_iter()
        .map(|(game_type, hashes)| ReplayGroup {
            game_type,
            player_name: String::new(), // set by caller
            hashes,
        })
        .collect();

    sorted_groups.sort_by(|a, b| a.game_type.cmp(&b.game_type));
    sorted_groups
}
```

Update any callers of `group_replays_by_type_and_player` → `group_replays_by_type`. Set `player_name` on each group from the hint after grouping.

- [ ] **Step 3: Update `upload_executor.rs` to use renamed grouping function**

In `src-tauri/src/services/upload_executor.rs`, update the import (line 9):
```rust
use crate::upload_manager::{group_replays_by_type, UploadStatus, UploadManagerState};
```

Update the call (line 83):
```rust
let groups = group_replays_by_type(&hashes, &tuple_map);
```

The `player_name` on each `ReplayGroup` is already set to `String::new()` by `group_replays_by_type`. Since all `PreparedReplay` entries share the same `player_name` hint (set in Task 1), `UploadExecutor` can read it from any replay in the group. After the grouping call, set the player name:

```rust
let mut groups = group_replays_by_type(&hashes, &tuple_map);
if let Some(first_replay) = prepared_replays.first() {
    for group in &mut groups {
        group.player_name = first_replay.player_name.clone();
    }
}
```

- [ ] **Step 4: Run `cargo test` to check for compilation and test failures**

Run: `cd src-tauri && cargo test`
Expected: Compilation succeeds. Some tests referencing `group_replays_by_type_and_player` may need updating.

- [ ] **Step 5: Fix any failing tests**

Update test references from `group_replays_by_type_and_player` to `group_replays_by_type`. Update test assertions that check player-name-based grouping.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/upload_manager.rs src-tauri/src/services/upload_executor.rs
git commit -m "refactor: player name is now a hint for upload, not a filter

grouping simplified from (game_type, player_name) to game_type only.
player_name_hint is resolved from API or detection and applied to all
replays in the batch."
```

---

## Chunk 2: Replace File Watcher with Polling

### Task 3: Delete `file_watcher.rs` and remove references

**Files:**
- Delete: `src-tauri/src/file_watcher.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/upload_manager.rs`

- [ ] **Step 1: Delete `file_watcher.rs`**

```bash
rm src-tauri/src/file_watcher.rs
```

- [ ] **Step 2: Remove `file_watcher` module declaration from `lib.rs`**

In `src-tauri/src/lib.rs`, remove line 22:
```rust
mod file_watcher;
```

- [ ] **Step 3: Remove file_watcher imports from `upload_manager.rs`**

Remove line 5:
```rust
use crate::file_watcher::{RobustFileWatcher, WatcherStats};
```

Remove the `#[cfg(test)]` re-export on lines 15-17:
```rust
#[cfg(test)]
pub use crate::file_watcher::is_sc2_replay;
```

Remove the `get_file_processing_delay_ms` test-only function (lines 19-28).

- [ ] **Step 4: Remove `start_watching` and `get_watcher_stats` from `UploadManager`**

Delete the `start_watching` method (lines 622-653) and `get_watcher_stats` (lines 656-660).

- [ ] **Step 5: Add `start_polling` method to `UploadManager`**

Add after the `scan_if_available` method:

```rust
/// Start a poll loop that scans for new replays every `interval_secs` seconds.
///
/// Runs an immediate scan on start, then polls every interval.
/// Uses `scan_if_available` to respect the single-flight semaphore.
/// Stops when `cancel_token` is cancelled.
pub fn start_polling(
    self: &Arc<Self>,
    interval_secs: u64,
    app: tauri::AppHandle,
) {
    let manager = Arc::clone(self);
    let logger = Arc::clone(&self.logger);
    let cancel = self.cancel_token.clone();

    // Update state
    {
        let mut state = self.state.lock()
            .unwrap_or_else(|e| e.into_inner());
        state.is_watching = true;
    }

    tokio::spawn(async move {
        logger.info(format!("Replay poller started (interval: {}s)", interval_secs));

        // Immediate first scan
        match manager.scan_if_available(10, &app).await {
            Ok(count) => {
                if count > 0 {
                    logger.info(format!("Poll: uploaded {} new replay(s)", count));
                }
            }
            Err(e) => {
                if e.contains("auth_expired") {
                    let _ = app.emit("auth-expired", ());
                }
                logger.error(format!("Poll scan error: {}", e));
            }
        }

        // Poll loop
        loop {
            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(interval_secs)) => {}
                _ = cancel.cancelled() => {
                    logger.info("Replay poller stopped (cancelled)".to_string());
                    break;
                }
            }

            if cancel.is_cancelled() {
                break;
            }

            match manager.scan_if_available(10, &app).await {
                Ok(count) => {
                    if count > 0 {
                        logger.info(format!("Poll: uploaded {} new replay(s)", count));
                    }
                }
                Err(e) => {
                    if e.contains("auth_expired") {
                        let _ = app.emit("auth-expired", ());
                    }
                    logger.error(format!("Poll scan error: {}", e));
                }
            }
        }
    });
}
```

Add `use tauri::Emitter;` to the imports at the top of `upload_manager.rs` if not already present.

- [ ] **Step 6: Run `cargo test` to verify compilation**

Run: `cd src-tauri && cargo test`
Expected: Compiles. Tests that referenced `file_watcher` or `is_sc2_replay` will fail — fix or remove them.

- [ ] **Step 7: Fix failing tests**

Remove any tests that reference `file_watcher::is_sc2_replay` or `get_file_processing_delay_ms`. These are no longer needed.

- [ ] **Step 8: Commit**

```bash
git add -A src-tauri/src/
git commit -m "refactor: replace file watcher with polling

delete file_watcher.rs (native watcher, heartbeat, event processor).
add start_polling() to UploadManager — immediate scan then poll
every 60s via scan_if_available()."
```

### Task 4: Update Tauri command and frontend

**Files:**
- Modify: `src-tauri/src/commands/upload.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src/modules/upload.ts`

- [ ] **Step 1: Replace `start_file_watcher` command with `start_polling`**

In `src-tauri/src/commands/upload.rs`, replace the `start_file_watcher` function (lines 112-169) with:

```rust
/// Start polling replay folders for new files
#[tauri::command]
pub async fn start_polling(
    state_manager: State<'_, AppStateManager>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    state_manager.debug_logger.info("Starting replay poller".to_string());

    let manager = {
        let upload_manager = state_manager.upload_manager.lock()
            .map_err(|_| "Upload manager mutex poisoned")?;
        match upload_manager.as_ref() {
            Some(m) => Arc::clone(m),
            None => {
                state_manager.debug_logger.error("Upload manager not initialized for polling".to_string());
                return Err("Upload manager not initialized".to_string());
            }
        }
    };

    manager.start_polling(60, app);
    state_manager.debug_logger.info("Replay poller started".to_string());
    Ok(())
}
```

Add `use std::sync::Arc;` to the top of the file if not already present.

- [ ] **Step 2: Update `lib.rs` invoke handler**

In `src-tauri/src/lib.rs`, replace `commands::upload::start_file_watcher` with `commands::upload::start_polling` in the `invoke_handler` (line 75).

- [ ] **Step 3: Update frontend to call `start_polling`**

In `src/modules/upload.ts`, change line 97:
```typescript
await invoke('start_file_watcher');
console.log('[DEBUG] File watcher started');
```
To:
```typescript
await invoke('start_polling');
console.log('[DEBUG] Replay poller started');
```

- [ ] **Step 4: Run `cargo test` and `npm test`**

Run: `cd src-tauri && cargo test && cd .. && npm test`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/upload.rs src-tauri/src/lib.rs src/modules/upload.ts
git commit -m "feat: wire up start_polling command and frontend

replaces start_file_watcher Tauri command with start_polling.
frontend calls start_polling after initializing upload manager."
```

---

## Chunk 3: Folder Management

### Task 5: Add `add_folder_path` command

**Files:**
- Modify: `src-tauri/src/commands/folders.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Write test for `add_folder_path` behavior**

Add to `src-tauri/src/commands/folders.rs` (at the bottom):

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_add_folder_deduplicates() {
        let existing = vec!["/a".to_string(), "/b".to_string()];
        let new_path = "/a";
        let mut merged = existing.clone();
        if !merged.contains(&new_path.to_string()) {
            merged.push(new_path.to_string());
        }
        assert_eq!(merged.len(), 2); // no duplicate added
    }

    #[test]
    fn test_add_folder_appends_new() {
        let existing = vec!["/a".to_string(), "/b".to_string()];
        let new_path = "/c";
        let mut merged = existing.clone();
        if !merged.contains(&new_path.to_string()) {
            merged.push(new_path.to_string());
        }
        assert_eq!(merged.len(), 3);
        assert!(merged.contains(&"/c".to_string()));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd src-tauri && cargo test folders`
Expected: Both tests pass

- [ ] **Step 3: Add `add_folder_path` Tauri command**

Add to `src-tauri/src/commands/folders.rs`:

```rust
/// Add a folder path to the persisted list (appends, deduplicates)
#[tauri::command]
pub async fn add_folder_path(
    state_manager: State<'_, AppStateManager>,
    path: String,
) -> Result<(), String> {
    state_manager.debug_logger.info(format!("Adding folder path: {}", path));

    // Load existing paths
    let config: Option<serde_json::Value> = config_utils::load_config_file("config.json")
        .inspect_err(|e| { state_manager.debug_logger.error(e.clone()); })?;

    let mut paths: Vec<String> = config
        .as_ref()
        .and_then(|c| c.get("replay_folders"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    // Append if not already present
    if !paths.contains(&path) {
        paths.push(path.clone());
    }

    // Save merged list
    let config = serde_json::json!({ "replay_folders": paths });
    config_utils::save_config_file("config.json", &config)
        .inspect_err(|e| { state_manager.debug_logger.error(e.clone()); })?;

    state_manager.debug_logger.info(format!("Folder list now has {} path(s)", paths.len()));
    Ok(())
}
```

- [ ] **Step 4: Update `pick_replay_folder_manual` to use `add_folder_path`**

In `pick_replay_folder_manual`, replace the call to `save_folder_path` (line 31):
```rust
if let Err(e) = save_folder_path(state_manager.clone(), &path_str).await {
```
With:
```rust
if let Err(e) = add_folder_path(state_manager.clone(), path_str.clone()).await {
```

- [ ] **Step 5: Delete legacy `save_folder_path` helper**

Remove the `save_folder_path` function (lines 62-68 in `folders.rs`). It overwrites the entire folder list with a single path — the opposite of what we want. All callers now use `add_folder_path`.

- [ ] **Step 6: Register `add_folder_path` in `lib.rs`**

Add `commands::folders::add_folder_path` to the `invoke_handler` in `lib.rs`.

- [ ] **Step 7: Run `cargo test`**

Run: `cd src-tauri && cargo test`
Expected: All pass

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/commands/folders.rs src-tauri/src/lib.rs
git commit -m "feat: add_folder_path command appends to persisted list

read-modify-write: loads existing paths, appends if not duplicate,
saves merged list. pick_replay_folder_manual now uses this instead
of replacing the list. deleted legacy save_folder_path helper."
```

---

## Chunk 4: Test Coverage

### Task 6: Add scanner tests for no-filter behavior and edge cases

**Files:**
- Modify: `src-tauri/src/services/replay_scanner.rs`

- [ ] **Step 1: Add test for scanning multiple folders**

Add to the `#[cfg(test)] mod tests` block:

```rust
#[test]
fn test_scan_multiple_folders() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    let dir3 = TempDir::new().unwrap();

    create_test_replay(dir1.path(), "a.SC2Replay", b"replay_a");
    create_test_replay(dir2.path(), "b.SC2Replay", b"replay_b");
    create_test_replay(dir3.path(), "c.SC2Replay", b"replay_c");

    let logger = Arc::new(DebugLogger::new());
    let scanner = ReplayScanner::new(
        vec![dir1.path().to_path_buf(), dir2.path().to_path_buf(), dir3.path().to_path_buf()],
        logger,
    );

    let result = scanner.scan_all_folders().unwrap();
    assert_eq!(result.len(), 3, "Should find replays across all 3 folders");
}
```

- [ ] **Step 2: Add test for dedup across folders**

```rust
#[test]
fn test_scan_dedup_across_folders_by_filename() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();

    // Same content in both folders
    create_test_replay(dir1.path(), "same.SC2Replay", b"identical_content");
    create_test_replay(dir2.path(), "same.SC2Replay", b"identical_content");

    let logger = Arc::new(DebugLogger::new());
    let scanner = ReplayScanner::new(
        vec![dir1.path().to_path_buf(), dir2.path().to_path_buf()],
        logger,
    );

    // scan_all_folders finds both — dedup happens at hash level in filter_and_hash_replays
    let result = scanner.scan_all_folders().unwrap();
    assert_eq!(result.len(), 2, "scan_all_folders returns all files, dedup happens later");
}
```

- [ ] **Step 3: Add test for empty and missing folders**

```rust
#[test]
fn test_scan_empty_and_missing_folders() {
    let empty_dir = TempDir::new().unwrap();
    let valid_dir = TempDir::new().unwrap();
    create_test_replay(valid_dir.path(), "test.SC2Replay", b"replay");

    let missing_path = PathBuf::from("/nonexistent/path/to/replays");

    let logger = Arc::new(DebugLogger::new());
    let scanner = ReplayScanner::new(
        vec![empty_dir.path().to_path_buf(), valid_dir.path().to_path_buf(), missing_path],
        logger,
    );

    let result = scanner.scan_all_folders().unwrap();
    assert_eq!(result.len(), 1, "Should find replay in valid dir, skip empty and missing");
}
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test replay_scanner`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/replay_scanner.rs
git commit -m "test: add scanner tests for multi-folder and edge cases

tests scanning across 3 folders, duplicate files across folders,
and graceful handling of empty/missing folders."
```

### Task 7: Add folder management tests

**Files:**
- Modify: `src-tauri/src/commands/folders.rs`

- [ ] **Step 1: Add integration-style test for auto-detect plus manual**

Add to the `#[cfg(test)] mod tests` block in `folders.rs`:

```rust
#[test]
fn test_auto_detect_plus_manual_merge() {
    let auto_detected = vec!["/sc2/account1/replays".to_string(), "/sc2/account2/replays".to_string()];
    let manual = "/custom/replays";

    let mut all_paths = auto_detected.clone();
    if !all_paths.contains(&manual.to_string()) {
        all_paths.push(manual.to_string());
    }

    assert_eq!(all_paths.len(), 3);
    assert!(all_paths.contains(&"/sc2/account1/replays".to_string()));
    assert!(all_paths.contains(&"/sc2/account2/replays".to_string()));
    assert!(all_paths.contains(&"/custom/replays".to_string()));
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test folders`
Expected: All pass

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/folders.rs
git commit -m "test: add folder management merge tests"
```

### Task 8: Add poller tests

**Files:**
- Modify: `src-tauri/src/upload_manager.rs`

- [ ] **Step 1: Add test for poller start and cancel**

Add to the `#[cfg(test)] mod tests` block in `upload_manager.rs`:

```rust
#[tokio::test]
async fn test_poller_cancel_token_stops_polling() {
    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    let handle = tokio::spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(60)) => {
                panic!("Poller should have been cancelled");
            }
            _ = cancel_clone.cancelled() => {
                // Expected path
            }
        }
    });

    // Cancel immediately
    cancel.cancel();
    handle.await.unwrap();
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test poller`
Expected: Pass

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/upload_manager.rs
git commit -m "test: add poller cancellation test"
```

### Task 9: Final verification

- [ ] **Step 1: Run full Rust test suite**

Run: `cd src-tauri && cargo test`
Expected: All pass, no warnings about unused imports

- [ ] **Step 2: Run full TypeScript test suite**

Run: `npm test`
Expected: All 39 tests pass

- [ ] **Step 3: Build the app to verify compilation**

Run: `npm run build`
Expected: Both TypeScript and Rust builds succeed

- [ ] **Step 4: Commit any final fixes**

```bash
git add -A
git commit -m "chore: final cleanup after uploader simplification"
```
