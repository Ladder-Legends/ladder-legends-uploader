# API Contract Types

Rust types that match the Academy TypeScript API contracts exactly.

## Purpose

These types ensure that the Desktop Uploader (Rust) and Academy API (TypeScript) stay in sync by:
- Defining exact API request/response structures
- Using Serde for JSON serialization/deserialization
- Providing type-safe methods for common operations
- Validating responses at runtime

## Migration Status

✅ **COMPLETE** - All modules now use `api_contracts` types:
- `replay_uploader.rs` - Migrated to use `StoredReplay`, `UploadReplayResponse`, `CheckHashesResponse`, etc.
- `replay_scanner.rs` - Uses `HashInfo` from `api_contracts`
- `test_harness.rs` - Updated mocks to match contract shapes
- All 129 tests passing

## Corresponding Academy Types

| Rust Type | Academy Type | Location |
|-----------|--------------|----------|
| `CheckHashesResponse` | `CheckHashesResponse` | `src/lib/contracts/uploader-contracts.ts` |
| `UploadReplayResponse` | `UploadReplayResponse` | `src/lib/contracts/uploader-contracts.ts` |
| `ManifestVersionResponse` | `ManifestVersionResponse` | `src/lib/contracts/uploader-contracts.ts` |
| `DevicePollResponse` | `DevicePollResponse` | `src/lib/contracts/uploader-contracts.ts` |
| `UserSettings` | `StoredUserSettings` | `src/lib/contracts/kv-contracts.ts` |

## Key Features

### Discriminated Unions

**UploadReplayResponse** - Uses `#[serde(untagged)]` for success/error variants:
```rust
match response {
    UploadReplayResponse::Success(s) => println!("Uploaded: {}", s.replay.id),
    UploadReplayResponse::Error(e) => eprintln!("Error: {}", e.error.message),
}
```

**DevicePollResponse** - Uses `#[serde(tag = "status")]` for status variants:
```rust
match poll_response {
    DevicePollResponse::Pending => println!("Still waiting..."),
    DevicePollResponse::Success { access_token, .. } => println!("Got token!"),
    _ => {}
}
```

### Helper Methods

**UploadReplayResponse:**
- `is_success()` - Check if upload succeeded
- `replay()` - Get replay if successful
- `error()` - Get error if failed

**DevicePollResponse:**
- `is_success()` - Check if auth complete
- `tokens()` - Get access/refresh tokens

## Testing

Run contract tests:
```bash
cargo test api_contracts
```

Tests validate:
- JSON deserialization from Academy responses
- Type conversions and helper methods
- Enum discriminants work correctly

## Maintaining Sync

When Academy TypeScript types change:

1. **Update Rust types in `api_contracts.rs`**
2. **Run tests:** `cargo test api_contracts`
3. **Update Academy fixtures if needed:** See `ladder-legends-academy/src/__fixtures__/responses/`
4. **Verify integration:** Upload a replay end-to-end

## Pre-Push Hooks

Contract tests run automatically before pushing to prevent breaking changes:

- **Uploader:** `.git/hooks/pre-push` runs `cargo test api_contracts`
- **Academy:** `.git/hooks/pre-push` runs contract + fixture tests
- **sc2reader:** `.git/hooks/pre-push` runs `npm run test:contracts`

If tests fail, the push is aborted. Fix failing tests before pushing.

## Common Patterns

### Handling Optional Fields

Use `Option<T>` explicitly:
```rust
pub struct StoredReplay {
    pub id: String,
    pub fingerprint: Option<ReplayFingerprint>,  // Can be null
}
```

### ISO 8601 Timestamps

Strings, not DateTime (parsed later if needed):
```rust
pub struct ManifestVersionResponse {
    pub manifest_version: String,  // "2025-12-01T15:14:56.505Z"
    pub checked_at: String,
}
```

### Error Codes

Use structured error objects:
```rust
pub struct UploadError {
    pub code: String,       // "REPLAY_DUPLICATE", "PARSE_FAILED", etc.
    pub message: String,    // Human-readable
    pub retryable: bool,    // Whether client should retry
}
```

## See Also

- Academy contracts: `/ladder-legends-academy/src/lib/contracts/`
- Academy fixtures: `/ladder-legends-academy/src/__fixtures__/responses/`
- Contract tests: `/ladder-legends-academy/src/lib/contracts/__tests__/`
