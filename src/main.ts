/**
 * Main application entry point
 * Coordinates initialization and application flow
 */

import { initTauri } from './lib/tauri';
import { initStateElements, showState } from './lib/state';
import { setupButton } from './lib/ui';
import { detectWithTimeout, showManualPickerOption, pickFolderManually } from './modules/detection';
import { startDeviceAuth, verifySavedTokens } from './modules/auth';
import type { AuthTokens } from './types';

/**
 * Initialize version display and update checker
 */
async function initVersionDisplay(): Promise<void> {
  try {
    const invoke = await initTauri();

    // Get app version from Tauri
    const version = await invoke('get_version') as string;
    const versionText = document.getElementById('version-text');
    if (versionText) {
      versionText.textContent = `v${version}`;
    }

    // Set up "Check for Updates" button
    const checkUpdateBtn = document.getElementById('check-update-btn');
    const updateBadge = document.getElementById('update-badge');

    if (checkUpdateBtn) {
      checkUpdateBtn.classList.remove('hidden');
      checkUpdateBtn.addEventListener('click', async () => {
        try {
          checkUpdateBtn.textContent = 'Checking...';
          const updateAvailable = await invoke('check_for_updates') as boolean;

          if (updateAvailable && updateBadge) {
            updateBadge.classList.remove('hidden');
            checkUpdateBtn.textContent = 'Install Update';

            // Change button to install update with proper error handling
            checkUpdateBtn.onclick = async () => {
              checkUpdateBtn.textContent = 'Installing...';
              (checkUpdateBtn as HTMLButtonElement).disabled = true;
              try {
                await invoke('install_update');
                // If we get here without restart, show restarting message
                checkUpdateBtn.textContent = 'Restarting...';
              } catch (error) {
                console.error('[DEBUG] Update installation failed:', error);
                checkUpdateBtn.textContent = 'Update Failed';
                (checkUpdateBtn as HTMLButtonElement).disabled = false;
                // Show error to user via alert with GitHub download link
                const githubUrl = 'https://github.com/Ladder-Legends/ladder-legends-uploader/releases/latest';
                alert(`Update failed: ${error}\n\nPlease try again or download the latest version from:\n${githubUrl}`);
                // Reset button after delay
                setTimeout(() => {
                  checkUpdateBtn.textContent = 'Install Update';
                }, 3000);
              }
            };
          } else {
            checkUpdateBtn.textContent = 'Up to date!';
            setTimeout(() => {
              checkUpdateBtn.textContent = 'Check for Updates';
            }, 2000);
          }
        } catch (error) {
          console.error('[DEBUG] Update check error:', error);
          checkUpdateBtn.textContent = 'Check Failed';
          setTimeout(() => {
            checkUpdateBtn.textContent = 'Check for Updates';
          }, 2000);
        }
      });
    }
  } catch (error) {
    console.error('[DEBUG] Failed to initialize version display:', error);
  }
}

/**
 * Set up retry button (can be called multiple times)
 */
function setupRetryButton(): void {
  setupButton('retry-btn', () => {
    console.log('[DEBUG] Retry button clicked');
    // Reset to initial state and restart
    location.reload();
  });
}

// Global flag to prevent multiple initializations
let isInitializing = false;
let hasInitialized = false;

/**
 * Initialize the application
 */
async function init(): Promise<void> {
  console.log('[DEBUG] init() called');

  // Prevent multiple simultaneous initializations
  if (isInitializing) {
    console.log('[DEBUG] Already initializing, skipping duplicate init() call');
    return;
  }

  if (hasInitialized) {
    console.log('[DEBUG] Already initialized, skipping init() call');
    return;
  }

  isInitializing = true;

  try {
    // Initialize state elements
    initStateElements();

    // Wait for Tauri to be ready
    console.log('[DEBUG] Checking for Tauri API...');
    const invoke = await initTauri();
    console.log('[DEBUG] invoke function loaded:', typeof invoke);

    // First, check if we have saved auth tokens
    const savedTokens = await invoke('load_auth_tokens') as AuthTokens | null;
    console.log('[DEBUG] Saved auth tokens:', savedTokens ? 'Found' : 'Not found');

    if (savedTokens && savedTokens.access_token) {
      // We have saved tokens - verify they still work
      const isValid = await verifySavedTokens(savedTokens);
      if (isValid) {
        // Token is valid, we're done
        hasInitialized = true;
        return;
      }
      // Fall through to normal auth flow if token is invalid
    }

    // No saved tokens, try to load saved folder paths (supports multiple accounts)
    const savedPaths = await invoke('load_folder_paths') as string[];
    console.log('[DEBUG] Saved folder paths:', savedPaths?.length || 0, 'folder(s)');

    if (savedPaths && savedPaths.length > 0) {
      // We have saved folders, skip detection and go straight to auth
      console.log('[DEBUG] Using', savedPaths.length, 'saved folder(s), starting device auth...');
      await startDeviceAuth();
      hasInitialized = true;
      return;
    }

    // No saved paths, show detecting state
    showState('detecting');
    console.log('[DEBUG] Showing detecting state');

    // Try to detect SC2 folders with timeout (supports multiple accounts)
    console.log('[DEBUG] Starting folder detection...');
    const folderPaths = await detectWithTimeout(invoke);
    console.log('[DEBUG] Detection result:', folderPaths.length, 'folder(s)');

    if (folderPaths && folderPaths.length > 0) {
      // Found folder(s), go straight to device auth
      console.log('[DEBUG] Found', folderPaths.length, 'folder(s), starting device auth...');
      await startDeviceAuth();
      hasInitialized = true;
    }
  } catch (error) {
    // If auto-detection fails, show option to pick manually
    console.error('[DEBUG] Detection error:', error);
    showManualPickerOption(error);

    // Set up manual picker button handler
    setTimeout(() => {
      const manualBtn = document.getElementById('manual-pick-btn');
      if (manualBtn) {
        manualBtn.addEventListener('click', async () => {
          const folderPath = await pickFolderManually();
          if (folderPath) {
            // Go straight to device auth
            await startDeviceAuth();
            hasInitialized = true;
          }
        });
      }
    }, 100);
  } finally {
    isInitializing = false;
  }

  // Set up error retry button
  setupRetryButton();
}

/**
 * Start the app when DOM is loaded
 */
window.addEventListener('DOMContentLoaded', () => {
  console.log('[DEBUG] DOMContentLoaded fired!');
  init();
  initVersionDisplay();
});
