# Authentication Flow

## Device Code Flow

```mermaid
sequenceDiagram
    participant FE as Frontend
    participant BE as Tauri Backend
    participant API as Academy API

    FE->>BE: invoke("request_device_code")
    BE->>API: POST /api/auth/device/code
    API-->>BE: { device_code, user_code, verification_uri, expires_in, interval }
    BE-->>FE: DeviceCodeResponse
    FE->>FE: display user_code + verification_uri to user
    loop poll every interval seconds
        FE->>BE: invoke("poll_device_authorization", device_code)
        BE->>API: GET /api/auth/device/poll?device_code=...
        alt still pending (428)
            API-->>BE: Err("pending")
        else approved (200)
            API-->>BE: { access_token, refresh_token, expires_in, user }
            BE-->>FE: AuthResponse
            FE->>BE: invoke("save_auth_tokens", ...)
        else expired (410)
            API-->>BE: Err("expired")
        end
    end
```

## Token Storage (`commands/tokens.rs`)

Tokens are stored in the OS keychain:
- macOS: Keychain (service `ladder-legends-uploader`, account `auth_tokens`)
- Windows: Credential Manager
- Linux: Secret Service

**Auto-migration**: On first `load_auth_tokens` call, if a legacy `auth.json` plaintext file exists in the config directory, it is read, stored in the keychain, and deleted.

**File fallback**: If the keychain is unavailable, tokens fall back to `auth.json` in the config directory.

## Token Verification on Launch

On app launch, the frontend calls `invoke("verify_auth_token")` which hits `POST /api/auth/device/verify`. If the token is invalid, the frontend starts the device code flow.

## 401 Detection and Re-Auth

Any API call in `replay_uploader.rs` that receives a 401 response returns `Err("auth_expired")`. This string propagates up through `UploadExecutor` → `UploadManager` → `commands/upload.rs`, which emits the Tauri event `auth-expired` to the frontend. The frontend then clears stored tokens and restarts the device auth flow.

```mermaid
sequenceDiagram
    participant API as Academy API
    participant RU as ReplayUploader
    participant UE as UploadExecutor
    participant CMD as upload.rs command
    participant FE as Frontend

    API-->>RU: 401 Unauthorized
    RU-->>UE: Err("auth_expired")
    UE-->>CMD: Err("auth_expired")
    CMD->>FE: emit("auth-expired")
    FE->>FE: clear tokens, show login UI
```

## Token Refresh

When `refresh_token` is present and `expires_at` is approaching, the frontend can call `POST /api/auth/device/refresh` directly. The backend does not auto-refresh — refresh is frontend-driven via `invoke("save_auth_tokens")` after receiving new tokens.
