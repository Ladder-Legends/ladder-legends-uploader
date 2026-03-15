# Uploader Bugfixes Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three bugs: infinite retry of non-retryable errors, manifest version thrashing on hash rollback, and full folder rescan every 60s.

**Architecture:** Bug 1 (retry) adds an `UploadError` enum to the uploader's Rust code and an in-memory `failed_hashes` set. Bug 2 (rollback) adds `skipVersionBump` to the academy's hash repository. Bug 3 (rescan) adds directory mtime tracking to the scanner.

**Tech Stack:** Rust (Tauri uploader), TypeScript (Next.js academy), Vercel KV (Redis)

**Spec:** `docs/superpowers/specs/2026-03-15-uploader-bugfixes-design.md`

---

## File Structure

**ladder-legends-uploader (Rust):**
- Modify: `src-tauri/src/replay_uploader.rs` — add `UploadError` enum, return it from `upload_replay()`
- Modify: `src-tauri/src/services/upload_executor.rs` — match on `UploadError`, add `permanently_failed` to `UploadResult`
- Modify: `src-tauri/src/upload_manager.rs` — add `failed_hashes` and `folder_mtimes` fields, filter failed hashes in `scan_and_upload`
- Modify: `src-tauri/src/services/replay_scanner.rs` — accept mtime map, skip unchanged folders in `scan_all_folders`

**ladder-legends-academy (TypeScript):**
- Modify: `src/lib/server/repositories/kv/hash.repository.ts` — add `skipVersionBump` param to `remove()` and `removeMany()`
- Modify: `src/lib/server/services/hash-service.ts` — pass `skipVersionBump` through, update `rollback()` to skip bump

---

## Chunk 1: Hash Rollback Fix (Academy)

### Task 1: Add `skipVersionBump` to hash repository and service

**Files:**
- Modify: `src/lib/server/repositories/kv/hash.repository.ts` (in ladder-legends-academy)
- Modify: `src/lib/server/services/hash-service.ts` (in ladder-legends-academy)

**Working directory:** `/Users/chadfurman/projects/lla/ladder-legends-academy`

- [ ] **Step 1: Add `skipVersionBump` parameter to `remove()` in hash.repository.ts**

Change the `remove` method signature (line 92) from:
```typescript
async remove(userId: string, hash: string): Promise<boolean> {
```
To:
```typescript
async remove(userId: string, hash: string, skipVersionBump = false): Promise<boolean> {
```

Wrap the `bumpVersion` call (line 96) in a conditional:
```typescript
if (removed > 0) {
  if (!skipVersionBump) {
    await this.bumpVersion(userId);
  }
  console.log(`[HashKV] Removed hash ${hash.substring(0, 8)}... for user ${userId}`);
  return true;
}
```

- [ ] **Step 2: Add `skipVersionBump` parameter to `removeMany()` in hash.repository.ts**

Change the `removeMany` method signature (line 111) from:
```typescript
async removeMany(userId: string, hashes: string[]): Promise<number> {
```
To:
```typescript
async removeMany(userId: string, hashes: string[], skipVersionBump = false): Promise<number> {
```

Wrap the `bumpVersion` call (line 117) in a conditional:
```typescript
if (removed > 0) {
  if (!skipVersionBump) {
    await this.bumpVersion(userId);
  }
  console.log(`[HashKV] Removed ${removed} hashes for user ${userId}`);
}
```

- [ ] **Step 3: Pass through `skipVersionBump` in hash-service.ts `remove()` and `removeMany()`**

Update `remove` (line 68):
```typescript
async remove(userId: string, hash: string, skipVersionBump = false): Promise<boolean> {
  return hashKVRepository.remove(userId, hash, skipVersionBump);
}
```

Update `removeMany` (line 76):
```typescript
async removeMany(userId: string, hashes: string[], skipVersionBump = false): Promise<number> {
  return hashKVRepository.removeMany(userId, hashes, skipVersionBump);
}
```

- [ ] **Step 4: Update `rollback()` in hash-service.ts to skip version bump**

Change line 116 from:
```typescript
await hashKVRepository.remove(userId, hash);
```
To:
```typescript
await hashKVRepository.remove(userId, hash, true);
```

