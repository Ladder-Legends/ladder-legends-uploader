/**
 * Tests for state management module
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { initStateElements, showState, showError } from '../lib/state';

describe('state', () => {
  beforeEach(() => {
    // Set up minimal DOM
    document.body.innerHTML = `
      <div id="detecting-state" class="hidden">Detecting</div>
      <div id="code-state" class="hidden">Code</div>
      <div id="authenticated-state" class="hidden">Authenticated</div>
      <div id="settings-state" class="hidden">Settings</div>
      <div id="error-state" class="hidden">
        <div id="error-message"></div>
      </div>
    `;
  });

  it('should initialize state elements', () => {
    initStateElements();
    expect(document.getElementById('detecting-state')).toBeTruthy();
    expect(document.getElementById('code-state')).toBeTruthy();
  });

  it('should show only one state at a time', () => {
    initStateElements();

    showState('detecting');
    expect(document.getElementById('detecting-state')?.classList.contains('hidden')).toBe(false);
    expect(document.getElementById('code-state')?.classList.contains('hidden')).toBe(true);

    showState('code');
    expect(document.getElementById('detecting-state')?.classList.contains('hidden')).toBe(true);
    expect(document.getElementById('code-state')?.classList.contains('hidden')).toBe(false);
  });

  it('should show error message', () => {
    initStateElements();

    showError('Test error message');
    expect(document.getElementById('error-state')?.classList.contains('hidden')).toBe(false);
    expect(document.getElementById('error-message')?.textContent).toBe('Test error message');
  });
});
