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

    // Back button
    setupButton('back-from-settings-btn', () => {
      showState('authenticated');
    });
  }, 100);
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
