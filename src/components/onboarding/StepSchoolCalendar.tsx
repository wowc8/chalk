import { useState } from "react";
import { motion } from "framer-motion";

export interface CalendarExceptionDraft {
  date: string;
  exception_type: "no_school" | "half_day" | "early_release";
  label: string;
}

interface Props {
  onNext: (data: {
    yearStart: string;
    yearEnd: string | null;
    exceptions: CalendarExceptionDraft[];
  }) => void;
  onBack: () => void;
  initialYearStart?: string;
  initialYearEnd?: string | null;
  initialExceptions?: CalendarExceptionDraft[];
}

/** Group consecutive exceptions with the same label + type into ranges. */
interface ExceptionGroup {
  label: string;
  exception_type: CalendarExceptionDraft["exception_type"];
  startDate: string;
  endDate: string;
  indices: number[];
}

function groupExceptions(exceptions: CalendarExceptionDraft[]): ExceptionGroup[] {
  if (exceptions.length === 0) return [];

  // Sort a copy by date
  const indexed = exceptions.map((ex, i) => ({ ...ex, idx: i }));
  indexed.sort((a, b) => a.date.localeCompare(b.date));

  const groups: ExceptionGroup[] = [];
  let current: ExceptionGroup | null = null;

  for (const entry of indexed) {
    if (
      current &&
      current.label === entry.label &&
      current.exception_type === entry.exception_type &&
      isNextDay(current.endDate, entry.date)
    ) {
      current.endDate = entry.date;
      current.indices.push(entry.idx);
    } else {
      if (current) groups.push(current);
      current = {
        label: entry.label,
        exception_type: entry.exception_type,
        startDate: entry.date,
        endDate: entry.date,
        indices: [entry.idx],
      };
    }
  }
  if (current) groups.push(current);
  return groups;
}

function isNextDay(dateA: string, dateB: string): boolean {
  const a = new Date(dateA + "T00:00:00");
  const b = new Date(dateB + "T00:00:00");
  const diff = b.getTime() - a.getTime();
  return diff === 86400000;
}

/** Expand a start–end range into individual dates (YYYY-MM-DD). */
function expandDateRange(start: string, end: string): string[] {
  const dates: string[] = [];
  const d = new Date(start + "T00:00:00");
  const last = new Date(end + "T00:00:00");
  while (d <= last) {
    dates.push(d.toISOString().slice(0, 10));
    d.setDate(d.getDate() + 1);
  }
  return dates;
}

