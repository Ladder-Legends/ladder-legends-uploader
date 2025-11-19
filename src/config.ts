/**
 * Application configuration
 * API host defaults to production but can be overridden by environment variable
 */

// Get API host from build-time environment variable or runtime window object
export const API_HOST =
  (import.meta as any).env?.VITE_API_HOST ||
  (typeof window !== 'undefined' && (window as  any).LADDER_LEGENDS_API_HOST) ||
  'https://ladderlegendsacademy.com';

export const DETECTION_TIMEOUT_MS = 8000;
export const BUTTON_INIT_DELAY_MS = 100;
