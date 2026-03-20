import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useNavigate } from "react-router-dom";
import { motion, AnimatePresence } from "framer-motion";
import { useToast } from "./Toast";

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
  week_start_date: string | null;
  week_end_date: string | null;
  school_year: string | null;
  created_at: string;
  updated_at: string;
}

interface MonthGroup {
  month: number;
  month_name: string;
  plans: LibraryPlanCard[];
}

interface SchoolYearGroup {
  school_year: string;
  months: MonthGroup[];
}

function formatWeekRange(start: string | null, end: string | null): string {
  if (!start) return "";
  const s = new Date(start + "T00:00:00");
  const startStr = s.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  if (!end) return startStr;
  const e = new Date(end + "T00:00:00");
  const endStr = e.toLocaleDateString(undefined, { month: "short", day: "numeric" });
  return `${startStr} – ${endStr}`;
}

function getCurrentMonth(): number {
  return new Date().getMonth() + 1;
}

function getCurrentSchoolYear(): string {
  const now = new Date();
  const year = now.getFullYear();
  const month = now.getMonth() + 1;
  // School year starts in August
  if (month >= 8) {
    return `${year}-${(year + 1).toString().slice(2)}`;
  }
  return `${year - 1}-${year.toString().slice(2)}`;
}

export function HistoricalLibrary() {
  const navigate = useNavigate();
  const { addToast } = useToast();
  const [groups, setGroups] = useState<SchoolYearGroup[]>([]);
  const [searchQuery, setSearchQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedYears, setExpandedYears] = useState<Set<string>>(new Set());
  const [expandedMonths, setExpandedMonths] = useState<Set<string>>(new Set());
  const initialExpandDone = useRef(false);

  const searchRef = useRef(searchQuery);
  searchRef.current = searchQuery;

  const loadPlans = async () => {
    setLoading(true);
    setError(null);
    try {
      const result = await invoke<SchoolYearGroup[]>(
        "list_library_plans_chronological",
        { search: searchRef.current || null }
      );
      setGroups(result);

      // Auto-expand current school year and current month on first load
      if (!initialExpandDone.current && result.length > 0) {
        initialExpandDone.current = true;
        const currentSY = getCurrentSchoolYear();
        const currentMonth = getCurrentMonth();
        const yearsToExpand = new Set<string>();
        const monthsToExpand = new Set<string>();

        // Try to expand current school year, fallback to first
        const matchingSY = result.find(g => g.school_year === currentSY);
        const targetSY = matchingSY || result[0];
        yearsToExpand.add(targetSY.school_year);

        // Expand current month within the target school year
        const matchingMonth = targetSY.months.find(m => m.month === currentMonth);
        if (matchingMonth) {
          monthsToExpand.add(`${targetSY.school_year}-${matchingMonth.month}`);
        } else if (targetSY.months.length > 0) {
          // Fallback: expand last month in the year
          const lastMonth = targetSY.months[targetSY.months.length - 1];
          monthsToExpand.add(`${targetSY.school_year}-${lastMonth.month}`);
        }

        setExpandedYears(yearsToExpand);
        setExpandedMonths(monthsToExpand);
      }
    } catch (e) {
      setError(`Failed to load plans: ${e}`);
      setGroups([]);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    const timer = setTimeout(() => loadPlans(), 300);
    return () => clearTimeout(timer);
  }, [searchQuery]);

  useEffect(() => {
    loadPlans();
  }, []);

  // Auto-refresh on focus
  useEffect(() => {
    const handleFocus = () => loadPlans();
    window.addEventListener("focus", handleFocus);
    return () => window.removeEventListener("focus", handleFocus);
  }, []);

  function toggleYear(sy: string) {
    setExpandedYears(prev => {
      const next = new Set(prev);
      if (next.has(sy)) next.delete(sy);
      else next.add(sy);
      return next;
    });
  }

  function toggleMonth(key: string) {
    setExpandedMonths(prev => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  async function handleUseAsTemplate(plan: LibraryPlanCard) {
    try {
      const newPlan = await invoke<{ id: string }>("duplicate_plan_as_template", {
        sourcePlanId: plan.id,
        newTitle: `${plan.title} (copy)`,
      });
      addToast("Plan duplicated — editing copy", "success");
      navigate(`/plan/${newPlan.id}`);
    } catch (e) {
      addToast(`Failed to duplicate: ${e}`, "error");
    }
  }

  const totalPlans = groups.reduce(
    (acc, g) => acc + g.months.reduce((a, m) => a + m.plans.length, 0),
    0
  );

  return (
    <div className="px-6 py-6">
      <div className="max-w-4xl mx-auto">
        {/* Header */}
        <div className="flex items-center justify-between mb-5">
          <div>
            <h2 className="text-lg font-semibold text-chalk-white">
              Lesson Plan History
            </h2>
            <p className="text-xs text-chalk-muted mt-0.5">
              Browse and reuse past lesson plans by week
            </p>
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={() => navigate("/")}
              className="btn btn-secondary"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M19 11H5m7-7l-7 7 7 7" />
              </svg>
              Library
            </button>
            <button
              onClick={() => navigate("/plan/new")}
              className="btn btn-primary"
            >
              <svg className="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M12 4v16m8-8H4" />
              </svg>
              New Plan
            </button>
          </div>
        </div>

        {/* Search */}
        <div className="relative mb-5">
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
            placeholder="Search all historical plans..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="w-full pl-10 pr-4 py-2 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-lg text-sm text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors"
          />
        </div>

        {/* Error */}
        {error && (
          <div className="mb-5 p-4 bg-chalk-red/10 border border-chalk-red/30 rounded-lg text-sm text-chalk-red">
            {error}
            <button onClick={loadPlans} className="ml-3 underline hover:no-underline">
              Retry
            </button>
          </div>
        )}

        {/* Loading */}
        {loading && (
          <div className="flex items-center justify-center py-16">
            <div className="spinner" />
            <span className="ml-3 text-chalk-muted text-sm">Loading plans...</span>
          </div>
        )}

        {/* Empty state */}
        {!loading && !error && groups.length === 0 && (
          <div className="text-center py-16">
            <div className="w-16 h-16 mx-auto mb-5 rounded-2xl bg-chalk-board-dark border border-chalk-white/8 flex items-center justify-center">
              <svg className="w-8 h-8 text-chalk-muted" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z" />
              </svg>
            </div>
            <h3 className="text-base font-medium text-chalk-white mb-1">
              {searchQuery ? "No matching plans" : "No historical plans yet"}
            </h3>
            <p className="text-chalk-muted text-sm">
              {searchQuery
                ? "Try a different search term"
                : "Plans with date metadata will appear here organized by school year and month."}
            </p>
          </div>
        )}

        {/* School year groups */}
        {!loading && groups.length > 0 && (
          <div>
            <p className="text-xs text-chalk-muted mb-4">
              {totalPlans} plan{totalPlans !== 1 ? "s" : ""} across {groups.length} school year{groups.length !== 1 ? "s" : ""}
              {searchQuery && ` matching "${searchQuery}"`}
            </p>

            <div className="space-y-3">
              {groups.map((yearGroup) => {
                const yearExpanded = expandedYears.has(yearGroup.school_year);
                const yearPlanCount = yearGroup.months.reduce((a, m) => a + m.plans.length, 0);

                return (
                  <div
                    key={yearGroup.school_year}
                    className="border border-chalk-white/8 rounded-lg overflow-hidden"
                  >
                    {/* School year header */}
                    <button
                      onClick={() => toggleYear(yearGroup.school_year)}
                      className="w-full flex items-center justify-between px-4 py-3 bg-chalk-board-dark/70 hover:bg-chalk-board-dark transition-colors text-left"
                    >
                      <div className="flex items-center gap-3">
                        <svg
                          className={`w-4 h-4 text-chalk-muted transition-transform ${yearExpanded ? "rotate-90" : ""}`}
                          fill="none"
                          stroke="currentColor"
                          viewBox="0 0 24 24"
                        >
                          <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                        </svg>
                        <span className="text-sm font-semibold text-chalk-white">
                          {yearGroup.school_year}
                        </span>
                      </div>
                      <span className="text-xs text-chalk-muted">
                        {yearPlanCount} plan{yearPlanCount !== 1 ? "s" : ""}
                      </span>
                    </button>

                    {/* Month groups */}
                    <AnimatePresence initial={false}>
                      {yearExpanded && (
                        <motion.div
                          initial={{ height: 0, opacity: 0 }}
                          animate={{ height: "auto", opacity: 1 }}
                          exit={{ height: 0, opacity: 0 }}
                          transition={{ duration: 0.2 }}
                          className="overflow-hidden"
                        >
                          <div className="divide-y divide-chalk-white/5">
                            {yearGroup.months.map((monthGroup) => {
                              const monthKey = `${yearGroup.school_year}-${monthGroup.month}`;
                              const monthExpanded = expandedMonths.has(monthKey);

                              return (
                                <div key={monthKey}>
                                  {/* Month divider/header */}
                                  <button
                                    onClick={() => toggleMonth(monthKey)}
                                    className="w-full flex items-center justify-between px-6 py-2.5 hover:bg-chalk-white/[0.02] transition-colors text-left"
                                  >
                                    <div className="flex items-center gap-2">
                                      <svg
                                        className={`w-3 h-3 text-chalk-muted/60 transition-transform ${monthExpanded ? "rotate-90" : ""}`}
                                        fill="none"
                                        stroke="currentColor"
                                        viewBox="0 0 24 24"
                                      >
                                        <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
                                      </svg>
                                      <span className="text-xs font-medium text-chalk-dust">
                                        {monthGroup.month_name}
                                      </span>
                                    </div>
                                    <span className="text-[10px] text-chalk-muted">
                                      {monthGroup.plans.length} week{monthGroup.plans.length !== 1 ? "s" : ""}
                                    </span>
                                  </button>

                                  {/* Plans within month */}
                                  <AnimatePresence initial={false}>
                                    {monthExpanded && (
                                      <motion.div
                                        initial={{ height: 0, opacity: 0 }}
                                        animate={{ height: "auto", opacity: 1 }}
                                        exit={{ height: 0, opacity: 0 }}
                                        transition={{ duration: 0.15 }}
                                        className="overflow-hidden"
                                      >
                                        <div className="px-6 pb-3 space-y-1.5">
                                          {monthGroup.plans.map((plan) => (
                                            <div
                                              key={plan.id}
                                              className="group flex items-center gap-3 px-3 py-2.5 rounded-lg hover:bg-chalk-white/[0.04] transition-colors"
                                            >
                                              {/* Click to open */}
                                              <div
                                                className="flex-1 min-w-0 cursor-pointer"
                                                onClick={() => navigate(`/plan/${plan.id}`)}
                                              >
                                                <span className="block text-sm text-chalk-white truncate">
                                                  {plan.title}
                                                </span>
                                                <span className="text-xs text-chalk-muted">
                                                  {formatWeekRange(plan.week_start_date, plan.week_end_date)}
                                                </span>
                                              </div>

                                              {/* Status badge */}
                                              <span
                                                className={`text-[10px] px-1.5 py-0.5 rounded capitalize flex-shrink-0 ${
                                                  plan.status === "published" || plan.status === "finalized"
                                                    ? "bg-chalk-green/10 text-chalk-green"
                                                    : "bg-chalk-ghost text-chalk-muted"
                                                }`}
                                              >
                                                {plan.status}
                                              </span>

                                              {/* Use as Template button */}
                                              <button
                                                onClick={(e) => {
                                                  e.stopPropagation();
                                                  handleUseAsTemplate(plan);
                                                }}
                                                className="opacity-0 group-hover:opacity-100 text-[10px] px-2 py-1 rounded bg-chalk-blue/10 text-chalk-blue hover:bg-chalk-blue/20 transition-all flex-shrink-0"
                                                title="Duplicate as new plan"
                                              >
                                                Use as Template
                                              </button>
                                            </div>
                                          ))}
                                        </div>
                                      </motion.div>
                                    )}
                                  </AnimatePresence>
                                </div>
                              );
                            })}
                          </div>
                        </motion.div>
                      )}
                    </AnimatePresence>
                  </div>
                );
              })}
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