- [ ] **Step 5: Run tests**

Run: `cd /Users/chadfurman/projects/lla/ladder-legends-academy && npx vitest --run`
Expected: All 2003 tests pass

- [ ] **Step 6: Commit**

```bash
git add src/lib/server/repositories/kv/hash.repository.ts src/lib/server/services/hash-service.ts
git commit -m "fix: don't bump manifest version on hash rollback

adds skipVersionBump parameter to remove() and removeMany().
rollback() now passes skipVersionBump=true so transactional
undos don't trigger uploader cache invalidation."
```

---

## Chunk 2: Non-Retryable Error Handling (Uploader)

### Task 2: Add `UploadError` enum to `replay_uploader.rs`

**Files:**
- Modify: `src-tauri/src/replay_uploader.rs` (in ladder-legends-uploader)

**Working directory:** `/Users/chadfurman/projects/lla/ladder-legends-uploader`

- [ ] **Step 1: Define `UploadError` enum**

Add near the top of `replay_uploader.rs`, after the imports:

```rust
/// Structured upload error with retryability information
#[derive(Debug)]
pub enum UploadError {
    /// Auth token expired — caller should trigger re-auth
    AuthExpired,
    /// Non-retryable error (400-level) — don't retry this replay
    NonRetryable { message: String },
    /// Retryable error (500-level, network) — retry on next poll
    Retryable { message: String },
}

impl std::fmt::Display for UploadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UploadError::AuthExpired => write!(f, "auth_expired"),
            UploadError::NonRetryable { message } => write!(f, "{}", message),
            UploadError::Retryable { message } => write!(f, "{}", message),
        }
    }
}
```

- [ ] **Step 2: Change `upload_replay()` return type from `Result<StoredReplay, String>` to `Result<StoredReplay, UploadError>`**

Update the signature (line 68):
```rust
pub async fn upload_replay(
    ...
) -> Result<StoredReplay, UploadError> {
```

- [ ] **Step 3: Update error returns in `upload_replay()` to use `UploadError`**

Change the network error (line 120):
```rust
.map_err(|e| UploadError::Retryable { message: format!("Network error: {}", e) })?;
```

Change the file read error (line 78):
```rust
.map_err(|e| UploadError::Retryable { message: format!("Failed to read file: {}", e) })?;
```

Change the filename error (line 82):
```rust
.ok_or(UploadError::NonRetryable { message: "Invalid filename".to_string() })?;
```

Change the URL parse error (line 88):
```rust
.map_err(|e| UploadError::NonRetryable { message: format!("Invalid base URL: {}", e) })?;
```

Change the non-success response block (lines 122-128):
```rust
if !response.status().is_success() {
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(UploadError::AuthExpired);
    }
    let error_text = response.text().await.unwrap_or_default();

    // Parse retryable field from server error response
    let retryable = serde_json::from_str::<serde_json::Value>(&error_text)
        .ok()
        .and_then(|v| v.get("retryable").and_then(|r| r.as_bool()))
        .unwrap_or(status.is_server_error()); // default: 5xx = retryable, 4xx = not

    let message = format!("Upload failed {} {}: {}", status.as_u16(), status.canonical_reason().unwrap_or(""), error_text);
    return if retryable {
        Err(UploadError::Retryable { message })
    } else {
        Err(UploadError::NonRetryable { message })
    };
}
```

Change the response parse error (line 134):
```rust
.map_err(|e| UploadError::Retryable { message: format!("Failed to parse response: {}", e) })?;
```

Change the error response handling (lines 140-143):
```rust
None => {
    if let Some(error) = data.error() {
        Err(UploadError::NonRetryable { message: format!("Upload failed: {} ({})", error.message, error.code) })
    } else {
        Err(UploadError::Retryable { message: "Upload failed with unknown error".to_string() })
    }
}
```

- [ ] **Step 4: Run `cargo check` to verify compilation**

Run: `cd src-tauri && cargo check`
Expected: Will fail — `upload_executor.rs` still expects `Err(String)`. That's expected, fixed in Task 3.

