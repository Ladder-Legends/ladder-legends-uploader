/**
 * Upload progress tracking module
 * Listens to Tauri events and updates UI with upload status
 */

import type {
  UploadState,
  UploadStartEvent,
  UploadCheckingEvent,
  UploadCheckCompleteEvent,
  UploadProgressEvent,
  UploadCompleteEvent,
} from '../types';

// Global upload state
let uploadState: UploadState = {
  isUploading: false,
  current: null,
  total: null,
  filename: null,
  completedCount: null,
  showCompleted: false,
};

// Timeout for hiding completed message
let completedTimeout: ReturnType<typeof setTimeout> | null = null;

/**
 * Get current upload state
 */
export function getUploadState(): UploadState {
  return { ...uploadState };
}

/**
 * Reset upload state
 */
export function resetUploadState(): void {
  uploadState = {
    isUploading: false,
    current: null,
    total: null,
    filename: null,
    completedCount: null,
    showCompleted: false,
  };
  updateUI();
}

/**
 * Update UI based on current upload state
 */
export function updateUI(): void {
  const statusEl = document.querySelector('#authenticated-state .status');
  if (!statusEl) return;

  if (uploadState.showCompleted && uploadState.completedCount !== null) {
    // Show completion message
    const count = uploadState.completedCount;
    statusEl.textContent = count > 0
      ? `Uploaded ${count} new replay${count === 1 ? '' : 's'}`
      : 'No new replays to upload';
  } else if (uploadState.isUploading) {
    // Show upload progress
    if (uploadState.current !== null && uploadState.total !== null && uploadState.filename) {
      statusEl.textContent = `Uploading replay ${uploadState.current} of ${uploadState.total}: ${uploadState.filename}`;
    } else if (uploadState.total !== null) {
      statusEl.textContent = `Detected ${uploadState.total} new replay${uploadState.total === 1 ? '' : 's'}`;
    } else {
      statusEl.textContent = 'Checking for new replays...';
    }
  } else {
    // Default watching state
    statusEl.textContent = 'Watching for new replays...';
  }
}

/**
 * Handle upload-start event
 */
export function handleUploadStart(event: UploadStartEvent): void {
  console.log('[DEBUG] Upload started, limit:', event.limit);
  uploadState.isUploading = true;
  uploadState.current = null;
  uploadState.total = null;
  uploadState.filename = null;
  uploadState.showCompleted = false;
  updateUI();
}

/**
 * Handle upload-checking event
 */
export function handleUploadChecking(event: UploadCheckingEvent): void {
  console.log('[DEBUG] Checking hashes:', event.count);
  uploadState.isUploading = true;
  updateUI();
}

/**
 * Handle upload-check-complete event
 */
export function handleUploadCheckComplete(event: UploadCheckCompleteEvent): void {
  console.log('[DEBUG] Check complete:', event.new_count, 'new,', event.existing_count, 'existing');
  uploadState.total = event.new_count;
  updateUI();
}

/**
 * Handle upload-progress event
 */
export function handleUploadProgress(event: UploadProgressEvent): void {
  console.log('[DEBUG] Upload progress:', event.current, 'of', event.total, '-', event.filename);
  uploadState.isUploading = true;
  uploadState.current = event.current;
  uploadState.total = event.total;
  uploadState.filename = event.filename;
  updateUI();
}

/**
 * Handle upload-complete event
 */
export function handleUploadComplete(event: UploadCompleteEvent): void {
  console.log('[DEBUG] Upload complete:', event.count, 'uploaded');
  uploadState.isUploading = false;
  uploadState.current = null;
  uploadState.total = null;
  uploadState.filename = null;
  uploadState.completedCount = event.count;
  uploadState.showCompleted = true;
  updateUI();

  // Clear previous timeout
  if (completedTimeout) {
    clearTimeout(completedTimeout);
  }

  // Hide completion message after 60 seconds
  completedTimeout = setTimeout(() => {
    uploadState.showCompleted = false;
    updateUI();
  }, 60000);
}

/**
 * Initialize upload progress listeners
 */
export async function initUploadProgress(): Promise<void> {
  try {
    // Dynamically import Tauri API
    const { listen } = await import('@tauri-apps/api/event');

    await listen<UploadStartEvent>('upload-start', (event) => {
      handleUploadStart(event.payload);
    });

    await listen<UploadCheckingEvent>('upload-checking', (event) => {
      handleUploadChecking(event.payload);
    });

    await listen<UploadCheckCompleteEvent>('upload-check-complete', (event) => {
      handleUploadCheckComplete(event.payload);
    });

    await listen<UploadProgressEvent>('upload-progress', (event) => {
      handleUploadProgress(event.payload);
    });

    await listen<UploadCompleteEvent>('upload-complete', (event) => {
      handleUploadComplete(event.payload);
    });

    console.log('[DEBUG] Upload progress listeners initialized');
  } catch (error) {
    console.error('[DEBUG] Failed to initialize upload progress listeners:', error);
  }
}
