import { useState, useEffect, useCallback, useRef } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { TipTapEditor } from "./editor/TipTapEditor";
import { ChatPane } from "./editor/ChatPane";
import { useToast } from "./Toast";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";
import "./editor/EditorStyles.css";

interface LessonPlan {
  id: string;
  subject_id: string;
  title: string;
  content: string;
  source_doc_id: string | null;
  source_table_index: number | null;
  learning_objectives: string | null;
  status: string;
  created_at: string;
  updated_at: string;
}

interface PlanVersion {
  id: string;
  plan_id: string;
  version: number;
  title: string;
  content: string;
  learning_objectives: string | null;
  created_at: string;
}


function SplitResizer({
  onDrag,
}: {
  onDrag: (deltaY: number) => void;
}) {
  const dragging = useRef(false);
  const lastY = useRef(0);

  const handleMouseDown = (e: React.MouseEvent) => {
    e.preventDefault();
    dragging.current = true;
    lastY.current = e.clientY;
    document.body.style.cursor = "row-resize";
    document.body.style.userSelect = "none";

    const handleMouseMove = (e: MouseEvent) => {
      if (!dragging.current) return;
      const delta = e.clientY - lastY.current;
      lastY.current = e.clientY;
      onDrag(delta);
    };

    const handleMouseUp = () => {
      dragging.current = false;
      document.body.style.cursor = "";
      document.body.style.userSelect = "";
      document.removeEventListener("mousemove", handleMouseMove);
      document.removeEventListener("mouseup", handleMouseUp);
    };

    document.addEventListener("mousemove", handleMouseMove);
    document.addEventListener("mouseup", handleMouseUp);
  };

  return (
    <div
      onMouseDown={handleMouseDown}
      className="h-1.5 cursor-row-resize bg-chalk-white/5 hover:bg-chalk-blue/20 transition-colors flex items-center justify-center group flex-shrink-0"
    >
      <div className="w-8 h-0.5 rounded-full bg-chalk-muted/30 group-hover:bg-chalk-blue/40 transition-colors" />
    </div>
  );
}

