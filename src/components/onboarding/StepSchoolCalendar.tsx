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
  const [newDate, setNewDate] = useState("");
  const [newType, setNewType] = useState<CalendarExceptionDraft["exception_type"]>("no_school");
  const [newLabel, setNewLabel] = useState("");

  const handleAddException = () => {
    if (!newDate) return;
    setExceptions((prev) => [
      ...prev,
      { date: newDate, exception_type: newType, label: newLabel.trim() || typeLabel(newType) },
    ]);
    setNewDate("");
    setNewLabel("");
  };

  const removeException = (idx: number) => {
    setExceptions((prev) => prev.filter((_, i) => i !== idx));
  };

  const handleNext = () => {
    onNext({
      yearStart,
      yearEnd: endUnknown ? null : yearEnd || null,
      exceptions,
    });
  };

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

        {exceptions.length > 0 && (
          <div className="space-y-1.5 mb-3 max-h-40 overflow-y-auto scrollbar-thin scrollbar-thumb-chalk-board-light">
            {exceptions.map((ex, i) => (
              <div
                key={i}
                className="flex items-center justify-between px-3 py-1.5 bg-chalk-board-dark/40 rounded-lg text-sm"
              >
                <div className="flex items-center gap-2">
                  <span className={`w-2 h-2 rounded-full ${typeDotColor(ex.exception_type)}`} />
                  <span className="text-chalk-dust">
                    {formatDate(ex.date)}
                  </span>
                  <span className="text-chalk-white">{ex.label}</span>
                </div>
                <button
                  onClick={() => removeException(i)}
                  className="text-chalk-muted hover:text-chalk-red transition-colors text-xs"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}

        <div className="flex gap-2 flex-wrap">
          <input
            type="date"
            value={newDate}
            onChange={(e) => setNewDate(e.target.value)}
            className={`${inputCls} flex-1 min-w-[140px]`}
          />
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
            disabled={!newDate}
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
