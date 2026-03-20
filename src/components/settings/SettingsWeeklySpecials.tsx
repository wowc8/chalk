import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { useRecurringEvents } from "../../hooks/useScheduleData";
import {
  DAYS_OF_WEEK,
  SPECIAL_SUGGESTIONS,
  type NewRecurringEvent,
  type NewEventOccurrence,
} from "../../types/schedule";

interface Props {
  addToast: (msg: string, type: "success" | "error") => void;
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

export function SettingsWeeklySpecials({ addToast }: Props) {
  const { events, loading, reload } = useRecurringEvents();
  const [form, setForm] = useState<SpecialForm>(emptyForm());
  const [showForm, setShowForm] = useState(false);

  // Filter to specials only
  const specials = events.filter((e) => e.event_type === "special");
  const addedNames = new Set(specials.map((s) => s.name));
  const availableSuggestions = SPECIAL_SUGGESTIONS.filter((s) => !addedNames.has(s));

  const handleAddSpecial = async () => {
    if (!form.name.trim()) return;
    const occurrences: { day_of_week: number; start_time: string; end_time: string }[] = [];
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

    try {
      const created = await invoke<{ id: string }>("create_recurring_event", {
        input: {
          name: form.name.trim(),
          event_type: "special",
          details_vary_daily: true,
        } as NewRecurringEvent,
      });
      for (const occ of occurrences) {
        await invoke("create_event_occurrence", {
          input: {
            event_id: created.id,
            day_of_week: occ.day_of_week,
            start_time: occ.start_time,
            end_time: occ.end_time,
          } as NewEventOccurrence,
        });
      }
      setForm(emptyForm());
      setShowForm(false);
      addToast(`Added "${form.name.trim()}"`, "success");
      reload();
    } catch {
      addToast("Failed to add special", "error");
    }
  };

  const handleDeleteSpecial = async (id: string, name: string) => {
    try {
      await invoke("delete_recurring_event", { id });
      addToast(`Removed "${name}"`, "success");
      reload();
    } catch {
      addToast("Failed to remove special", "error");
    }
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

  const updateTime = (field: "startTimes" | "endTimes", dayIdx: number, value: string) => {
    setForm((prev) => {
      const times = [...prev[field]];
      times[dayIdx] = value;
      return { ...prev, [field]: times };
    });
  };

  if (loading) {
    return (
      <section className="mb-8">
        <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
          Weekly Specials
        </h3>
        <div className="flex items-center gap-3 py-4 justify-center">
          <div className="w-4 h-4 border-2 border-chalk-blue border-t-transparent rounded-full animate-spin" />
        </div>
      </section>
    );
  }

  const inputCls =
    "px-2 py-1 bg-chalk-board/50 border border-chalk-white/8 rounded text-sm text-chalk-white focus:outline-none focus:border-chalk-blue/40 transition-colors";

  return (
    <section className="mb-8">
      <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
        Weekly Specials
      </h3>

      <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4">
        <p className="text-xs text-chalk-muted mb-3">
          Events that happen on specific days — PE, Art, Music, Library, etc.
        </p>

        {/* Existing specials */}
        {specials.length > 0 && (
          <div className="space-y-2 mb-4">
            {specials.map((sp) => (
              <div
                key={sp.id}
                className="flex items-center justify-between px-3 py-2 bg-chalk-board-dark/40 rounded-lg"
              >
                <div>
                  <span className="text-sm font-medium text-chalk-white">
                    {sp.name}
                  </span>
                  <span className="text-xs text-chalk-muted ml-2">
                    {sp.occurrences.map((o) => DAYS_OF_WEEK[o.day_of_week]).join(", ")}
                  </span>
                </div>
                <button
                  onClick={() => handleDeleteSpecial(sp.id, sp.name)}
                  className="text-xs text-chalk-muted hover:text-chalk-red transition-colors"
                >
                  Remove
                </button>
              </div>
            ))}
          </div>
        )}

        {specials.length === 0 && !showForm && (
          <p className="text-xs text-chalk-muted text-center py-3 mb-3">
            No specials yet. Add one below.
          </p>
        )}

        {/* Quick suggestions */}
        {!showForm && availableSuggestions.length > 0 && (
          <div className="mb-3">
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
          </div>
        )}

        {/* Add special form */}
        <AnimatePresence>
          {showForm && (
            <motion.div
              initial={{ opacity: 0, height: 0 }}
              animate={{ opacity: 1, height: "auto" }}
              exit={{ opacity: 0, height: 0 }}
              className="mb-3 border border-chalk-white/8 rounded-lg p-3 bg-chalk-board-dark/30"
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
                        <div key={dayIdx} className="flex items-center gap-2 text-sm">
                          <span className="w-10 text-chalk-muted text-xs">
                            {DAYS_OF_WEEK[dayIdx]}
                          </span>
                          <input
                            type="time"
                            value={form.startTimes[dayIdx]}
                            onChange={(e) => updateTime("startTimes", dayIdx, e.target.value)}
                            className={inputCls}
                          />
                          <span className="text-chalk-muted text-xs">to</span>
                          <input
                            type="time"
                            value={form.endTimes[dayIdx]}
                            onChange={(e) => updateTime("endTimes", dayIdx, e.target.value)}
                            className={inputCls}
                          />
                        </div>
                      ),
                  )}
                </div>
              )}

              <div className="flex gap-2">
                <motion.button
                  whileHover={{ scale: 1.02 }}
                  whileTap={{ scale: 0.98 }}
                  onClick={handleAddSpecial}
                  disabled={
                    !form.name.trim() ||
                    !form.days.some((d, i) => d && form.startTimes[i] && form.endTimes[i])
                  }
                  className="px-4 py-1.5 bg-chalk-blue/10 border border-chalk-blue/30 rounded-lg text-chalk-blue text-xs hover:bg-chalk-blue/20 transition-colors disabled:opacity-40"
                >
                  Add Special
                </motion.button>
                <button
                  onClick={() => {
                    setShowForm(false);
                    setForm(emptyForm());
                  }}
                  className="text-xs text-chalk-muted hover:text-chalk-dust transition-colors"
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
            className="text-sm text-chalk-blue hover:text-chalk-blue/80 transition-colors"
          >
            + Add Special
          </button>
        )}
      </div>
    </section>
  );
}
