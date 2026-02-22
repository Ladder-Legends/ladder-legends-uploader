/**
 * Upload system module
 * Handles initialization and management of replay uploads
 */

import { getInvoke } from '../lib/tauri';
import { getApiHost } from '../config';
import { initUploadProgress, clearError } from './upload-progress';
import { pickFolderManually } from './detection';

// Guard to prevent multiple initializations
let isInitializing = false;
let hasInitialized = false;

/**
 * Initialize upload manager and start file watcher
 */
export async function initializeUploadSystem(accessToken: string): Promise<void> {
  // Prevent duplicate initialization
  if (isInitializing) {
    console.log('[DEBUG] Upload system already initializing, skipping duplicate call');
    return;
  }
  if (hasInitialized) {
    console.log('[DEBUG] Upload system already initialized, skipping duplicate call');
    return;
  }

  isInitializing = true;

  try {
    console.log('[DEBUG] Initializing upload system...');
    const invoke = getInvoke();

    // Get saved folder paths (supports multiple accounts/regions)
    const savedPaths = await invoke('load_folder_paths') as string[];
    if (!savedPaths || savedPaths.length === 0) {
      console.error('[DEBUG] No folder paths saved');
      // Hide the default "watching" status and show manual picker option
      const watchingStatus = document.getElementById('watching-status');
      if (watchingStatus) {
        watchingStatus.classList.add('hidden');
      }

      // Show manual folder picker button
      const settingsBtn = document.getElementById('settings-btn');
      if (settingsBtn && settingsBtn.parentElement) {
        const pickFolderBtn = document.createElement('button');
        pickFolderBtn.id = 'pick-folder-btn';
        pickFolderBtn.className = 'btn-secondary';
        pickFolderBtn.textContent = 'Choose Replay Folder';
        pickFolderBtn.style.marginTop = '15px';

        // Insert before settings button
        settingsBtn.parentElement.insertBefore(pickFolderBtn, settingsBtn);

        // Set up click handler
        pickFolderBtn.addEventListener('click', async () => {
          const folderPath = await pickFolderManually();
          if (folderPath) {
            // Reload the app to reinitialize with new folder
            location.reload();
          }
        });
      }
      return;
    }

    console.log('[DEBUG] Found', savedPaths.length, 'replay folder(s)');

    // Initialize upload progress listeners
    await initUploadProgress();
    console.log('[DEBUG] Upload progress listeners initialized');

    // Get API host (reads from window.LADDER_LEGENDS_API_HOST if set)
    const baseUrl = getApiHost();
    console.log('[DEBUG] Using base URL:', baseUrl);

    // Initialize upload manager with all folders
    await invoke('initialize_upload_manager', {
      replayFolders: savedPaths,
      baseUrl: baseUrl,
      accessToken: accessToken
    });
    console.log('[DEBUG] Upload manager initialized with', savedPaths.length, 'folder(s)');

    // Start file watcher
    await invoke('start_file_watcher');
    console.log('[DEBUG] File watcher started');

    // Trigger initial scan (limit 10 replays)
    console.log('[DEBUG] Starting initial scan...');
    const uploaded = await invoke('scan_and_upload_replays', { limit: 10 });
    console.log(`[DEBUG] Initial scan complete - uploaded ${uploaded} replays`);

    hasInitialized = true;
  } catch (error) {
    console.error('[DEBUG] Failed to initialize upload system:', error);
    const errorEl = document.getElementById('upload-init-error');
    if (errorEl) {
      errorEl.textContent = `Upload failed to start: ${error}. Please restart the app.`;
      errorEl.classList.remove('hidden');
    }
  } finally {
    isInitializing = false;
  }
}

/**
 * Retry upload - triggers a new scan and upload
 * This is called when the user clicks the retry button after an error
 */
export async function retryUpload(): Promise<void> {
  try {
    console.log('[DEBUG] Retrying upload...');
    const invoke = getInvoke();

    // Clear the error state in the UI
    clearError();

    // Trigger a new scan and upload
    const uploaded = await invoke('scan_and_upload_replays', { limit: 10 });
    console.log(`[DEBUG] Retry complete - uploaded ${uploaded} replays`);
  } catch (error) {
    console.error('[DEBUG] Retry failed:', error);
    // The error event will be emitted by the backend and handled by the UI
  }
}
