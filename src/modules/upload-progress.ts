/**
 * Upload progress tracking module
 * Listens to Tauri events and updates UI with upload status
 */

/**
 * Decode HTML entities in a string
 * Handles common entities like &lt; &gt; &amp; &#91; &#93; etc.
 */
function decodeHTMLEntities(text: string): string {
  const doc = new DOMParser().parseFromString(text, 'text/html');
  return doc.documentElement.textContent ?? text;
}

import type {
  UploadState,
  UploadStartEvent,
  UploadCheckingEvent,
  UploadCheckCompleteEvent,
  UploadProgressEvent,
  UploadBatchStartEvent,
  UploadBatchCompleteEvent,
  UploadCompleteEvent,
  UploadErrorEvent,
  ReplayDetectedEvent,
} from '../types';

// Default upload state - used for initialization and reset
const DEFAULT_UPLOAD_STATE: UploadState = {
  isUploading: false,
  current: null,
  total: null,
  filename: null,
  completedCount: null,
  showCompleted: false,
  checkingCount: null,
  totalReplays: null,
  currentBatchGameType: null,
  currentBatchPlayerName: null,
  currentBatchCount: null,
  backgroundDetectedCount: 0,
  showBackgroundNotification: false,
  hasError: false,
  errorFilename: null,
  errorMessage: null,
};

// Track detected replays in background (list of filenames)
let backgroundDetectedReplays: string[] = [];

// Timeout for fading out notification
let notificationFadeTimeout: ReturnType<typeof setTimeout> | null = null;

// Global upload state
let uploadState: UploadState = { ...DEFAULT_UPLOAD_STATE };

// Timeout for hiding completed message
let completedTimeout: ReturnType<typeof setTimeout> | null = null;

// Track if window is currently focused
let isWindowFocused = true;

/**
 * Extract filename from a full path
 */
function getFilenameFromPath(path: string): string {
  // Handle both Windows (backslash) and Unix (forward slash) paths
  const parts = path.split(/[/\\]/);
  return parts[parts.length - 1] || path;
}

/**
 * Handle new replay detected event (from file watcher)
 */
export function handleReplayDetected(path: string): void {
  const filename = getFilenameFromPath(path);
  console.log('[DEBUG] New replay detected in background:', filename);

  // Only track if window is not focused (background detection)
  if (!isWindowFocused) {
    backgroundDetectedReplays.push(filename);
    uploadState.backgroundDetectedCount = backgroundDetectedReplays.length;
    console.log('[DEBUG] Background replay count:', uploadState.backgroundDetectedCount);
  }
}

/**
 * Show background notification when window gains focus
 */
export function showBackgroundNotification(): void {
  if (backgroundDetectedReplays.length === 0) return;

  const count = backgroundDetectedReplays.length;
  console.log('[DEBUG] Showing background notification for', count, 'replay(s)');

  uploadState.showBackgroundNotification = true;
  uploadState.backgroundDetectedCount = count;

  // Show the notification
  const message = count === 1
    ? `Found 1 new replay while away`
    : `Found ${count} new replays while away`;
  updateWatchingStatus(message, false);

  // Clear the tracked replays
  backgroundDetectedReplays = [];

  // Start fade out after 4 seconds
  if (notificationFadeTimeout) {
    clearTimeout(notificationFadeTimeout);
  }
  notificationFadeTimeout = setTimeout(() => {
    uploadState.showBackgroundNotification = false;
    uploadState.backgroundDetectedCount = 0;
    updateUI();
  }, 4000);
}

/**
 * Handle window focus event
 */
export function handleWindowFocus(): void {
  console.log('[DEBUG] Window gained focus');
  isWindowFocused = true;

  // Show notification if we have background detections
  if (backgroundDetectedReplays.length > 0) {
    showBackgroundNotification();
  }
}

/**
 * Handle window blur event
 */
export function handleWindowBlur(): void {
  console.log('[DEBUG] Window lost focus');
  isWindowFocused = false;
}

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
  uploadState = { ...DEFAULT_UPLOAD_STATE };
  backgroundDetectedReplays = [];
  updateUI();
}

/**
 * Update the batch header UI element
 */
export function updateBatchHeader(gameType: string | null, playerName: string | null): void {
  const uploadStatusEl = document.getElementById('upload-status');
  const batchGameTypeEl = document.getElementById('batch-game-type');
  const batchPlayerNameEl = document.getElementById('batch-player-name');

  if (!uploadStatusEl || !batchGameTypeEl || !batchPlayerNameEl) return;

  if (gameType && playerName) {
    uploadStatusEl.classList.remove('hidden');
    batchGameTypeEl.textContent = gameType;
    batchPlayerNameEl.textContent = decodeHTMLEntities(playerName);
  } else {
    uploadStatusEl.classList.add('hidden');
  }
}

