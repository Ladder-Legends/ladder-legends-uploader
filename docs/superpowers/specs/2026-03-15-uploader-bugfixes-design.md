# Uploader Bugfixes Design

**Date:** 2026-03-15
**Status:** Approved
**Goal:** Fix three bugs: infinite retry of non-retryable errors, manifest version thrashing on hash rollback, and full folder rescan every 60s.

---

## Context

Three bugs found during v0.1.26 testing:

1. **Non-retryable errors retried forever** — 400 `REPLAY_INVALID_FORMAT` errors are retried every 60s poll cycle because the uploader doesn't distinguish retryable from non-retryable failures. The server response includes `retryable: false` but the uploader ignores it.

2. **Hash rollback bumps manifest version** — when an upload fails after hash reservation, the server rolls back the hash by calling `remove()`, which calls `bumpVersion()`. The uploader sees the version change, clears its local cache, and retries everything — creating an infinite failure loop.

3. **Full folder rescan every 60s** — `scan_all_folders()` does `fs::read_dir` + per-file metadata on every poll cycle. For users with tens of thousands of replay files, this is excessive. Scanning should skip directories whose mtime hasn't changed.

---

## Design

### 1. Stop Retrying Non-Retryable Errors

**Uploader changes (Rust):**

**`replay_uploader.rs`:** `upload_replay()` currently returns `Err(String)` on failure, discarding the structured error response. Change it to return a richer error that includes the `retryable` flag. Specifically: when a non-2xx response is received, parse the JSON body for `retryable`. Return an `UploadError` enum with variants `NonRetryable { message }` and `Retryable { message }` (plus `AuthExpired`). This replaces the current `Err(format!("Upload failed ..."))` string.

**`upload_executor.rs`:** Match on the new `UploadError` variants. For `NonRetryable`, add the hash to `permanently_failed`. For `Retryable`, log and continue (existing behavior). For `AuthExpired`, propagate (existing behavior).

**`UploadResult` struct:** Add `permanently_failed: Vec<String>` — hashes of replays that failed with non-retryable errors.

**`UploadManager`:** Add `failed_hashes: Arc<Mutex<HashSet<String>>>` field — in-memory, not persisted to disk. Cleared on app restart.

**`scan_and_upload` method:** After `scan_and_prepare` returns prepared replays, filter out any whose hash is in `failed_hashes`. After upload completes, add `upload_result.permanently_failed` hashes to `failed_hashes`.

**Behavior:**
- Non-retryable error (400) → hash added to `failed_hashes` → skipped on all future polls this session
- Retryable error (500, network) → NOT added to `failed_hashes` → retried next poll
- App restart → `failed_hashes` cleared → replays get a fresh try
- No UI needed for clearing failed replays — restart is the reset mechanism

### 2. Don't Bump Manifest Version on Hash Rollback

**Server changes (TypeScript, ladder-legends-academy):**

**`hash.repository.ts`:** Add `skipVersionBump: boolean = false` parameter to `remove()` and `removeMany()`. When true, skip the `bumpVersion()` call after SREM.

**`hash-service.ts`:** Pass through `skipVersionBump` parameter on `remove()` and `removeMany()`. Update `rollback()` (line 115) to call `hashKVRepository.remove(userId, hash, true)` — rollback is a transactional undo, not a user-initiated deletion, so it should never bump the manifest version.

**`replay-service.ts`:** No changes needed — it calls `hashSvc.rollback()` which now internally skips the version bump.

**Normal deletes stay unchanged:** When a user deletes a replay via the UI, `remove()` is called without `skipVersionBump` (defaults to `false`), so `bumpVersion()` fires as before. The uploader correctly detects the change and re-syncs.

### 3. Skip Unchanged Directories via mtime

**Uploader changes (Rust):**

**`UploadManager`:** Add `folder_mtimes: Arc<Mutex<HashMap<PathBuf, SystemTime>>>` field — in-memory cache of last-seen directory mtime per folder.

**`ReplayScanner`:** Accept a mutable reference to the mtime map. In `scan_all_folders()`, for each folder:

1. Call `fs::metadata(&folder).modified()` — one stat call
2. If mtime matches the stored value → skip this folder entirely (log debug)
3. If mtime is newer or folder not in map → do full `scan_replay_folder()`, update stored mtime
4. If metadata call fails → log warning, skip folder (existing behavior)

**Impact for large libraries:**
- 10,000 replays across 3 folders: 3 `stat()` calls per poll instead of 3 `read_dir` + 10,000 `stat()` calls
- Only when a new replay is saved (directory mtime changes) does the full scan run
- First scan after app start always runs (map is empty)
- **Known limitation:** directory mtime updates on file add/delete, NOT on in-place file modification. This is fine for SC2 replays (always new files), but a replay overwritten in place would be missed until restart.

---

## What's NOT Changing

- Upload endpoint (`POST /api/my-replays`) — server-side validation stays the same
- Server error response format — `retryable` field already exists
- Local tracker persistence format (`replays.json`) — no schema change
- Poll interval (60s) — unchanged
- Hash deduplication flow — unchanged except for version bump skip on rollback

---

## Files Affected

**ladder-legends-uploader (Rust):**
- `src-tauri/src/replay_uploader.rs` — return `UploadError` enum instead of `Err(String)`, parse `retryable` from server response
- `src-tauri/src/services/upload_executor.rs` — match on `UploadError` variants, populate `permanently_failed`
- `src-tauri/src/upload_manager.rs` — add `failed_hashes` and `folder_mtimes` fields, filter in `scan_and_upload`
- `src-tauri/src/services/replay_scanner.rs` — accept mtime map, skip unchanged folders

**ladder-legends-academy (TypeScript):**
- `src/lib/server/repositories/kv/hash.repository.ts` — add `skipVersionBump` to `remove()` and `removeMany()`
- `src/lib/server/services/hash-service.ts` — pass through `skipVersionBump`
- `src/lib/server/services/replay-service.ts` — pass `skipVersionBump: true` on rollback

---

## Testing

**Uploader tests:**
- `test_non_retryable_error_skipped_on_retry` — upload fails with 400, verify hash is in failed set, verify it's filtered on next scan
- `test_retryable_error_not_skipped` — upload fails with 500, verify hash is NOT in failed set
- `test_failed_hashes_cleared_on_new_instance` — new UploadManager has empty failed set
- `test_scan_skips_unchanged_folder` — set mtime in map, call scan, verify `scan_replay_folder` not called
- `test_scan_processes_changed_folder` — set old mtime, call scan, verify folder is scanned

**Academy tests:**
- `test_hash_remove_with_skip_version_bump` — call `remove(userId, hash, true)`, verify version NOT bumped
- `test_hash_remove_without_skip_bumps_version` — call `remove(userId, hash)`, verify version IS bumped (existing behavior preserved)
- `test_hash_rollback_does_not_bump_version` — call `rollback(userId, hash)`, verify version NOT bumped
