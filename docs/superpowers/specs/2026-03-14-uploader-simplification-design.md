# Uploader Simplification Design

**Date:** 2026-03-14
**Status:** Approved
**Goal:** Simplify the uploader to a dumb pipe — find, dedup, upload. The server owns identity, ownership, and analysis.

---

## Context

Three issues with the current uploader:

1. **File watcher dies immediately** — the native OS file watcher thread exits right after start, logging "Native watcher event loop ended." The heartbeat/polling fallback partially compensates but the architecture is fragile.
2. **Player name filtering drops replays** — replays where the user's name isn't found as an active player are silently skipped. This breaks for shared replays, multiple accounts with different names, name changes, and coaching review scenarios.
3. **Missing test coverage** — no integration tests for scanning across multiple folders, no tests for the watcher lifecycle, no tests verifying all competitive replays are uploaded.

The server already handles identity via toon_id matching and identity backfill. The uploader shouldn't make ownership decisions.

---

## Design

### 1. Remove Player Name Filtering from Scan Pipeline

**Current flow:**
```
scan folders -> filter game type -> filter player name -> dedup local -> hash -> dedup server -> upload
```

**New flow:**
```
scan folders -> filter game type -> dedup local -> hash -> dedup server -> upload
```

**Changes in `services/replay_scanner.rs`:**

- `scan_and_prepare()` drops the `player_names` parameter
- `filter_and_hash_replays()` removes Filter 2 (player name check, lines 253-276)
- Delete `find_user_in_game()` (lines 330-342)
- Delete `detect_players_from_replays()` (lines 187-215)
- `PreparedReplay.player_name` becomes the globally detected best-guess name, same for all replays in a batch

**Changes in `upload_manager.rs`:**

- `fetch_player_names()` still runs — result used as upload hint for sc2reader perspective detection
- Player name frequency detection stays for when API returns empty — result is only a hint, never a filter
- Upload grouping changes from `(game_type, player_name)` to `game_type` only — all replays share the best-guess player name
- **Fallback when no names detected:** If both `fetch_player_names()` and local frequency detection return empty, `player_name` in the upload payload is `""` (empty string). The server upload endpoint already handles this — sc2reader uses its own `suggested_player` logic when no player name hint is provided.
- `ReplayGroup` struct and `group_replays_by_type_and_player()` should be updated to reflect the new grouping key (game_type only). Rename to `group_replays_by_type()`.

**Kept filters:**
- Game type: only competitive games (`game_type.should_upload()`)
- Local tracker dedup: skip if filename+size already tracked
- Local hash dedup: skip if hash already uploaded
- Server hash dedup: skip if server already has the hash

### 2. Replace Native Watcher with Polling

**Delete:** `file_watcher.rs` entirely — native watcher, heartbeat monitor, event processor, `RobustFileWatcher`, `WatcherConfig`.

**New:** Simple `ReplayPoller` (in `services/replay_poller.rs` or inline in `upload_manager.rs`):

```
scan_and_prepare() -> upload new replays   // immediate first scan
loop {
    sleep(60s)
    if cancellation_token.is_cancelled() -> break
    scan_and_prepare() -> upload new replays
}
```

**Details:**
- Poll interval: 60 seconds (configurable via constant)
- Uses existing `scan_and_prepare()` — local tracker dedup makes re-scanning cheap (skips already-tracked files by filename+size before hashing)
- Single `tokio::spawn` task, no OS threads, no channels
- Clean shutdown via existing `CancellationToken`
- First scan runs immediately on startup (before first sleep), then every 60s after
- Logs only when new replays found, silent otherwise

**Removed components:**
- `RobustFileWatcher` struct and all methods
- `WatcherConfig` struct
- Native watcher OS thread + mpsc channel
- Heartbeat monitor task
- Event processor + deduplication logic
- `start_file_watcher` Tauri command (replaced with `start_polling`)

**Kept components:**
- `scan_replay_folder()` in `replay_tracker.rs`
- `scan_semaphore` for single-flight scanning
- `rescan_needed` atomic flag

**Poller should call `scan_if_available()`** (which respects `scan_semaphore` and `rescan_needed`) rather than calling `scan_and_upload` directly, to preserve the single-flight guard.

### 3. Folder Management — Allow Adding Additional Folders

**Current:** Auto-detect finds folders or user picks one manually. Manual pick replaces the list.

**New:** Folders are a persisted list that grows from three sources:
1. Auto-detected SC2 folders (on first launch)
2. Manually picked folders (user clicks "Add Folder")
3. Previously saved folders (loaded from config on restart)

**Rust changes:**
- New command `add_folder_path(path)` — loads the current persisted list via `load_folder_paths()`, appends the new path if not already present, then calls `save_folder_paths()` with the merged list. This is NOT a rename of `save_folder_path` — it requires a read-modify-write.
- `pick_replay_folder_manual` calls `add_folder_path` instead of `save_folder_path`

**Frontend changes:**
- After auth succeeds, display current folder list with an "Add Folder" button
- "Add Folder" opens the folder picker, appends result
- Show folder list in the main view so users know what's being scanned

**Config:** No schema change — `load_folder_paths()` already returns `Vec<String>`, `save_folder_paths()` already persists the list.

**Not doing:** Remove folder UI, re-running auto-detection, folder existence validation (scan handles missing folders with a warning log).

### 4. Test Coverage

**New scanner tests (`replay_scanner.rs`):**
- `test_scan_uploads_all_competitive_regardless_of_player` — replays with different player names all make it through
- `test_scan_skips_non_competitive` — custom/AI games still filtered
- `test_scan_multiple_folders` — 3 folders with replays, all found
- `test_scan_dedup_across_folders` — same replay in 2 folders, uploaded once
- `test_scan_empty_and_missing_folders` — mix of valid, empty, nonexistent paths handled gracefully

**New poller tests:**
- `test_poller_starts_and_stops` — start, verify runs, cancel, verify stops
- `test_poller_detects_new_file` — file added mid-poll picked up next cycle

**New folder management tests:**
- `test_add_folder_appends` — 2 existing + 1 added = 3 persisted
- `test_add_folder_deduplicates` — adding existing path is idempotent
- `test_auto_detect_plus_manual` — auto-detect 2 + manual 1 = 3 total

**New player name hint tests:**
- `test_scan_upload_hint_when_no_names_detected` — empty player names from API and detection, uploads with `""` player_name

**Existing test changes:**
- Keep all `test_detect_user_player_names_*` tests — detection still used for upload hint
- Remove/update tests that assert replays are filtered by player name

---

## What's NOT Changing

- Server-side upload endpoint (`POST /api/my-replays`)
- Hash deduplication (check-hashes endpoint)
- Identity backfill and toon_id matching
- sc2reader analysis pipeline
- Metrics versioning system
- Auth flow (device code + bearer token)

---

## Files Affected

**Delete:**
- `src-tauri/src/file_watcher.rs`

**Modify:**
- `src-tauri/src/services/replay_scanner.rs` — remove player filtering
- `src-tauri/src/upload_manager.rs` — remove player filtering from scan call, update upload grouping
- `src-tauri/src/commands/upload.rs` — replace `start_file_watcher` with `start_polling`
- `src-tauri/src/commands/folders.rs` or `commands/detection.rs` — add `add_folder_path` command
- `src-tauri/src/lib.rs` — update module declarations, remove file_watcher
- `src/modules/detection.ts` — fix double-click-handler bug (already patched)
- `src/main.ts` — update to use polling instead of watcher

**Create:**
- `src-tauri/src/services/replay_poller.rs` (optional — could be inline in upload_manager)
