/** Mirrors the Rust RecurringEvent model. */
export interface RecurringEvent {
  id: string;
  name: string;
  /** "fixed" | "special" | "teaching_slot" */
  event_type: string;
  linked_to: string | null;
  details_vary_daily: boolean;
  created_at: string;
  updated_at: string;
}

export interface NewRecurringEvent {
  name: string;
  event_type?: string;
  linked_to?: string | null;
  details_vary_daily?: boolean;
}

/** A specific time occurrence of a recurring event on a given day. */
export interface EventOccurrence {
  id: string;
  event_id: string;
  /** 0 = Monday, 4 = Friday */
  day_of_week: number;
  /** "HH:MM" format */
  start_time: string;
  /** "HH:MM" format */
  end_time: string;
}

export interface NewEventOccurrence {
  event_id: string;
  day_of_week: number;
  start_time: string;
  end_time: string;
}

export interface SchoolCalendar {
  id: string;
  year_start: string;
  year_end: string | null;
  created_at: string;
  updated_at: string;
}

export interface NewSchoolCalendar {
  year_start: string;
  year_end?: string | null;
}

export interface CalendarException {
  id: string;
  calendar_id: string;
  date: string;
  /** "no_school" | "half_day" | "early_release" */
  exception_type: string;
  label: string;
}

export interface NewCalendarException {
  calendar_id: string;
  date: string;
  exception_type: string;
  label?: string;
}

/** Frontend-only type used during wizard to collect schedule data before saving. */
export interface DraftEvent {
  id: string;
  name: string;
  event_type: "fixed" | "special" | "teaching_slot";
  occurrences: DraftOccurrence[];
}

export interface DraftOccurrence {
  day_of_week: number;
  start_time: string;
  end_time: string;
}

export const DAYS_OF_WEEK = ["Mon", "Tue", "Wed", "Thu", "Fri"] as const;
export const DAY_LABELS: Record<number, string> = {
  0: "Monday",
  1: "Tuesday",
  2: "Wednesday",
  3: "Thursday",
  4: "Friday",
};

export const GRADE_LEVELS = [
  "Pre-K",
  "TK",
  "K",
  "1st",
  "2nd",
  "3rd",
  "4th",
  "5th",
] as const;

export type GradeLevel = (typeof GRADE_LEVELS)[number];

/** Pre-seeded daily events by grade level. */
export const DAILY_SUGGESTIONS: Record<string, { name: string; start: string; end: string }[]> = {
  "Pre-K": [
    { name: "Arrival / Free Play", start: "08:00", end: "08:30" },
    { name: "Morning Meeting", start: "08:30", end: "09:00" },
    { name: "Snack", start: "10:00", end: "10:15" },
    { name: "Lunch", start: "11:30", end: "12:00" },
    { name: "Rest Time", start: "12:00", end: "13:00" },
    { name: "Dismissal", start: "14:00", end: "14:15" },
  ],
  TK: [
    { name: "Morning Meeting", start: "08:15", end: "08:45" },
    { name: "Snack", start: "10:00", end: "10:15" },
    { name: "Recess", start: "10:15", end: "10:45" },
    { name: "Lunch", start: "11:45", end: "12:15" },
    { name: "Rest / Quiet Time", start: "12:15", end: "12:45" },
    { name: "Dismissal", start: "14:00", end: "14:15" },
  ],
  K: [
    { name: "Morning Meeting", start: "08:15", end: "08:45" },
    { name: "Recess", start: "10:00", end: "10:20" },
    { name: "Lunch", start: "11:30", end: "12:00" },
    { name: "Recess", start: "12:00", end: "12:20" },
    { name: "Dismissal", start: "14:30", end: "14:45" },
  ],
  default: [
    { name: "Morning Meeting", start: "08:15", end: "08:30" },
    { name: "Recess", start: "10:15", end: "10:30" },
    { name: "Lunch", start: "11:45", end: "12:15" },
    { name: "Recess", start: "12:15", end: "12:45" },
    { name: "Dismissal", start: "15:00", end: "15:15" },
  ],
};

export const SPECIAL_SUGGESTIONS = [
  "PE",
  "Art",
  "Music",
  "Drama",
  "Library",
  "Computer Lab",
  "Spanish",
  "STEM",
];
