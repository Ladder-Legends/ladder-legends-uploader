/**
 * Folder detection module
 * Handles SC2 replay folder detection and manual selection
 */

import { getInvoke } from '../lib/tauri';
import { showError } from '../lib/state';
import { DETECTION_TIMEOUT_MS } from '../config';
import type { TauriInvoke } from '../types';

/**
 * Detect replay folder with timeout
 */
export async function detectWithTimeout(invoke: TauriInvoke): Promise<string> {
  console.log('[DEBUG] detectWithTimeout starting...');

  const detectionPromise = invoke('detect_replay_folder')
    .then(result => {
      console.log('[DEBUG] invoke SUCCESS:', result);
      return result as string;
    })
    .catch(err => {
      console.error('[DEBUG] invoke ERROR:', err);
      throw err;
    });

  const timeoutPromise = new Promise<never>((_, reject) =>
    setTimeout(() => {
      console.log('[DEBUG] Timeout reached!');
      reject('timeout');
    }, DETECTION_TIMEOUT_MS)
  );

  return Promise.race([detectionPromise, timeoutPromise]);
}

/**
 * Show manual folder picker option
 */
export function showManualPickerOption(_error: unknown): void {
  const detectingState = document.getElementById('detecting-state');
  if (!detectingState) return;

  // Update the detecting state to show manual option
  const statusText = detectingState.querySelector('.status');
  if (statusText) {
    statusText.textContent = 'Could not automatically find your SC2 replay folder.';
  }

  const spinner = detectingState.querySelector('.spinner') as HTMLElement | null;
  if (spinner) {
    spinner.style.display = 'none';
  }

  // Add manual pick button if it doesn't exist
  if (!document.getElementById('manual-pick-btn')) {
    const button = document.createElement('button');
    button.id = 'manual-pick-btn';
    button.className = 'btn btn-primary';
    button.textContent = 'Choose Folder Manually';
    detectingState.appendChild(button);
    button.addEventListener('click', () => pickFolderManually());
  }
}

/**
 * Pick folder manually via file dialog
 */
export async function pickFolderManually(): Promise<string | null> {
  try {
    const invoke = getInvoke();
    const folderPath = await invoke('pick_replay_folder_manual') as string;
    return folderPath;
  } catch (error) {
    if (error !== 'No folder selected') {
      showError(`Failed to select folder: ${error}`);
    }
    // If user cancelled, return null
    return null;
  }
}
