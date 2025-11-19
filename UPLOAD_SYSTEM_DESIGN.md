# Replay Upload System - Complete Design Document

## Overview

This document describes the complete replay upload system with client-side and server-side deduplication.

## Problem Statement

**Goal**: Automatically upload new SC2 replays while preventing duplicates.

**Challenges**:
1. Large files (~5MB each) - expensive to upload
2. User may have thousands of existing replays
3. User may reinstall app or use multiple devices
4. Need to detect duplicates efficiently without uploading

**Solution**: Two-layer deduplication with hash-based checking.

---

## Architecture

### Layer 1: Client-Side Deduplication

**Location**: Local cache in `~/.config/ladder-legends-uploader/replays.json`

**Purpose**:
- Fast filtering of already-uploaded replays
- Works offline
- Prevents re-scanning same files

**Data Structure**:
```json
{
  "replays": {
    "abc123hash...": {
      "hash": "abc123hash...",
      "filename": "replay1.SC2Replay",
      "filesize": 123456,
      "uploaded_at": 1234567890,
      "filepath": "/path/to/replay1.SC2Replay"
    }
  },
  "total_uploaded": 1
}
```

**Implementation**: ✅ Already implemented in `replay_tracker.rs`

---

### Layer 2: Server-Side Deduplication

**Location**: Vercel Blob Storage per user

**Purpose**:
- Authoritative source of uploaded replays
- Works across devices and reinstalls
- Prevents duplicate uploads from multiple clients

**Storage Path**: `replay-hashes/{discord_user_id}.json`

**Data Structure**:
```json
{
  "discord_user_id": "123456789",
  "updated_at": "2024-01-01T00:00:00Z",
  "hashes": {
    "abc123hash...": {
      "hash": "abc123hash...",
      "filename": "replay1.SC2Replay",
      "filesize": 123456,
      "uploaded_at": "2024-01-01T00:00:00Z",
      "replay_id": "nanoid123"
    }
  },
  "total_count": 1
}
```

**Implementation**: ⚠️ Needs to be created

---

## Complete Upload Flow

### Step-by-Step Process

```
┌─────────────────────────────────────────────────────────────────┐
│ 1. CLIENT: Scan Folder for .SC2Replay Files                    │
│    - Find latest 10 (in dev) or all (in production)            │
│    - Sort by modification time (newest first)                  │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 2. CLIENT: Calculate Hashes                                     │
│    - For each file: SHA-256 hash                                │
│    - Collect: hash, filename, filesize                          │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 3. CLIENT: Local Deduplication (Layer 1)                       │
│    - Check local tracker (replays.json)                        │
│    - Filter out already-uploaded hashes                        │
│    - Result: Potentially new replays                           │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 4. CLIENT → SERVER: Check which hashes are new                 │
│    POST /api/my-replays/check-hashes                           │
│    Body: { hashes: [{ hash, filename, filesize }] }            │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 5. SERVER: Load Hash Manifest                                  │
│    - Fetch from blob: replay-hashes/{user_id}.json             │
│    - If doesn't exist, create empty manifest                   │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 6. SERVER: Compare Hashes                                      │
│    - Check each submitted hash against manifest                │
│    - Return list of truly new hashes                           │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 7. SERVER → CLIENT: Response                                   │
│    { new_hashes: ["hash1", "hash2"], existing_count: 8 }       │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 8. CLIENT: Upload Only New Replays                             │
│    - For each new hash, upload the file                        │
│    - POST /api/my-replays (existing endpoint)                  │
│    - Upload one at a time sequentially                         │
│    - Show progress: "Uploading X of Y..."                      │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 9. SERVER: Process Upload                                      │
│    - Receive file                                              │
│    - Analyze with sc2reader (existing logic)                   │
│    - Store in Vercel KV (existing logic)                       │
│    - Update hash manifest in blob storage (NEW)                │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 10. SERVER → CLIENT: Success Response                          │
│     { success: true, replay: {...} }                            │
└─────────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────────┐
│ 11. CLIENT: Update Local Tracker                               │
│     - Add hash to replays.json                                 │
│     - Increment total_uploaded count                           │
│     - Persist to disk                                          │
└─────────────────────────────────────────────────────────────────┘
```

---

## API Endpoints

### New Endpoint: Check Hashes

