/**
 * TypeScript types for the uploader application
 */

export type StateKey = 'detecting' | 'code' | 'authenticated' | 'settings' | 'error';

export interface AuthTokens {
  access_token: string;
  refresh_token: string | null;
  expires_at: number | null;
  user?: {
    username: string;
    avatar_url?: string;
  };
}

export interface DeviceCodeResponse {
  device_code: string;
  user_code: string;
  verification_uri: string;
  expires_in: number;
  interval: number;
}

export interface AuthorizationResponse {
  access_token: string;
  refresh_token?: string;
  expires_at?: number;
  user: {
    username: string;
    avatar_url?: string;
  };
}

export interface TauriInvoke {
  (cmd: string, args?: Record<string, unknown>): Promise<any>;
}

// Upload progress event payloads
export interface UploadStartEvent {
  limit: number;
}

export interface UploadCheckingEvent {
  count: number;
}

export interface UploadCheckCompleteEvent {
  new_count: number;
  existing_count: number;
}

export interface UploadBatchStartEvent {
  game_type: string;
  player_name: string;
  count: number;
}

export interface UploadProgressEvent {
  current: number;
  total: number;
  filename: string;
  game_type: string;
  player_name: string;
}

export interface UploadBatchCompleteEvent {
  game_type: string;
  player_name: string;
  count: number;
}

export interface UploadCompleteEvent {
  count: number;
}

// The new-replay-detected event payload is just a string path
export type ReplayDetectedEvent = string;

export interface UploadState {
  isUploading: boolean;
  current: number | null;
  total: number | null;
  filename: string | null;
  completedCount: number | null;
  showCompleted: boolean;
  checkingCount: number | null;
  totalReplays: number | null;
  // Batch information
  currentBatchGameType: string | null;
  currentBatchPlayerName: string | null;
  currentBatchCount: number | null;
  // Background detection notification
  backgroundDetectedCount: number;
  showBackgroundNotification: boolean;
}

declare global {
  interface Window {
    __TAURI_INTERNALS__?: {
      invoke: TauriInvoke;
    };
    __TAURI__?: {
      core?: {
        invoke: TauriInvoke;
      };
      event?: {
        listen: <T = any>(event: string, handler: (event: { payload: T }) => void) => Promise<() => void>;
      };
    };
    LADDER_LEGENDS_API_HOST?: string;
    verificationUrl?: string;
  }
}
