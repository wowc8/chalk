import { useState, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import {
  DAYS_OF_WEEK,
  type DraftEvent,
  type DraftOccurrence,
} from "../../types/schedule";
import type {
  RecurringEvent,
  NewRecurringEvent,
  NewEventOccurrence,
  NewSchoolCalendar,
  NewCalendarException,
} from "../../types/schedule";
import type { CalendarExceptionDraft } from "./StepSchoolCalendar";

interface Props {
  onNext: () => void;
  onBack: () => void;
  dailyEvents: DraftEvent[];
  specials: DraftEvent[];
  calendarData: {
    yearStart: string;
    yearEnd: string | null;
    exceptions: CalendarExceptionDraft[];
  } | null;
}

interface GridSlot {
  event: DraftEvent;
  occurrence: DraftOccurrence;
}

function timeToMinutes(t: string): number {
  const [h, m] = t.split(":").map(Number);
  return h * 60 + m;
}

function eventColor(type: string): string {
  switch (type) {
    case "fixed":
      return "bg-chalk-board-light/60 border-chalk-white/10 text-chalk-dust";
    case "special":
      return "bg-chalk-blue/15 border-chalk-blue/30 text-chalk-blue";
    case "teaching_slot":
      return "bg-chalk-green/15 border-chalk-green/30 text-chalk-green";
    default:
      return "bg-chalk-board-light/40 border-chalk-white/8 text-chalk-muted";
  }
}

export function StepScheduleReview({
  onNext,
  onBack,
  dailyEvents,
  specials,
  calendarData,
}: Props) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Combine daily events + specials
  const allEvents = useMemo(
    () => [...dailyEvents, ...specials],
    [dailyEvents, specials],
  );

  // Build a grid lookup: day -> time -> slot[]
  const grid = useMemo(() => {
    const g: Map<number, GridSlot[]> = new Map();
    for (let d = 0; d < 5; d++) g.set(d, []);

    for (const event of allEvents) {
      for (const occ of event.occurrences) {
        g.get(occ.day_of_week)?.push({ event, occurrence: occ });
      }
    }
    // Sort each day by start time
    for (const slots of g.values()) {
      slots.sort(
        (a, b) =>
          timeToMinutes(a.occurrence.start_time) -
          timeToMinutes(b.occurrence.start_time),
      );
    }
    return g;
  }, [allEvents]);

  const handleConfirm = useCallback(async () => {
    setSaving(true);
    setError(null);
    try {
      // 1. Save school calendar if provided
      if (calendarData?.yearStart) {
        const cal = await invoke<{ id: string }>("update_school_calendar", {
          input: {
            year_start: calendarData.yearStart,
            year_end: calendarData.yearEnd,
          } as NewSchoolCalendar,
        });

        // Save exceptions
        for (const ex of calendarData.exceptions) {
          await invoke("add_calendar_exception", {
            input: {
              calendar_id: cal.id,
              date: ex.date,
              exception_type: ex.exception_type,
              label: ex.label,
            } as NewCalendarException,
          });
        }
      }

      // 2. Save all recurring events + occurrences
      for (const event of allEvents) {
        const created = await invoke<RecurringEvent>(
          "create_recurring_event",
          {
            input: {
              name: event.name,
              event_type: event.event_type,
              details_vary_daily:
                event.event_type === "special" ? true : false,
            } as NewRecurringEvent,
          },
        );

        for (const occ of event.occurrences) {
          await invoke("create_event_occurrence", {
            input: {
              event_id: created.id,
              day_of_week: occ.day_of_week,
              start_time: occ.start_time,
              end_time: occ.end_time,
            } as NewEventOccurrence,
          });
        }
      }

      onNext();
    } catch (e) {
      console.error("Failed to save schedule:", e);
      setError(String(e));
      setSaving(false);
    }
  }, [allEvents, calendarData, onNext]);

  return (
    <div>
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        className="text-center mb-4"
      >
        <div className="text-4xl mb-3">&#x1F4CB;</div>
        <h2 className="text-2xl font-bold text-chalk-blue">
          Schedule Review
        </h2>
        <p className="text-chalk-dust text-sm mt-2">
          Does this look right? This becomes your teaching template.
        </p>
      </motion.div>

      {error && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          className="mb-3 p-2 bg-chalk-red/20 border border-chalk-red/40 rounded-lg text-xs text-chalk-red"
        >
          {error}
        </motion.div>
      )}

      {/* Weekly grid */}
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.15 }}
        className="mb-4 overflow-x-auto"
      >
        <div className="min-w-[480px]">
          {/* Day headers */}
          <div className="grid grid-cols-5 gap-1 mb-1">
            {DAYS_OF_WEEK.map((day) => (
              <div
                key={day}
                className="text-center text-xs font-medium text-chalk-muted py-1"
              >
                {day}
              </div>
            ))}
          </div>

          {/* Event columns */}
          <div className="grid grid-cols-5 gap-1">
            {[0, 1, 2, 3, 4].map((dayIdx) => {
              const daySlots = grid.get(dayIdx) ?? [];
              return (
                <div key={dayIdx} className="space-y-1">
                  {daySlots.length === 0 ? (
                    <div className="h-8 rounded bg-chalk-board-dark/20 border border-dashed border-chalk-white/5" />
                  ) : (
                    daySlots.map((slot, i) => (
                      <div
                        key={`${slot.event.id}-${i}`}
                        className={`rounded px-2 py-1.5 border text-xs leading-tight ${eventColor(
                          slot.event.event_type,
                        )}`}
                      >
                        <div className="font-medium truncate">
                          {slot.event.name}
                        </div>
                        <div className="opacity-70 text-[10px]">
                          {formatTime(slot.occurrence.start_time)} -{" "}
                          {formatTime(slot.occurrence.end_time)}
                        </div>
                      </div>
                    ))
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </motion.div>

      {/* Legend */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.25 }}
        className="flex justify-center gap-4 mb-4 text-[10px] text-chalk-muted"
      >
        <div className="flex items-center gap-1">
          <span className="w-2.5 h-2.5 rounded bg-chalk-board-light/60 border border-chalk-white/10" />
          Fixed
        </div>
        <div className="flex items-center gap-1">
          <span className="w-2.5 h-2.5 rounded bg-chalk-blue/15 border border-chalk-blue/30" />
          Specials
        </div>
        <div className="flex items-center gap-1">
          <span className="w-2.5 h-2.5 rounded bg-chalk-green/15 border border-chalk-green/30" />
          Teaching
        </div>
      </motion.div>

      {/* Stats */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.3 }}
        className="text-center text-xs text-chalk-muted mb-4"
      >
        {dailyEvents.length} daily events, {specials.length} specials
        {calendarData?.yearStart && (
          <span>
            {" "}
            &middot; School year starts{" "}
            {new Date(calendarData.yearStart + "T00:00:00").toLocaleDateString(
              "en-US",
              { month: "short", day: "numeric" },
            )}
          </span>
        )}
        {calendarData && calendarData.exceptions.length > 0 && (
          <span>
            {" "}
            &middot; {calendarData.exceptions.length} holidays/half days
          </span>
        )}
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
          disabled={saving}
          className="text-sm text-chalk-muted hover:text-chalk-dust transition-colors disabled:opacity-50"
        >
          Back
        </button>
        <button
          onClick={handleConfirm}
          disabled={saving}
          className="btn btn-primary px-6 py-2 disabled:opacity-60"
        >
          {saving ? "Saving..." : "Looks Good!"}
        </button>
      </motion.div>
    </div>
  );
}

function formatTime(t: string): string {
  const [h, m] = t.split(":");
  const hour = parseInt(h);
  const ampm = hour >= 12 ? "PM" : "AM";
  const h12 = hour === 0 ? 12 : hour > 12 ? hour - 12 : hour;
  return `${h12}:${m} ${ampm}`;
}