**Endpoint**: `POST /api/my-replays/check-hashes`

**Purpose**: Check which replay hashes the server hasn't seen yet

**Authentication**: Required (Bearer token)

**Request**:
```typescript
{
  hashes: Array<{
    hash: string;        // SHA-256 hash
    filename: string;    // e.g., "replay1.SC2Replay"
    filesize: number;    // Size in bytes
  }>
}
```

**Response**:
```typescript
{
  new_hashes: string[];      // Hashes the server hasn't seen
  existing_count: number;    // How many already exist
  total_submitted: number;   // How many were submitted
}
```

**Example**:
```bash
curl -X POST https://ladderlegendsacademy.com/api/my-replays/check-hashes \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "hashes": [
      {
        "hash": "abc123...",
        "filename": "game1.SC2Replay",
        "filesize": 5242880
      },
      {
        "hash": "def456...",
        "filename": "game2.SC2Replay",
        "filesize": 5123456
      }
    ]
  }'

# Response:
{
  "new_hashes": ["abc123..."],  // Only game1 is new
  "existing_count": 1,           // game2 already exists
  "total_submitted": 2
}
```

---

### Modified Endpoint: Upload Replay

**Endpoint**: `POST /api/my-replays` (existing)

**New Behavior**: After successful upload, update hash manifest

**Before**:
1. Receive file
2. Analyze
3. Store in KV
4. Return success

**After**:
1. Receive file
2. Calculate hash
3. Analyze
4. Store in KV
5. **Update hash manifest in blob storage** ← NEW
6. Return success

---

## Implementation Tasks

### Server-Side (ladder-legends-academy)

#### Task 1: Create Hash Manifest Manager

**File**: `src/lib/replay-hash-manifest.ts`

```typescript
import { put, get } from '@vercel/blob';

export interface ReplayHash {
  hash: string;
  filename: string;
  filesize: number;
  uploaded_at: string;
  replay_id: string;
}

export interface HashManifest {
  discord_user_id: string;
  updated_at: string;
  hashes: Record<string, ReplayHash>;
  total_count: number;
}

export class HashManifestManager {
  private getUserManifestPath(discordUserId: string): string {
    return `replay-hashes/${discordUserId}.json`;
  }

  async loadManifest(discordUserId: string): Promise<HashManifest> {
    const path = this.getUserManifestPath(discordUserId);

    try {
      const response = await fetch(`https://...blob.vercel-storage.com/${path}`);
      if (!response.ok) {
        // Manifest doesn't exist, return empty
        return this.createEmptyManifest(discordUserId);
      }
      return await response.json();
    } catch (error) {
      // Return empty manifest if error
      return this.createEmptyManifest(discordUserId);
    }
  }

  async saveManifest(manifest: HashManifest): Promise<void> {
    const path = this.getUserManifestPath(manifest.discord_user_id);
    manifest.updated_at = new Date().toISOString();

    await put(path, JSON.stringify(manifest, null, 2), {
      access: 'public', // Or 'private' if we add signed URLs
      contentType: 'application/json',
    });
  }

  async addHash(
    discordUserId: string,
    hash: string,
    filename: string,
    filesize: number,
    replayId: string
  ): Promise<void> {
    const manifest = await this.loadManifest(discordUserId);

    manifest.hashes[hash] = {
      hash,
      filename,
      filesize,
      uploaded_at: new Date().toISOString(),
      replay_id: replayId,
    };

    manifest.total_count = Object.keys(manifest.hashes).length;

    await this.saveManifest(manifest);
  }

  async checkHashes(
    discordUserId: string,
    hashes: Array<{ hash: string; filename: string; filesize: number }>
  ): Promise<string[]> {
    const manifest = await this.loadManifest(discordUserId);

    return hashes
      .filter(h => !manifest.hashes[h.hash])
      .map(h => h.hash);
  }

  private createEmptyManifest(discordUserId: string): HashManifest {
    return {
      discord_user_id: discordUserId,
      updated_at: new Date().toISOString(),
      hashes: {},
      total_count: 0,
    };
  }
}
```

---

#### Task 2: Create Check Hashes Endpoint

**File**: `src/app/api/my-replays/check-hashes/route.ts`

```typescript
import { NextRequest, NextResponse } from 'next/server';
import { auth } from '@/lib/auth';
import { HashManifestManager } from '@/lib/replay-hash-manifest';

