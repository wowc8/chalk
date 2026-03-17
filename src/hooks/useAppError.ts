import { useState, useCallback } from "react";
import { useEventBus } from "./useEventBus";
import {
  CHANNEL_APP_ERROR,
  type AppErrorPayload,
} from "../types/events";
import { parseError, matchError, getUserMessage, type ChalkError, type ErrorHandlerMap } from "../types/errors";

/**
 * Hook for handling app errors from the event bus.
 *
 * Subscribes to the `app:error` channel and provides the latest error
 * payload. Errors can be dismissed and pattern-matched.
 *
 * @example
 * ```tsx
 * const { error, dismiss } = useAppError({
 *   OAUTH_TOKEN_EXPIRED: () => showReconnectBanner(),
 * });
 * ```
 */
export function useAppError(handlers?: ErrorHandlerMap) {
  const [error, setError] = useState<AppErrorPayload | null>(null);

  useEventBus(CHANNEL_APP_ERROR, (payload) => {
    setError(payload);
    if (handlers) {
      matchError(payload.error, handlers);
    }
  });

  const dismiss = useCallback(() => setError(null), []);

  const message = error ? getUserMessage(error.error) : null;

  return { error, message, dismiss };
}

/**
 * Wrap an async invoke call with structured error handling.
 *
 * @example
 * ```ts
 * const result = await withErrorHandling(
 *   () => invoke("some_command"),
 *   {
 *     OAUTH_TOKEN_EXPIRED: () => redirectToLogin(),
 *     _default: (e) => showToast(e.message),
 *   }
 * );
 * ```
 */
export async function withErrorHandling<T>(
  fn: () => Promise<T>,
  handlers: ErrorHandlerMap,
): Promise<T | null> {
  try {
    return await fn();
  } catch (err) {
    const chalkError: ChalkError = parseError(err);
    matchError(chalkError, handlers);
    return null;
  }
}