/**
 * Update the replay info UI element
 */
export function updateReplayInfo(current: number | null, total: number | null, filename: string | null): void {
  const replayProgressEl = document.getElementById('replay-progress');
  const replayFilenameEl = document.getElementById('replay-filename');

  if (!replayProgressEl || !replayFilenameEl) return;

  if (current !== null && total !== null && filename) {
    replayProgressEl.textContent = `[${current}/${total}]`;
    replayFilenameEl.textContent = decodeHTMLEntities(filename);
  } else {
    replayProgressEl.textContent = '';
    replayFilenameEl.textContent = '';
  }
}

/**
 * Update the watching status UI element
 */
export function updateWatchingStatus(text: string, fadeOut: boolean = false): void {
  const watchingStatusEl = document.getElementById('watching-status');
  if (!watchingStatusEl) return;

  watchingStatusEl.textContent = text;
  if (fadeOut) {
    watchingStatusEl.classList.add('fade-out');
  } else {
    watchingStatusEl.classList.remove('fade-out');
  }
}

/**
 * Update the error display UI element
 */
export function updateErrorDisplay(hasError: boolean, filename: string | null, error: string | null): void {
  const errorContainerEl = document.getElementById('error-container');
  const errorMessageEl = document.getElementById('error-message');
  const errorFilenameEl = document.getElementById('error-filename');

  if (!errorContainerEl) return;

  if (hasError && error) {
    errorContainerEl.classList.remove('hidden');
    if (errorMessageEl) {
      // Truncate long error messages
      const displayError = error.length > 100 ? error.substring(0, 100) + '...' : error;
      errorMessageEl.textContent = displayError;
    }
    if (errorFilenameEl && filename) {
      errorFilenameEl.textContent = filename;
    }
  } else {
    errorContainerEl.classList.add('hidden');
  }
}

/**
 * Update UI based on current upload state
 */
