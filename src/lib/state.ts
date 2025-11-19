/**
 * UI state management
 * Handles showing/hiding different application states
 */

import type { StateKey } from '../types';

const stateElements: Record<StateKey, HTMLElement | null> = {
  detecting: null,
  code: null,
  authenticated: null,
  settings: null,
  error: null,
};

/**
 * Initialize state elements
 * Must be called after DOM is loaded
 */
export function initStateElements(): void {
  stateElements.detecting = document.getElementById('detecting-state');
  stateElements.code = document.getElementById('code-state');
  stateElements.authenticated = document.getElementById('authenticated-state');
  stateElements.settings = document.getElementById('settings-state');
  stateElements.error = document.getElementById('error-state');
}

/**
 * Show only one state at a time
 */
export function showState(stateName: StateKey): void {
  Object.values(stateElements).forEach(el => {
    if (el) el.classList.add('hidden');
  });

  const targetState = stateElements[stateName];
  if (targetState) {
    targetState.classList.remove('hidden');
  }
}

/**
 * Show error message
 */
export function showError(message: string): void {
  console.log('[DEBUG] Showing error:', message);
  const errorElement = document.getElementById('error-message');
  if (errorElement) {
    errorElement.textContent = message;
  }
  showState('error');

  // Set up retry button immediately when showing error
  // This ensures the button works even if error occurs early in init
  setTimeout(() => {
    const retryBtn = document.getElementById('retry-btn');
    if (retryBtn) {
      // Clone to remove old listeners
      const newBtn = retryBtn.cloneNode(true) as HTMLElement;
      retryBtn.parentNode?.replaceChild(newBtn, retryBtn);
      // Add click handler
      newBtn.addEventListener('click', () => {
        console.log('[DEBUG] Retry button clicked');
        location.reload();
      });
    }
  }, 100);
}
