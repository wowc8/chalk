import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { useConnectors, type ConnectionDetails, type PendingOp } from "../hooks/useConnectors";
import { useAiConfig } from "../hooks/useChat";
import { useToast } from "./Toast";

const spring = { type: "spring" as const, stiffness: 300, damping: 30 };

interface SettingsPanelProps {
  open: boolean;
  onClose: () => void;
  onReconnect: () => void;
}

export function SettingsPanel({ open, onClose, onReconnect }: SettingsPanelProps) {
  const { connections, loading, pendingOp, disconnect, rescan } = useConnectors();
  const { addToast } = useToast();

  const [crashReportingEnabled, setCrashReportingEnabled] = useState(false);
  const [reportText, setReportText] = useState("");
  const [reportStatus, setReportStatus] = useState<"idle" | "sending" | "sent" | "error">("idle");
  const [privacyLoading, setPrivacyLoading] = useState(true);

  useEffect(() => {
    if (!open) return;
    invoke<{ consent_shown: boolean; crash_reporting_enabled: boolean }>(
      "get_privacy_consent_status"
    )
      .then((status) => setCrashReportingEnabled(status.crash_reporting_enabled))
      .catch(() => {})
      .finally(() => setPrivacyLoading(false));
  }, [open]);

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
      // silent fail
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
    <AnimatePresence>
      {open && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.2 }}
          className="fixed inset-0 z-40 settings-backdrop flex justify-end"
          onClick={(e) => {
            if (e.target === e.currentTarget) onClose();
          }}
        >
          <motion.div
            initial={{ x: "100%" }}
            animate={{ x: 0 }}
            exit={{ x: "100%" }}
            transition={spring}
            className="w-full max-w-md h-full overflow-y-auto bg-chalk-board border-l border-chalk-white/8"
          >
            <div className="px-6 py-6">
              {/* Header */}
              <div className="flex items-center justify-between mb-8">
                <h2 className="chalk-heading text-xl tracking-wide text-chalk-white">
                  Settings
                </h2>
                <button
                  onClick={onClose}
                  className="p-2 rounded-lg text-chalk-muted hover:text-chalk-white transition-colors"
                  aria-label="Close settings"
                >
                  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
              </div>

              {/* Connections Section */}
              <section className="mb-8">
                <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
                  Connections
                </h3>

                {loading ? (
                  <div className="flex items-center gap-3 py-8 justify-center">
                    <motion.div
                      animate={{ rotate: 360 }}
                      transition={{ duration: 1.5, repeat: Infinity, ease: "linear" }}
                      className="w-5 h-5 border-2 border-chalk-blue border-t-transparent rounded-full"
                    />
                    <span className="text-chalk-muted text-sm">Loading connections...</span>
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
                        <p className="text-chalk-muted text-sm mb-4">No connections configured</p>
                        <motion.button
                          whileHover={{ scale: 1.05 }}
                          whileTap={{ scale: 0.95 }}
                          onClick={onReconnect}
                          className="px-4 py-2 bg-chalk-blue/10 border border-chalk-blue/30 rounded-lg text-chalk-blue text-sm hover:bg-chalk-blue/20 transition-colors"
                        >
                          + Add Connection
                        </motion.button>
                      </div>
                    )}
                  </div>
                )}
              </section>

              {/* AI Assistant */}
              <AiSettingsSection />

              {/* Privacy & Crash Reporting */}
              <section className="mb-8">
                <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
                  Privacy & Crash Reporting
                </h3>

                {privacyLoading ? (
                  <div className="flex items-center gap-3 py-4 justify-center">
                    <div className="w-4 h-4 border-2 border-chalk-blue border-t-transparent rounded-full animate-spin" />
                  </div>
                ) : (
                  <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4 space-y-4">
                    <div className="flex items-center justify-between">
                      <div>
                        <p className="text-sm text-chalk-dust">Automatic crash reporting</p>
                        <p className="text-xs text-chalk-muted mt-0.5">
                          Send anonymous error reports to help improve Chalk
                        </p>
                      </div>
                      <button
                        onClick={toggleCrashReporting}
                        className={`relative w-12 h-7 rounded-full transition-colors ${
                          crashReportingEnabled ? "bg-chalk-blue" : "bg-chalk-board-light"
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
                    <p className="text-xs text-chalk-muted/70">
                      No student data, document content, or personal information is collected.
                    </p>
                  </div>
                )}
              </section>

              {/* Send Report */}
              <section className="mb-8">
                <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
                  Send Report
                </h3>
                <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4">
                  <p className="text-xs text-chalk-muted mb-3">
                    Encountered a bug? Describe what happened and we'll look into it.
                  </p>
                  <textarea
                    value={reportText}
                    onChange={(e) => setReportText(e.target.value)}
                    placeholder="Describe what went wrong..."
                    rows={4}
                    className="w-full bg-chalk-board/50 border border-chalk-white/8 rounded-lg p-3 text-sm text-chalk-white placeholder-chalk-muted resize-none focus:outline-none focus:border-chalk-blue/40 transition-colors"
                  />
                  <div className="flex items-center justify-between mt-3">
                    {reportStatus === "sent" && (
                      <p className="text-xs text-chalk-green">Report sent. Thank you!</p>
                    )}
                    {reportStatus === "error" && (
                      <p className="text-xs text-chalk-red">Failed to send. Please try again.</p>
                    )}
                    {(reportStatus === "idle" || reportStatus === "sending") && <span />}
                    <motion.button
                      whileHover={{ scale: 1.02 }}
                      whileTap={{ scale: 0.98 }}
                      disabled={!reportText.trim() || reportStatus === "sending"}
                      onClick={sendReport}
                      className="px-5 py-2 bg-chalk-yellow text-chalk-board-dark font-semibold rounded-lg text-sm hover:bg-chalk-yellow/90 transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
                    >
                      {reportStatus === "sending" ? "Sending..." : "Send Report"}
                    </motion.button>
                  </div>
                </div>
              </section>

              {/* App Info */}
              <section>
                <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
                  App
                </h3>
                <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4">
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="text-sm text-chalk-dust">Version</p>
                      <p className="text-xs text-chalk-muted">0.1.0</p>
                    </div>
                  </div>
                </div>
              </section>
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

// ── AI Settings Section ────────────────────────────────────────────

const AVAILABLE_MODELS = [
  { id: "gpt-4o", label: "GPT-4o", description: "Most capable" },
  { id: "gpt-4o-mini", label: "GPT-4o Mini", description: "Fast & affordable" },
];

function AiSettingsSection() {
  const { config, loading, saveConfig } = useAiConfig();
  const { addToast } = useToast();
  const [apiKeyInput, setApiKeyInput] = useState("");
  const [baseUrlInput, setBaseUrlInput] = useState("");
  const [saving, setSaving] = useState(false);
  const [showApiKey, setShowApiKey] = useState(false);

  useEffect(() => {
    if (config) {
      setBaseUrlInput(config.base_url);
    }
  }, [config]);

  const handleSaveApiKey = async () => {
    if (!apiKeyInput.trim()) return;
    setSaving(true);
    try {
      await saveConfig({ api_key: apiKeyInput.trim() });
      setApiKeyInput("");
      setShowApiKey(false);
      addToast("API key saved", "success");
    } catch {
      addToast("Failed to save API key", "error");
    } finally {
      setSaving(false);
    }
  };

  const handleSaveBaseUrl = async () => {
    setSaving(true);
    try {
      await saveConfig({ base_url: baseUrlInput.trim() || "https://api.openai.com/v1" });
      addToast("Base URL saved", "success");
    } catch {
      addToast("Failed to save base URL", "error");
    } finally {
      setSaving(false);
    }
  };

  const handleSelectModel = async (modelId: string) => {
    try {
      await saveConfig({ model: modelId });
      addToast(`Model set to ${modelId}`, "success");
    } catch {
      addToast("Failed to save model", "error");
    }
  };

  if (loading) {
    return (
      <section className="mb-8">
        <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
          AI Assistant
        </h3>
        <div className="flex items-center gap-3 py-4 justify-center">
          <div className="w-4 h-4 border-2 border-chalk-blue border-t-transparent rounded-full animate-spin" />
        </div>
      </section>
    );
  }

  return (
    <section className="mb-8">
      <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
        AI Assistant
      </h3>

      <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4 space-y-5">
        {/* API Key */}
        <div>
          <label className="block text-sm text-chalk-dust mb-1.5">
            OpenAI API Key
          </label>
          <div className="flex items-center gap-2">
            {config?.has_api_key && !showApiKey ? (
              <>
                <span className="flex-1 text-sm text-chalk-muted font-mono">
                  sk-••••••••••••
                </span>
                <button
                  onClick={() => setShowApiKey(true)}
                  className="px-3 py-1.5 text-xs border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-white hover:border-chalk-white/20 transition-colors"
                >
                  Change
                </button>
              </>
            ) : (
              <>
                <input
                  type="password"
                  value={apiKeyInput}
                  onChange={(e) => setApiKeyInput(e.target.value)}
                  placeholder="sk-..."
                  className="flex-1 bg-chalk-board/50 border border-chalk-white/8 rounded-lg px-3 py-2 text-sm text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors font-mono"
                />
                <motion.button
                  whileHover={{ scale: 1.02 }}
                  whileTap={{ scale: 0.98 }}
                  disabled={!apiKeyInput.trim() || saving}
                  onClick={handleSaveApiKey}
                  className="px-3 py-2 bg-chalk-blue/10 border border-chalk-blue/30 rounded-lg text-chalk-blue text-xs hover:bg-chalk-blue/20 transition-colors disabled:opacity-50"
                >
                  {saving ? "Saving..." : "Save"}
                </motion.button>
                {showApiKey && (
                  <button
                    onClick={() => {
                      setShowApiKey(false);
                      setApiKeyInput("");
                    }}
                    className="px-2 py-2 text-xs text-chalk-muted hover:text-chalk-dust transition-colors"
                  >
                    Cancel
                  </button>
                )}
              </>
            )}
          </div>
          <p className="text-xs text-chalk-muted/70 mt-1">
            Used for chat and embeddings. Never sent to our servers.
          </p>
        </div>

        {/* Model Picker */}
        <div>
          <label className="block text-sm text-chalk-dust mb-1.5">
            Model
          </label>
          <div className="grid grid-cols-2 gap-2">
            {AVAILABLE_MODELS.map((m) => (
              <button
                key={m.id}
                onClick={() => handleSelectModel(m.id)}
                className={`p-3 rounded-lg border text-left transition-colors ${
                  config?.model === m.id
                    ? "border-chalk-blue/50 bg-chalk-blue/10"
                    : "border-chalk-white/10 hover:border-chalk-white/20"
                }`}
              >
                <span
                  className={`block text-sm font-medium ${
                    config?.model === m.id ? "text-chalk-blue" : "text-chalk-dust"
                  }`}
                >
                  {m.label}
                </span>
                <span className="block text-[10px] text-chalk-muted mt-0.5">
                  {m.description}
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* Base URL (advanced) */}
        <div>
          <label className="block text-sm text-chalk-dust mb-1.5">
            API Base URL
            <span className="text-[10px] text-chalk-muted ml-1.5">(advanced)</span>
          </label>
          <div className="flex items-center gap-2">
            <input
              type="text"
              value={baseUrlInput}
              onChange={(e) => setBaseUrlInput(e.target.value)}
              placeholder="https://api.openai.com/v1"
              className="flex-1 bg-chalk-board/50 border border-chalk-white/8 rounded-lg px-3 py-2 text-sm text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors font-mono text-xs"
            />
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              disabled={saving || baseUrlInput === config?.base_url}
              onClick={handleSaveBaseUrl}
              className="px-3 py-2 text-xs border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-white hover:border-chalk-white/20 transition-colors disabled:opacity-50"
            >
              Save
            </motion.button>
          </div>
          <p className="text-xs text-chalk-muted/70 mt-1">
            Override for compatible endpoints (e.g., Azure OpenAI, local proxies).
          </p>
        </div>
      </div>
    </section>
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
  const isPending = pendingOp !== null && pendingOp.connectorId === connection.id;
  const isPendingDisconnect = isPending && pendingOp?.type === "disconnect";
  const isPendingRescan = isPending && pendingOp?.type === "rescan";

  return (
    <div
      className={`bg-chalk-board-dark/60 rounded-lg border p-4 transition-colors ${
        isPending
          ? "border-chalk-yellow/30 opacity-80"
          : isConnected
            ? "border-chalk-white/5 hover:border-chalk-blue/20"
            : "border-chalk-white/3"
      }`}
    >
      <div className="flex items-start gap-3">
        <div
          className={`w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0 ${
            isConnected ? "bg-chalk-blue/10" : "bg-chalk-board-light"
          }`}
        >
          <svg
            className={`w-5 h-5 ${isConnected ? "text-chalk-blue" : "text-chalk-muted"}`}
            fill="currentColor"
            viewBox="0 0 24 24"
          >
            <path d="M7.71 3.5L1.15 15l4.58 7.5h6.56l-4.58-7.5L14.28 3.5H7.71zm2.57 0l6.57 11.5H23.4L16.85 3.5h-6.57zm6.56 12.5L12.28 23.5h13.13l4.56-7.5H16.84z" />
          </svg>
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-chalk-white">{connection.display_name}</span>
            <span
              className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium uppercase tracking-wider ${
                isPendingDisconnect
                  ? "bg-chalk-yellow/10 text-chalk-yellow"
                  : isConnected
                    ? "bg-chalk-green/10 text-chalk-green"
                    : "bg-chalk-board-light text-chalk-muted"
              }`}
            >
              {isPendingDisconnect ? (
                <>
                  <span className="w-1.5 h-1.5 rounded-full bg-chalk-yellow animate-pulse" />
                  Disconnecting...
                </>
              ) : isConnected ? (
                <>
                  <span className="w-1.5 h-1.5 rounded-full bg-chalk-green" />
                  Connected
                </>
              ) : (
                <>
                  <span className="w-1.5 h-1.5 rounded-full bg-chalk-muted" />
                  Disconnected
                </>
              )}
            </span>
          </div>

          {connection.account_email && (
            <p className="text-xs text-chalk-muted mt-0.5">{connection.account_email}</p>
          )}

          {isConnected && connection.source_name && (
            <div className="mt-2 flex items-center gap-1.5 text-xs text-chalk-muted">
              <svg className="w-3.5 h-3.5 text-chalk-blue/60" fill="currentColor" viewBox="0 0 20 20">
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
              </svg>
              <span className="truncate">{connection.source_name}</span>
              {connection.document_count != null && (
                <span className="text-chalk-muted/60"> &middot; {connection.document_count} docs</span>
              )}
            </div>
          )}

          {connection.last_scan_at && (
            <p className="text-[10px] text-chalk-muted/60 mt-1">
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

      <div className="mt-3 pt-3 border-t border-chalk-white/5 flex gap-2">
        {isConnected ? (
          <>
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={onReconnect}
              disabled={isPending}
              className="px-3 py-1.5 text-xs border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-white hover:border-chalk-white/20 transition-colors disabled:opacity-50"
            >
              Change Source
            </motion.button>
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              onClick={onRescan}
              disabled={isPending}
              className="px-3 py-1.5 text-xs border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-blue hover:border-chalk-blue/30 transition-colors disabled:opacity-50 flex items-center gap-1.5"
            >
              {isPendingRescan ? (
                <>
                  <motion.span
                    animate={{ rotate: 360 }}
                    transition={{ duration: 1, repeat: Infinity, ease: "linear" }}
                    className="inline-block w-3 h-3 border border-chalk-blue border-t-transparent rounded-full"
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
              className="px-3 py-1.5 text-xs border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-red hover:border-chalk-red/30 transition-colors disabled:opacity-50 ml-auto"
            >
              Disconnect
            </motion.button>
          </>
        ) : (
          <motion.button
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            onClick={onReconnect}
            className="px-3 py-1.5 text-xs bg-chalk-blue/10 border border-chalk-blue/30 rounded-lg text-chalk-blue hover:bg-chalk-blue/20 transition-colors"
          >
            Connect
          </motion.button>
        )}
      </div>
    </div>
  );
}
