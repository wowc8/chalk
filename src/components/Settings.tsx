import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { save, open } from "@tauri-apps/plugin-dialog";
import { motion } from "framer-motion";
import { useConnectors, type ConnectionDetails, type PendingOp } from "../hooks/useConnectors";
import { useToast } from "./Toast";

const spring = { type: "spring" as const, stiffness: 300, damping: 30 };

export function Settings({
  onBack,
  onReconnect,
}: {
  onBack: () => void;
  onReconnect: () => void;
}) {
  const { connections, loading, pendingOp, disconnect, rescan } =
    useConnectors();
  const { addToast } = useToast();

  const [crashReportingEnabled, setCrashReportingEnabled] = useState(false);
  const [reportText, setReportText] = useState("");
  const [reportStatus, setReportStatus] = useState<
    "idle" | "sending" | "sent" | "error"
  >("idle");
  const [privacyLoading, setPrivacyLoading] = useState(true);

  useEffect(() => {
    invoke<{ consent_shown: boolean; crash_reporting_enabled: boolean }>(
      "get_privacy_consent_status"
    )
      .then((status) => {
        setCrashReportingEnabled(status.crash_reporting_enabled);
      })
      .catch(() => {})
      .finally(() => setPrivacyLoading(false));
  }, []);

  const handleDisconnect = async (id: string) => {
    const result = await disconnect(id);
    if (result.success) {
      addToast("Google Drive disconnected", "success");
    } else {
      addToast(result.error ?? "Failed to disconnect", "error");
    }
  };

  const handleRescan = async (id: string) => {
    const result = await rescan(id);
    if (result.success) {
      addToast(
        `Re-scan complete — ${result.docCount} document${result.docCount !== 1 ? "s" : ""} found`,
        "success"
      );
    } else {
      addToast(result.error ?? "Re-scan failed", "error");
    }
  };

  const toggleCrashReporting = async () => {
    const newValue = !crashReportingEnabled;
    try {
      await invoke("save_privacy_consent", { consented: newValue });
      setCrashReportingEnabled(newValue);
    } catch {
      // silent fail — will retry on next toggle
    }
  };

  const sendReport = async () => {
    if (!reportText.trim()) return;
    setReportStatus("sending");
    try {
      await invoke("send_crash_report", { message: reportText.trim() });
      setReportStatus("sent");
      setReportText("");
      setTimeout(() => setReportStatus("idle"), 3000);
    } catch {
      setReportStatus("error");
      setTimeout(() => setReportStatus("idle"), 3000);
    }
  };

  return (
    <div className="min-h-screen bg-bat-dark text-white relative overflow-hidden">
      {/* Background grid */}
      <div className="absolute inset-0 opacity-5">
        <div
          className="w-full h-full"
          style={{
            backgroundImage:
              "linear-gradient(rgba(0,212,255,0.3) 1px, transparent 1px), linear-gradient(90deg, rgba(0,212,255,0.3) 1px, transparent 1px)",
            backgroundSize: "40px 40px",
          }}
        />
      </div>

      <div className="relative z-10 max-w-2xl mx-auto px-6 py-10">
        {/* Header */}
        <motion.div
          initial={{ opacity: 0, y: -20 }}
          animate={{ opacity: 1, y: 0 }}
          className="mb-8 flex items-center gap-4"
        >
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={onBack}
            className="p-2 rounded-lg border border-gray-700 hover:border-gray-500 transition-colors"
          >
            <svg
              className="w-4 h-4"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M15 19l-7-7 7-7"
              />
            </svg>
          </motion.button>
          <h1 className="text-2xl font-bold bg-gradient-to-r from-bat-gold to-bat-cyan bg-clip-text text-transparent">
            Settings
          </h1>
        </motion.div>

        {/* Connections Section */}
        <motion.section
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.1 }}
          className="mb-8"
        >
          <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Connections
          </h2>

          {loading ? (
            <div className="flex items-center gap-3 py-8 justify-center">
              <motion.div
                animate={{ rotate: 360 }}
                transition={{
                  duration: 1.5,
                  repeat: Infinity,
                  ease: "linear",
                }}
                className="w-5 h-5 border-2 border-bat-cyan border-t-transparent rounded-full"
              />
              <span className="text-gray-500 text-sm">
                Loading connections...
              </span>
            </div>
          ) : (
            <div className="space-y-3">
              {connections.map((conn) => (
                <ConnectionCard
                  key={conn.id}
                  connection={conn}
                  pendingOp={pendingOp}
                  onDisconnect={() => handleDisconnect(conn.id)}
                  onRescan={() => handleRescan(conn.id)}
                  onReconnect={onReconnect}
                />
              ))}

              {connections.length === 0 && (
                <div className="text-center py-8">
                  <p className="text-gray-500 text-sm mb-4">
                    No connections configured
                  </p>
                  <motion.button
                    whileHover={{ scale: 1.05 }}
                    whileTap={{ scale: 0.95 }}
                    onClick={onReconnect}
                    className="px-4 py-2 bg-bat-cyan/10 border border-bat-cyan/30 rounded-lg text-bat-cyan text-sm hover:bg-bat-cyan/20 transition-colors"
                  >
                    + Add Connection
                  </motion.button>
                </div>
              )}
            </div>
          )}
        </motion.section>

        {/* Privacy & Crash Reporting Section */}
        <motion.section
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.2 }}
          className="mb-8"
        >
          <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Privacy & Crash Reporting
          </h2>

          {privacyLoading ? (
            <div className="flex items-center gap-3 py-4 justify-center">
              <div className="w-4 h-4 border-2 border-bat-cyan border-t-transparent rounded-full animate-spin" />
            </div>
          ) : (
            <div className="bg-bat-charcoal/50 rounded-lg border border-gray-800 p-4 space-y-4">
              <div className="flex items-center justify-between">
                <div>
                  <p className="text-sm text-gray-300">
                    Automatic crash reporting
                  </p>
                  <p className="text-xs text-gray-500 mt-0.5">
                    Send anonymous error reports to help improve Chalk
                  </p>
                </div>
                <button
                  onClick={toggleCrashReporting}
                  className={`relative w-12 h-7 rounded-full transition-colors ${
                    crashReportingEnabled ? "bg-bat-cyan" : "bg-gray-600"
                  }`}
                  role="switch"
                  aria-checked={crashReportingEnabled}
                >
                  <motion.div
                    animate={{ x: crashReportingEnabled ? 20 : 2 }}
                    transition={{ type: "spring", stiffness: 500, damping: 30 }}
                    className="absolute top-1 w-5 h-5 bg-white rounded-full shadow-sm"
                  />
                </button>
              </div>

              <p className="text-xs text-gray-600">
                No student data, document content, or personal information is
                collected. Only OS version, app version, and error traces are sent.
                Changes take effect on next app launch.
              </p>
            </div>
          )}
        </motion.section>

        {/* Send Report Section */}
        <motion.section
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.3 }}
          className="mb-8"
        >
          <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            Send Report
          </h2>

          <div className="bg-bat-charcoal/50 rounded-lg border border-gray-800 p-4">
            <p className="text-xs text-gray-500 mb-3">
              Encountered a bug? Describe what happened and we'll look into it.
            </p>

            <textarea
              value={reportText}
              onChange={(e) => setReportText(e.target.value)}
              placeholder="Describe what went wrong..."
              rows={4}
              className="w-full bg-bat-dark/50 border border-gray-700 rounded-lg p-3 text-sm text-white placeholder-gray-600 resize-none focus:outline-none focus:border-bat-cyan/50 transition-colors"
            />

            <div className="flex items-center justify-between mt-3">
              {reportStatus === "sent" && (
                <p className="text-xs text-bat-green">Report sent. Thank you!</p>
              )}
              {reportStatus === "error" && (
                <p className="text-xs text-bat-red">
                  Failed to send. Please try again.
                </p>
              )}
              {(reportStatus === "idle" || reportStatus === "sending") && (
                <span />
              )}
              <motion.button
                whileHover={{ scale: 1.02 }}
                whileTap={{ scale: 0.98 }}
                disabled={
                  !reportText.trim() || reportStatus === "sending"
                }
                onClick={sendReport}
                className="px-5 py-2 bg-bat-gold text-bat-dark font-semibold rounded-lg text-sm hover:bg-bat-gold/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {reportStatus === "sending" ? "Sending..." : "Send Report"}
              </motion.button>
            </div>
          </div>
        </motion.section>

        {/* Backup & Restore Section */}
        <BackupSection addToast={addToast} />

        {/* App Info Section */}
        <motion.section
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ delay: 0.4 }}
        >
          <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
            App
          </h2>
          <div className="bg-bat-charcoal/50 rounded-lg border border-gray-800 p-4">
            <div className="flex items-center justify-between">
              <div>
                <p className="text-sm text-gray-300">Version</p>
                <p className="text-xs text-gray-500">0.1.0</p>
              </div>
            </div>
          </div>
        </motion.section>
      </div>
    </div>
  );
}

