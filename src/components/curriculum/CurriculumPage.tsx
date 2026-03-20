import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { motion, AnimatePresence } from "framer-motion";
import { useToast } from "../Toast";
import { DeleteConfirmDialog } from "../DeleteConfirmDialog";
import { LtpGrid } from "./LtpGrid";
import { ImportDropZone } from "./ImportDropZone";
import "./CurriculumStyles.css";

interface LtpDocument {
  id: string;
  filename: string;
  file_hash: string;
  school_year: string | null;
  doc_type: string;
  imported_at: string;
  updated_at: string;
}

interface ImportResult {
  status: "imported" | "skipped";
  id: string;
  filename: string;
  doc_type?: string;
  school_year?: string | null;
  cells_parsed?: number;
  months?: string[];
  subjects?: string[];
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

export function CurriculumPage() {
  const { addToast } = useToast();
  const [documents, setDocuments] = useState<LtpDocument[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedDoc, setSelectedDoc] = useState<LtpDocument | null>(null);
  const [deletingDoc, setDeletingDoc] = useState<LtpDocument | null>(null);
  const [importing, setImporting] = useState(false);
  const [showImportZone, setShowImportZone] = useState(false);

  const loadDocuments = useCallback(async () => {
    try {
      const docs = await invoke<LtpDocument[]>("list_ltp_documents");
      setDocuments(docs);
      // Auto-select first document if none selected
      if (!selectedDoc && docs.length > 0) {
        setSelectedDoc(docs[0]);
      }
    } catch (e) {
      addToast(`Failed to load documents: ${e}`, "error");
    } finally {
      setLoading(false);
    }
  }, [selectedDoc]);

  useEffect(() => {
    loadDocuments();
  }, []);

  async function handleImportFile(filePath?: string) {
    setImporting(true);
    try {
      let path = filePath;
      if (!path) {
        const selected = await open({
          multiple: false,
          filters: [{ name: "HTML Files", extensions: ["html", "htm"] }],
        });
        if (!selected) {
          setImporting(false);
          return;
        }
        path = selected as string;
      }

      const result = await invoke<ImportResult>("import_ltp_document", {
        path,
      });

      if (result.status === "imported") {
        addToast(
          `Imported "${result.filename}" — ${result.cells_parsed ?? 0} cells parsed`,
          "success",
        );
      } else {
        addToast(
          `"${result.filename}" is unchanged (skipped)`,
          "info",
        );
      }

      await loadDocuments();
      // Select the newly imported doc
      const docs = await invoke<LtpDocument[]>("list_ltp_documents");
      const imported = docs.find((d) => d.id === result.id);
      if (imported) setSelectedDoc(imported);
      setShowImportZone(false);
    } catch (e) {
      addToast(`Import failed: ${e}`, "error");
    } finally {
      setImporting(false);
    }
  }

  async function handleDeleteDoc() {
    if (!deletingDoc) return;
    try {
      await invoke("delete_ltp_document", { id: deletingDoc.id });
      addToast(`"${deletingDoc.filename}" deleted`, "success");
      if (selectedDoc?.id === deletingDoc.id) {
        setSelectedDoc(null);
      }
      setDeletingDoc(null);
      await loadDocuments();
    } catch (e) {
      addToast(`Failed to delete: ${e}`, "error");
      setDeletingDoc(null);
    }
  }

  // Show import zone when no documents exist
  const showEmpty = !loading && documents.length === 0 && !showImportZone;

  return (
    <div className="h-full flex flex-col overflow-hidden">
      {/* Toolbar */}
      <div className="flex-shrink-0 px-5 py-3 border-b border-chalk-white/6 flex items-center justify-between gap-4">
        <div className="flex items-center gap-3 min-w-0">
          <h2 className="text-sm font-semibold text-chalk-white whitespace-nowrap">
            Long-Term Plan
          </h2>

          {/* Document selector tabs */}
          {documents.length > 0 && (
            <div className="flex items-center gap-1 overflow-x-auto">
              {documents.map((doc) => (
                <button
                  key={doc.id}
                  onClick={() => setSelectedDoc(doc)}
                  className={`group relative flex items-center gap-2 px-3 py-1.5 rounded-md text-xs font-medium transition-all whitespace-nowrap ${
                    selectedDoc?.id === doc.id
                      ? "bg-chalk-blue/12 text-chalk-blue border border-chalk-blue/20"
                      : "text-chalk-muted hover:text-chalk-dust hover:bg-chalk-white/4"
                  }`}
                >
                  <span className="truncate max-w-[140px]">{doc.filename.replace(/\.html?$/i, "")}</span>
                  <span className={`text-[10px] px-1 py-0.5 rounded ${
                    doc.doc_type === "calendar"
                      ? "bg-chalk-green/10 text-chalk-green"
                      : "bg-chalk-yellow/10 text-chalk-yellow"
                  }`}>
                    {doc.doc_type === "calendar" ? "Cal" : "LTP"}
                  </span>
                  {/* Delete button on hover */}
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setDeletingDoc(doc);
                    }}
                    className="opacity-0 group-hover:opacity-100 p-0.5 rounded hover:bg-chalk-red/15 text-chalk-muted hover:text-chalk-red transition-all"
                    title="Delete document"
                  >
                    <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                    </svg>
                  </button>
                </button>
              ))}
            </div>
          )}
        </div>

