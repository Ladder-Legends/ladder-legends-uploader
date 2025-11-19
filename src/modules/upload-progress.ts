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
  checkingCount: null,
  totalReplays: null,
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
    checkingCount: null,
    totalReplays: null,
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
    const totalText = uploadState.totalReplays !== null ? ` (${uploadState.totalReplays} total)` : '';
    statusEl.textContent = count > 0
      ? `Uploaded ${count} new replay${count === 1 ? '' : 's'}${totalText}`
      : `No new replays to upload${totalText}`;
  } else if (uploadState.isUploading) {
    // Show upload progress
    if (uploadState.current !== null && uploadState.total !== null && uploadState.filename) {
      statusEl.textContent = `Uploading replay ${uploadState.current} of ${uploadState.total}: ${uploadState.filename}`;
    } else if (uploadState.total !== null) {
      statusEl.textContent = `Found ${uploadState.total} new replay${uploadState.total === 1 ? '' : 's'} to upload`;
    } else if (uploadState.checkingCount !== null) {
      statusEl.textContent = `Checking ${uploadState.checkingCount} replay${uploadState.checkingCount === 1 ? '' : 's'} for duplicates...`;
    } else {
      statusEl.textContent = 'Scanning for replays...';
    }
  } else {
    // Default watching state - show total replay count if available
    const totalText = uploadState.totalReplays !== null ? ` (${uploadState.totalReplays} replays tracked)` : '';
    statusEl.textContent = `Watching for new replays${totalText}`;
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
  uploadState.checkingCount = null;
  updateUI();
}

/**
 * Handle upload-checking event
 */
export function handleUploadChecking(event: UploadCheckingEvent): void {
  console.log('[DEBUG] Checking hashes:', event.count);
  uploadState.isUploading = true;
  uploadState.checkingCount = event.count;
  updateUI();
}

/**
 * Handle upload-check-complete event
 */
export function handleUploadCheckComplete(event: UploadCheckCompleteEvent): void {
  console.log('[DEBUG] Check complete:', event.new_count, 'new,', event.existing_count, 'existing');
  uploadState.total = event.new_count;
  uploadState.checkingCount = null;
  uploadState.totalReplays = event.new_count + event.existing_count;
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
  uploadState.checkingCount = null;
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
