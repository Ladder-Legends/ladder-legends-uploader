/**
 * Application configuration
 * API host defaults to production but can be overridden by environment variable
 */

/// <reference types="vite/client" />

// Extend Window interface for runtime config
declare global {
  interface Window {
    LADDER_LEGENDS_API_HOST?: string;
  }
}

// Get API host from build-time environment variable or runtime window object
// This is a function to ensure we read the window variable at runtime, not module load time
export function getApiHost(): string {
  const viteHost = import.meta.env?.VITE_API_HOST as string | undefined;
  const windowHost = typeof window !== 'undefined' ? window.LADDER_LEGENDS_API_HOST : undefined;
  const defaultHost = 'https://www.ladderlegendsacademy.com';

  const selected = viteHost || windowHost || defaultHost;

  console.log('[CONFIG] getApiHost() called:');
  console.log('  - VITE_API_HOST:', viteHost);
  console.log('  - window.LADDER_LEGENDS_API_HOST:', windowHost);
  console.log('  - Selected:', selected);

  return selected;
}

// For backwards compatibility, but use getApiHost() instead
export const API_HOST = getApiHost();

export const DETECTION_TIMEOUT_MS = 8000;
export const BUTTON_INIT_DELAY_MS = 100;
