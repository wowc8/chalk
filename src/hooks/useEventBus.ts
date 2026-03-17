import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import type { EventChannelMap, EventChannel } from "../types/events";

/**
 * Subscribe to a typed Tauri event channel.
 *
 * Automatically unsubscribes on unmount. The callback receives the
 * strongly-typed payload for the given channel.
 *
 * @example
 * ```tsx
 * useEventBus("shredder:progress", (payload) => {
 *   setProgress(payload.current / payload.total);
 * });
 * ```
 */
export function useEventBus<C extends EventChannel>(
  channel: C,
  callback: (payload: EventChannelMap[C]) => void,
): void {
  useEffect(() => {
    const unlisten = listen<EventChannelMap[C]>(channel, (event) => {
      callback(event.payload);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [channel]);
}
