/**
 * Upload system module
 * Handles initialization and management of replay uploads
 */

import { getInvoke } from '../lib/tauri';
import { getApiHost } from '../config';
import { initUploadProgress } from './upload-progress';

/**
 * Initialize upload manager and start file watcher
 */
export async function initializeUploadSystem(accessToken: string): Promise<void> {
  try {
    console.log('[DEBUG] Initializing upload system...');
    const invoke = getInvoke();

    // Get saved folder path
    const savedPath = await invoke('load_folder_path') as string;
    if (!savedPath) {
      console.error('[DEBUG] No folder path saved');
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
          const { pickFolderManually } = await import('./detection');
          const folderPath = await pickFolderManually();
          if (folderPath) {
            // Reload the app to reinitialize with new folder
            location.reload();
          }
        });
      }
      return;
    }

    // Initialize upload progress listeners
    await initUploadProgress();
    console.log('[DEBUG] Upload progress listeners initialized');

    // Get API host (reads from window.LADDER_LEGENDS_API_HOST if set)
    const baseUrl = getApiHost();
    console.log('[DEBUG] Using base URL:', baseUrl);

    // Initialize upload manager
    await invoke('initialize_upload_manager', {
      replayFolder: savedPath,
      baseUrl: baseUrl,
      accessToken: accessToken
    });
    console.log('[DEBUG] Upload manager initialized');

    // Start file watcher
    await invoke('start_file_watcher');
    console.log('[DEBUG] File watcher started');

    // Trigger initial scan (limit 10 replays)
    console.log('[DEBUG] Starting initial scan...');
    const uploaded = await invoke('scan_and_upload_replays', { limit: 10 });
    console.log(`[DEBUG] Initial scan complete - uploaded ${uploaded} replays`);
  } catch (error) {
    console.error('[DEBUG] Failed to initialize upload system:', error);
    // Don't show error to user - just log it
  }
}
