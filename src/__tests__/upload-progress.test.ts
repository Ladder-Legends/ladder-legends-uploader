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
    // Set up minimal DOM matching actual app structure
    document.body.innerHTML = `
      <div id="authenticated-state">
        <div id="upload-status" style="display: none;">
          <div id="batch-game-type"></div>
          <div id="batch-player-name"></div>
        </div>
        <div id="replay-progress"></div>
        <div id="replay-filename"></div>
        <div id="watching-status" class="status">Watching for new replays...</div>
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
      handleUploadProgress({ current: 1, total: 5, filename: 'test.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });

      resetUploadState();

      const state = getUploadState();
      expect(state.isUploading).toBe(false);
      expect(state.current).toBeNull();
      expect(state.total).toBeNull();
    });

    it('should update UI after reset', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadProgress({ current: 1, total: 5, filename: 'test.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });
      resetUploadState();

      expect(statusEl?.textContent).toBe('Waiting for new replays');
    });
  });

  describe('updateUI', () => {
    it('should show default message when not uploading', () => {
      const statusEl = document.getElementById('watching-status');

      updateUI();

      expect(statusEl?.textContent).toBe('Waiting for new replays');
    });

    it('should show checking message when uploading without details', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadStart({ limit: 100 });

      expect(statusEl?.textContent).toBe('Scanning for replays...');
    });

    it('should show detected count after check complete', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadStart({ limit: 100 });
      handleUploadCheckComplete({ new_count: 5, existing_count: 10 });

      expect(statusEl?.textContent).toBe('Found 5 new replays to upload');
    });

    it('should show singular replay when count is 1', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadStart({ limit: 100 });
      handleUploadCheckComplete({ new_count: 1, existing_count: 0 });

      expect(statusEl?.textContent).toBe('Found 1 new replay to upload');
    });

    it('should show upload progress with filename', () => {
      const progressEl = document.getElementById('replay-progress');
      const filenameEl = document.getElementById('replay-filename');

      handleUploadProgress({ current: 3, total: 10, filename: 'MyReplay.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });

      expect(progressEl?.textContent).toBe('[3/10]');
      expect(filenameEl?.textContent).toBe('MyReplay.SC2Replay');
    });

    it('should show completion message', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadComplete({ count: 7 });

      expect(statusEl?.textContent).toBe('Uploaded 7 new replays');
    });

    it('should show singular replay in completion message when count is 1', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadComplete({ count: 1 });

      expect(statusEl?.textContent).toBe('Uploaded 1 new replay');
    });

    it('should show no new replays message when count is 0', () => {
      const statusEl = document.getElementById('watching-status');

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
      handleUploadProgress({ current: 5, total: 10, filename: 'test.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });
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
      handleUploadProgress({ current: 3, total: 10, filename: 'MyReplay.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });

      const state = getUploadState();
      expect(state.current).toBe(3);
      expect(state.total).toBe(10);
      expect(state.filename).toBe('MyReplay.SC2Replay');
    });

    it('should update UI with progress', () => {
      const progressEl = document.getElementById('replay-progress');
      const filenameEl = document.getElementById('replay-filename');

      handleUploadProgress({ current: 2, total: 5, filename: 'Test.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });

      expect(progressEl?.textContent).toBe('[2/5]');
      expect(filenameEl?.textContent).toBe('Test.SC2Replay');
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
      handleUploadProgress({ current: 3, total: 5, filename: 'test.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });
      handleUploadComplete({ count: 5 });

      const state = getUploadState();
      expect(state.current).toBeNull();
      expect(state.total).toBeNull();
      expect(state.filename).toBeNull();
    });

    it('should hide completed message after 4 seconds (3s wait + 1s fade)', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadComplete({ count: 3 });
      expect(statusEl?.textContent).toBe('Uploaded 3 new replays');

      // After 3 seconds, fade-out class should be added
      vi.advanceTimersByTime(3000);
      expect(statusEl?.classList.contains('fade-out')).toBe(true);

      // After 1 more second (total 4s), message should change
      vi.advanceTimersByTime(1000);
      expect(statusEl?.textContent).toBe('Waiting for new replays');
      expect(statusEl?.classList.contains('fade-out')).toBe(false);
    });

    it('should cancel previous timeout when called multiple times', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadComplete({ count: 1 });
      vi.advanceTimersByTime(2000); // Advance 2 seconds (before 3s timeout)

      handleUploadComplete({ count: 2 }); // New complete event resets timer
      expect(statusEl?.textContent).toBe('Uploaded 2 new replays');
      expect(statusEl?.classList.contains('fade-out')).toBe(false);

      vi.advanceTimersByTime(2000); // Advance 2 more seconds (still < 3s from second event)
      expect(statusEl?.textContent).toBe('Uploaded 2 new replays'); // Still showing completed

      vi.advanceTimersByTime(2000); // Advance 2 more seconds (total 4s from second event)
      expect(statusEl?.textContent).toBe('Waiting for new replays'); // Now faded out
    });
  });

  describe('complete upload flow', () => {
    it('should handle full upload sequence correctly', () => {
      const statusEl = document.getElementById('watching-status');

      // Start
      handleUploadStart({ limit: 100 });
      expect(statusEl?.textContent).toBe('Scanning for replays...');
      expect(getUploadState().isUploading).toBe(true);

      // Checking
      handleUploadChecking({ count: 50 });
      expect(getUploadState().isUploading).toBe(true);

      // Check complete
      handleUploadCheckComplete({ new_count: 10, existing_count: 40 });
      expect(statusEl?.textContent).toBe('Found 10 new replays to upload');
      expect(getUploadState().total).toBe(10);

      // Progress
      const progressEl = document.getElementById('replay-progress');
      const filenameEl = document.getElementById('replay-filename');

      handleUploadProgress({ current: 1, total: 10, filename: 'replay1.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });
      expect(progressEl?.textContent).toBe('[1/10]');
      expect(filenameEl?.textContent).toBe('replay1.SC2Replay');

      handleUploadProgress({ current: 5, total: 10, filename: 'replay5.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });
      expect(progressEl?.textContent).toBe('[5/10]');
      expect(filenameEl?.textContent).toBe('replay5.SC2Replay');

      handleUploadProgress({ current: 10, total: 10, filename: 'replay10.SC2Replay', game_type: '1v1-ladder', player_name: 'testplayer' });
      expect(progressEl?.textContent).toBe('[10/10]');
      expect(filenameEl?.textContent).toBe('replay10.SC2Replay');

      // Complete
      handleUploadComplete({ count: 10 });
      expect(statusEl?.textContent).toBe('Uploaded 10 new replays (50 total)');
      expect(getUploadState().isUploading).toBe(false);
      expect(getUploadState().showCompleted).toBe(true);

      // After timeout (3s + 1s fade)
      vi.advanceTimersByTime(4000);
      expect(statusEl?.textContent).toBe('Waiting for new replays (50 replays tracked)');
      expect(getUploadState().showCompleted).toBe(false);
    });

    it('should handle zero uploads correctly', () => {
      const statusEl = document.getElementById('watching-status');

      handleUploadStart({ limit: 100 });
      handleUploadCheckComplete({ new_count: 0, existing_count: 50 });
      handleUploadComplete({ count: 0 });

      expect(statusEl?.textContent).toBe('No new replays to upload (50 total)');
      expect(getUploadState().isUploading).toBe(false);
    });
  });
});
