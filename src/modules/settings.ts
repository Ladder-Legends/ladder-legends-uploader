/**
 * Settings module
 * Handles application settings (autostart, logout, etc.)
 */

import { getInvoke } from '../lib/tauri';
import { showError, showState } from '../lib/state';
import { setupButton, getElement } from '../lib/ui';

/**
 * Open settings panel
 */
export async function openSettings(): Promise<void> {
  console.log('[DEBUG] Opening settings');
  showState('settings');

  // Scroll settings content to top
  const settingsContent = document.querySelector('#settings-state .settings-content');
  if (settingsContent) {
    settingsContent.scrollTop = 0;
  }

  // Load current autostart setting
  try {
    const invoke = getInvoke();
    const enabled = await invoke('get_autostart_enabled') as boolean;
    const toggle = getElement<HTMLInputElement>('autostart-toggle');
    if (toggle) {
      toggle.checked = enabled;
    }
  } catch (error) {
    console.error('Failed to load autostart setting:', error);
  }

  // Set up event listeners
  setTimeout(() => {
    // Autostart toggle
    const autostartToggle = getElement<HTMLInputElement>('autostart-toggle');
    if (autostartToggle) {
      autostartToggle.addEventListener('change', async (e) => {
        try {
          const invoke = getInvoke();
          const target = e.target as HTMLInputElement;
          await invoke('set_autostart_enabled', { enabled: target.checked });
        } catch (error) {
          console.error('Failed to set autostart:', error);
          // Revert on error
          const target = e.target as HTMLInputElement;
          target.checked = !target.checked;
        }
      });
    }

    // Logout button
    setupButton('logout-btn', () => handleLogout());

    // Export debug log button
    setupButton('export-debug-log-btn', () => handleExportDebugLog());

    // Back button
    setupButton('back-from-settings-btn', () => {
      showState('authenticated');
    });
  }, 100);
}

// Store the last exported log path for the "Open Folder" button
let lastExportedLogPath: string | null = null;

/**
 * Handle debug log export
 */
export async function handleExportDebugLog(): Promise<void> {
  console.log('[DEBUG] Exporting debug log');

  const button = getElement('export-debug-log-btn');
  const resultContainer = getElement('debug-log-result');
  const pathDisplay = getElement('debug-log-path');

  if (!button || !resultContainer || !pathDisplay) {
    return;
  }

  try {
    // Show loading state
    const originalText = button.textContent;
    button.textContent = 'Exporting...';
    button.setAttribute('disabled', 'true');

    const invoke = getInvoke();
    const logPath = await invoke('export_debug_log') as string;

    // Store the path for the "Open Folder" button
    lastExportedLogPath = logPath;

    // Show success and log path
    button.textContent = '✓ Exported!';
    pathDisplay.textContent = logPath;
    resultContainer.classList.remove('hidden');

    // Set up the "Open Folder" button
    setupButton('open-debug-folder-btn', () => handleOpenDebugFolder());

    // Reset button after 3 seconds
    setTimeout(() => {
      button.textContent = originalText;
      button.removeAttribute('disabled');
    }, 3000);
  } catch (error) {
    console.error('Failed to export debug log:', error);
    button.textContent = 'Export Failed';
    button.removeAttribute('disabled');

    setTimeout(() => {
      button.textContent = 'Export Debug Log';
    }, 2000);

    showError(`Failed to export debug log: ${error}`);
  }
}

/**
 * Handle opening the folder containing the debug log
 */
export async function handleOpenDebugFolder(): Promise<void> {
  if (!lastExportedLogPath) {
    showError('No debug log has been exported yet');
    return;
  }

  try {
    const invoke = getInvoke();
    await invoke('open_folder_for_path', { path: lastExportedLogPath });
  } catch (error) {
    console.error('Failed to open folder:', error);
    showError(`Failed to open folder: ${error}`);
  }
}

/**
 * Handle logout
 */
export async function handleLogout(): Promise<void> {
  console.log('[DEBUG] Logging out');

  if (!confirm('Are you sure you want to logout?')) {
    return;
  }

  try {
    const invoke = getInvoke();
    // Clear tokens
    await invoke('clear_auth_tokens');

    // Restart the app
    location.reload();
  } catch (error) {
    console.error('Failed to logout:', error);
    showError(`Failed to logout: ${error}`);
  }
}
