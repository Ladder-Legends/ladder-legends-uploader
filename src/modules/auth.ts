/**
 * Authentication module
 * Handles device code flow and token management
 */

import { getInvoke } from '../lib/tauri';
import { showError, showState } from '../lib/state';
import { setTextContent, setImageSrc, setupButton } from '../lib/ui';
import { initializeUploadSystem } from './upload';
import type { DeviceCodeResponse, AuthorizationResponse, AuthTokens } from '../types';

/**
 * Start device code authentication flow
 */
export async function startDeviceAuth(): Promise<void> {
  try {
    showState('detecting');
    const invoke = getInvoke();

    // Request device code from server
    const response = await invoke('request_device_code') as DeviceCodeResponse;

    // Display code to user
    setTextContent('user-code', response.user_code);

    // Store verification URL for opening browser
    window.verificationUrl = response.verification_uri;

    showState('code');

    // Set up buttons
    setupButton('open-browser-btn', () => openBrowser());
    setupButton('check-auth-btn', () => checkAuthorization(response.device_code));
  } catch (error) {
    showError(`Failed to request device code: ${error}`);
  }
}

/**
 * Open browser to activation page
 */
export async function openBrowser(): Promise<void> {
  try {
    if (!window.verificationUrl) {
      console.error('No verification URL available');
      return;
    }
    const invoke = getInvoke();
    await invoke('open_browser', { url: window.verificationUrl });
  } catch (error) {
    console.error('Failed to open browser:', error);
  }
}

/**
 * Check authorization status (called when user clicks button)
 * If code is expired/invalid, automatically regenerates a new one
 */
export async function checkAuthorization(deviceCode: string): Promise<void> {
  const checkBtn = document.getElementById('check-auth-btn') as HTMLButtonElement | null;
  const openBrowserBtn = document.getElementById('open-browser-btn') as HTMLButtonElement | null;
  if (!checkBtn) return;

  const originalText = checkBtn.textContent || 'Check Authorization';

  try {
    checkBtn.disabled = true;
    checkBtn.textContent = 'Checking...';

    const invoke = getInvoke();
    const response = await invoke('poll_device_authorization', { deviceCode }) as AuthorizationResponse;

    // Success! Show authenticated state
    setTextContent('username', response.user.username);
    if (response.user.avatar_url) {
      setImageSrc('user-avatar', response.user.avatar_url);
    }

    showState('authenticated');

    // Store tokens securely with user data
    await invoke('save_auth_tokens', {
      accessToken: response.access_token,
      refreshToken: response.refresh_token || null,
      expiresAt: response.expires_at || null,
      username: response.user.username || null,
      avatarUrl: response.user.avatar_url || null
    });

    // Set up settings button
    const { openSettings } = await import('./settings');
    setupButton('settings-btn', () => openSettings());

    // Listen for settings events from tray/menu
    try {
      const { listen } = await import('@tauri-apps/api/event');
      await listen('open-settings', () => {
        openSettings();
      });
    } catch (error) {
      console.error('[DEBUG] Failed to setup settings event listener:', error);
    }

    // Initialize upload system
    await initializeUploadSystem(response.access_token);
  } catch (error) {
    checkBtn.disabled = false;
    checkBtn.textContent = originalText;

    // Handle different error types
    if (error === 'pending') {
      // Still waiting - show a message
      const statusEl = document.querySelector('#code-state .status');
      if (statusEl) {
        statusEl.textContent = 'Not authorized yet. Please complete authorization on the website.';
        setTimeout(() => {
          statusEl.textContent = 'After completing activation, click below:';
        }, 3000);
      }
      return;
    }

    // Code expired or invalid - auto-regenerate
    if (error === 'expired' || String(error).includes('invalid') || String(error).includes('not found')) {
      console.log('[DEBUG] Code expired/invalid, regenerating...');
      await regenerateDeviceCode(checkBtn, openBrowserBtn);
      return;
    }

    if (error === 'denied') {
      showError('Authorization denied. Please try again.');
      return;
    }

    // Unknown error
    console.error('Authorization error:', error);
    showError(`Authorization failed: ${error}`);
  }
}

/**
 * Regenerate a new device code when the current one expires
 */
async function regenerateDeviceCode(
  checkBtn: HTMLButtonElement | null,
  openBrowserBtn: HTMLButtonElement | null
): Promise<void> {
  try {
    if (checkBtn) {
      checkBtn.disabled = true;
      checkBtn.textContent = 'Getting new code...';
    }

    const invoke = getInvoke();
    const response = await invoke('request_device_code') as DeviceCodeResponse;

    // Update the displayed code
    setTextContent('user-code', response.user_code);

    // Update the verification URL
    window.verificationUrl = response.verification_uri;

    // Show status message
    const statusEl = document.querySelector('#code-state .status');
    if (statusEl) {
      statusEl.textContent = 'Code expired - here\'s a fresh one! Click below to continue:';
    }

    // Re-enable buttons and update handlers
    if (openBrowserBtn) {
      openBrowserBtn.classList.remove('hidden');
      openBrowserBtn.onclick = () => openBrowser();
    }

    if (checkBtn) {
      checkBtn.disabled = false;
      checkBtn.textContent = 'Check Authorization';
      // Update the click handler with the new device code
      checkBtn.onclick = () => checkAuthorization(response.device_code);
    }

  } catch (error) {
    console.error('[DEBUG] Failed to regenerate device code:', error);
    showError('Failed to get new code. Please restart the app.');

    if (checkBtn) {
      checkBtn.disabled = false;
      checkBtn.textContent = 'Retry';
    }
  }
}

/**
 * Verify saved auth tokens and show authenticated state if valid
 */
export async function verifySavedTokens(tokens: AuthTokens): Promise<boolean> {
  try {
    console.log('[DEBUG] Found saved auth tokens, verifying...');
    const invoke = getInvoke();

    const isValid = await invoke('verify_auth_token', { accessToken: tokens.access_token }) as boolean;
    console.log('[DEBUG] Token verification result:', isValid);

    if (isValid) {
      // Token is valid, show authenticated state
      showState('authenticated');

      // Load user info from saved tokens
      if (tokens.user) {
        setTextContent('username', tokens.user.username || 'Logged In');
        if (tokens.user.avatar_url) {
          setImageSrc('user-avatar', tokens.user.avatar_url);
        }
      } else {
        // Fallback if no user data saved
        setTextContent('username', 'Logged In');
      }

      // Set up settings button
      const { openSettings } = await import('./settings');
      setupButton('settings-btn', () => openSettings());

      // Listen for settings events from tray/menu
      try {
        const { listen } = await import('@tauri-apps/api/event');
        await listen('open-settings', () => {
          openSettings();
        });
      } catch (error) {
        console.error('[DEBUG] Failed to setup settings event listener:', error);
      }

      // Initialize upload system
      await initializeUploadSystem(tokens.access_token);

      return true;
    } else {
      // Token is invalid, clear it
      console.log('[DEBUG] Token is invalid, clearing and re-authenticating...');
      await invoke('clear_auth_tokens');
      return false;
    }
  } catch (error) {
    // Verification failed (network error, etc.), clear tokens
    console.error('[DEBUG] Token verification failed:', error);
    const invoke = getInvoke();
    await invoke('clear_auth_tokens');
    return false;
  }
}
