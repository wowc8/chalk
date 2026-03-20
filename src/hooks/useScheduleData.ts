import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import type {
  RecurringEvent,
  EventOccurrence,
  SchoolCalendar,
  CalendarException,
} from "../types/schedule";

export interface RecurringEventWithOccurrences extends RecurringEvent {
  occurrences: EventOccurrence[];
}

/** Load all recurring events with their occurrences. */
export function useRecurringEvents() {
  const [events, setEvents] = useState<RecurringEventWithOccurrences[]>([]);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const raw = await invoke<RecurringEvent[]>("get_recurring_events");
      const withOcc: RecurringEventWithOccurrences[] = await Promise.all(
        raw.map(async (ev) => {
          const occurrences = await invoke<EventOccurrence[]>(
            "list_event_occurrences",
            { eventId: ev.id },
          );
          return { ...ev, occurrences };
        }),
      );
      setEvents(withOcc);
    } catch (e) {
      console.error("Failed to load recurring events:", e);
      setEvents([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  return { events, loading, reload };
}

/** Load the school calendar + exceptions. */
export function useSchoolCalendar() {
  const [calendar, setCalendar] = useState<SchoolCalendar | null>(null);
  const [exceptions, setExceptions] = useState<CalendarException[]>([]);
  const [loading, setLoading] = useState(true);

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const cal = await invoke<SchoolCalendar | null>("get_school_calendar");
      setCalendar(cal);
      if (cal) {
        const excs = await invoke<CalendarException[]>(
          "list_calendar_exceptions",
          { calendarId: cal.id },
        );
        setExceptions(excs);
      } else {
        setExceptions([]);
      }
    } catch (e) {
      console.error("Failed to load school calendar:", e);
      setCalendar(null);
      setExceptions([]);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  return { calendar, exceptions, loading, reload };
}
