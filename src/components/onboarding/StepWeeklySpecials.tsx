import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  DAYS_OF_WEEK,
  SPECIAL_SUGGESTIONS,
  type DraftEvent,
  type DraftOccurrence,
} from "../../types/schedule";

interface Props {
  onNext: (specials: DraftEvent[]) => void;
  onBack: () => void;
  dailyEvents: DraftEvent[];
  initialSpecials?: DraftEvent[];
}

interface SpecialForm {
  name: string;
  days: boolean[];
  startTimes: string[];
  endTimes: string[];
}

const emptyForm = (): SpecialForm => ({
  name: "",
  days: [false, false, false, false, false],
  startTimes: ["", "", "", "", ""],
  endTimes: ["", "", "", "", ""],
});

export function StepWeeklySpecials({
  onNext,
  onBack,
  dailyEvents,
  initialSpecials = [],
}: Props) {
  const [specials, setSpecials] = useState<DraftEvent[]>(initialSpecials);
  const [form, setForm] = useState<SpecialForm>(emptyForm());
  const [showForm, setShowForm] = useState(false);

  const addSpecial = () => {
    if (!form.name.trim()) return;
    const occurrences: DraftOccurrence[] = [];
    form.days.forEach((checked, dayIdx) => {
      if (checked && form.startTimes[dayIdx] && form.endTimes[dayIdx]) {
        occurrences.push({
          day_of_week: dayIdx,
          start_time: form.startTimes[dayIdx],
          end_time: form.endTimes[dayIdx],
        });
      }
    });
    if (occurrences.length === 0) return;
    const newSpecial: DraftEvent = {
      id: `draft-special-${Date.now()}`,
      name: form.name.trim(),
      event_type: "special",
      occurrences,
    };
    setSpecials((prev) => [...prev, newSpecial]);
    setForm(emptyForm());
    setShowForm(false);
  };

  const removeSpecial = (idx: number) => {
    setSpecials((prev) => prev.filter((_, i) => i !== idx));
  };

  const useSuggestion = (name: string) => {
    setForm({ ...emptyForm(), name });
    setShowForm(true);
  };

  const toggleDay = (dayIdx: number) => {
    setForm((prev) => {
      const days = [...prev.days];
      days[dayIdx] = !days[dayIdx];
      return { ...prev, days };
    });
  };

  const updateTime = (
    field: "startTimes" | "endTimes",
    dayIdx: number,
    value: string,
  ) => {
    setForm((prev) => {
      const times = [...prev[field]];
      times[dayIdx] = value;
      return { ...prev, [field]: times };
    });
  };

  const handleNext = () => {
    onNext(specials);
  };

  // Build a quick reference of daily events for context
  const dailySummary = dailyEvents
    .slice(0, 3)
    .map((e) => e.name)
    .join(", ");

  const inputCls =
    "px-2 py-1 bg-chalk-board-dark/60 border border-chalk-white/8 rounded text-sm text-chalk-white focus:outline-none focus:border-chalk-blue/40 transition-colors";

  // Filter out already-added suggestions
  const addedNames = new Set(specials.map((s) => s.name));
  const availableSuggestions = SPECIAL_SUGGESTIONS.filter(
    (s) => !addedNames.has(s),
  );

  return (
    <div>
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        className="text-center mb-5"
      >
        <div className="text-4xl mb-3">&#x1F3A8;</div>
        <h2 className="text-2xl font-bold text-chalk-blue">Weekly Specials</h2>
        <p className="text-chalk-dust text-sm mt-2">
          Add events that happen on specific days &mdash; PE, Art, Music,
          Library, etc.
        </p>
        {dailySummary && (
          <p className="text-chalk-muted text-xs mt-1">
            Your daily schedule: {dailySummary}...
          </p>
        )}
      </motion.div>

      {/* Added specials list */}
      {specials.length > 0 && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="space-y-2 mb-4"
        >
          {specials.map((sp, i) => (
            <div
              key={sp.id}
              className="flex items-center justify-between px-3 py-2 bg-chalk-board-dark/40 rounded-lg"
            >
              <div>
                <span className="text-sm font-medium text-chalk-white">
                  {sp.name}
                </span>
                <span className="text-xs text-chalk-muted ml-2">
                  {sp.occurrences
                    .map((o) => DAYS_OF_WEEK[o.day_of_week])
                    .join(", ")}
                </span>
              </div>
              <button
                onClick={() => removeSpecial(i)}
                className="text-xs text-chalk-muted hover:text-chalk-red transition-colors"
              >
                Remove
              </button>
            </div>
          ))}
        </motion.div>
      )}

      {/* Quick suggestions */}
      {!showForm && availableSuggestions.length > 0 && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="mb-4"
        >
          <p className="text-xs text-chalk-muted mb-2">Quick add:</p>
          <div className="flex flex-wrap gap-2">
            {availableSuggestions.map((s) => (
              <button
                key={s}
                onClick={() => useSuggestion(s)}
                className="px-3 py-1 text-xs rounded-full border border-chalk-white/10 bg-chalk-board-dark/30 text-chalk-dust hover:border-chalk-blue/30 hover:text-chalk-white transition-colors"
              >
                + {s}
              </button>
            ))}
          </div>
        </motion.div>
      )}

      {/* Add special form */}
      <AnimatePresence>
        {showForm && (
          <motion.div
            initial={{ opacity: 0, height: 0 }}
            animate={{ opacity: 1, height: "auto" }}
            exit={{ opacity: 0, height: 0 }}
            className="mb-4 border border-chalk-white/8 rounded-lg p-3 bg-chalk-board-dark/30"
          >
            <div className="mb-3">
              <label className="block text-xs text-chalk-muted mb-1">
                Special Name
              </label>
              <input
                type="text"
                value={form.name}
                onChange={(e) => setForm((p) => ({ ...p, name: e.target.value }))}
                placeholder="e.g. PE"
                className={`w-full ${inputCls}`}
                autoFocus
              />
            </div>

            <div className="mb-3">
              <label className="block text-xs text-chalk-muted mb-2">
                Which days?
              </label>
              <div className="flex gap-2">
                {DAYS_OF_WEEK.map((day, dayIdx) => (
                  <button
                    key={day}
                    onClick={() => toggleDay(dayIdx)}
                    className={`w-10 h-10 rounded-lg text-xs font-medium transition-all ${
                      form.days[dayIdx]
                        ? "bg-chalk-blue/20 border-chalk-blue/50 text-chalk-blue border"
                        : "bg-chalk-board-dark/40 border-chalk-white/8 text-chalk-muted border hover:border-chalk-white/20"
                    }`}
                  >
                    {day}
                  </button>
                ))}
              </div>
            </div>

            {/* Time inputs for selected days */}
            {form.days.some(Boolean) && (
              <div className="space-y-2 mb-3">
                {form.days.map(
                  (checked, dayIdx) =>
                    checked && (
                      <div
                        key={dayIdx}
                        className="flex items-center gap-2 text-sm"
                      >
                        <span className="w-10 text-chalk-muted text-xs">
                          {DAYS_OF_WEEK[dayIdx]}
                        </span>
                        <input
                          type="time"
                          value={form.startTimes[dayIdx]}
                          onChange={(e) =>
                            updateTime("startTimes", dayIdx, e.target.value)
                          }
                          className={inputCls}
                        />
                        <span className="text-chalk-muted text-xs">to</span>
                        <input
                          type="time"
                          value={form.endTimes[dayIdx]}
                          onChange={(e) =>
                            updateTime("endTimes", dayIdx, e.target.value)
                          }
                          className={inputCls}
                        />
                      </div>
                    ),
                )}
              </div>
            )}

            <div className="flex gap-2">
              <button
                onClick={addSpecial}
                disabled={
                  !form.name.trim() ||
                  !form.days.some(
                    (d, i) => d && form.startTimes[i] && form.endTimes[i],
                  )
                }
                className="btn btn-primary px-4 py-1.5 text-sm disabled:opacity-40"
              >
                Add Special
              </button>
              <button
                onClick={() => {
                  setShowForm(false);
                  setForm(emptyForm());
                }}
                className="text-sm text-chalk-muted hover:text-chalk-dust transition-colors"
              >
                Cancel
              </button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {!showForm && (
        <button
          onClick={() => setShowForm(true)}
          className="mb-6 text-sm text-chalk-blue hover:text-chalk-blue/80 transition-colors"
        >
          + Add Special
        </button>
      )}

      {/* Navigation */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.3 }}
        className="flex justify-between mt-4"
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
