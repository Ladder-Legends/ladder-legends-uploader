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

    // No saved tokens, try to load saved folder path
    const savedPath = await invoke('load_folder_path') as string | null;
    console.log('[DEBUG] Saved folder path:', savedPath);

    if (savedPath) {
      // We have a saved folder, skip detection and go straight to auth
      console.log('[DEBUG] Using saved folder, starting device auth...');
      await startDeviceAuth();
      hasInitialized = true;
      return;
    }

    // No saved path, show detecting state
    showState('detecting');
    console.log('[DEBUG] Showing detecting state');

    // Try to detect SC2 folder with timeout
    console.log('[DEBUG] Starting folder detection...');
    const folderPath = await detectWithTimeout(invoke);
    console.log('[DEBUG] Detection result:', folderPath);

    if (folderPath) {
      // Found folder, go straight to device auth
      console.log('[DEBUG] Found folder, starting device auth...');
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
});
