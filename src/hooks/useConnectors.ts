import { useState, useEffect, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";

// ── Types ──────────────────────────────────────────────────────────

export type AuthStatus = "connected" | "disconnected" | "expired";

export interface ConnectionDetails {
  id: string;
  connector_type: string;
  display_name: string;
  auth_status: AuthStatus;
  account_email: string | null;
  source_name: string | null;
  source_id: string | null;
  last_scan_at: string | null;
  document_count: number | null;
}

/** Which operation is currently pending (optimistic). */
export type PendingOp =
  | { type: "disconnect"; connectorId: string }
  | { type: "rescan"; connectorId: string }
  | null;

// ── Hook ───────────────────────────────────────────────────────────

export function useConnectors() {
  const [connections, setConnections] = useState<ConnectionDetails[]>([]);
  const [loading, setLoading] = useState(true);
  const [pendingOp, setPendingOp] = useState<PendingOp>(null);

  // Snapshot for rollback on failure.
  const snapshotRef = useRef<ConnectionDetails[]>([]);

  const refresh = useCallback(async () => {
    try {
      const details = await invoke<ConnectionDetails[]>("get_connection_details");
      setConnections(details);
      snapshotRef.current = details;
    } catch {
      // Silently fail on refresh — stale data is better than no data.
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  /**
   * Disconnect a connector with optimistic UI update.
   * Returns { success, error } for the caller to show toasts.
   */
  const disconnect = useCallback(
    async (connectorId: string): Promise<{ success: boolean; error?: string }> => {
      // Save snapshot for rollback.
      snapshotRef.current = [...connections];

      // Optimistic update: immediately mark as disconnected.
      setConnections((prev) =>
        prev.map((c) =>
          c.id === connectorId
            ? {
                ...c,
                auth_status: "disconnected" as AuthStatus,
                source_name: null,
                source_id: null,
                document_count: null,
              }
            : c
        )
      );
      setPendingOp({ type: "disconnect", connectorId });

      try {
        await invoke("disconnect_connector", { connectorId });
        // Reconcile with actual backend state.
        await refresh();
        setPendingOp(null);
        return { success: true };
      } catch (e) {
        // Rollback to snapshot.
        setConnections(snapshotRef.current);
        setPendingOp(null);
        return { success: false, error: String(e) };
      }
    },
    [connections, refresh]
  );

  /**
   * Re-scan a connector with optimistic UI update.
   * Returns { success, docCount?, error? }.
   */
  const rescan = useCallback(
    async (
      connectorId: string
    ): Promise<{ success: boolean; docCount?: number; error?: string }> => {
      snapshotRef.current = [...connections];
      setPendingOp({ type: "rescan", connectorId });

      try {
        const docCount = await invoke<number>("rescan_connector", {
          connectorId,
        });
        // Optimistic: update document count immediately.
        setConnections((prev) =>
          prev.map((c) =>
            c.id === connectorId ? { ...c, document_count: docCount } : c
          )
        );
        // Reconcile.
        await refresh();
        setPendingOp(null);
        return { success: true, docCount };
      } catch (e) {
        setConnections(snapshotRef.current);
        setPendingOp(null);
        return { success: false, error: String(e) };
      }
    },
    [connections, refresh]
  );

  return {
    connections,
    loading,
    pendingOp,
    refresh,
    disconnect,
    rescan,
  };
}
