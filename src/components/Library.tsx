import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";

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

type TabKey = "my_plans" | "imported";

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

export function Library() {
  const navigate = useNavigate();
  const [plans, setPlans] = useState<LibraryPlanCard[]>([]);
  const [allTags, setAllTags] = useState<Tag[]>([]);
  const [selectedTagIds, setSelectedTagIds] = useState<string[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [activeTab, setActiveTab] = useState<TabKey>("my_plans");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const sourceType = activeTab === "my_plans" ? "created" : "imported";

  const loadPlans = async () => {
    setLoading(true);
    setError(null);
    try {
      const [fetchedPlans, fetchedTags] = await Promise.all([
        invoke<LibraryPlanCard[]>("list_library_plans", {
          sourceType,
          search: searchQuery || null,
          tagIds: selectedTagIds.length > 0 ? selectedTagIds : null,
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
  }, [activeTab, selectedTagIds]);

  // Debounced search
  useEffect(() => {
    const timer = setTimeout(() => {
      loadPlans();
    }, 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  function toggleTag(tagId: string) {
    setSelectedTagIds((prev) =>
      prev.includes(tagId)
        ? prev.filter((id) => id !== tagId)
        : [...prev, tagId]
    );
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
              Your lesson plans and imported documents
            </p>
          </div>
          <button className="btn btn-primary">
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

        {/* Tabs */}
        <div className="flex gap-1 mb-4 p-1 rounded-lg bg-chalk-board-dark/50">
          {(
            [
              { key: "my_plans" as TabKey, label: "My Plans" },
              { key: "imported" as TabKey, label: "Imported" },
            ] as const
          ).map((tab) => (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              className={`flex-1 py-1.5 px-4 rounded-md text-sm font-medium transition-all ${
                activeTab === tab.key
                  ? "bg-chalk-white/10 text-chalk-white"
                  : "text-chalk-muted hover:text-chalk-dust"
              }`}
            >
              {tab.label}
            </button>
          ))}
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
              {activeTab === "my_plans"
                ? searchQuery
                  ? "No matching plans"
                  : "No plans yet"
                : "No imported plans"}
            </h3>
            <p className="text-chalk-muted text-sm mb-5">
              {activeTab === "my_plans"
                ? searchQuery
                  ? "Try a different search term"
                  : "Create your first lesson plan to get started."
                : "Import plans from Google Drive to see them here."}
            </p>
            {activeTab === "my_plans" && !searchQuery && (
              <button className="btn btn-secondary">
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
            <div className="flex items-center justify-between mb-3">
              <p className="text-xs text-chalk-muted">
                {plans.length} plan{plans.length !== 1 ? "s" : ""}
                {searchQuery && ` matching "${searchQuery}"`}
              </p>
              <button
                onClick={loadPlans}
                className="btn btn-ghost text-xs"
              >
                Refresh
              </button>
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
              {plans.map((plan) => (
                <button
                  key={plan.id}
                  onClick={() => navigate(`/plan/${plan.id}`)}
                  className="text-left p-4 bg-chalk-board-dark/50 border border-chalk-white/5 hover:border-chalk-blue/20 rounded-lg transition-all group hover:bg-chalk-board-dark/80"
                >
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
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Footer */}
        {!loading && (
          <div className="mt-8 pt-5">
            <hr className="chalk-line mb-3" />
            <p className="text-xs text-chalk-muted/60 text-center">
              Chalk is indexing your documents in the background.
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