export function PlanDetail() {
  const { planId } = useParams<{ planId: string }>();
  const navigate = useNavigate();
  const { addToast } = useToast();
  const [plan, setPlan] = useState<LessonPlan | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saveStatus, setSaveStatus] = useState<"saved" | "saving" | "error">("saved");
  const [editorRatio, setEditorRatio] = useState(0.667); // 2/3 by default
  const editorContentRef = useRef<string>("");
  const containerRef = useRef<HTMLDivElement>(null);

  // Versioning state
  const [finalizing, setFinalizing] = useState(false);
  const [versions, setVersions] = useState<PlanVersion[]>([]);
  const [showVersionHistory, setShowVersionHistory] = useState(false);
  const [reverting, setReverting] = useState(false);
  const versionDropdownRef = useRef<HTMLDivElement>(null);

  // Delete state
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);

  const isNewPlan = planId === "new";

  // Close version dropdown on outside click
  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (versionDropdownRef.current && !versionDropdownRef.current.contains(e.target as Node)) {
        setShowVersionHistory(false);
      }
    }
    if (showVersionHistory) {
      document.addEventListener("mousedown", handleClickOutside);
      return () => document.removeEventListener("mousedown", handleClickOutside);
    }
  }, [showVersionHistory]);

  useEffect(() => {
    async function load() {
      if (isNewPlan) {
        // Create a new plan
        try {
          const newPlan = await invoke<{
            id: string;
            title: string;
            status: string;
            source_type: string;
            version: number;
            tags: { id: string; name: string; color: string | null; created_at: string }[];
            created_at: string;
            updated_at: string;
          }>("create_plan", {
            title: "Untitled Plan",
            subjectId: "default",
            content: "",
            sourceType: "created",
          });
          // Now fetch the full plan
          const fullPlan = await invoke<LessonPlan>("get_plan", {
            id: newPlan.id,
          });
          setPlan(fullPlan);
          // Update the URL without adding history entry
          window.history.replaceState(null, "", `/plan/${fullPlan.id}`);
        } catch (e) {
          setError(`Failed to create plan: ${e}`);
        } finally {
          setLoading(false);
        }
        return;
      }

      try {
        const fetched = await invoke<LessonPlan>("get_plan", { id: planId });
        setPlan(fetched);
      } catch (e) {
        setError(`Failed to load plan: ${e}`);
      } finally {
        setLoading(false);
      }
    }
    load();
  }, [planId]);

  // Load versions when plan is available
  const loadVersions = useCallback(async () => {
    if (!plan) return;
    try {
      const v = await invoke<PlanVersion[]>("list_plan_versions", { planId: plan.id });
      setVersions(v);
    } catch {
      // silently fail — versions are non-critical
    }
  }, [plan?.id]);

  useEffect(() => {
    loadVersions();
  }, [loadVersions]);

  const handleEditorUpdate = useCallback(
    async (content: string) => {
      if (!plan) return;
      editorContentRef.current = content;
      setSaveStatus("saving");
      try {
        const updated = await invoke<LessonPlan>("update_plan_content", {
          id: plan.id,
          content,
        });
        setPlan(updated);
        setSaveStatus("saved");
      } catch {
        setSaveStatus("error");
      }
    },
    [plan]
  );

  // Keep content ref in sync when plan loads/changes.
  useEffect(() => {
    if (plan) {
      editorContentRef.current = plan.content;
    }
  }, [plan?.id]);

  const handleFinalize = useCallback(async () => {
    if (!plan || finalizing) return;
    setFinalizing(true);
    try {
      await invoke<PlanVersion>("finalize_plan", { id: plan.id });
      // Refresh plan and versions
      const updated = await invoke<LessonPlan>("get_plan", { id: plan.id });
      setPlan(updated);
      await loadVersions();
    } catch (e) {
      setError(`Failed to finalize: ${e}`);
    } finally {
      setFinalizing(false);
    }
  }, [plan, finalizing, loadVersions]);

  const handleRevert = useCallback(async (version: number) => {
    if (!plan || reverting) return;
    setReverting(true);
    try {
      const reverted = await invoke<LessonPlan>("revert_plan_version", {
        planId: plan.id,
        version,
      });
      setPlan(reverted);
      setShowVersionHistory(false);
      await loadVersions();
    } catch (e) {
      setError(`Failed to revert: ${e}`);
    } finally {
      setReverting(false);
    }
  }, [plan, reverting, loadVersions]);

  const handleResize = useCallback(
    (deltaY: number) => {
      if (!containerRef.current) return;
      const containerHeight = containerRef.current.getBoundingClientRect().height;
      // Use functional update to avoid stale closure when called from mousemove listener
      setEditorRatio((prev) => Math.max(0.3, Math.min(0.85, prev + deltaY / containerHeight)));
    },
    []
  );

  const handleDelete = useCallback(async () => {
    if (!plan) return;
    try {
      await invoke("delete_plan", { id: plan.id });
      addToast(`"${plan.title}" deleted`, "success");
      navigate("/");
    } catch (e) {
      addToast(`Failed to delete plan: ${e}`, "error");
      setShowDeleteDialog(false);
    }
  }, [plan, addToast, navigate]);

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <motion.div
          animate={{ rotate: 360 }}
          transition={{ duration: 1.5, repeat: Infinity, ease: "linear" }}
          className="w-8 h-8 border-2 border-chalk-blue border-t-transparent rounded-full"
        />
        <span className="ml-3 text-chalk-muted text-sm">
          {isNewPlan ? "Creating plan..." : "Loading plan..."}
        </span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="text-center">
          <div className="w-16 h-16 mx-auto mb-4 rounded-full bg-chalk-red/10 border border-chalk-red/20 flex items-center justify-center">
            <svg className="w-8 h-8 text-chalk-red" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z" />
            </svg>
          </div>
          <p className="text-chalk-red text-sm mb-2">{error}</p>
          <button
            onClick={() => window.history.back()}
            className="text-chalk-muted text-sm hover:text-chalk-white transition-colors"
          >
            Go back to Library
          </button>
        </div>
      </div>
    );
  }

  if (!plan) return null;

  return (
    <div ref={containerRef} className="flex-1 flex flex-col min-h-0">
      {/* Plan header bar */}
      <div className="flex items-center justify-between px-4 py-2 border-b border-chalk-white/5 flex-shrink-0">
        <div className="flex items-center gap-3 min-w-0">
          <div className="w-7 h-7 rounded-lg bg-chalk-blue/10 flex items-center justify-center flex-shrink-0">
            <svg className="w-3.5 h-3.5 text-chalk-blue" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z" />
            </svg>
          </div>
          <PlanTitle
            title={plan.title}
            planId={plan.id}
            onTitleChange={(newTitle) => setPlan((p) => p ? { ...p, title: newTitle } : p)}
          />
        </div>
        <div className="flex items-center gap-3">
          <SaveIndicator status={saveStatus} />

          {/* Version history dropdown */}
          <div className="relative" ref={versionDropdownRef}>
            <button
              onClick={() => {
                setShowVersionHistory((v) => !v);
                loadVersions();
              }}
              className="flex items-center gap-1.5 text-xs px-2 py-1 rounded border border-chalk-white/8 bg-chalk-ghost text-chalk-muted hover:text-chalk-white hover:border-chalk-white/15 transition-colors"
              title="Version history"
            >
              <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 8v4l3 3m6-3a9 9 0 11-18 0 9 9 0 0118 0z" />
              </svg>
              {versions.length > 0 ? `v${versions[0].version}` : "v0"}
            </button>

            <AnimatePresence>
              {showVersionHistory && (
                <motion.div
                  initial={{ opacity: 0, y: -4 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -4 }}
                  transition={{ duration: 0.15 }}
                  className="absolute right-0 top-full mt-1 w-72 bg-chalk-board border border-chalk-white/10 rounded-lg shadow-xl z-50 overflow-hidden"
                >
                  <div className="px-3 py-2 border-b border-chalk-white/5">
                    <span className="text-xs font-medium text-chalk-muted">Version History</span>
                  </div>
                  {versions.length === 0 ? (
                    <div className="px-3 py-4 text-center text-xs text-chalk-muted/60">
                      No versions yet. Click Finalize to save your first version.
                    </div>
                  ) : (
                    <div className="max-h-64 overflow-y-auto">
                      {versions.map((v) => (
                        <div
                          key={v.id}
                          className="px-3 py-2 border-b border-chalk-white/5 last:border-0 hover:bg-chalk-white/3 transition-colors"
                        >
                          <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2 min-w-0">
                              <span className="text-xs font-mono font-medium text-chalk-blue">v{v.version}</span>
                              <span className="text-xs text-chalk-white/80 truncate">{v.title}</span>
                            </div>
                            <button
                              onClick={() => handleRevert(v.version)}
                              disabled={reverting}
                              className="text-[10px] px-1.5 py-0.5 rounded bg-chalk-white/5 text-chalk-muted hover:text-chalk-white hover:bg-chalk-white/10 transition-colors disabled:opacity-50 flex-shrink-0 ml-2"
                            >
                              {reverting ? "..." : "Revert"}
                            </button>
                          </div>
                          <div className="text-[10px] text-chalk-muted/50 mt-0.5">
                            {new Date(v.created_at + "Z").toLocaleString()}
                          </div>
                        </div>
                      ))}
                    </div>
                  )}
                </motion.div>
              )}
            </AnimatePresence>
          </div>

          {/* Status badge */}
          <span
            className={`text-xs px-2 py-0.5 rounded capitalize ${
              plan.status === "finalized"
                ? "bg-chalk-green/10 text-chalk-green border border-chalk-green/20"
                : plan.status === "published"
                ? "bg-chalk-green/10 text-chalk-green border border-chalk-green/20"
                : "bg-chalk-ghost text-chalk-muted border border-chalk-white/8"
            }`}
          >
            {plan.status}
          </span>

          {/* Delete button */}
          <button
            onClick={() => setShowDeleteDialog(true)}
            className="flex items-center gap-1.5 text-xs px-2 py-1 rounded border border-chalk-white/8 bg-chalk-ghost text-chalk-muted hover:text-chalk-red hover:border-chalk-red/30 hover:bg-chalk-red/10 transition-colors"
            title="Delete plan"
          >
            <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
            </svg>
          </button>

          {/* Finalize button */}
          <button
            onClick={handleFinalize}
            disabled={finalizing}
            className="flex items-center gap-1.5 text-xs px-3 py-1.5 rounded-md bg-chalk-blue/15 text-chalk-blue border border-chalk-blue/25 hover:bg-chalk-blue/25 hover:border-chalk-blue/40 transition-colors disabled:opacity-50"
          >
            {finalizing ? (
              <motion.div
                animate={{ rotate: 360 }}
                transition={{ duration: 1, repeat: Infinity, ease: "linear" }}
                className="w-3 h-3 border border-chalk-blue/40 border-t-transparent rounded-full"
              />
            ) : (
              <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
              </svg>
            )}
            {finalizing ? "Finalizing..." : "Finalize"}
          </button>
        </div>
      </div>

      {/* Split pane: editor top, chat bottom */}
      <div className="flex-1 flex flex-col min-h-0">
        {/* Editor pane */}
        <div
          className="min-h-0 overflow-hidden bg-chalk-board/50"
          style={{ flex: `0 0 ${editorRatio * 100}%` }}
        >
          <TipTapEditor
            content={plan.content}
            onUpdate={handleEditorUpdate}
          />
        </div>

        {/* Resizer */}
        <SplitResizer onDrag={handleResize} />

        {/* Chat pane */}
        <div className="flex-1 min-h-0 overflow-hidden bg-chalk-board-dark/30">
          <ChatPane
            planId={plan.id}
            planTitle={plan.title}
            planContentRef={editorContentRef}
            onApplyToEditor={handleEditorUpdate}
          />
        </div>
      </div>

      {showDeleteDialog && plan && (
        <DeleteConfirmDialog
          planTitle={plan.title}
          onConfirm={handleDelete}
          onCancel={() => setShowDeleteDialog(false)}
        />
      )}
    </div>
  );
}