export function StepSchoolCalendar({
  onNext,
  onBack,
  initialYearStart = "",
  initialYearEnd = null,
  initialExceptions = [],
}: Props) {
  const [yearStart, setYearStart] = useState(initialYearStart);
  const [yearEnd, setYearEnd] = useState(initialYearEnd ?? "");
  const [endUnknown, setEndUnknown] = useState(initialYearEnd === null && !initialYearStart);
  const [exceptions, setExceptions] = useState<CalendarExceptionDraft[]>(initialExceptions);

  // Exception form state
  const [newStartDate, setNewStartDate] = useState("");
  const [newEndDate, setNewEndDate] = useState("");
  const [newType, setNewType] = useState<CalendarExceptionDraft["exception_type"]>("no_school");
  const [newLabel, setNewLabel] = useState("");

  const handleAddException = () => {
    if (!newStartDate) return;
    const endDate = newEndDate || newStartDate;
    if (endDate < newStartDate) return;
    const label = newLabel.trim() || typeLabel(newType);
    const dates = expandDateRange(newStartDate, endDate);
    const newEntries = dates.map((date) => ({
      date,
      exception_type: newType,
      label,
    }));
    setExceptions((prev) => [...prev, ...newEntries]);
    setNewStartDate("");
    setNewEndDate("");
    setNewLabel("");
  };

  const removeGroup = (indices: number[]) => {
    const toRemove = new Set(indices);
    setExceptions((prev) => prev.filter((_, i) => !toRemove.has(i)));
  };

  const handleNext = () => {
    onNext({
      yearStart,
      yearEnd: endUnknown ? null : yearEnd || null,
      exceptions,
    });
  };

  const groups = groupExceptions(exceptions);

  const inputCls =
    "w-full px-3 py-2 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-lg text-sm text-chalk-white focus:outline-none focus:border-chalk-blue/40 transition-colors";

  return (
    <div>
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        className="text-center mb-6"
      >
        <div className="text-4xl mb-3">&#x1F4C5;</div>
        <h2 className="text-2xl font-bold text-chalk-blue">School Calendar</h2>
        <p className="text-chalk-dust text-sm mt-2">
          Help Chalk understand your school year so it can plan around breaks
          and holidays.
        </p>
      </motion.div>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.15 }}
        className="space-y-4 mb-6"
      >
        {/* Year start */}
        <div>
          <label className="block text-sm text-chalk-muted mb-1">
            When does your school year start?
          </label>
          <input
            type="date"
            value={yearStart}
            onChange={(e) => setYearStart(e.target.value)}
            className={inputCls}
          />
        </div>

        {/* Year end */}
        <div>
          <label className="block text-sm text-chalk-muted mb-1">
            When does it end?
          </label>
          {!endUnknown && (
            <input
              type="date"
              value={yearEnd}
              onChange={(e) => setYearEnd(e.target.value)}
              className={inputCls}
            />
          )}
          <label className="flex items-center gap-2 mt-2 text-sm text-chalk-muted cursor-pointer">
            <input
              type="checkbox"
              checked={endUnknown}
              onChange={(e) => {
                setEndUnknown(e.target.checked);
                if (e.target.checked) setYearEnd("");
              }}
              className="rounded border-chalk-white/20 bg-chalk-board-dark/60 text-chalk-blue focus:ring-chalk-blue/40"
            />
            I don't know yet
          </label>
        </div>
      </motion.div>

      {/* Exceptions */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.25 }}
        className="mb-6"
      >
        <h3 className="text-sm font-medium text-chalk-dust mb-3">
          Holidays & Half Days
        </h3>

        {groups.length > 0 && (
          <div className="space-y-1.5 mb-3 max-h-40 overflow-y-auto scrollbar-thin scrollbar-thumb-chalk-board-light">
            {groups.map((g, i) => (
              <div
                key={i}
                className="flex items-center justify-between px-3 py-1.5 bg-chalk-board-dark/40 rounded-lg text-sm"
              >
                <div className="flex items-center gap-2">
                  <span className={`w-2 h-2 rounded-full ${typeDotColor(g.exception_type)}`} />
                  <span className="text-chalk-dust">
                    {g.startDate === g.endDate
                      ? formatDate(g.startDate)
                      : `${formatDate(g.startDate)} – ${formatDate(g.endDate)}`}
                  </span>
                  <span className="text-chalk-white">{g.label}</span>
                </div>
                <button
                  onClick={() => removeGroup(g.indices)}
                  className="text-chalk-muted hover:text-chalk-red transition-colors text-xs"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="flex gap-2 flex-wrap">
          <div className="flex gap-1.5 flex-1 min-w-[280px]">
            <input
              type="date"
              value={newStartDate}
              onChange={(e) => {
                setNewStartDate(e.target.value);
                if (!newEndDate || newEndDate < e.target.value) {
                  setNewEndDate(e.target.value);
                }
              }}
              className={`${inputCls} flex-1`}
              title="Start date"
            />
            <span className="self-center text-chalk-muted text-xs">to</span>
            <input
              type="date"
              value={newEndDate}
              min={newStartDate}
              onChange={(e) => setNewEndDate(e.target.value)}
              className={`${inputCls} flex-1`}
              title="End date"
            />
          </div>
          <select
            value={newType}
            onChange={(e) =>
              setNewType(e.target.value as CalendarExceptionDraft["exception_type"])
            }
            className={`${inputCls} w-auto`}
          >
            <option value="no_school">No School</option>
            <option value="half_day">Half Day</option>
            <option value="early_release">Early Release</option>
          </select>
          <input
            type="text"
            value={newLabel}
            onChange={(e) => setNewLabel(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleAddException();
            }}
            placeholder="Label (e.g. Spring Break)"
            className={`${inputCls} flex-1 min-w-[160px]`}
          />
          <button
            onClick={handleAddException}
            disabled={!newStartDate}
            className="btn btn-secondary px-3 py-2 text-sm disabled:opacity-40"
          >
            + Add
          </button>
        </div>
      </motion.div>

      {/* Navigation */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.35 }}
        className="flex justify-between"
      >
        <button
          onClick={onBack}
          className="text-sm text-chalk-muted hover:text-chalk-dust transition-colors"
        >
          Back
        </button>
        <button onClick={handleNext} className="btn btn-primary px-6 py-2">
          Next
        </button>
      </motion.div>
    </div>
  );
}

function typeLabel(t: string): string {
  switch (t) {
    case "no_school":
      return "No School";
    case "half_day":
      return "Half Day";
    case "early_release":
      return "Early Release";
    default:
      return t;
  }
}

function typeDotColor(t: string): string {
  switch (t) {
    case "no_school":
      return "bg-chalk-red";
    case "half_day":
      return "bg-chalk-yellow";
    case "early_release":
      return "bg-chalk-orange";
    default:
      return "bg-chalk-muted";
  }
}

function formatDate(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso + "T00:00:00");
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}