- [ ] **Step 5: Commit (won't compile yet — Task 3 completes this)**

```bash
git add src-tauri/src/replay_uploader.rs
git commit -m "refactor: add UploadError enum with retryability info

upload_replay() now returns UploadError::NonRetryable for 400s
and UploadError::Retryable for 500s/network errors. parses the
server's retryable field from the JSON error response."
```

### Task 3: Update `upload_executor.rs` to handle `UploadError`

**Files:**
- Modify: `src-tauri/src/services/upload_executor.rs` (in ladder-legends-uploader)

- [ ] **Step 1: Add `permanently_failed` to `UploadResult`**

Update the struct (line 17):
```rust
#[derive(Debug)]
pub struct UploadResult {
    pub uploaded_count: usize,
    pub errors: Vec<String>,
    pub permanently_failed: Vec<String>,
}
```

Initialize it in `execute()` where `UploadResult` is created (find the empty-replays early return):
```rust
return Ok(UploadResult {
    uploaded_count: 0,
    errors: Vec::new(),
    permanently_failed: Vec::new(),
});
```

- [ ] **Step 2: Import `UploadError` and update the error handling in `upload_single_replay`**

Add import:
```rust
use crate::replay_uploader::UploadError;
```

In the `upload_single_replay` method, the match on `self.uploader.upload_replay(...)` currently has `Err(e)` where `e` is a `String`. Change it to match on `UploadError`:

```rust
Err(e) => {
    match &e {
        UploadError::AuthExpired => {
            return Err("auth_expired".to_string());
        }
        _ => {
            // Check for 409 duplicate in the error message
            let msg = e.to_string();
            if msg.contains("409") || msg.contains("REPLAY_DUPLICATE") || msg.contains("already been uploaded") {
                self.logger.info(format!(
                    "Replay {} already exists on server (treating as success)",
                    prepared.file_info.filename
                ));
                if let Err(e) = self.handle_upload_success(prepared, hash) {
                    self.handle_upload_failure(&prepared.file_info.filename, &format!("save failed: {}", e), app);
                    return Err(e);
                }
                return Ok(());
            }

            self.handle_upload_failure(&prepared.file_info.filename, &msg, app);
            Err(format!("Failed to upload {}: {}", prepared.file_info.filename, msg))
        }
    }
}
```

- [ ] **Step 3: Track permanently failed hashes in `execute()`**

In the `execute()` method, after iterating through all replays in all groups, collect permanently failed hashes. Find where individual upload errors are collected into `upload_errors`. After an upload fails, check if the error came from a `NonRetryable` variant. The simplest approach: change the per-replay error collection to also track the hash.

Add a `permanently_failed: Vec<String>` alongside `upload_errors`. When a replay fails and the error string contains "Upload failed 4" (indicating a 4xx status), add its hash to `permanently_failed`.

Actually, cleaner approach: have `upload_single_replay` return a richer result. But to minimize changes, just check the error message:

After each failed upload in the group loop, check if the error indicates non-retryable:
```rust
// In the group iteration loop where upload_errors are collected:
if error_msg.contains("Upload failed 4") {
    permanently_failed.push(hash.clone());
}
```

Include `permanently_failed` in the final `UploadResult`.

- [ ] **Step 4: Run `cargo test`**

Run: `cd src-tauri && cargo test`
Expected: All tests pass (compilation should succeed now)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/services/upload_executor.rs
git commit -m "feat: track permanently failed uploads in UploadResult

executor now handles UploadError enum, tracks non-retryable
failures separately from retryable ones."
```

### Task 4: Add `failed_hashes` to `UploadManager`

**Files:**
- Modify: `src-tauri/src/upload_manager.rs` (in ladder-legends-uploader)

- [ ] **Step 1: Add `failed_hashes` field to `UploadManager` struct**

Add after `in_flight_hashes` (line 199):
```rust
/// Hashes that failed with non-retryable errors this session (in-memory only)
failed_hashes: Arc<Mutex<HashSet<String>>>,
```

Initialize in `new()` (after line 234):
```rust
failed_hashes: Arc::new(Mutex::new(HashSet::new())),
```

- [ ] **Step 2: Filter failed hashes in `scan_and_upload`**

After Step 4 (the `in_flight_hashes` filter, around line 340), add another filter for `failed_hashes`:

```rust
// Step 4b: Filter out permanently failed replays
let filtered_replays: Vec<_> = {
    let failed = self.failed_hashes.lock()
        .unwrap_or_else(|e| e.into_inner());

    let (to_upload, skipped): (Vec<_>, Vec<_>) = filtered_replays
        .into_iter()
        .partition(|r| !failed.contains(&r.hash));

    if !skipped.is_empty() {
        self.logger.info(format!(
            "Skipped {} replay(s) with non-retryable errors",
            skipped.len()
        ));
    }

    to_upload
};
```

- [ ] **Step 3: Add permanently failed hashes after upload completes**

After the `upload_result` is returned from the executor (around line 394), add:

```rust
// Add permanently failed hashes to the skip set
let upload_result = upload_result?;
if !upload_result.permanently_failed.is_empty() {
    let mut failed = self.failed_hashes.lock()
        .unwrap_or_else(|e| e.into_inner());
    for hash in &upload_result.permanently_failed {
        failed.insert(hash.clone());
    }
    self.logger.info(format!(
        "Marked {} replay(s) as permanently failed (won't retry this session)",
        upload_result.permanently_failed.len()
    ));
}
```

- [ ] **Step 4: Run `cargo test`**

Run: `cd src-tauri && cargo test`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/upload_manager.rs
git commit -m "feat: skip permanently failed replays on future polls

in-memory failed_hashes set filters out non-retryable upload
failures. cleared on app restart to give replays a fresh try."
```

---

## Chunk 3: Directory mtime Optimization (Uploader)

### Task 5: Add mtime-based scan skipping to `ReplayScanner`

**Files:**
- Modify: `src-tauri/src/services/replay_scanner.rs` (in ladder-legends-uploader)
- Modify: `src-tauri/src/upload_manager.rs` (in ladder-legends-uploader)

- [ ] **Step 1: Add `folder_mtimes` field to `UploadManager`**

Add to the struct (after `failed_hashes`):
```rust
/// Last-seen directory mtime per folder (for scan optimization)
folder_mtimes: Arc<Mutex<HashMap<PathBuf, std::time::SystemTime>>>,
```

Add import at top if not present:
```rust
use std::time::SystemTime;
```

Initialize in `new()`:
```rust
folder_mtimes: Arc::new(Mutex::new(HashMap::new())),
```

- [ ] **Step 2: Pass mtime map to `ReplayScanner`**

Update the `ReplayScanner::new()` call in `scan_and_upload` (line 306) to also accept the mtime map:

```rust
let folder_mtimes = Arc::clone(&self.folder_mtimes);
let scanner = ReplayScanner::new(self.replay_folders.clone(), Arc::clone(&self.logger), folder_mtimes);
```

- [ ] **Step 3: Update `ReplayScanner` to accept and use mtime map**

Add the field to `ReplayScanner`:
```rust
pub struct ReplayScanner {
    replay_folders: Vec<PathBuf>,
    logger: Arc<DebugLogger>,
    folder_mtimes: Arc<Mutex<HashMap<PathBuf, SystemTime>>>,
}
```

Update `new()`:
```rust
pub fn new(
    replay_folders: Vec<PathBuf>,
    logger: Arc<DebugLogger>,
    folder_mtimes: Arc<Mutex<HashMap<PathBuf, SystemTime>>>,
) -> Self {
    Self { replay_folders, logger, folder_mtimes }
}
```

- [ ] **Step 4: Add mtime check to `scan_all_folders()`**

In `scan_all_folders()`, before calling `scan_replay_folder`, check directory mtime:

```rust
fn scan_all_folders(&self) -> Result<Vec<ReplayFileInfo>, String> {
    let mut all_replays = Vec::new();
    let mut mtimes = self.folder_mtimes.lock()
        .unwrap_or_else(|e| e.into_inner());

    for folder in &self.replay_folders {
        // Check directory mtime — skip if unchanged since last scan
        let current_mtime = match std::fs::metadata(folder).and_then(|m| m.modified()) {
            Ok(mtime) => mtime,
            Err(e) => {
                self.logger.warn(format!(
                    "Cannot stat {}: {}, skipping",
                    folder.display(), e
                ));
                continue;
            }
        };

        if let Some(last_mtime) = mtimes.get(folder) {
            if *last_mtime == current_mtime {
                self.logger.debug(format!(
                    "Skipping {} (mtime unchanged)",
                    folder.display()
                ));
                continue;
            }
        }

        // mtime changed or first scan — do full scan
        match scan_replay_folder(folder) {
            Ok(replays) => {
                self.logger.debug(format!(
                    "Found {} replays in {}",
                    replays.len(),
                    folder.display()
                ));
                all_replays.extend(replays);
                mtimes.insert(folder.clone(), current_mtime);
            }
            Err(e) => {
                self.logger.warn(format!(
                    "Error scanning {}: {}",
                    folder.display(), e
                ));
            }
        }
    }

    Ok(all_replays)
}
```

- [ ] **Step 5: Fix existing tests that call `ReplayScanner::new` with 2 args**

Update all test calls from `ReplayScanner::new(folders, logger)` to `ReplayScanner::new(folders, logger, Arc::new(Mutex::new(HashMap::new())))`.

Add imports to test module:
```rust
use std::sync::Mutex;
use std::collections::HashMap;
```

- [ ] **Step 6: Add test for mtime skip**

```rust
#[test]
fn test_scan_skips_unchanged_folder() {
    let dir = TempDir::new().unwrap();
    create_test_replay(dir.path(), "test.SC2Replay", b"replay");

    let current_mtime = std::fs::metadata(dir.path()).unwrap().modified().unwrap();

    // Pre-populate mtime map with current value
    let mtimes = Arc::new(Mutex::new(HashMap::from([
        (dir.path().to_path_buf(), current_mtime),
    ])));

    let logger = Arc::new(DebugLogger::new());
    let scanner = ReplayScanner::new(
        vec![dir.path().to_path_buf()],
        logger,
        mtimes,
    );

    let result = scanner.scan_all_folders().unwrap();
    assert_eq!(result.len(), 0, "Should skip folder with unchanged mtime");
}

#[test]
fn test_scan_processes_new_folder() {
    let dir = TempDir::new().unwrap();
    create_test_replay(dir.path(), "test.SC2Replay", b"replay");

    // Empty mtime map — folder not seen before
    let mtimes = Arc::new(Mutex::new(HashMap::new()));

    let logger = Arc::new(DebugLogger::new());
    let scanner = ReplayScanner::new(
        vec![dir.path().to_path_buf()],
        logger,
        Arc::clone(&mtimes),
    );

    let result = scanner.scan_all_folders().unwrap();
    assert_eq!(result.len(), 1, "Should scan folder not in mtime map");

    // Verify mtime was recorded
    let map = mtimes.lock().unwrap();
    assert!(map.contains_key(dir.path()), "Should record mtime after scan");
}
```

- [ ] **Step 7: Run `cargo test`**

Run: `cd src-tauri && cargo test`
Expected: All pass

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/services/replay_scanner.rs src-tauri/src/upload_manager.rs
git commit -m "perf: skip unchanged directories via mtime check

scan_all_folders now checks directory mtime before read_dir.
folders with unchanged mtime are skipped entirely. saves
thousands of stat() calls for users with large replay libraries."
```

---

## Chunk 4: Final Verification

### Task 6: Final verification across both repos

- [ ] **Step 1: Run full academy test suite**

Run: `cd /Users/chadfurman/projects/lla/ladder-legends-academy && npx vitest --run`
Expected: All 2003+ tests pass

- [ ] **Step 2: Run full uploader test suite**

Run: `cd /Users/chadfurman/projects/lla/ladder-legends-uploader/src-tauri && cargo test`
Expected: All 174+ tests pass

- [ ] **Step 3: Run uploader TypeScript tests**

Run: `cd /Users/chadfurman/projects/lla/ladder-legends-uploader && npm test`
Expected: All 39 tests pass

- [ ] **Step 4: Push academy changes**

Run: `cd /Users/chadfurman/projects/lla/ladder-legends-academy && git push origin main`

- [ ] **Step 5: Push uploader changes**

Run: `cd /Users/chadfurman/projects/lla/ladder-legends-uploader && git push origin main`