// ── Connection Card ────────────────────────────────────────────────

function ConnectionCard({
  connection,
  pendingOp,
  onDisconnect,
  onRescan,
  onReconnect,
}: {
  connection: ConnectionDetails;
  pendingOp: PendingOp;
  onDisconnect: () => void;
  onRescan: () => void;
  onReconnect: () => void;
}) {
  const isConnected = connection.auth_status === "connected";
  const isPending =
    pendingOp !== null && pendingOp.connectorId === connection.id;
  const isPendingDisconnect = isPending && pendingOp?.type === "disconnect";
  const isPendingRescan = isPending && pendingOp?.type === "rescan";

  return (
    <motion.div
      layout
      transition={spring}
      className={`bg-bat-charcoal/50 rounded-lg border p-4 transition-colors ${
        isPending
          ? "border-bat-gold/30 opacity-80"
          : isConnected
            ? "border-gray-800 hover:border-bat-purple/30"
            : "border-gray-800/50"
      }`}
    >
      {/* Header row */}
      <div className="flex items-start gap-3">
        {/* Drive icon */}
        <div
          className={`w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0 ${
            isConnected ? "bg-bat-cyan/10" : "bg-gray-800"
          }`}
        >
          <svg
            className={`w-5 h-5 ${isConnected ? "text-bat-cyan" : "text-gray-500"}`}
            fill="currentColor"
            viewBox="0 0 24 24"
          >
            <path d="M7.71 3.5L1.15 15l4.58 7.5h6.56l-4.58-7.5L14.28 3.5H7.71zm2.57 0l6.57 11.5H23.4L16.85 3.5h-6.57zm6.56 12.5L12.28 23.5h13.13l4.56-7.5H16.84z" />
          </svg>
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium">
              {connection.display_name}
            </span>
            {/* Status badge */}
            <span
              className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium uppercase tracking-wider ${
                isPendingDisconnect
                  ? "bg-bat-gold/10 text-bat-gold"
                  : isConnected
                    ? "bg-bat-green/10 text-bat-green"
                    : "bg-gray-700 text-gray-400"
              }`}
            >
              {isPendingDisconnect ? (
                <>
                  <span className="w-1.5 h-1.5 rounded-full bg-bat-gold animate-pulse" />
                  Disconnecting...
                </>
              ) : isConnected ? (
                <>
                  <span className="w-1.5 h-1.5 rounded-full bg-bat-green" />
                  Connected
                </>
              ) : (
                <>
                  <span className="w-1.5 h-1.5 rounded-full bg-gray-500" />
                  Disconnected
                </>
              )}
            </span>
          </div>

          {/* Account email */}
          {connection.account_email && (
            <p className="text-xs text-gray-500 mt-0.5">
              {connection.account_email}
            </p>
          )}

          {/* Connected folder/doc info */}
          {isConnected && connection.source_name && (
            <div className="mt-2 flex items-center gap-1.5 text-xs text-gray-400">
              <svg
                className="w-3.5 h-3.5 text-bat-cyan/60"
                fill="currentColor"
                viewBox="0 0 20 20"
              >
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
              </svg>
              <span className="truncate">{connection.source_name}</span>
              {connection.document_count != null && (
                <span className="text-gray-600">
                  {" "}
                  &middot; {connection.document_count} docs
                </span>
              )}
            </div>
          )}

          {/* Last scan timestamp */}
          {connection.last_scan_at && (
            <p className="text-[10px] text-gray-600 mt-1">
              Last scan:{" "}
              {new Date(connection.last_scan_at).toLocaleDateString(undefined, {
                month: "short",
                day: "numeric",
                year: "numeric",
              })}
            </p>
          )}
        </div>
      </div>

      {/* Action buttons */}
      <div className="mt-3 pt-3 border-t border-gray-800/50 flex gap-2">
        {isConnected ? (
          <>
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={onReconnect}
              disabled={isPending}
              className="px-3 py-1.5 text-xs border border-gray-700 rounded-lg text-gray-400 hover:text-white hover:border-gray-500 transition-colors disabled:opacity-50"
            >
              Change Source
            </motion.button>
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={onRescan}
              disabled={isPending}
              className="px-3 py-1.5 text-xs border border-gray-700 rounded-lg text-gray-400 hover:text-bat-cyan hover:border-bat-cyan/30 transition-colors disabled:opacity-50 flex items-center gap-1.5"
            >
              {isPendingRescan ? (
                <>
                  <motion.span
                    animate={{ rotate: 360 }}
                    transition={{
                      duration: 1,
                      repeat: Infinity,
                      ease: "linear",
                    }}
                    className="inline-block w-3 h-3 border border-bat-cyan border-t-transparent rounded-full"
                  />
                  Scanning...
                </>
              ) : (
                "Re-scan"
              )}
            </motion.button>
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={onDisconnect}
              disabled={isPending}
              className="px-3 py-1.5 text-xs border border-gray-700 rounded-lg text-gray-400 hover:text-bat-red hover:border-bat-red/30 transition-colors disabled:opacity-50 ml-auto"
            >
              Disconnect
            </motion.button>
          </>
        ) : (
          <motion.button
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            onClick={onReconnect}
            className="px-3 py-1.5 text-xs bg-bat-cyan/10 border border-bat-cyan/30 rounded-lg text-bat-cyan hover:bg-bat-cyan/20 transition-colors"
          >
            Connect
          </motion.button>
        )}
      </div>
    </motion.div>
  );
}

// ── Backup & Restore Section ──────────────────────────────────────

interface BackupInfo {
  format_version: number;
  created_at: string;
  plan_count: number;
  tag_count: number;
  conversation_count: number;
}

function BackupSection({
  addToast,
}: {
  addToast: (msg: string, type: "success" | "error") => void;
}) {
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [confirmInfo, setConfirmInfo] = useState<BackupInfo | null>(null);
  const [confirmPath, setConfirmPath] = useState<string | null>(null);

  const handleExport = async () => {
    try {
      const path = await save({
        defaultPath: `chalk-backup-${new Date().toISOString().slice(0, 10)}.chalk-backup.zip`,
        filters: [{ name: "Chalk Backup", extensions: ["chalk-backup.zip"] }],
      });
      if (!path) return;
      setExporting(true);
      await invoke("export_backup", { path });
      addToast("Backup exported successfully", "success");
    } catch (e) {
      addToast(`Export failed: ${e}`, "error");
    } finally {
      setExporting(false);
    }
  };

  const handleImportPick = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Chalk Backup", extensions: ["chalk-backup.zip", "zip"] }],
      });
      if (!path) return;
      const info = await invoke<BackupInfo>("get_backup_info", { path });
      setConfirmInfo(info);
      setConfirmPath(path);
    } catch (e) {
      addToast(`Could not read backup: ${e}`, "error");
    }
  };

  const handleConfirmImport = async () => {
    if (!confirmPath) return;
    setImporting(true);
    setConfirmInfo(null);
    try {
      await invoke("import_backup", { path: confirmPath });
      addToast("Backup restored successfully", "success");
      // Re-vectorize in background
      invoke("vectorize_all_plans").catch(() => {});
    } catch (e) {
      addToast(`Import failed: ${e}`, "error");
    } finally {
      setImporting(false);
      setConfirmPath(null);
    }
  };

  return (
    <>
      <motion.section
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.35 }}
        className="mb-8"
      >
        <h2 className="text-sm font-semibold text-gray-400 uppercase tracking-wider mb-4">
          Backup & Restore
        </h2>

        <div className="bg-bat-charcoal/50 rounded-lg border border-gray-800 p-4 space-y-4">
          <p className="text-xs text-gray-500">
            Export all lesson plans, tags, chat history, and settings to a
            portable backup file, or restore from a previous backup.
          </p>

          <div className="flex gap-3">
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              disabled={exporting || importing}
              onClick={handleExport}
              className="px-4 py-2 bg-bat-cyan/10 border border-bat-cyan/30 rounded-lg text-bat-cyan text-sm hover:bg-bat-cyan/20 transition-colors disabled:opacity-50"
            >
              {exporting ? "Exporting..." : "Export Backup"}
            </motion.button>

            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              disabled={exporting || importing}
              onClick={handleImportPick}
              className="px-4 py-2 bg-bat-gold/10 border border-bat-gold/30 rounded-lg text-bat-gold text-sm hover:bg-bat-gold/20 transition-colors disabled:opacity-50"
            >
              {importing ? "Restoring..." : "Restore Backup"}
            </motion.button>
          </div>
        </div>
      </motion.section>

      {/* Import confirmation dialog */}
      {confirmInfo && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60">
          <motion.div
            initial={{ scale: 0.9, opacity: 0 }}
            animate={{ scale: 1, opacity: 1 }}
            className="bg-bat-charcoal rounded-xl border border-gray-700 p-6 max-w-sm w-full mx-4 shadow-xl"
          >
            <h3 className="text-lg font-semibold text-white mb-3">
              Restore Backup?
            </h3>
            <div className="text-sm text-gray-300 space-y-1 mb-4">
              <p>Created: {new Date(confirmInfo.created_at).toLocaleDateString()}</p>
              <p>{confirmInfo.plan_count} lesson plan{confirmInfo.plan_count !== 1 ? "s" : ""}</p>
              <p>{confirmInfo.tag_count} tag{confirmInfo.tag_count !== 1 ? "s" : ""}</p>
              <p>{confirmInfo.conversation_count} conversation{confirmInfo.conversation_count !== 1 ? "s" : ""}</p>
            </div>
            <p className="text-xs text-bat-gold mb-4">
              Your current data will be auto-backed up before restoring.
            </p>
            <div className="flex gap-3 justify-end">
              <button
                onClick={() => { setConfirmInfo(null); setConfirmPath(null); }}
                className="px-4 py-2 text-sm text-gray-400 hover:text-white transition-colors"
              >
                Cancel
              </button>
              <motion.button
                whileHover={{ scale: 1.02 }}
                whileTap={{ scale: 0.98 }}
                onClick={handleConfirmImport}
                className="px-4 py-2 bg-bat-gold text-bat-dark font-semibold rounded-lg text-sm hover:bg-bat-gold/90 transition-colors"
              >
                Confirm Restore
              </motion.button>
            </div>
          </motion.div>
        </div>
      )}
    </>
  );
}