const manifestManager = new HashManifestManager();

export async function POST(request: NextRequest) {
  try {
    const session = await auth();

    if (!session?.user?.discordId) {
      return NextResponse.json({ error: 'Unauthorized' }, { status: 401 });
    }

    const body = await request.json();
    const { hashes } = body;

    if (!Array.isArray(hashes)) {
      return NextResponse.json(
        { error: 'hashes must be an array' },
        { status: 400 }
      );
    }

    // Validate hash format
    for (const h of hashes) {
      if (!h.hash || !h.filename || typeof h.filesize !== 'number') {
        return NextResponse.json(
          { error: 'Each hash must have hash, filename, and filesize' },
          { status: 400 }
        );
      }
    }

    const newHashes = await manifestManager.checkHashes(
      session.user.discordId,
      hashes
    );

    return NextResponse.json({
      new_hashes: newHashes,
      existing_count: hashes.length - newHashes.length,
      total_submitted: hashes.length,
    });
  } catch (error) {
    console.error('Error checking hashes:', error);
    return NextResponse.json(
      { error: 'Failed to check hashes' },
      { status: 500 }
    );
  }
}
```

---

#### Task 3: Update Upload Endpoint to Save Hashes

**File**: `src/app/api/my-replays/route.ts`

**Modify the POST handler**:

```typescript
import { HashManifestManager } from '@/lib/replay-hash-manifest';
import crypto from 'crypto';

const manifestManager = new HashManifestManager();

