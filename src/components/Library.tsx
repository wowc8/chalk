import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { motion, AnimatePresence } from "framer-motion";
import { useTeacherName } from "../hooks/useTeacherName";
import { useToast } from "./Toast";
import { DeleteConfirmDialog } from "./DeleteConfirmDialog";

interface Tag {
  id: string;
  name: string;
  color: string | null;
  created_at: string;
}

interface LibraryPlanCard {
  id: string;
  title: string;
  status: string;
  source_type: string;
  version: number;
  tags: Tag[];
  created_at: string;
  updated_at: string;
}

const TAG_COLORS = [
  "#74b9ff",
  "#55efc4",
  "#ffeaa7",
  "#fd79a8",
  "#f0b060",
  "#a29bfe",
  "#81ecec",
  "#fab1a0",
];

function getTagColor(tag: Tag, index: number): string {
  return tag.color || TAG_COLORS[index % TAG_COLORS.length];
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

const LIBRARY_GREETINGS = [
  "Ready to plan?",
  "What are we building today?",
  "Let's make something great.",
  "Good to see you!",
];

function getGreeting(name: string | null): string {
  const greeting =
    LIBRARY_GREETINGS[Math.floor(Math.random() * LIBRARY_GREETINGS.length)];
  return name ? `Hey ${name}! ${greeting}` : greeting;
}

export function Library() {
  const navigate = useNavigate();
  const { name: teacherName } = useTeacherName();
  const [greeting] = useState(() => getGreeting(null));
  const [plans, setPlans] = useState<LibraryPlanCard[]>([]);
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [selectedTagIds, setSelectedTagIds] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [deletingPlan, setDeletingPlan] = useState<LibraryPlanCard | null>(null);
  const [indexingDone, setIndexingDone] = useState<boolean | null>(null);
  const [showComplete, setShowComplete] = useState(false);
  const wasIndexingRef = useRef(false);
  const { addToast } = useToast();

  // Refs to capture current filter state for the focus handler
  const searchRef = useRef(searchQuery);
  searchRef.current = searchQuery;
  const tagsRef = useRef(selectedTagIds);
  tagsRef.current = selectedTagIds;

  const loadPlans = async () => {
    setLoading(true);
    setError(null);
    try {
      const [fetchedPlans, fetchedTags] = await Promise.all([
        invoke<LibraryPlanCard[]>("list_library_plans", {
          search: searchRef.current || null,
          tagIds: tagsRef.current.length > 0 ? tagsRef.current : null,
        }),
        invoke<Tag[]>("list_tags"),
      ]);
      setPlans(fetchedPlans);
      setAllTags(fetchedTags);
    } catch (e) {
      setError(`Failed to load plans: ${e}`);
      setPlans([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    loadPlans();
  }, [selectedTagIds]);

  // Debounced search
  useEffect(() => {
    const timer = setTimeout(() => {
      loadPlans();
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  // Check indexing status, poll while incomplete
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout>;

    async function checkIndexing() {
      try {
        const status = await invoke<{ initial_digest_complete: boolean }>(
          "check_onboarding_status",
        );
        if (cancelled) return;

        if (status.initial_digest_complete) {
          if (wasIndexingRef.current) {
            setShowComplete(true);
            setTimeout(() => {
              if (!cancelled) setShowComplete(false);
            }, 3000);
          }
          setIndexingDone(true);
        } else {
          wasIndexingRef.current = true;
          setIndexingDone(false);
          timer = setTimeout(checkIndexing, 10_000);
        }
      } catch {
        if (!cancelled) setIndexingDone(true);
      }
    }

    checkIndexing();
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, []);

  // Auto-refresh when window regains focus (e.g. returning from plan detail)
  useEffect(() => {
    const handleFocus = () => loadPlans();
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, []);

  function toggleTag(tagId: string) {
    setSelectedTagIds((prev) =>
      prev.includes(tagId)
        ? prev.filter((id) => id !== tagId)
        : [...prev, tagId]
    );
  }

  async function handleDeletePlan() {
    if (!deletingPlan) return;
    try {
      await invoke("delete_plan", { id: deletingPlan.id });
      addToast(`"${deletingPlan.title}" deleted`, "success");
      setDeletingPlan(null);
      loadPlans();
    } catch (e) {
      addToast(`Failed to delete plan: ${e}`, "error");
      setDeletingPlan(null);
    }
  }

  return (
    <div className="px-6 py-6">
      <div className="max-w-4xl mx-auto">
        {/* Library header */}
        <div className="flex items-center justify-between mb-5">
          <div>
            <h2 className="text-lg font-semibold text-chalk-white">
              Library
            </h2>
            <p className="text-xs text-chalk-muted mt-0.5">
              {teacherName ? getGreeting(teacherName) : greeting}
            </p>
          </div>
          <button onClick={() => navigate("/plan/new")} className="btn btn-primary">
            <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
              <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
            </svg>
            New Plan
          </button>
        </div>

        {/* Search bar */}
        <div className="relative mb-4">
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
            className="w-full pl-10 pr-4 py-2 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-lg text-sm text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors"
          />
        </div>

        {/* Tag chips */}
        {allTags.length > 0 && (
          <div className="flex flex-wrap gap-2 mb-5">
            {allTags.map((tag, i) => {
              const isSelected = selectedTagIds.includes(tag.id);
              const tagColor = getTagColor(tag, i);
              return (
                <button
                  key={tag.id}
                  onClick={() => toggleTag(tag.id)}
                  className="px-3 py-1 rounded-full text-xs font-medium transition-all"
                  style={{
                    backgroundColor: isSelected
                      ? `${tagColor}22`
                      : "rgba(45,52,54,0.5)",
                    border: `1px solid ${isSelected ? `${tagColor}66` : "rgba(232,228,223,0.08)"}`,
                    color: isSelected ? tagColor : "var(--color-chalk-muted)",
                  }}
                >
                  {tag.name}
                </button>
              );
            })}
            {selectedTagIds.length > 0 && (
              <button
                onClick={() => setSelectedTagIds([])}
                className="px-3 py-1 rounded-full text-xs text-chalk-muted hover:text-chalk-dust transition-colors"
              >
                Clear filters
              </button>
            )}
          </div>
        )}

        {/* Error state */}
        {error && (
          <div className="mb-5 p-4 bg-chalk-red/10 border border-chalk-red/30 rounded-lg text-sm text-chalk-red">
            {error}
            <button
              onClick={loadPlans}
              className="ml-3 underline hover:no-underline"
            >
              Retry
            </button>
          </div>
        )}

        {/* Loading state */}
        {loading && (
          <div className="flex items-center justify-center py-16">
            <div className="spinner" />
            <span className="ml-3 text-chalk-muted text-sm">Loading plans...</span>
          </div>
        )}

        {/* Empty state */}
        {!loading && !error && plans.length === 0 && (
          <div className="text-center py-16">
            <div className="w-16 h-16 mx-auto mb-5 rounded-2xl bg-chalk-board-dark border border-chalk-white/8 flex items-center justify-center">
              <svg
                className="w-8 h-8 text-chalk-muted"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={1.5}
                  d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"
                />
              </svg>
            </div>
            <h3 className="text-base font-medium text-chalk-white mb-1">
              {searchQuery ? "No matching plans" : "No plans yet"}
            </h3>
            <p className="text-chalk-muted text-sm mb-5">
              {searchQuery
                ? "Try a different search term"
                : "Create your first lesson plan to get started."}
            </p>
            {!searchQuery && (
              <button onClick={() => navigate("/plan/new")} className="btn btn-secondary">
                <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                  <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
                </svg>
                Create Plan
              </button>
            )}
          </div>
        )}

        {/* Plan cards */}
        {!loading && plans.length > 0 && (
          <div>
            <p className="text-xs text-chalk-muted mb-3">
              {plans.length} plan{plans.length !== 1 ? "s" : ""}
              {searchQuery && ` matching "${searchQuery}"`}
            </p>

            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {plans.map((plan) => (
                <div
                  key={plan.id}
                  onClick={() => navigate(`/plan/${plan.id}`)}
                  className="relative text-left p-4 bg-chalk-board-dark/50 border border-chalk-white/5 hover:border-chalk-blue/20 rounded-lg transition-all group hover:bg-chalk-board-dark/80 cursor-pointer"
                >
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      setDeletingPlan(plan);
                    }}
                    className="absolute top-2 right-2 p-1.5 rounded-md opacity-0 group-hover:opacity-100 hover:bg-chalk-red/15 text-chalk-muted hover:text-chalk-red transition-all"
                    title="Delete plan"
                  >
                    <svg className="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                      <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16" />
                    </svg>
                  </button>
                  <div className="flex items-start gap-3">
                    <div className="w-9 h-9 rounded-lg bg-chalk-blue/8 flex items-center justify-center flex-shrink-0 group-hover:bg-chalk-blue/15 transition-colors">
                      <svg
                        className="w-4 h-4 text-chalk-blue/60 group-hover:text-chalk-blue transition-colors"
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
                        {plan.title}
                      </span>
                      <div className="flex items-center gap-2 mt-1">
                        <span className="text-xs text-chalk-muted">
                          {formatDate(plan.updated_at)}
                        </span>
                        <span className="text-xs px-1.5 py-0.5 rounded bg-chalk-ghost text-chalk-muted">
                          v{plan.version}
                        </span>
                        <span
                          className={`text-xs px-1.5 py-0.5 rounded capitalize ${
                            plan.status === "published"
                              ? "bg-chalk-green/10 text-chalk-green"
                              : "bg-chalk-ghost text-chalk-muted"
                          }`}
                        >
                          {plan.status}
                        </span>
                      </div>
                      {/* Tags */}
                      {plan.tags.length > 0 && (
                        <div className="flex flex-wrap gap-1 mt-2">
                          {plan.tags.map((tag, ti) => {
                            const tagColor = getTagColor(tag, ti);
                            return (
                              <span
                                key={tag.id}
                                className="px-1.5 py-0.5 rounded-full text-[10px] font-medium"
                                style={{
                                  backgroundColor: `${tagColor}15`,
                                  border: `1px solid ${tagColor}30`,
                                  color: `${tagColor}cc`,
                                }}
                              >
                                {tag.name}
                              </span>
                            );
                          })}
                        </div>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Indexing status footer */}
        <AnimatePresence>
          {!loading && indexingDone === false && (
            <motion.div
              key="indexing"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 8 }}
              transition={{ duration: 0.3 }}
              className="mt-8 pt-5"
            >
              <hr className="chalk-line mb-3" />
              <p className="text-xs text-chalk-muted/60 text-center flex items-center justify-center gap-2">
                <span
                  className="spinner spinner-sm"
                  style={{ width: 10, height: 10, borderWidth: 1.5 }}
                />
                Chalk is indexing your documents in the background.
              </p>
            </motion.div>
          )}
          {!loading && showComplete && (
            <motion.div
              key="complete"
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 8 }}
              transition={{ duration: 0.3 }}
              className="mt-8 pt-5"
            >
              <hr className="chalk-line mb-3" />
              <p className="text-xs text-chalk-green/70 text-center">
                Indexing complete.
              </p>
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {deletingPlan && (
        <DeleteConfirmDialog
          planTitle={deletingPlan.title}
          onConfirm={handleDeletePlan}
          onCancel={() => setDeletingPlan(null)}
        />
      )}
    </div>
  );
}
