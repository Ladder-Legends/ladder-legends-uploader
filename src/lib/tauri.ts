/**
 * Tauri invoke wrapper
 * Provides a typed interface to Tauri commands
 */

import type { TauriInvoke } from '../types';

let invoke: TauriInvoke | null = null;

/**
 * Initialize Tauri invoke function
 * Tries multiple locations where Tauri v2 might expose it
 */
export async function initTauri(): Promise<TauriInvoke> {
  // Already initialized
  if (invoke) return invoke;

  // Try __TAURI_INTERNALS__ first (Tauri v2 primary location)
  if (window.__TAURI_INTERNALS__?.invoke) {
    invoke = window.__TAURI_INTERNALS__.invoke;
    console.log('[DEBUG] Using __TAURI_INTERNALS__.invoke');
    return invoke;
  }

  // Fallback to __TAURI__.core (Tauri v2 alternative location)
  if (window.__TAURI__?.core?.invoke) {
    invoke = window.__TAURI__.core.invoke;
    console.log('[DEBUG] Using __TAURI__.core.invoke');
    return invoke;
  }

  // Not available yet, wait and retry
  console.error('[DEBUG] Tauri not available yet, waiting...');
  await new Promise(resolve => setTimeout(resolve, 100));
  return initTauri();
}

/**
 * Get the initialized invoke function
 * Throws if not initialized
 */
export function getInvoke(): TauriInvoke {
  if (!invoke) {
    throw new Error('Tauri not initialized. Call initTauri() first.');
  }
  return invoke;
}