        <div className="flex items-center gap-2 flex-shrink-0">
          {selectedDoc && (
            <span className="text-[10px] text-chalk-muted">
              Imported {formatDate(selectedDoc.imported_at)}
            </span>
          )}
          <button
            onClick={() => setShowImportZone(true)}
            className="btn btn-secondary text-xs"
            disabled={importing}
          >
            {importing ? (
              <>
                <span className="spinner spinner-sm" style={{ width: 12, height: 12, borderWidth: 1.5 }} />
                Importing...
              </>
            ) : (
              <>
                <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M4 16v1a3 3 0 003 3h10a3 3 0 003-3v-1m-4-8l-4-4m0 0L8 8m4-4v12" />
                </svg>
                Import
              </>
            )}
          </button>
        </div>
      </div>

      {/* Content area */}
      <div className="flex-1 overflow-hidden">
        <AnimatePresence mode="wait">
          {loading && (
            <motion.div
              key="loading"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="flex items-center justify-center h-full"
            >
              <div className="spinner" />
              <span className="ml-3 text-chalk-muted text-sm">Loading documents...</span>
            </motion.div>
          )}

          {showEmpty && (
            <motion.div
              key="empty"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              className="flex items-center justify-center h-full"
            >
              <ImportDropZone onImport={handleImportFile} importing={importing} />
            </motion.div>
          )}

          {showImportZone && (
            <motion.div
              key="import"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -8 }}
              className="flex items-center justify-center h-full"
            >
              <div className="relative">
                <button
                  onClick={() => setShowImportZone(false)}
                  className="absolute -top-2 -right-2 z-10 p-1.5 rounded-full bg-chalk-board-dark border border-chalk-white/10 text-chalk-muted hover:text-chalk-white hover:bg-chalk-board-light transition-all"
                >
                  <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M6 18L18 6M6 6l12 12" />
                  </svg>
                </button>
                <ImportDropZone onImport={handleImportFile} importing={importing} />
              </div>
            </motion.div>
          )}

          {!loading && selectedDoc && !showImportZone && (
            <motion.div
              key={`grid-${selectedDoc.id}`}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="h-full"
            >
              <LtpGrid document={selectedDoc} />
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {/* Delete confirmation */}
      {deletingDoc && (
        <DeleteConfirmDialog
          planTitle={deletingDoc.filename}
          onConfirm={handleDeleteDoc}
          onCancel={() => setDeletingDoc(null)}
        />
      )}
    </div>
  );
}
