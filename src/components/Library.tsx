import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { motion, AnimatePresence } from "framer-motion";

interface ScannedDocument {
  id: string;
  name: string;
  modified_time: string | null;
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

export function Library() {
  const navigate = useNavigate();
  const [documents, setDocuments] = useState<ScannedDocument[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [searchQuery, setSearchQuery] = useState("");

  const loadDocuments = async () => {
    setLoading(true);
    setError(null);
    try {
      const docs = await invoke<ScannedDocument[]>("list_scanned_documents");
      setDocuments(docs);
    } catch (e) {
      setError(`Failed to load documents: ${e}`);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadDocuments();
  }, []);

  const filteredDocs = searchQuery
    ? documents.filter((d) =>
        d.name.toLowerCase().includes(searchQuery.toLowerCase())
      )
    : documents;

  return (
    <div className="flex-1 px-6 py-8">
      <div className="max-w-6xl mx-auto">
        {/* Library header */}
        <div className="flex items-center justify-between mb-6">
          <div>
            <h2 className="chalk-heading text-2xl tracking-wide text-chalk-white mb-1">
              Library
            </h2>
            <p className="text-sm text-chalk-muted">
              Your lesson plans and imported documents
            </p>
          </div>
          <motion.button
            whileHover={{ scale: 1.03 }}
            whileTap={{ scale: 0.97 }}
            className="px-4 py-2.5 bg-chalk-blue/15 border border-chalk-blue/30 rounded-lg text-chalk-blue text-sm font-medium hover:bg-chalk-blue/25 transition-colors"
          >
            + New Plan
          </motion.button>
        </div>

        {/* Search bar */}
        <div className="relative mb-6">
          <svg
            className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-chalk-muted"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M21 21l-6-6m2-5a7 7 0 11-14 0 7 7 0 0114 0z" />
          </svg>
          <input
            type="text"
            placeholder="Search plans..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-10 pr-4 py-2.5 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-lg text-sm text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors"
          />
        </div>

        {/* Error state */}
        {error && (
          <motion.div
            initial={{ opacity: 0, y: -10 }}
            animate={{ opacity: 1, y: 0 }}
            className="mb-6 p-4 bg-chalk-red/10 border border-chalk-red/30 rounded-lg text-sm text-chalk-red"
          >
            {error}
            <button
              onClick={loadDocuments}
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
              className="w-8 h-8 border-2 border-chalk-blue border-t-transparent rounded-full"
            />
            <span className="ml-3 text-chalk-muted">Loading documents...</span>
          </div>
        )}

        {/* Empty state */}
        {!loading && !error && filteredDocs.length === 0 && (
          <motion.div
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="text-center py-20"
          >
            <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-chalk-board-dark border-2 border-chalk-white/10 flex items-center justify-center">
              <svg
                className="w-10 h-10 text-chalk-muted"
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
            <h3 className="text-lg chalk-text mb-2">
              {searchQuery ? "No matching plans" : "No documents found"}
            </h3>
            <p className="text-chalk-muted text-sm mb-6">
              {searchQuery
                ? "Try a different search term"
                : "Add Google Docs to your connected folder and they'll appear here."}
            </p>
            {!searchQuery && (
              <motion.button
                whileHover={{ scale: 1.05 }}
                whileTap={{ scale: 0.95 }}
                onClick={loadDocuments}
                className="px-6 py-2.5 border border-chalk-blue/30 rounded-lg text-chalk-blue hover:bg-chalk-blue/10 transition-colors"
              >
                Refresh
              </motion.button>
            )}
          </motion.div>
        )}

        {/* Document grid */}
        {!loading && filteredDocs.length > 0 && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ delay: 0.1 }}
          >
            <div className="flex items-center justify-between mb-4">
              <p className="text-sm text-chalk-muted">
                {filteredDocs.length} plan{filteredDocs.length !== 1 ? "s" : ""}
                {searchQuery && ` matching "${searchQuery}"`}
              </p>
              <motion.button
                whileHover={{ scale: 1.05 }}
                whileTap={{ scale: 0.95 }}
                onClick={loadDocuments}
                className="px-3 py-1.5 text-xs border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-white hover:border-chalk-white/20 transition-colors"
              >
                Refresh
              </motion.button>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              <AnimatePresence>
                {filteredDocs.map((doc, i) => (
                  <motion.button
                    key={doc.id}
                    initial={{ opacity: 0, y: 10 }}
                    animate={{ opacity: 1, y: 0 }}
                    transition={{ delay: i * 0.02 }}
                    onClick={() => navigate(`/plan/${doc.id}`)}
                    className="text-left p-4 bg-chalk-board-dark/50 border border-chalk-white/5 hover:border-chalk-blue/20 rounded-lg transition-all group hover:bg-chalk-board-dark/80"
                  >
                    <div className="flex items-start gap-3">
                      <div className="w-9 h-9 rounded-lg bg-chalk-blue/8 flex items-center justify-center flex-shrink-0 group-hover:bg-chalk-blue/15 transition-colors">
                        <svg
                          className="w-4.5 h-4.5 text-chalk-blue/60 group-hover:text-chalk-blue transition-colors"
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
                      <div className="flex-1 min-w-0">
                        <span className="block text-sm font-medium text-chalk-white truncate">
                          {doc.name}
                        </span>
                        {doc.modified_time && (
                          <span className="block text-xs text-chalk-muted mt-1">
                            {formatDate(doc.modified_time)}
                          </span>
                        )}
                      </div>
                    </div>
                  </motion.button>
                ))}
              </AnimatePresence>
            </div>
          </motion.div>
        )}

        {/* Footer */}
        {!loading && (
          <div className="mt-10 pt-6">
            <hr className="chalk-line mb-4" />
            <p className="text-xs text-chalk-muted/60 text-center">
              Chalk is indexing your documents in the background.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