export function updateUI(): void {
  // Always update error display first
  updateErrorDisplay(uploadState.hasError, uploadState.errorFilename, uploadState.errorMessage);

  if (uploadState.hasError) {
    // Show error state - watching status shows "Upload failed"
    updateWatchingStatus('Upload failed', false);
    updateBatchHeader(null, null);
    updateReplayInfo(null, null, null);
  } else if (uploadState.showBackgroundNotification) {
    // Background notification is showing, don't override it
    // The notification message was already set by showBackgroundNotification()
    updateBatchHeader(null, null);
    updateReplayInfo(null, null, null);
  } else if (uploadState.showCompleted && uploadState.completedCount !== null) {
    // Show completion message
    const count = uploadState.completedCount;
    const totalText = uploadState.totalReplays !== null ? ` (${uploadState.totalReplays} total)` : '';
    const message = count > 0
      ? `Uploaded ${count} new replay${count === 1 ? '' : 's'}${totalText}`
      : `No new replays to upload${totalText}`;

    updateWatchingStatus(message, false);
    updateBatchHeader(null, null); // Hide batch header
    updateReplayInfo(null, null, null); // Hide replay info
  } else if (uploadState.isUploading) {
    // Show upload progress with batch information
    if (uploadState.current !== null && uploadState.total !== null && uploadState.filename) {
      // Update batch header if we have batch info
      updateBatchHeader(uploadState.currentBatchGameType, uploadState.currentBatchPlayerName);
      // Update individual replay progress
      updateReplayInfo(uploadState.current, uploadState.total, uploadState.filename);
      // Hide watching status during upload
      updateWatchingStatus('', false);
    } else if (uploadState.total !== null) {
      updateWatchingStatus(`Found ${uploadState.total} new replay${uploadState.total === 1 ? '' : 's'} to upload`, false);
      updateBatchHeader(null, null);
      updateReplayInfo(null, null, null);
    } else if (uploadState.checkingCount !== null) {
      updateWatchingStatus(`Checking ${uploadState.checkingCount} replay${uploadState.checkingCount === 1 ? '' : 's'} for duplicates...`, false);
      updateBatchHeader(null, null);
      updateReplayInfo(null, null, null);
    } else {
      updateWatchingStatus('Scanning for replays...', false);
      updateBatchHeader(null, null);
      updateReplayInfo(null, null, null);
    }
  } else {
    // Default watching state - show total replay count if available
    const totalText = uploadState.totalReplays !== null ? ` (${uploadState.totalReplays} replays tracked)` : '';
    updateWatchingStatus(`Waiting for new replays${totalText}`, false);
    updateBatchHeader(null, null);
    updateReplayInfo(null, null, null);
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
 * Handle upload-batch-start event
 */
export function handleUploadBatchStart(event: UploadBatchStartEvent): void {
  console.log('[DEBUG] Batch started:', event.game_type, 'for', event.player_name, '-', event.count, 'replays');
  uploadState.currentBatchGameType = event.game_type;
  uploadState.currentBatchPlayerName = event.player_name;
  uploadState.currentBatchCount = event.count;
  updateUI();
}

/**
 * Handle upload-progress event
 */
export function handleUploadProgress(event: UploadProgressEvent): void {
  console.log('[DEBUG] Upload progress:', event.current, 'of', event.total, '-', event.filename, `(${event.game_type} for ${event.player_name})`);
  uploadState.isUploading = true;
  uploadState.current = event.current;
  uploadState.total = event.total;
  uploadState.filename = event.filename;
  // Update batch info from progress event (in case batch-start was missed)
  uploadState.currentBatchGameType = event.game_type;
  uploadState.currentBatchPlayerName = event.player_name;
  updateUI();
}

/**
 * Handle upload-batch-complete event
 */
export function handleUploadBatchComplete(event: UploadBatchCompleteEvent): void {
  console.log('[DEBUG] Batch completed:', event.game_type, 'for', event.player_name, '-', event.count, 'replays');
  // Batch is complete, but don't clear batch info yet - wait for next batch or upload complete
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
  // Clear batch info
  uploadState.currentBatchGameType = null;
  uploadState.currentBatchPlayerName = null;
  uploadState.currentBatchCount = null;
  // Clear any previous error
  uploadState.hasError = false;
  uploadState.errorFilename = null;
  uploadState.errorMessage = null;
  updateUI();

  // Clear previous timeout
  if (completedTimeout) {
    clearTimeout(completedTimeout);
  }

  // Start fade out after 3 seconds, then transition to watching state
  completedTimeout = setTimeout(() => {
    const statusEl = document.querySelector('#authenticated-state .status') as HTMLElement;
    if (!statusEl) return;

    // Add fade-out class
    statusEl.classList.add('fade-out');

    // After fade completes (1 second), switch to watching state
    setTimeout(() => {
      uploadState.showCompleted = false;
      updateUI();
    }, 1000);
  }, 3000);
}

/**
 * Handle upload-error event
 */
export function handleUploadError(event: UploadErrorEvent): void {
  console.log('[DEBUG] Upload error:', event.filename, '-', event.error);
  uploadState.isUploading = false;
  uploadState.hasError = true;
  uploadState.errorFilename = event.filename;
  uploadState.errorMessage = event.error;
  // Keep current/total for context
  uploadState.showCompleted = false;
  // Clear batch info
  uploadState.currentBatchGameType = null;
  uploadState.currentBatchPlayerName = null;
  uploadState.currentBatchCount = null;
  updateUI();
}

/**
 * Clear error state (called when retry is clicked)
 */
export function clearError(): void {
  uploadState.hasError = false;
  uploadState.errorFilename = null;
  uploadState.errorMessage = null;
  updateUI();
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

    await listen<UploadBatchStartEvent>('upload-batch-start', (event) => {
      handleUploadBatchStart(event.payload);
    });

    await listen<UploadProgressEvent>('upload-progress', (event) => {
      handleUploadProgress(event.payload);
    });

    await listen<UploadBatchCompleteEvent>('upload-batch-complete', (event) => {
      handleUploadBatchComplete(event.payload);
    });

    await listen<UploadCompleteEvent>('upload-complete', (event) => {
      handleUploadComplete(event.payload);
    });

    await listen<UploadErrorEvent>('upload-error', (event) => {
      handleUploadError(event.payload);
    });

    // Listen for new replays detected by file watcher (background detection)
    await listen<ReplayDetectedEvent>('new-replay-detected', (event) => {
      handleReplayDetected(event.payload);
    });

    // Set up window focus/blur listeners for background notification
    window.addEventListener('focus', handleWindowFocus);
    window.addEventListener('blur', handleWindowBlur);

    console.log('[DEBUG] Upload progress listeners initialized');
  } catch (error) {
    console.error('[DEBUG] Failed to initialize upload progress listeners:', error);
  }
}
