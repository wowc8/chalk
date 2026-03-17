import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useEventBus } from "./useEventBus";
import { CHANNEL_FEATURE_FLAG_CHANGED } from "../types/events";

/** A feature flag record from the backend. */
export interface FeatureFlag {
  name: string;
  enabled: boolean;
  description: string | null;
  created_at: string;
  updated_at: string;
}

/**
 * Hook to load and manage all feature flags.
 * Automatically refreshes when a flag changes via the event bus.
 */
export function useFeatureFlags() {
  const [flags, setFlags] = useState<FeatureFlag[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await invoke<FeatureFlag[]>("list_feature_flags");
      setFlags(result);
      setError(null);
    } catch (e) {
      setError(`Failed to load feature flags: ${e}`);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  // Auto-refresh when any flag changes.
  useEventBus(CHANNEL_FEATURE_FLAG_CHANGED, () => {
    refresh();
  });

  const toggle = useCallback(async (name: string) => {
    try {
      const result = await invoke<FeatureFlag>("toggle_feature_flag", { name });
      setFlags((prev) =>
        prev.map((f) => (f.name === result.name ? result : f)),
      );
      return result;
    } catch (e) {
      setError(`Failed to toggle flag: ${e}`);
      throw e;
    }
  }, []);

  const setFlag = useCallback(
    async (name: string, enabled: boolean, description?: string) => {
      try {
        const result = await invoke<FeatureFlag>("set_feature_flag", {
          name,
          enabled,
          description: description ?? null,
        });
        setFlags((prev) => {
          const existing = prev.find((f) => f.name === result.name);
          if (existing) {
            return prev.map((f) => (f.name === result.name ? result : f));
          }
          return [...prev, result];
        });
        return result;
      } catch (e) {
        setError(`Failed to set flag: ${e}`);
        throw e;
      }
    },
    [],
  );

  return { flags, loading, error, refresh, toggle, setFlag };
}

/**
 * Hook to check a single feature flag's enabled state.
 * Returns false for unknown flags (consistent with backend behavior).
 */
export function useIsFeatureEnabled(flagName: string): boolean {
  const [enabled, setEnabled] = useState(false);

  useEffect(() => {
    invoke<boolean>("is_feature_enabled", { name: flagName })
      .then(setEnabled)
      .catch(() => setEnabled(false));
  }, [flagName]);

  // Listen for changes to this specific flag.
  useEventBus(CHANNEL_FEATURE_FLAG_CHANGED, (payload) => {
    if (payload.flag_name === flagName) {
      setEnabled(payload.enabled);
    }
  });

  return enabled;
}
