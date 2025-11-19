/**
 * Tests for upload progress module
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import {
  getUploadState,
  resetUploadState,
  updateUI,
  handleUploadStart,
  handleUploadChecking,
  handleUploadCheckComplete,
  handleUploadProgress,
  handleUploadComplete,
} from '../modules/upload-progress';

describe('upload-progress', () => {
  beforeEach(() => {
    // Set up minimal DOM
    document.body.innerHTML = `
      <div id="authenticated-state">
        <p class="status">Watching for new replays...</p>
      </div>
    `;
    resetUploadState();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  describe('getUploadState', () => {
    it('should return default state initially', () => {
      const state = getUploadState();
      expect(state.isUploading).toBe(false);
      expect(state.current).toBeNull();
      expect(state.total).toBeNull();
      expect(state.filename).toBeNull();
      expect(state.completedCount).toBeNull();
      expect(state.showCompleted).toBe(false);
    });

    it('should return a copy of the state', () => {
      const state1 = getUploadState();
      const state2 = getUploadState();
      expect(state1).not.toBe(state2);
      expect(state1).toEqual(state2);
    });
  });

  describe('resetUploadState', () => {
    it('should reset state to initial values', () => {
      handleUploadProgress({ current: 1, total: 5, filename: 'test.SC2Replay' });

      resetUploadState();

      const state = getUploadState();
      expect(state.isUploading).toBe(false);
      expect(state.current).toBeNull();
      expect(state.total).toBeNull();
    });

    it('should update UI after reset', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadProgress({ current: 1, total: 5, filename: 'test.SC2Replay' });
      resetUploadState();

      expect(statusEl?.textContent).toBe('Watching for new replays...');
    });
  });

  describe('updateUI', () => {
    it('should show default message when not uploading', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      updateUI();

      expect(statusEl?.textContent).toBe('Watching for new replays...');
    });

    it('should show checking message when uploading without details', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadStart({ limit: 100 });

      expect(statusEl?.textContent).toBe('Checking for new replays...');
    });

    it('should show detected count after check complete', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadStart({ limit: 100 });
      handleUploadCheckComplete({ new_count: 5, existing_count: 10 });

      expect(statusEl?.textContent).toBe('Detected 5 new replays');
    });

    it('should show singular replay when count is 1', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadStart({ limit: 100 });
      handleUploadCheckComplete({ new_count: 1, existing_count: 0 });

      expect(statusEl?.textContent).toBe('Detected 1 new replay');
    });

    it('should show upload progress with filename', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadProgress({ current: 3, total: 10, filename: 'MyReplay.SC2Replay' });

      expect(statusEl?.textContent).toBe('Uploading replay 3 of 10: MyReplay.SC2Replay');
    });

    it('should show completion message', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadComplete({ count: 7 });

      expect(statusEl?.textContent).toBe('Uploaded 7 new replays');
    });

    it('should show singular replay in completion message when count is 1', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadComplete({ count: 1 });

      expect(statusEl?.textContent).toBe('Uploaded 1 new replay');
    });

    it('should show no new replays message when count is 0', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadComplete({ count: 0 });

      expect(statusEl?.textContent).toBe('No new replays to upload');
    });
  });

  describe('handleUploadStart', () => {
    it('should set isUploading to true', () => {
      handleUploadStart({ limit: 100 });

      const state = getUploadState();
      expect(state.isUploading).toBe(true);
    });

    it('should clear previous upload details', () => {
      handleUploadProgress({ current: 5, total: 10, filename: 'test.SC2Replay' });
      handleUploadStart({ limit: 100 });

      const state = getUploadState();
      expect(state.current).toBeNull();
      expect(state.total).toBeNull();
      expect(state.filename).toBeNull();
    });

    it('should hide completed message', () => {
      handleUploadComplete({ count: 5 });
      handleUploadStart({ limit: 100 });

      const state = getUploadState();
      expect(state.showCompleted).toBe(false);
    });
  });

  describe('handleUploadCheckComplete', () => {
    it('should set total count', () => {
      handleUploadCheckComplete({ new_count: 15, existing_count: 20 });

      const state = getUploadState();
      expect(state.total).toBe(15);
    });

    it('should maintain isUploading state', () => {
      handleUploadStart({ limit: 100 });
      handleUploadCheckComplete({ new_count: 5, existing_count: 10 });

      const state = getUploadState();
      expect(state.isUploading).toBe(true);
    });
  });

  describe('handleUploadProgress', () => {
    it('should update progress details', () => {
      handleUploadProgress({ current: 3, total: 10, filename: 'MyReplay.SC2Replay' });

      const state = getUploadState();
      expect(state.current).toBe(3);
      expect(state.total).toBe(10);
      expect(state.filename).toBe('MyReplay.SC2Replay');
    });

    it('should update UI with progress', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadProgress({ current: 2, total: 5, filename: 'Test.SC2Replay' });

      expect(statusEl?.textContent).toContain('Uploading replay 2 of 5');
      expect(statusEl?.textContent).toContain('Test.SC2Replay');
    });
  });

  describe('handleUploadComplete', () => {
    it('should set isUploading to false', () => {
      handleUploadStart({ limit: 100 });
      handleUploadComplete({ count: 5 });

      const state = getUploadState();
      expect(state.isUploading).toBe(false);
    });

    it('should set completed count and show completed', () => {
      handleUploadComplete({ count: 7 });

      const state = getUploadState();
      expect(state.completedCount).toBe(7);
      expect(state.showCompleted).toBe(true);
    });

    it('should clear upload details', () => {
      handleUploadProgress({ current: 3, total: 5, filename: 'test.SC2Replay' });
      handleUploadComplete({ count: 5 });

      const state = getUploadState();
      expect(state.current).toBeNull();
      expect(state.total).toBeNull();
      expect(state.filename).toBeNull();
    });

    it('should hide completed message after 60 seconds', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadComplete({ count: 3 });
      expect(statusEl?.textContent).toBe('Uploaded 3 new replays');

      vi.advanceTimersByTime(60000);

      expect(statusEl?.textContent).toBe('Watching for new replays...');
    });

    it('should cancel previous timeout when called multiple times', () => {
      handleUploadComplete({ count: 1 });
      vi.advanceTimersByTime(30000); // Advance 30 seconds

      handleUploadComplete({ count: 2 }); // New complete event
      vi.advanceTimersByTime(50000); // Advance 50 more seconds (80 total)

      const statusEl = document.querySelector('#authenticated-state .status');
      // Should still show completed because timer was reset
      expect(statusEl?.textContent).toBe('Uploaded 2 new replays');

      vi.advanceTimersByTime(10000); // Advance final 10 seconds (60 from second event)
      expect(statusEl?.textContent).toBe('Watching for new replays...');
    });
  });

  describe('complete upload flow', () => {
    it('should handle full upload sequence correctly', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      // Start
      handleUploadStart({ limit: 100 });
      expect(statusEl?.textContent).toBe('Checking for new replays...');
      expect(getUploadState().isUploading).toBe(true);

      // Checking
      handleUploadChecking({ count: 50 });
      expect(getUploadState().isUploading).toBe(true);

      // Check complete
      handleUploadCheckComplete({ new_count: 10, existing_count: 40 });
      expect(statusEl?.textContent).toBe('Detected 10 new replays');
      expect(getUploadState().total).toBe(10);

      // Progress
      handleUploadProgress({ current: 1, total: 10, filename: 'replay1.SC2Replay' });
      expect(statusEl?.textContent).toBe('Uploading replay 1 of 10: replay1.SC2Replay');

      handleUploadProgress({ current: 5, total: 10, filename: 'replay5.SC2Replay' });
      expect(statusEl?.textContent).toBe('Uploading replay 5 of 10: replay5.SC2Replay');

      handleUploadProgress({ current: 10, total: 10, filename: 'replay10.SC2Replay' });
      expect(statusEl?.textContent).toBe('Uploading replay 10 of 10: replay10.SC2Replay');

      // Complete
      handleUploadComplete({ count: 10 });
      expect(statusEl?.textContent).toBe('Uploaded 10 new replays');
      expect(getUploadState().isUploading).toBe(false);
      expect(getUploadState().showCompleted).toBe(true);

      // After timeout
      vi.advanceTimersByTime(60000);
      expect(statusEl?.textContent).toBe('Watching for new replays...');
      expect(getUploadState().showCompleted).toBe(false);
    });

    it('should handle zero uploads correctly', () => {
      const statusEl = document.querySelector('#authenticated-state .status');

      handleUploadStart({ limit: 100 });
      handleUploadCheckComplete({ new_count: 0, existing_count: 50 });
      handleUploadComplete({ count: 0 });

      expect(statusEl?.textContent).toBe('No new replays to upload');
      expect(getUploadState().isUploading).toBe(false);
    });
  });
});