function PlanTitle({
  title,
  planId,
  onTitleChange,
}: {
  title: string;
  planId: string;
  onTitleChange: (title: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [value, setValue] = useState(title);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setValue(title);
  }, [title]);

  useEffect(() => {
    if (editing && inputRef.current) {
      inputRef.current.focus();
      inputRef.current.select();
    }
  }, [editing]);

  const save = async () => {
    setEditing(false);
    const trimmed = value.trim();
    if (!trimmed || trimmed === title) {
      setValue(title);
      return;
    }
    try {
      const updated = await invoke<LessonPlan>("update_plan_title", {
        id: planId,
        title: trimmed,
      });
      onTitleChange(updated.title);
    } catch {
      setValue(title);
    }
  };

  if (editing) {
    return (
      <input
        ref={inputRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onBlur={save}
        onKeyDown={(e) => {
          if (e.key === "Enter") save();
          if (e.key === "Escape") {
            setValue(title);
            setEditing(false);
          }
        }}
        className="text-sm font-medium text-chalk-white bg-transparent border-b border-chalk-blue/40 focus:outline-none px-0 py-0.5 min-w-0"
      />
    );
  }

  return (
    <button
      onClick={() => setEditing(true)}
      className="text-sm font-medium text-chalk-white hover:text-chalk-blue truncate max-w-xs transition-colors"
      title="Click to rename"
    >
      {title}
    </button>
  );
}

function SaveIndicator({ status }: { status: "saved" | "saving" | "error" }) {
  return (
    <span
      className={`flex items-center gap-1.5 text-[11px] ${
        status === "saved"
          ? "text-chalk-muted/50"
          : status === "saving"
          ? "text-chalk-blue/60"
          : "text-chalk-red/60"
      }`}
    >
      {status === "saving" && (
        <motion.div
          animate={{ rotate: 360 }}
          transition={{ duration: 1, repeat: Infinity, ease: "linear" }}
          className="w-3 h-3 border border-chalk-blue/40 border-t-transparent rounded-full"
        />
      )}
      {status === "saved" && (
        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M5 13l4 4L19 7" />
        </svg>
      )}
      {status === "error" && (
        <svg className="w-3 h-3" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 9v2m0 4h.01" />
        </svg>
      )}
      {status === "saved" ? "Saved" : status === "saving" ? "Saving..." : "Save failed"}
    </span>
  );
}
