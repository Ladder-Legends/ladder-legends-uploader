# Ladder Legends Uploader - Project Instructions

## Development Methodology

### Domain-Driven Development with Layered Architecture
All code should follow clean layered architecture:

```
Presentation Layer → Application Layer → Service Layer → Repository Layer
```

**Rust Backend Layers:**
- **Presentation**: `src/commands/*.rs` - Tauri commands, event emitters
- **Application**: `src/upload_manager.rs`, `src/services/upload_executor.rs` - Orchestration, use cases
- **Service**: `src/replay_parser.rs`, `src/services/replay_scanner.rs` - Business logic, pure functions
- **Repository**: `src/replay_uploader.rs`, `src/replay_tracker.rs` - API clients, file I/O, persistence

**TypeScript Frontend Layers:**
- **Presentation**: `src/main.ts`, `src/lib/ui.ts` - DOM manipulation, user events
- **Application**: `src/modules/*.ts` - Feature orchestration
- **Service**: `src/lib/*.ts` - Business logic utilities
- **Repository**: API calls via Tauri invoke

### Documentation-Driven Development (BEFORE writing code)
1. **Document first**: Update CLAUDE.md with feature/change description
2. **Define contracts**: Error codes, API schemas, data structures
3. **Write tests**: Based on documented behavior
4. **Implement last**: Code to pass tests and match documentation

### Test-Driven Development (BEFORE implementation)
- Write tests FIRST based on documented behavior
- Unit tests for service/repository layers (pure functions)
- Integration tests for application layer orchestration
- 100% coverage on error handling paths
- **Rust**: `cargo test` - all tests in `#[cfg(test)]` modules
- **TypeScript**: Tests should validate Tauri command responses

### Error Handling Standards
- Use structured error responses with error codes
- Classify errors: 4xx (client/skip) vs 5xx (server/retry)
- Log errors with context (filename, hash, operation)
- Frontend should show user-friendly messages with recovery options

---

## Ladder Legends Uploader - Important Patterns
- **Batch Upload Grouping**: Replays are grouped by (game_type, player_name) using `group_replays_by_type_and_player()` function
- **Event-Driven UI**: Backend emits events (upload-batch-start, upload-progress, upload-batch-complete) that frontend listens to
- **Modular UI Updates**: Separate functions for batch header, replay info, and watching status updates
- **Player Name Extraction**: Uses s2protocol to extract player names from replays before grouping
- **Player Name Detection Algorithm**: Client-side should scan ALL replays to guess user's player name(s) when API doesn't return them
  - Frequency-based heuristic: User appears in ALL their replays (highest frequency)
  - Co-occurrence filtering: "most frequently seen NOT with other most frequently seen"
  - Algorithm: Sort by frequency → filter out names that co-occur with higher-frequency names (practice partners/teammates)
  - 1v1: Filter out frequent opponents | 2v2: Filter out frequent teammates
  - Top 1-2 names after filtering = user's account name(s)
  - Submit guessed names to API as "possible user names" for server-side verification
- **Auth Flow**: 401 error means auth is broken - should use same localhost URL and device auth logic as Academy
- **IMPORTANT - Flush Script**: Located at `ladder-legends-academy/scripts/flush-user-replays.ts`
  ```bash
  # DRY RUN (shows what will be deleted):
  cd ladder-legends-academy
  npx tsx scripts/flush-user-replays.ts

  # EXECUTE (actually deletes data):
  npx tsx scripts/flush-user-replays.ts 161384451518103552 --execute

  # Clears:
  # - All replays from KV (server-side)
  # - Hash manifest (duplicate detection)
  # - Uploader local tracker (replays.json)
  # - possible_player_names and confirmed_player_names (user settings)
  ```
- **Data Directory**: macOS stores app data in `~/Library/Application Support/ladder-legends-uploader/`
- **Replay Tracker**: Uses JSON file (`replays.json`), NOT sqlite
- **Error Handling**: `UploaderError` enum in `errors.rs` (thiserror). 401 API responses propagate as the sentinel string `"auth_expired"` — callers check `e.contains("auth_expired")` and emit the `auth-expired` Tauri event.
- **Concurrency**: `scan_semaphore: Semaphore(1)` in `UploadManager` for single-flight scan; `rescan_needed: AtomicBool` flags a follow-up scan if one is already running. `CancellationToken` (tokio_util) for clean shutdown — checked at each await point in `scan_and_upload` and `scan_if_available`.
- **Atomic Writes**: `atomic_write_json()` in `config_utils.rs` — writes to `.tmp`, fsyncs, then renames to target. Ensures the target file is never partially written.
- **Token Storage**: OS keychain (macOS Keychain / Windows Credential Manager / Linux Secret Service) via the `keyring` crate. `load_auth_tokens` auto-migrates legacy plaintext `auth.json` → keychain on first call. File-based fallback if keychain is unavailable.
- **Auth Flow**: 401 → `"auth_expired"` error string → `auth-expired` Tauri event → frontend clears tokens → restart device auth flow. Device auth: `POST /api/auth/device/code` → display code to user → poll `GET /api/auth/device/poll` → `save_auth_tokens` to keychain.
