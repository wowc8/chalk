import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { useRecurringEvents } from "../../hooks/useScheduleData";
import type {
  NewRecurringEvent,
  NewEventOccurrence,
} from "../../types/schedule";

interface Props {
  addToast: (msg: string, type: "success" | "error") => void;
}

interface ManualRow {
  name: string;
  start_time: string;
  end_time: string;
}

export function SettingsDailySchedule({ addToast }: Props) {
  const { events, loading, reload } = useRecurringEvents();
  const [newRow, setNewRow] = useState<ManualRow>({ name: "", start_time: "", end_time: "" });

  // Filter to fixed (daily) events only
  const dailyEvents = events.filter((e) => e.event_type === "fixed");

  const handleAddEvent = async () => {
    if (!newRow.name.trim() || !newRow.start_time || !newRow.end_time) return;
    try {
      const created = await invoke<{ id: string }>("create_recurring_event", {
        input: {
          name: newRow.name.trim(),
          event_type: "fixed",
          details_vary_daily: false,
        } as NewRecurringEvent,
      });
      // Create occurrences for all weekdays (Mon-Fri)
      for (let day = 0; day < 5; day++) {
        await invoke("create_event_occurrence", {
          input: {
            event_id: created.id,
            day_of_week: day,
            start_time: newRow.start_time,
            end_time: newRow.end_time,
          } as NewEventOccurrence,
        });
      }
      setNewRow({ name: "", start_time: "", end_time: "" });
      addToast(`Added "${newRow.name.trim()}"`, "success");
      reload();
    } catch {
      addToast("Failed to add event", "error");
    }
  };

  const handleDeleteEvent = async (id: string, name: string) => {
    try {
      await invoke("delete_recurring_event", { id });
      addToast(`Removed "${name}"`, "success");
      reload();
    } catch {
      addToast("Failed to remove event", "error");
    }
  };

  if (loading) {
    return (
      <section className="mb-8">
        <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
          Daily Schedule
        </h3>
        <div className="flex items-center gap-3 py-4 justify-center">
          <div className="w-4 h-4 border-2 border-chalk-blue border-t-transparent rounded-full animate-spin" />
        </div>
      </section>
    );
  }

  const inputCls =
    "w-full px-2.5 py-1.5 bg-chalk-board/50 border border-chalk-white/8 rounded text-sm text-chalk-white focus:outline-none focus:border-chalk-blue/40 transition-colors";

  return (
    <section className="mb-8">
      <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
        Daily Schedule
      </h3>

      <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4">
        <p className="text-xs text-chalk-muted mb-3">
          Events that happen every day — breakfast, lunch, recess, morning
          meeting, dismissal, etc.
        </p>

        {/* Column headers */}
        <div className="grid grid-cols-[1fr_80px_80px_32px] gap-2 mb-2 text-xs text-chalk-muted px-1">
          <span>Event</span>
          <span>Start</span>
          <span>End</span>
          <span />
        </div>

        {/* Existing events */}
        <div className="space-y-1.5 max-h-56 overflow-y-auto scrollbar-thin scrollbar-thumb-chalk-board-light pr-1 mb-3">
          {dailyEvents.map((ev) => {
            // Show the first occurrence's times (they're the same for all days)
            const occ = ev.occurrences[0];
            return (
              <div
                key={ev.id}
                className="grid grid-cols-[1fr_80px_80px_32px] gap-2 items-center"
              >
                <span className="text-sm text-chalk-white truncate px-1">
                  {ev.name}
                </span>
                <span className="text-xs text-chalk-dust px-1">
                  {occ ? formatTime(occ.start_time) : "-"}
                </span>
                <span className="text-xs text-chalk-dust px-1">
                  {occ ? formatTime(occ.end_time) : "-"}
                </span>
                <button
                  onClick={() => handleDeleteEvent(ev.id, ev.name)}
                  className="text-chalk-muted hover:text-chalk-red transition-colors text-sm text-center"
                >
                  &times;
                </button>
              </div>
            );
          })}

          {dailyEvents.length === 0 && (
            <p className="text-xs text-chalk-muted text-center py-4">
              No daily events yet. Add one below.
            </p>
          )}
        </div>

        {/* Add new row */}
        <div className="grid grid-cols-[1fr_80px_80px_32px] gap-2 items-center pt-2 border-t border-chalk-white/5">
          <input
            type="text"
            value={newRow.name}
            onChange={(e) => setNewRow((r) => ({ ...r, name: e.target.value }))}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleAddEvent();
            }}
            placeholder="Event name"
            className={inputCls}
          />
          <input
            type="time"
            value={newRow.start_time}
            onChange={(e) => setNewRow((r) => ({ ...r, start_time: e.target.value }))}
            className={inputCls}
          />
          <input
            type="time"
            value={newRow.end_time}
            onChange={(e) => setNewRow((r) => ({ ...r, end_time: e.target.value }))}
            className={inputCls}
          />
          <motion.button
            whileHover={{ scale: 1.1 }}
            whileTap={{ scale: 0.9 }}
            onClick={handleAddEvent}
            disabled={!newRow.name.trim() || !newRow.start_time || !newRow.end_time}
            className="text-chalk-blue hover:text-chalk-blue/80 transition-colors text-lg text-center disabled:opacity-40 disabled:cursor-not-allowed"
            title="Add event"
          >
            +
          </motion.button>
        </div>
      </div>
    </section>
  );
}

function formatTime(t: string): string {
  const [h, m] = t.split(":");
  const hour = parseInt(h);
  const ampm = hour >= 12 ? "PM" : "AM";
  const h12 = hour === 0 ? 12 : hour > 12 ? hour - 12 : hour;
  return `${h12}:${m} ${ampm}`;
}
