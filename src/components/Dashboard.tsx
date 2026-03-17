import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";

interface ScannedDocument {
  id: string;
  name: string;
  modified_time: string | null;
}

interface OnboardingStatus {
  oauth_configured: boolean;
  tokens_stored: boolean;
  folder_selected: boolean;
  folder_accessible: boolean;
  initial_shred_complete: boolean;
  selected_folder_id: string | null;
  selected_folder_name: string | null;
}

function formatDate(iso: string | null): string {
  if (!iso) return "";
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function Dashboard({ onResetOnboarding, onOpenSettings }: { onResetOnboarding?: () => void; onOpenSettings?: () => void }) {
  const [documents, setDocuments] = useState<ScannedDocument[]>([]);
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    async function load() {
      try {
        const [s, docs] = await Promise.all([
          invoke<OnboardingStatus>("check_onboarding_status"),
          invoke<ScannedDocument[]>("list_scanned_documents"),
        ]);
        setStatus(s);
        setDocuments(docs);
      } catch (e) {
        setError(`Failed to load documents: ${e}`);
      } finally {
        setLoading(false);
      }
    }
    load();
  }, []);

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

      <div className="relative z-10 max-w-3xl mx-auto px-6 py-10">
        {/* Header */}
        <motion.div
          initial={{ opacity: 0, y: -20 }}
          animate={{ opacity: 1, y: 0 }}
          className="mb-8"
        >
          <div className="flex items-center justify-between mb-2">
            <h1 className="text-3xl font-bold bg-gradient-to-r from-bat-gold to-bat-cyan bg-clip-text text-transparent">
              Your Lesson Plans
            </h1>
            {onOpenSettings && (
              <motion.button
                whileHover={{ scale: 1.1, rotate: 90 }}
                whileTap={{ scale: 0.9 }}
                transition={{ type: "spring", stiffness: 300, damping: 20 }}
                onClick={onOpenSettings}
                className="p-2 rounded-lg border border-gray-700 hover:border-gray-500 text-gray-400 hover:text-white transition-colors"
                title="Settings"
              >
                <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M10.325 4.317c.426-1.756 2.924-1.756 3.35 0a1.724 1.724 0 002.573 1.066c1.543-.94 3.31.826 2.37 2.37a1.724 1.724 0 001.066 2.573c1.756.426 1.756 2.924 0 3.35a1.724 1.724 0 00-1.066 2.573c.94 1.543-.826 3.31-2.37 2.37a1.724 1.724 0 00-2.573 1.066c-.426 1.756-2.924 1.756-3.35 0a1.724 1.724 0 00-2.573-1.066c-1.543.94-3.31-.826-2.37-2.37a1.724 1.724 0 00-1.066-2.573c-1.756-.426-1.756-2.924 0-3.35a1.724 1.724 0 001.066-2.573c-.94-1.543.826-3.31 2.37-2.37.996.608 2.296.07 2.572-1.065z" />
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
                </svg>
              </motion.button>
            )}
          </div>
          {status?.selected_folder_name && (
            <p className="text-gray-400 text-sm flex items-center gap-2">
              <svg
                className="w-4 h-4 text-bat-cyan"
                fill="currentColor"
                viewBox="0 0 20 20"
              >
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z" />
              </svg>
              Connected to{" "}
              <span className="text-white font-medium">
                {status.selected_folder_name}
              </span>
            </p>
          )}
        </motion.div>

        {/* Error state */}
        {error && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            className="mb-6 p-4 bg-bat-red/20 border border-bat-red/40 rounded-lg text-sm text-bat-red"
          >
            {error}
            <button
              onClick={() => {
                setError(null);
                setLoading(true);
                invoke<ScannedDocument[]>("list_scanned_documents")
                  .then(setDocuments)
                  .catch((e) => setError(`Retry failed: ${e}`))
                  .finally(() => setLoading(false));
              }}
              className="ml-3 underline hover:no-underline"
            >
              Retry
            </button>
          </motion.div>
        )}

        {/* Loading state */}
        {loading && (
          <div className="flex items-center justify-center py-20">
            <motion.div
              animate={{ rotate: 360 }}
              transition={{ duration: 1.5, repeat: Infinity, ease: "linear" }}
              className="w-8 h-8 border-2 border-bat-cyan border-t-transparent rounded-full"
            />
            <span className="ml-3 text-gray-400">Loading documents...</span>
          </div>
        )}

        {/* Empty state */}
        {!loading && !error && documents.length === 0 && (
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="text-center py-16"
          >
            <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-charcoal border-2 border-bat-purple/40 flex items-center justify-center">
              <svg
                className="w-10 h-10 text-bat-purple"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                />
              </svg>
            </div>
            <h3 className="text-lg font-semibold text-gray-300 mb-2">
              No documents found
            </h3>
            <p className="text-gray-500 text-sm mb-6">
              Add Google Docs to your connected folder and they'll appear here.
            </p>
            <motion.button
              whileHover={{ scale: 1.05 }}
              whileTap={{ scale: 0.95 }}
              onClick={() => {
                setLoading(true);
                setError(null);
                invoke<ScannedDocument[]>("list_scanned_documents")
                  .then(setDocuments)
                  .catch((e) => setError(`${e}`))
                  .finally(() => setLoading(false));
              }}
              className="px-6 py-2.5 border border-bat-cyan rounded-lg text-bat-cyan hover:bg-bat-cyan/10 transition-colors"
            >
              Refresh
            </motion.button>
          </motion.div>
        )}

        {/* Document list */}
        {!loading && documents.length > 0 && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.1 }}
          >
            <div className="flex items-center justify-between mb-4">
              <p className="text-sm text-gray-400">
                {documents.length} document{documents.length !== 1 ? "s" : ""}{" "}
                found
              </p>
              <motion.button
                whileHover={{ scale: 1.05 }}
                whileTap={{ scale: 0.95 }}
                onClick={() => {
                  setLoading(true);
                  setError(null);
                  invoke<ScannedDocument[]>("list_scanned_documents")
                    .then(setDocuments)
                    .catch((e) => setError(`${e}`))
                    .finally(() => setLoading(false));
                }}
                className="px-3 py-1.5 text-xs border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors"
              >
                Refresh
              </motion.button>
            </div>

            <div className="space-y-2">
              <AnimatePresence>
                {documents.map((doc, i) => (
                  <motion.div
                    key={doc.id}
                    initial={{ opacity: 0, y: 10 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: i * 0.03 }}
                    className="flex items-center gap-3 px-4 py-3 bg-bat-charcoal/50 border border-transparent hover:border-bat-purple/30 rounded-lg transition-colors group cursor-default"
                  >
                    <svg
                      className="w-5 h-5 flex-shrink-0 text-bat-cyan/60 group-hover:text-bat-cyan transition-colors"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={1.5}
                        d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
                      />
                    </svg>
                    <span className="flex-1 truncate text-sm">{doc.name}</span>
                    {doc.modified_time && (
                      <span className="text-xs text-gray-500 flex-shrink-0">
                        {formatDate(doc.modified_time)}
                      </span>
                    )}
                  </motion.div>
                ))}
              </AnimatePresence>
            </div>
          </motion.div>
        )}

        {/* Footer with settings hint */}
        {!loading && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.3 }}
            className="mt-10 pt-6 border-t border-bat-charcoal text-center"
          >
            <p className="text-xs text-gray-600">
              Chalk is indexing your documents in the background.
              {onResetOnboarding && (
                <>
                  {" "}
                  <button
                    onClick={onResetOnboarding}
                    className="text-gray-500 hover:text-bat-cyan transition-colors underline"
                  >
                    Change connected folder
                  </button>
                </>
              )}
            </p>
          </motion.div>
        )}
      </div>
    </div>
  );
}
