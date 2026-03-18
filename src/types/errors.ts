/**
 * Domain error types — TypeScript mirror of Rust ChalkError.
 *
 * Provides structured error types for frontend pattern matching.
 * Use `matchError()` for exhaustive handling by domain/code.
 */

/** Error domain — which subsystem produced the error. */
export type ErrorDomain =
  | "database"
  | "connector"
  | "oauth"
  | "digest"
  | "cache"
  | "feature_flag"
  | "io"
  | "unknown";

/** Machine-readable error code for pattern matching. */
export type ErrorCode =
  // Database
  | "DB_CONNECTION_FAILED"
  | "DB_QUERY_FAILED"
  | "DB_NOT_FOUND"
  | "DB_MIGRATION_FAILED"
  // Connector / OAuth
  | "OAUTH_NOT_CONFIGURED"
  | "OAUTH_TOKEN_EXPIRED"
  | "OAUTH_TOKEN_REFRESH_FAILED"
  | "CONNECTOR_NOT_FOUND"
  | "CONNECTOR_API_ERROR"
  // Digest
  | "DIGEST_PARSE_FAILED"
  | "DIGEST_NO_TABLES"
  // Cache
  | "CACHE_EXPIRED"
  | "CACHE_MISS"
  // Feature flags
  | "FLAG_NOT_FOUND"
  // IO
  | "IO_READ_FAILED"
  | "IO_WRITE_FAILED"
  // Catch-all
  | "INTERNAL_ERROR";

/** Structured error from the Rust backend. */
export interface ChalkError {
  domain: ErrorDomain;
  code: ErrorCode;
  message: string;
  details?: Record<string, unknown>;
}

/** Type guard: check if a value is a ChalkError. */
export function isChalkError(value: unknown): value is ChalkError {
  if (typeof value !== "object" || value === null) return false;
  const obj = value as Record<string, unknown>;
  return (
    typeof obj.domain === "string" &&
    typeof obj.code === "string" &&
    typeof obj.message === "string"
  );
}

/**
 * Parse an unknown error (from invoke catch) into a ChalkError.
 * Falls back to a generic INTERNAL_ERROR if the shape doesn't match.
 */
export function parseError(error: unknown): ChalkError {
  if (isChalkError(error)) return error;

  // Tauri invoke errors come as strings.
  if (typeof error === "string") {
    // Try to parse as JSON first (structured error from backend).
    try {
      const parsed = JSON.parse(error);
      if (isChalkError(parsed)) return parsed;
    } catch {
      // Not JSON — use as message.
    }
    return {
      domain: "unknown",
      code: "INTERNAL_ERROR",
      message: error,
    };
  }

  if (error instanceof Error) {
    return {
      domain: "unknown",
      code: "INTERNAL_ERROR",
      message: error.message,
    };
  }

  return {
    domain: "unknown",
    code: "INTERNAL_ERROR",
    message: String(error),
  };
}

/** Handler map for error pattern matching. */
export type ErrorHandlerMap = {
  [K in ErrorCode]?: (error: ChalkError) => void;
} & {
  _default?: (error: ChalkError) => void;
};

/**
 * Pattern-match on error codes with a handler map.
 *
 * @example
 * ```ts
 * matchError(error, {
 *   OAUTH_TOKEN_EXPIRED: () => showReconnectBanner(),
 *   DB_NOT_FOUND: (e) => showToast(`Not found: ${e.message}`),
 *   _default: (e) => showToast(e.message),
 * });
 * ```
 */
export function matchError(error: ChalkError, handlers: ErrorHandlerMap): void {
  const handler = handlers[error.code] ?? handlers._default;
  if (handler) handler(error);
}

/**
 * User-friendly messages for common error codes.
 * Used as fallback when no specific handler is provided.
 */
export const ERROR_MESSAGES: Partial<Record<ErrorCode, string>> = {
  OAUTH_TOKEN_EXPIRED: "Your Google connection has expired. Please reconnect.",
  OAUTH_NOT_CONFIGURED: "Google Drive is not configured. Complete setup in Settings.",
  CONNECTOR_API_ERROR: "Failed to communicate with the external service. Please try again.",
  DB_NOT_FOUND: "The requested item was not found.",
  DB_CONNECTION_FAILED: "Database connection failed. Please restart the app.",
  INTERNAL_ERROR: "An unexpected error occurred. Please try again.",
};

/** Get a user-friendly message for an error. */
export function getUserMessage(error: ChalkError): string {
  return ERROR_MESSAGES[error.code] ?? error.message;
}