export async function POST(request: NextRequest) {
  try {
    // ... existing auth and file validation ...

    const formData = await request.formData();
    const file = formData.get('file') as File;

    // Calculate hash
    const buffer = await file.arrayBuffer();
    const hash = crypto
      .createHash('sha256')
      .update(Buffer.from(buffer))
      .digest('hex');

    console.log('📊 Calculated hash:', hash);

    // ... existing analysis code ...

    // Create replay data
    const replayData: UserReplayData = {
      id: nanoid(),
      discord_user_id: session.user.discordId,
      uploaded_at: new Date().toISOString(),
      filename: file.name,
      target_build_id: targetBuildId || detection?.build_id,
      detection,
      comparison,
      fingerprint,
    };

    // Save to KV (existing code)
    await saveReplay(replayData);

    // NEW: Save hash to manifest
    await manifestManager.addHash(
      session.user.discordId,
      hash,
      file.name,
      file.size,
      replayData.id
    );

    console.log('✅ Replay uploaded and hash saved');

    return NextResponse.json({
      success: true,
      replay: replayData,
    });
  } catch (error) {
    // ... existing error handling ...
  }
}
```

---

### Client-Side (ladder-legends-uploader)

#### Task 4: Add Check Hashes Method to ReplayUploader

**File**: `src-tauri/src/replay_uploader.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckHashesRequest {
    pub hashes: Vec<HashInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashInfo {
    pub hash: String,
    pub filename: String,
    pub filesize: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckHashesResponse {
    pub new_hashes: Vec<String>,
    pub existing_count: usize,
    pub total_submitted: usize,
}

impl ReplayUploader {
    /// Check which hashes are new on the server
    pub async fn check_hashes(
        &self,
        hashes: Vec<HashInfo>,
    ) -> Result<CheckHashesResponse, String> {
        let url = format!("{}/api/my-replays/check-hashes", self.base_url);

        let response = self.client
            .post(&url)
            .bearer_auth(&self.access_token)
            .json(&CheckHashesRequest { hashes })
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(format!("Server error {}: {}", status, error_text));
        }

        let data: CheckHashesResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        Ok(data)
    }
}
```

---

#### Task 5: Update Upload Manager to Use Two-Layer Deduplication

**File**: `src-tauri/src/upload_manager.rs`

```rust
/// Scan for new replays and upload them (up to limit)
/// Uses two-layer deduplication: local tracker + server check
pub async fn scan_and_upload(&self, limit: usize) -> Result<usize, String> {
    let tracker = self.tracker.lock().unwrap().clone();

    // Step 1: Find replays in folder
    let all_replays = find_new_replays(&self.replay_folder, &tracker, limit * 2)?;

    if all_replays.is_empty() {
        println!("No new replays found locally");
        return Ok(0);
    }

    // Step 2: Calculate hashes and prepare for server check
    let mut hash_infos = Vec::new();
    let mut replay_map = std::collections::HashMap::new();

    for replay_info in all_replays {
        let hash = ReplayTracker::calculate_hash(&replay_info.path)?;

        hash_infos.push(HashInfo {
            hash: hash.clone(),
            filename: replay_info.filename.clone(),
            filesize: replay_info.filesize,
        });

        replay_map.insert(hash, replay_info);
    }

    // Step 3: Check with server which hashes are new
    println!("Checking {} hashes with server...", hash_infos.len());
    let check_result = self.uploader.check_hashes(hash_infos).await?;

    println!(
        "Server check: {} new, {} existing",
        check_result.new_hashes.len(),
        check_result.existing_count
    );

    // Step 4: Upload only the new replays (up to limit)
    let mut uploaded_count = 0;
    let to_upload: Vec<_> = check_result.new_hashes
        .into_iter()
        .take(limit)
        .collect();

    {
        let mut state = self.state.lock().unwrap();
        state.pending_count = to_upload.len();
    }

    for hash in to_upload {
        let replay_info = match replay_map.get(&hash) {
            Some(info) => info,
            None => continue, // Shouldn't happen
        };

        // Update status to uploading
        {
            let mut state = self.state.lock().unwrap();
            state.current_upload = Some(UploadStatus::Uploading {
                filename: replay_info.filename.clone(),
            });
        }

        // Perform upload
        match self.uploader.upload_replay(&replay_info.path, None, None).await {
            Ok(_) => {
                let tracked_replay = TrackedReplay {
                    hash: hash.clone(),
                    filename: replay_info.filename.clone(),
                    filesize: replay_info.filesize,
                    uploaded_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_secs(),
                    filepath: replay_info.path.to_string_lossy().to_string(),
                };

                // Add to tracker and save
                {
                    let mut tracker = self.tracker.lock().unwrap();
                    tracker.add_replay(tracked_replay);
                    tracker.save()?;
                }

                // Update state
                {
                    let mut state = self.state.lock().unwrap();
                    let tracker = self.tracker.lock().unwrap();
                    state.total_uploaded = tracker.total_uploaded;
                    state.current_upload = Some(UploadStatus::Completed {
                        filename: replay_info.filename.clone(),
                    });
                    state.pending_count = state.pending_count.saturating_sub(1);
                }

                uploaded_count += 1;
            }
            Err(e) => {
                let mut state = self.state.lock().unwrap();
                state.current_upload = Some(UploadStatus::Failed {
                    filename: replay_info.filename.clone(),
                    error: e.clone(),
                });
                state.pending_count = state.pending_count.saturating_sub(1);

                return Err(format!("Failed to upload {}: {}", replay_info.filename, e));
            }
        }
    }

    // Clear current upload status
    {
        let mut state = self.state.lock().unwrap();
        state.current_upload = None;
    }

    Ok(uploaded_count)
}
```

---

## Testing Plan

### Unit Tests

1. **Hash Manifest Manager**
   - ✅ Create empty manifest
   - ✅ Load existing manifest
   - ✅ Add hash to manifest
   - ✅ Check hashes against manifest
   - ✅ Save manifest to blob

2. **Check Hashes Endpoint**
   - ✅ Returns new hashes correctly
   - ✅ Handles empty array
   - ✅ Validates input format
   - ✅ Requires authentication

3. **Client Hash Checking**
   - ✅ Calculates hashes correctly
   - ✅ Sends request to server
   - ✅ Parses response
   - ✅ Handles network errors

### Integration Tests

1. **End-to-End Upload Flow**
   ```
   1. Create 10 test replays
   2. Upload 5 of them
   3. Restart client
   4. Scan folder (should find all 10)
   5. Check hashes with server (should find 5 new)
   6. Upload the 5 new ones
   7. Verify all 10 are now in manifest
   8. Scan again (should find 0 new)
   ```

2. **Cross-Device Scenario**
   ```
   1. Device A uploads 5 replays
   2. Device B (fresh install) scans same folder
   3. Device B should detect 0 new replays (all exist on server)
   ```

3. **Collision Handling**
   ```
   1. Two replays with same content but different filenames
   2. Should have same hash
   3. Only upload once
   ```

---

## Performance Considerations

### Hash Calculation

- **Speed**: ~50ms per 5MB file (SHA-256)
- **10 replays**: ~500ms total
- **Impact**: Negligible, done before network request

### Server Hash Check

- **Request size**: ~1KB for 10 hashes
- **Response size**: ~500 bytes
- **Latency**: <100ms
- **Blob storage read**: <200ms

**Total overhead**: ~300ms before uploading

**Savings**: Avoid uploading 5MB files that already exist

**Example**:
- 10 replays to check
- 5 already exist
- Without checking: Upload 50MB (5 files × 10MB)
- With checking: Upload 25MB (only 5 new files)
- Time saved: ~20 seconds (at 2MB/s upload speed)

### Blob Storage Costs

**Hash manifest size**:
- Per hash: ~150 bytes
- 1000 replays: ~150KB
- 10,000 replays: ~1.5MB

**Vercel Blob pricing** (as of 2024):
- Free tier: 1GB storage
- Even 10K users × 1.5MB = 15GB = $0.15/month

---

## Migration Plan

### Phase 1: Deploy Server-Side (No Breaking Changes)

1. Deploy hash manifest manager
2. Deploy check-hashes endpoint
3. Update upload endpoint to save hashes
4. Test with curl/Postman

**Backward compatible**: Old clients still work

### Phase 2: Update Client

1. Add check_hashes method to ReplayUploader
2. Update upload_manager to use two-layer deduplication
3. Test locally
4. Deploy to users

### Phase 3: Backfill Existing Replays (Optional)

For users with existing replays already uploaded:

1. Create migration script
2. Read all replays from KV
3. Calculate hashes (if stored) or use filename/filesize
4. Generate hash manifest for each user
5. Upload to blob storage

---

## Monitoring & Debugging

### Logs to Add

**Server-side**:
```typescript
console.log('📊 Checking hashes for user:', userId);
console.log('📊 Submitted:', hashes.length, 'New:', newHashes.length);
console.log('💾 Saving hash to manifest:', hash);
```

**Client-side**:
```rust
println!("📁 Found {} replays in folder", all_replays.len());
println!("🔍 Checking {} hashes with server", hash_infos.len());
println!("✅ Server says {} are new", check_result.new_hashes.len());
println!("⬆️  Uploading {}/{}", i+1, to_upload.len());
```

### Metrics to Track

1. **Hash check efficiency**
   - Avg hashes submitted per check
   - Avg new hashes returned
   - Deduplication rate: (existing / total) × 100%

2. **Upload stats**
   - Replays uploaded per user
   - Bytes saved via deduplication
   - Time saved

3. **Manifest size**
   - Avg hashes per user
   - Total blob storage used

---

## Security Considerations

### Hash Manifest Access

**Current**: Public blobs (anyone with URL can read)

**Better**: Private blobs with signed URLs

**Implementation**:
```typescript
import { generateSignedUrl } from '@vercel/blob';

const url = await generateSignedUrl(manifestPath, {
  expiresIn: 3600, // 1 hour
});
```

### Hash Collision Attacks

**Risk**: User creates file with same hash as another user's replay

**Mitigation**:
- SHA-256 is collision-resistant
- Each user has separate manifest
- Files are still stored separately in KV

**Verdict**: Not a concern

---

## Summary

### What We're Building

1. ✅ **Server-side hash manifest** - Blob storage per user
2. ✅ **Check hashes endpoint** - Returns which hashes are new
3. ✅ **Updated upload endpoint** - Saves hashes after upload
4. ✅ **Client two-layer dedup** - Local tracker + server check
5. ✅ **Smart upload flow** - Only upload what's needed

### Benefits

✅ **Efficient**: Avoid uploading duplicates
✅ **Fast**: Hash check is <300ms
✅ **Scalable**: Works with thousands of replays
✅ **Cross-device**: Works across reinstalls/devices
✅ **Reliable**: Double verification (client + server)

### Next Steps

1. Implement server-side hash manifest manager
2. Create check-hashes endpoint
3. Update upload endpoint
4. Test with Postman/curl
5. Update client code
6. Integration testing
7. Deploy! 🚀
