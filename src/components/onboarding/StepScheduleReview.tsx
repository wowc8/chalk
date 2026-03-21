import { useState, useMemo, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
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

function formatTime(t: string): string {
  const [h, m] = t.split(":");
  const hour = parseInt(h);
  const ampm = hour >= 12 ? "PM" : "AM";
  const h12 = hour === 0 ? 12 : hour > 12 ? hour - 12 : hour;
  return `${h12}:${m} ${ampm}`;
}

/** Generate a unique id for new draft events. */
let nextId = 1;
function newDraftId(): string {
  return `draft-${Date.now()}-${nextId++}`;
}

export function StepScheduleReview({
  onNext,
  onBack,
  dailyEvents: initialDaily,
  specials: initialSpecials,
  calendarData,
}: Props) {
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // ── Editable state ─────────────────────────────────────────
  const [events, setEvents] = useState<DraftEvent[]>(() => [
    ...initialDaily,
    ...initialSpecials,
  ]);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [addingNew, setAddingNew] = useState(false);

  // Build a grid lookup: day -> time -> slot[]
  const grid = useMemo(() => {
    const g: Map<number, GridSlot[]> = new Map();
    for (let d = 0; d < 5; d++) g.set(d, []);

    for (const event of events) {
      for (const occ of event.occurrences) {
        g.get(occ.day_of_week)?.push({ event, occurrence: occ });
      }
    }
    for (const slots of g.values()) {
      slots.sort(
        (a, b) =>
          timeToMinutes(a.occurrence.start_time) -
          timeToMinutes(b.occurrence.start_time),
      );
    }
    return g;
  }, [events]);

  // ── Event mutations ────────────────────────────────────────
  const updateEvent = useCallback(
    (id: string, updater: (ev: DraftEvent) => DraftEvent) => {
      setEvents((prev) => prev.map((e) => (e.id === id ? updater(e) : e)));
    },
    [],
  );

  const deleteEvent = useCallback((id: string) => {
    setEvents((prev) => prev.filter((e) => e.id !== id));
    setEditingId(null);
  }, []);

  const addEvent = useCallback((ev: DraftEvent) => {
    setEvents((prev) => [...prev, ev]);
    setAddingNew(false);
  }, []);

  // ── Save ───────────────────────────────────────────────────
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
      for (const event of events) {
        if (event.occurrences.length === 0) continue;

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
  }, [events, calendarData, onNext]);

  const editingEvent = editingId
    ? events.find((e) => e.id === editingId) ?? null
    : null;

  return (
    <div>
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        className="text-center mb-4"
      >
        <div className="text-4xl mb-3">&#x1F4CB;</div>
        <h2 className="text-2xl font-bold text-chalk-blue">Schedule Review</h2>
        <p className="text-chalk-dust text-sm mt-2">
          Tap any block to edit. This becomes your teaching template.
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
        className="mb-3 overflow-x-auto"
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
                      <button
                        type="button"
                        key={`${slot.event.id}-${dayIdx}-${i}`}
                        onClick={() => setEditingId(slot.event.id)}
                        className={`w-full rounded px-2 py-1.5 border text-xs leading-tight text-left cursor-pointer transition-all hover:ring-1 hover:ring-chalk-blue/50 ${eventColor(
                          slot.event.event_type,
                        )} ${editingId === slot.event.id ? "ring-2 ring-chalk-blue" : ""}`}
                      >
                        <div className="font-medium truncate">
                          {slot.event.name}
                        </div>
                        <div className="opacity-70 text-[10px]">
                          {formatTime(slot.occurrence.start_time)} -{" "}
                          {formatTime(slot.occurrence.end_time)}
                        </div>
                      </button>
                    ))
                  )}
                </div>
              );
            })}
          </div>
        </div>
      </motion.div>

      {/* Legend + Add button */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.25 }}
        className="flex items-center justify-between mb-3"
      >
        <div className="flex gap-3 text-[10px] text-chalk-muted">
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
        </div>

        <button
          type="button"
          onClick={() => {
            setEditingId(null);
            setAddingNew(true);
          }}
          className="text-xs text-chalk-blue hover:text-chalk-blue/80 transition-colors flex items-center gap-1"
        >
          <svg
            className="w-3.5 h-3.5"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M12 4v16m8-8H4"
            />
          </svg>
          Add Block
        </button>
      </motion.div>

      {/* Inline editor panel */}
      <AnimatePresence mode="wait">
        {editingEvent && (
          <EditPanel
            key={editingEvent.id}
            event={editingEvent}
            onUpdate={(updater) => updateEvent(editingEvent.id, updater)}
            onDelete={() => deleteEvent(editingEvent.id)}
            onClose={() => setEditingId(null)}
          />
        )}
        {addingNew && (
          <AddPanel
            key="add-new"
            onAdd={addEvent}
            onClose={() => setAddingNew(false)}
          />
        )}
      </AnimatePresence>

      {/* Stats */}
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.3 }}
        className="text-center text-xs text-chalk-muted mb-4"
      >
        {events.length} event{events.length !== 1 ? "s" : ""}
        {calendarData?.yearStart && (
          <span>
            {" "}
            &middot; School year starts{" "}
            {new Date(
              calendarData.yearStart + "T00:00:00",
            ).toLocaleDateString("en-US", { month: "short", day: "numeric" })}
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

// ── Edit Panel ─────────────────────────────────────────────────

interface EditPanelProps {
  event: DraftEvent;
  onUpdate: (updater: (ev: DraftEvent) => DraftEvent) => void;
  onDelete: () => void;
  onClose: () => void;
}

function EditPanel({ event, onUpdate, onDelete, onClose }: EditPanelProps) {
  const [name, setName] = useState(event.name);
  const [startTime, setStartTime] = useState(
    event.occurrences[0]?.start_time ?? "08:00",
  );
  const [endTime, setEndTime] = useState(
    event.occurrences[0]?.end_time ?? "08:30",
  );
  const [eventType, setEventType] = useState(event.event_type);

  // Which days this event currently appears on
  const currentDays = new Set(event.occurrences.map((o) => o.day_of_week));
  const [days, setDays] = useState<Set<number>>(currentDays);

  const toggleDay = (d: number) => {
    setDays((prev) => {
      const next = new Set(prev);
      if (next.has(d)) {
        next.delete(d);
      } else {
        next.add(d);
      }
      return next;
    });
  };

  const applyChanges = () => {
    onUpdate(() => ({
      id: event.id,
      name: name.trim() || event.name,
      event_type: eventType,
      occurrences: Array.from(days)
        .sort()
        .map((d) => ({
          day_of_week: d,
          start_time: startTime,
          end_time: endTime,
        })),
    }));
    onClose();
  };

  const [confirmDelete, setConfirmDelete] = useState(false);

  return (
    <motion.div
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: "auto" }}
      exit={{ opacity: 0, height: 0 }}
      className="mb-3 overflow-hidden"
    >
      <div className="p-3 bg-chalk-board-dark/60 border border-chalk-white/10 rounded-lg space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-chalk-dust">
            Edit Block
          </span>
          <button
            type="button"
            onClick={onClose}
            className="text-chalk-muted hover:text-chalk-dust text-xs"
          >
            &#x2715;
          </button>
        </div>

        {/* Name */}
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Event name"
          className="w-full bg-chalk-board-light/40 border border-chalk-white/10 rounded px-2 py-1.5 text-sm text-chalk-white placeholder-chalk-muted/50 focus:outline-none focus:border-chalk-blue/50"
        />

        {/* Time */}
        <div className="flex gap-2 items-center">
          <input
            type="time"
            value={startTime}
            onChange={(e) => setStartTime(e.target.value)}
            className="bg-chalk-board-light/40 border border-chalk-white/10 rounded px-2 py-1 text-xs text-chalk-white focus:outline-none focus:border-chalk-blue/50"
          />
          <span className="text-chalk-muted text-xs">to</span>
          <input
            type="time"
            value={endTime}
            onChange={(e) => setEndTime(e.target.value)}
            className="bg-chalk-board-light/40 border border-chalk-white/10 rounded px-2 py-1 text-xs text-chalk-white focus:outline-none focus:border-chalk-blue/50"
          />
        </div>

        {/* Type */}
        <div className="flex gap-1.5">
          {(
            [
              ["fixed", "Fixed"],
              ["special", "Special"],
              ["teaching_slot", "Teaching"],
            ] as const
          ).map(([val, label]) => (
            <button
              type="button"
              key={val}
              onClick={() => setEventType(val)}
              className={`px-2 py-0.5 rounded text-[10px] border transition-colors ${
                eventType === val
                  ? "bg-chalk-blue/20 border-chalk-blue/40 text-chalk-blue"
                  : "bg-chalk-board-light/30 border-chalk-white/8 text-chalk-muted hover:text-chalk-dust"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {/* Days */}
        <div>
          <span className="text-[10px] text-chalk-muted block mb-1">Days</span>
          <div className="flex gap-1">
            {DAYS_OF_WEEK.map((day, idx) => (
              <button
                type="button"
                key={day}
                onClick={() => toggleDay(idx)}
                className={`flex-1 py-1 rounded text-[10px] font-medium border transition-colors ${
                  days.has(idx)
                    ? "bg-chalk-blue/20 border-chalk-blue/40 text-chalk-blue"
                    : "bg-chalk-board-light/30 border-chalk-white/8 text-chalk-muted hover:text-chalk-dust"
                }`}
              >
                {day}
              </button>
            ))}
          </div>
        </div>

        {/* Actions */}
        <div className="flex justify-between pt-1">
          {!confirmDelete ? (
            <button
              type="button"
              onClick={() => setConfirmDelete(true)}
              className="text-[10px] text-chalk-red/70 hover:text-chalk-red transition-colors"
            >
              Delete
            </button>
          ) : (
            <div className="flex items-center gap-2">
              <span className="text-[10px] text-chalk-red">Delete?</span>
              <button
                type="button"
                onClick={onDelete}
                className="text-[10px] text-chalk-red font-medium hover:underline"
              >
                Yes
              </button>
              <button
                type="button"
                onClick={() => setConfirmDelete(false)}
                className="text-[10px] text-chalk-muted hover:text-chalk-dust"
              >
                No
              </button>
            </div>
          )}
          <button
            type="button"
            onClick={applyChanges}
            className="px-3 py-1 bg-chalk-blue/20 border border-chalk-blue/40 rounded text-xs text-chalk-blue hover:bg-chalk-blue/30 transition-colors"
          >
            Apply
          </button>
        </div>
      </div>
    </motion.div>
  );
}

// ── Add Panel ──────────────────────────────────────────────────

interface AddPanelProps {
  onAdd: (ev: DraftEvent) => void;
  onClose: () => void;
}

function AddPanel({ onAdd, onClose }: AddPanelProps) {
  const [name, setName] = useState("");
  const [startTime, setStartTime] = useState("08:00");
  const [endTime, setEndTime] = useState("08:30");
  const [eventType, setEventType] = useState<DraftEvent["event_type"]>("fixed");
  const [days, setDays] = useState<Set<number>>(new Set([0, 1, 2, 3, 4]));

  const toggleDay = (d: number) => {
    setDays((prev) => {
      const next = new Set(prev);
      if (next.has(d)) next.delete(d);
      else next.add(d);
      return next;
    });
  };

  const handleAdd = () => {
    if (!name.trim()) return;
    onAdd({
      id: newDraftId(),
      name: name.trim(),
      event_type: eventType,
      occurrences: Array.from(days)
        .sort()
        .map((d) => ({
          day_of_week: d,
          start_time: startTime,
          end_time: endTime,
        })),
    });
  };

  return (
    <motion.div
      initial={{ opacity: 0, height: 0 }}
      animate={{ opacity: 1, height: "auto" }}
      exit={{ opacity: 0, height: 0 }}
      className="mb-3 overflow-hidden"
    >
      <div className="p-3 bg-chalk-board-dark/60 border border-chalk-blue/20 rounded-lg space-y-3">
        <div className="flex items-center justify-between">
          <span className="text-xs font-medium text-chalk-blue">
            Add New Block
          </span>
          <button
            type="button"
            onClick={onClose}
            className="text-chalk-muted hover:text-chalk-dust text-xs"
          >
            &#x2715;
          </button>
        </div>

        {/* Name */}
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="Event name (e.g. PE, Lunch, Math)"
          autoFocus
          className="w-full bg-chalk-board-light/40 border border-chalk-white/10 rounded px-2 py-1.5 text-sm text-chalk-white placeholder-chalk-muted/50 focus:outline-none focus:border-chalk-blue/50"
        />

        {/* Time */}
        <div className="flex gap-2 items-center">
          <input
            type="time"
            value={startTime}
            onChange={(e) => setStartTime(e.target.value)}
            className="bg-chalk-board-light/40 border border-chalk-white/10 rounded px-2 py-1 text-xs text-chalk-white focus:outline-none focus:border-chalk-blue/50"
          />
          <span className="text-chalk-muted text-xs">to</span>
          <input
            type="time"
            value={endTime}
            onChange={(e) => setEndTime(e.target.value)}
            className="bg-chalk-board-light/40 border border-chalk-white/10 rounded px-2 py-1 text-xs text-chalk-white focus:outline-none focus:border-chalk-blue/50"
          />
        </div>

        {/* Type */}
        <div className="flex gap-1.5">
          {(
            [
              ["fixed", "Fixed"],
              ["special", "Special"],
              ["teaching_slot", "Teaching"],
            ] as const
          ).map(([val, label]) => (
            <button
              type="button"
              key={val}
              onClick={() => setEventType(val)}
              className={`px-2 py-0.5 rounded text-[10px] border transition-colors ${
                eventType === val
                  ? "bg-chalk-blue/20 border-chalk-blue/40 text-chalk-blue"
                  : "bg-chalk-board-light/30 border-chalk-white/8 text-chalk-muted hover:text-chalk-dust"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {/* Days */}
        <div>
          <span className="text-[10px] text-chalk-muted block mb-1">Days</span>
          <div className="flex gap-1">
            {DAYS_OF_WEEK.map((day, idx) => (
              <button
                type="button"
                key={day}
                onClick={() => toggleDay(idx)}
                className={`flex-1 py-1 rounded text-[10px] font-medium border transition-colors ${
                  days.has(idx)
                    ? "bg-chalk-blue/20 border-chalk-blue/40 text-chalk-blue"
                    : "bg-chalk-board-light/30 border-chalk-white/8 text-chalk-muted hover:text-chalk-dust"
                }`}
              >
                {day}
              </button>
            ))}
          </div>
        </div>

        {/* Add button */}
        <div className="flex justify-end">
          <button
            type="button"
            onClick={handleAdd}
            disabled={!name.trim()}
            className="px-3 py-1 bg-chalk-blue/20 border border-chalk-blue/40 rounded text-xs text-chalk-blue hover:bg-chalk-blue/30 transition-colors disabled:opacity-40"
          >
            Add
          </button>
        </div>
      </div>
    </motion.div>
  );
}
