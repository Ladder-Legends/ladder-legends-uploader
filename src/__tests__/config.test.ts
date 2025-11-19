/**
 * Tests for configuration module
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('config', () => {
  beforeEach(() => {
    // Clear any module cache
    vi.resetModules();
  });

  it('should default API_HOST to production URL', async () => {
    const { API_HOST } = await import('../config');
    expect(API_HOST).toBe('https://ladderlegendsacademy.com');
  });

  it('should have correct timeout values', async () => {
    const { DETECTION_TIMEOUT_MS, BUTTON_INIT_DELAY_MS } = await import('../config');
    expect(DETECTION_TIMEOUT_MS).toBe(8000);
    expect(BUTTON_INIT_DELAY_MS).toBe(100);
  });
});
