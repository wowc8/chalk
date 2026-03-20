import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { useSchoolCalendar } from "../../hooks/useScheduleData";
import type {
  NewSchoolCalendar,
  NewCalendarException,
} from "../../types/schedule";

interface Props {
  addToast: (msg: string, type: "success" | "error") => void;
}

export function SettingsSchoolCalendar({ addToast }: Props) {
  const { calendar, exceptions, loading, reload } = useSchoolCalendar();

  const [yearStart, setYearStart] = useState("");
  const [yearEnd, setYearEnd] = useState("");
  const [endUnknown, setEndUnknown] = useState(false);
  const [saving, setSaving] = useState(false);
  const [initialized, setInitialized] = useState(false);

  // Sync loaded data into form state once
  if (!initialized && !loading) {
    setYearStart(calendar?.year_start ?? "");
    setYearEnd(calendar?.year_end ?? "");
    setEndUnknown(calendar ? !calendar.year_end : false);
    setInitialized(true);
  }

  // Exception form
  const [newDate, setNewDate] = useState("");
  const [newType, setNewType] = useState<"no_school" | "half_day" | "early_release">("no_school");
  const [newLabel, setNewLabel] = useState("");

  const handleSaveDates = async () => {
    setSaving(true);
    try {
      await invoke("update_school_calendar", {
        input: {
          year_start: yearStart,
          year_end: endUnknown ? null : yearEnd || null,
        } as NewSchoolCalendar,
      });
      addToast("School calendar dates saved", "success");
      reload();
    } catch {
      addToast("Failed to save calendar dates", "error");
    } finally {
      setSaving(false);
    }
  };

  const handleAddException = async () => {
    if (!newDate || !calendar) return;
    try {
      await invoke("add_calendar_exception", {
        input: {
          calendar_id: calendar.id,
          date: newDate,
          exception_type: newType,
          label: newLabel.trim() || typeLabel(newType),
        } as NewCalendarException,
      });
      setNewDate("");
      setNewLabel("");
      addToast("Exception added", "success");
      reload();
    } catch {
      addToast("Failed to add exception", "error");
    }
  };

  const handleDeleteException = async (id: string) => {
    try {
      await invoke("delete_calendar_exception", { id });
      reload();
    } catch {
      addToast("Failed to remove exception", "error");
    }
  };

  if (loading) {
    return (
      <section className="mb-8">
        <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
          School Calendar
        </h3>
        <div className="flex items-center gap-3 py-4 justify-center">
          <div className="w-4 h-4 border-2 border-chalk-blue border-t-transparent rounded-full animate-spin" />
        </div>
      </section>
    );
  }

  const inputCls =
    "w-full px-3 py-2 bg-chalk-board/50 border border-chalk-white/8 rounded-lg text-sm text-chalk-white focus:outline-none focus:border-chalk-blue/40 transition-colors";

  return (
    <section className="mb-8">
      <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
        School Calendar
      </h3>

      <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4 space-y-4">
        {/* Year start/end */}
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="block text-sm text-chalk-dust mb-1">
              Year Start
            </label>
            <input
              type="date"
              value={yearStart}
              onChange={(e) => setYearStart(e.target.value)}
              className={inputCls}
            />
          </div>
          <div>
            <label className="block text-sm text-chalk-dust mb-1">
              Year End
            </label>
            {!endUnknown && (
              <input
                type="date"
                value={yearEnd}
                onChange={(e) => setYearEnd(e.target.value)}
                className={inputCls}
              />
            )}
            <label className="flex items-center gap-2 mt-1.5 text-xs text-chalk-muted cursor-pointer">
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
        </div>

        <div className="flex justify-end">
          <motion.button
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            disabled={saving || !yearStart}
            onClick={handleSaveDates}
            className="px-4 py-1.5 bg-chalk-blue/10 border border-chalk-blue/30 rounded-lg text-chalk-blue text-xs hover:bg-chalk-blue/20 transition-colors disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save Dates"}
          </motion.button>
        </div>

        {/* Exceptions */}
        <div className="pt-3 border-t border-chalk-white/5">
          <h4 className="text-sm font-medium text-chalk-dust mb-3">
            Holidays & Half Days
          </h4>

          {exceptions.length > 0 && (
            <div className="space-y-1.5 mb-3 max-h-40 overflow-y-auto scrollbar-thin scrollbar-thumb-chalk-board-light">
              {exceptions.map((ex) => (
                <div
                  key={ex.id}
                  className="flex items-center justify-between px-3 py-1.5 bg-chalk-board-dark/40 rounded-lg text-sm"
                >
                  <div className="flex items-center gap-2">
                    <span className={`w-2 h-2 rounded-full ${typeDotColor(ex.exception_type)}`} />
                    <span className="text-chalk-dust">{formatDate(ex.date)}</span>
                    <span className="text-chalk-white">{ex.label}</span>
                  </div>
                  <button
                    onClick={() => handleDeleteException(ex.id)}
                    className="text-chalk-muted hover:text-chalk-red transition-colors text-xs"
                  >
                    Remove
                  </button>
                </div>
              ))}
            </div>
          )}

          {calendar && (
            <div className="flex gap-2 flex-wrap">
              <input
                type="date"
                value={newDate}
                onChange={(e) => setNewDate(e.target.value)}
                className={`${inputCls} flex-1 min-w-[140px]`}
              />
              <select
                value={newType}
                onChange={(e) => setNewType(e.target.value as typeof newType)}
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
                className={`${inputCls} flex-1 min-w-[140px]`}
              />
              <motion.button
                whileHover={{ scale: 1.02 }}
                whileTap={{ scale: 0.98 }}
                onClick={handleAddException}
                disabled={!newDate}
                className="px-3 py-2 bg-chalk-blue/10 border border-chalk-blue/30 rounded-lg text-chalk-blue text-xs hover:bg-chalk-blue/20 transition-colors disabled:opacity-40"
              >
                + Add
              </motion.button>
            </div>
          )}

          {!calendar && (
            <p className="text-xs text-chalk-muted">
              Save the calendar dates above first to add exceptions.
            </p>
          )}
        </div>
      </div>
    </section>
  );
}

function typeLabel(t: string): string {
  switch (t) {
    case "no_school": return "No School";
    case "half_day": return "Half Day";
    case "early_release": return "Early Release";
    default: return t;
  }
}

function typeDotColor(t: string): string {
  switch (t) {
    case "no_school": return "bg-chalk-red";
    case "half_day": return "bg-chalk-yellow";
    case "early_release": return "bg-chalk-orange";
    default: return "bg-chalk-muted";
  }
}

function formatDate(iso: string): string {
  if (!iso) return "";
  const d = new Date(iso + "T00:00:00");
  return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
}
