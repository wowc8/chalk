import { useState, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  DAILY_SUGGESTIONS,
  type DraftEvent,
} from "../../types/schedule";

type EditMode = "confirm" | "chat" | "document" | "manual" | null;

interface ManualRow {
  name: string;
  start_time: string;
  end_time: string;
}

interface Props {
  onNext: (events: DraftEvent[]) => void;
  onBack: () => void;
  gradeLevel: string;
  initialEvents?: DraftEvent[];
  extractedEvents?: DraftEvent[];
}

export function StepDailySchedule({
  onNext,
  onBack,
  gradeLevel,
  initialEvents = [],
  extractedEvents = [],
}: Props) {
  const hasExtracted = extractedEvents.length > 0;

  const [mode, setMode] = useState<EditMode>(() => {
    if (hasExtracted) return "confirm";
    if (initialEvents.length > 0) return "manual";
    return null;
  });

  const [rows, setRows] = useState<ManualRow[]>(() => {
    const source = initialEvents.length > 0 ? initialEvents : extractedEvents;
    if (source.length > 0) {
      return source.map((e) => ({
        name: e.name,
        start_time: e.occurrences[0]?.start_time ?? "",
        end_time: e.occurrences[0]?.end_time ?? "",
      }));
    }
    return [];
  });

  const seedSuggestions = useCallback(() => {
    const suggestions =
      DAILY_SUGGESTIONS[gradeLevel] ?? DAILY_SUGGESTIONS["default"];
    setRows(
      suggestions.map((s) => ({
        name: s.name,
        start_time: s.start,
        end_time: s.end,
      })),
    );
  }, [gradeLevel]);

  const selectMethod = (m: EditMode) => {
    setMode(m);
    if (m === "manual" && rows.length === 0) {
      seedSuggestions();
    }
  };

  const startEditing = () => {
    // Switch from confirmation to manual editing mode with current data
    setMode("manual");
  };

  const addRow = () => {
    setRows((prev) => [...prev, { name: "", start_time: "", end_time: "" }]);
  };

  const updateRow = (idx: number, field: keyof ManualRow, value: string) => {
    setRows((prev) =>
      prev.map((r, i) => (i === idx ? { ...r, [field]: value } : r)),
    );
  };

  const removeRow = (idx: number) => {
    setRows((prev) => prev.filter((_, i) => i !== idx));
  };

  const buildEvents = (): DraftEvent[] => {
    return rows
      .filter((r) => r.name.trim() && r.start_time && r.end_time)
      .map((r, i) => ({
        id: `draft-daily-${i}`,
        name: r.name.trim(),
        event_type: "fixed" as const,
        occurrences: [0, 1, 2, 3, 4].map((day) => ({
          day_of_week: day,
          start_time: r.start_time,
          end_time: r.end_time,
        })),
      }));
  };

  const handleNext = () => {
    onNext(buildEvents());
  };

  const handleConfirm = () => {
    onNext(buildEvents());
  };

  const inputCls =
    "w-full px-2.5 py-1.5 bg-chalk-board-dark/60 border border-chalk-white/8 rounded text-sm text-chalk-white focus:outline-none focus:border-chalk-blue/40 transition-colors";

  return (
    <div>
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        className="text-center mb-6"
      >
        <div className="text-4xl mb-3">&#x23F0;</div>
        <h2 className="text-2xl font-bold text-chalk-blue">Daily Schedule</h2>
        <p className="text-chalk-dust text-sm mt-2">
          {hasExtracted && mode === "confirm"
            ? "Here\u2019s what we figured out from your lesson plans. Tell us if this looks right."
            : "Tell Chalk about the events that happen every day \u2014 breakfast, lunch, recess, morning meeting, dismissal, etc."}
        </p>
      </motion.div>

      <AnimatePresence mode="wait">
        {/* Confirmation mode — pre-filled from LTP extraction */}
        {mode === "confirm" && (
          <motion.div
            key="confirm"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="mb-6"
          >
            {/* Extracted events list */}
            <div className="space-y-1.5 max-h-64 overflow-y-auto scrollbar-thin scrollbar-thumb-chalk-board-light pr-1">
              {/* Header */}
              <div className="grid grid-cols-[1fr_90px_90px] gap-2 mb-2 text-xs text-chalk-muted px-1">
                <span>Event</span>
                <span>Start</span>
                <span>End</span>
              </div>
              {rows.map((row, i) => (
                <div
                  key={i}
                  className="grid grid-cols-[1fr_90px_90px] gap-2 items-center px-1 py-1.5 bg-chalk-board-dark/30 rounded"
                >
                  <span className="text-sm text-chalk-white truncate">
                    {row.name || <span className="text-chalk-muted italic">Unnamed</span>}
                  </span>
                  <span className="text-sm text-chalk-dust">
                    {row.start_time || "\u2014"}
                  </span>
                  <span className="text-sm text-chalk-dust">
                    {row.end_time || "\u2014"}
                  </span>
                </div>
              ))}
            </div>

            {/* Edit options */}
            <div className="mt-4 space-y-2">
              <p className="text-xs text-chalk-muted mb-2">Need to make changes?</p>
              <div className="flex flex-wrap gap-2">
                <button
                  onClick={startEditing}
                  className="text-xs px-3 py-1.5 rounded border border-chalk-white/10 bg-chalk-board-dark/40 text-chalk-dust hover:border-chalk-blue/30 hover:text-chalk-white transition-colors"
                >
                  Edit Schedule
                </button>
                <button
                  onClick={() => selectMethod("chat")}
                  disabled
                  className="text-xs px-3 py-1.5 rounded border border-chalk-white/5 bg-chalk-board-dark/20 text-chalk-muted opacity-50 cursor-not-allowed"
                >
                  Chat to Adjust
                  <span className="ml-1 text-[9px] px-1 py-0.5 rounded bg-chalk-muted/20">Soon</span>
                </button>
                <button
                  onClick={() => selectMethod("document")}
                  disabled
                  className="text-xs px-3 py-1.5 rounded border border-chalk-white/5 bg-chalk-board-dark/20 text-chalk-muted opacity-50 cursor-not-allowed"
                >
                  Add from Document
                  <span className="ml-1 text-[9px] px-1 py-0.5 rounded bg-chalk-muted/20">Soon</span>
                </button>
              </div>
            </div>
          </motion.div>
        )}

        {/* Method picker — shown when no extracted events */}
        {mode === null && (
          <motion.div
            key="picker"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="space-y-3 mb-6"
          >
            <MethodCard
              icon="&#x1F4AC;"
              title="Let's Chat"
              desc="Describe your schedule and Chalk will figure it out"
              onClick={() => selectMethod("chat")}
              disabled
              tag="Coming Soon"
            />
            <MethodCard
              icon="&#x1F4C4;"
              title="I Have a Document"
              desc="Upload a file or paste a URL with your schedule"
              onClick={() => selectMethod("document")}
              disabled
              tag="Coming Soon"
            />
            <MethodCard
              icon="&#x2328;&#xFE0F;"
              title="I'll Type It Out"
              desc="Enter your daily events one by one"
              onClick={() => selectMethod("manual")}
            />
          </motion.div>
        )}

        {mode === "manual" && (
          <motion.div
            key="manual"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -20 }}
            className="mb-6"
          >
            <div className="flex items-center justify-between mb-3">
              <button
                onClick={() => setMode(hasExtracted ? "confirm" : null)}
                className="text-xs text-chalk-muted hover:text-chalk-dust transition-colors"
              >
                &larr; {hasExtracted ? "Back to extracted" : "Change method"}
              </button>
              <button
                onClick={seedSuggestions}
                className="text-xs text-chalk-blue hover:text-chalk-blue/80 transition-colors"
              >
                Reset to suggestions
              </button>
            </div>

            {/* Header */}
            <div className="grid grid-cols-[1fr_90px_90px_32px] gap-2 mb-2 text-xs text-chalk-muted px-1">
              <span>Event</span>
              <span>Start</span>
              <span>End</span>
              <span />
            </div>

            {/* Rows */}
            <div className="space-y-1.5 max-h-56 overflow-y-auto scrollbar-thin scrollbar-thumb-chalk-board-light pr-1">
              {rows.map((row, i) => (
                <div
                  key={i}
                  className="grid grid-cols-[1fr_90px_90px_32px] gap-2 items-center"
                >
                  <input
                    type="text"
                    value={row.name}
                    onChange={(e) => updateRow(i, "name", e.target.value)}
                    placeholder="Event name"
                    className={inputCls}
                  />
                  <input
                    type="time"
                    value={row.start_time}
                    onChange={(e) => updateRow(i, "start_time", e.target.value)}
                    className={inputCls}
                  />
                  <input
                    type="time"
                    value={row.end_time}
                    onChange={(e) => updateRow(i, "end_time", e.target.value)}
                    className={inputCls}
                  />
                  <button
                    onClick={() => removeRow(i)}
                    className="text-chalk-muted hover:text-chalk-red transition-colors text-sm text-center"
                  >
                    &times;
                  </button>
                </div>
              ))}
            </div>

            <button
              onClick={addRow}
              className="mt-2 text-sm text-chalk-blue hover:text-chalk-blue/80 transition-colors"
            >
              + Add Row
            </button>
          </motion.div>
        )}

        {(mode === "chat" || mode === "document") && (
          <motion.div
            key="placeholder"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="text-center mb-6 py-8"
          >
            <p className="text-chalk-muted text-sm">
              This input method is coming soon. Please use &quot;I&apos;ll Type It Out&quot;
              for now.
            </p>
            <button
              onClick={() => setMode(hasExtracted ? "confirm" : null)}
              className="mt-3 text-sm text-chalk-blue hover:text-chalk-blue/80 transition-colors"
            >
              &larr; Back to options
            </button>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Navigation */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.3 }}
        className="flex justify-between"
      >
        <button
          onClick={onBack}
          className="text-sm text-chalk-muted hover:text-chalk-dust transition-colors"
        >
          Back
        </button>
        {mode === "confirm" && (
          <button
            onClick={handleConfirm}
            disabled={rows.filter((r) => r.name.trim()).length === 0}
            className="btn btn-primary px-6 py-2 disabled:opacity-40"
          >
            Looks Good!
          </button>
        )}
        {mode === "manual" && (
          <button
            onClick={handleNext}
            disabled={rows.filter((r) => r.name.trim()).length === 0}
            className="btn btn-primary px-6 py-2 disabled:opacity-40"
          >
            Next
          </button>
        )}
      </motion.div>
    </div>
  );
}

function MethodCard({
  icon,
  title,
  desc,
  onClick,
  disabled,
  tag,
}: {
  icon: string;
  title: string;
  desc: string;
  onClick: () => void;
  disabled?: boolean;
  tag?: string;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className={`w-full text-left px-4 py-3 rounded-lg border transition-all ${
        disabled
          ? "border-chalk-white/5 bg-chalk-board-dark/30 opacity-50 cursor-not-allowed"
          : "border-chalk-white/8 bg-chalk-board-dark/40 hover:border-chalk-blue/30 hover:bg-chalk-board-dark/60"
      }`}
    >
      <div className="flex items-center gap-3">
        <span className="text-2xl" dangerouslySetInnerHTML={{ __html: icon }} />
        <div className="flex-1">
          <div className="flex items-center gap-2">
            <span className="text-sm font-medium text-chalk-white">
              {title}
            </span>
            {tag && (
              <span className="text-[10px] px-1.5 py-0.5 rounded bg-chalk-muted/20 text-chalk-muted">
                {tag}
              </span>
            )}
          </div>
          <span className="text-xs text-chalk-muted">{desc}</span>
        </div>
      </div>
    </button>
  );
}
