/**
 * Tauri Event Bus types — TypeScript mirror of Rust event payloads.
 */

import type { ChalkError } from "./errors";

// ── Channel constants ────────────────────────────────────────

export const CHANNEL_CONNECTOR_STATUS = "connector:status_changed" as const;
export const CHANNEL_SHREDDER_PROGRESS = "shredder:progress" as const;
export const CHANNEL_SHREDDER_COMPLETE = "shredder:complete" as const;
export const CHANNEL_CACHE_INVALIDATED = "cache:invalidated" as const;
export const CHANNEL_APP_ERROR = "app:error" as const;
export const CHANNEL_FEATURE_FLAG_CHANGED = "feature_flag:changed" as const;

// ── Payload types ────────────────────────────────────────────

export type ConnectorStatus = "connected" | "disconnected" | "syncing" | "error";

export interface ConnectorStatusPayload {
  connector_id: string;
  connector_type: string;
  status: ConnectorStatus;
  message: string | null;
}

export interface ShredderProgressPayload {
  current: number;
  total: number;
  current_document: string | null;
  tables_found: number;
}

export interface ShredderCompletePayload {
  documents_processed: number;
  total_tables: number;
  total_plans: number;
  errors: string[];
}

export type CacheInvalidationReason = "expired" | "manual_clear" | "data_changed";

export interface CacheInvalidatedPayload {
  cache_key: string;
  reason: CacheInvalidationReason;
}

export interface FeatureFlagChangedPayload {
  flag_name: string;
  enabled: boolean;
}

export interface AppErrorPayload {
  error: ChalkError;
  recoverable: boolean;
  action: string | null;
}

// ── Channel → Payload mapping ────────────────────────────────

export interface EventChannelMap {
  [CHANNEL_CONNECTOR_STATUS]: ConnectorStatusPayload;
  [CHANNEL_SHREDDER_PROGRESS]: ShredderProgressPayload;
  [CHANNEL_SHREDDER_COMPLETE]: ShredderCompletePayload;
  [CHANNEL_CACHE_INVALIDATED]: CacheInvalidatedPayload;
  [CHANNEL_APP_ERROR]: AppErrorPayload;
  [CHANNEL_FEATURE_FLAG_CHANGED]: FeatureFlagChangedPayload;
}

export type EventChannel = keyof EventChannelMap;
