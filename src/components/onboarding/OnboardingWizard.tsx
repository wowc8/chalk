import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { StepWelcome } from "./StepWelcome";
import { StepSchoolCalendar, type CalendarExceptionDraft } from "./StepSchoolCalendar";
import { StepDailySchedule } from "./StepDailySchedule";
import { StepWeeklySpecials } from "./StepWeeklySpecials";
import { StepScheduleReview } from "./StepScheduleReview";
import { StepGoogleAuth } from "./StepGoogleAuth";
import { StepFolderSelect } from "./StepFolderSelect";
import { StepInitialDigest } from "./StepInitialDigest";
import { StepComplete } from "./StepComplete";

import { useTeacherName } from "../../hooks/useTeacherName";
import type { DraftEvent } from "../../types/schedule";

export interface OnboardingStatus {
  oauth_configured: boolean;
  tokens_stored: boolean;
  folder_selected: boolean;
  folder_accessible: boolean;
  initial_digest_complete: boolean;
  selected_folder_id: string | null;
  selected_folder_name: string | null;
}

const STEPS = [
  "welcome",
  "school-calendar",
  "daily-schedule",
  "weekly-specials",
  "schedule-review",
  "google-auth",
  "folder-select",
  "initial-digest",
  "complete",
] as const;

type Step = (typeof STEPS)[number];

const spring = { type: "spring" as const, stiffness: 300, damping: 30 };

export function OnboardingWizard({
  onComplete,
}: {
  onComplete: () => void;
}) {
  const [step, setStep] = useState<Step>("welcome");
  const [direction, setDirection] = useState(1);
  const [error, setError] = useState<string | null>(null);
  const { name: teacherName, setName: saveTeacherName } = useTeacherName();

  // Wizard-level state shared across steps
  const [gradeLevel, setGradeLevel] = useState("");
  const [schoolName, setSchoolName] = useState("");
  const [calendarData, setCalendarData] = useState<{
    yearStart: string;
    yearEnd: string | null;
    exceptions: CalendarExceptionDraft[];
  } | null>(null);
  const [dailyEvents, setDailyEvents] = useState<DraftEvent[]>([]);
  const [specials, setSpecials] = useState<DraftEvent[]>([]);

  useEffect(() => {
    invoke("initialize_oauth").catch(() => {});
  }, []);

  const goTo = (next: Step) => {
    const curIdx = STEPS.indexOf(step);
    const nextIdx = STEPS.indexOf(next);
    setDirection(nextIdx > curIdx ? 1 : -1);
    setError(null);
    setStep(next);
  };

  // Step 1: Welcome
  const handleWelcomeNext = useCallback(
    (data: { name: string; gradeLevel: string; schoolName: string }) => {
      if (data.name) saveTeacherName(data.name);
      if (data.gradeLevel) {
        setGradeLevel(data.gradeLevel);
        invoke("set_app_setting", {
          key: "grade_level",
          value: data.gradeLevel,
        }).catch(() => {});
      }
      if (data.schoolName) {
        setSchoolName(data.schoolName);
        invoke("set_app_setting", {
          key: "school_name",
          value: data.schoolName,
        }).catch(() => {});
      }
      goTo("school-calendar");
    },
    [saveTeacherName],
  );

  // Step 2: School Calendar
  const handleCalendarNext = useCallback(
    (data: {
      yearStart: string;
      yearEnd: string | null;
      exceptions: CalendarExceptionDraft[];
    }) => {
      setCalendarData(data);
      goTo("daily-schedule");
    },
    [],
  );

  // Step 3: Daily Schedule
  const handleDailyNext = useCallback((events: DraftEvent[]) => {
    setDailyEvents(events);
    goTo("weekly-specials");
  }, []);

  // Step 4: Weekly Specials
  const handleSpecialsNext = useCallback((newSpecials: DraftEvent[]) => {
    setSpecials(newSpecials);
    goTo("schedule-review");
  }, []);

  // Step 5: Schedule Review — saves to DB, then moves to Google Auth
  const handleReviewNext = useCallback(() => {
    goTo("google-auth");
  }, []);

  const stepIndex = STEPS.indexOf(step);

  const variants = {
    enter: (d: number) => ({ x: d > 0 ? 300 : -300, opacity: 0 }),
    center: { x: 0, opacity: 1 },
    exit: (d: number) => ({ x: d > 0 ? -300 : 300, opacity: 0 }),
  };

  return (
    <div className="min-h-screen chalk-bg text-chalk-white relative overflow-hidden">
      {/* Chalk grid overlay */}
      <div className="absolute inset-0 chalk-grid" />

      {/* Progress bar */}
      <div className="absolute top-0 left-0 right-0 h-1 bg-chalk-board-dark z-20">
        <motion.div
          className="h-full bg-gradient-to-r from-chalk-blue to-chalk-green"
          animate={{ width: `${((stepIndex + 1) / STEPS.length) * 100}%` }}
          transition={spring}
        />
      </div>

      {/* Step indicators */}
      <div className="absolute top-6 left-1/2 -translate-x-1/2 flex gap-3 z-20">
        {STEPS.map((s, i) => (
          <div
            key={s}
            className={`w-2.5 h-2.5 rounded-full transition-colors duration-300 ${
              i <= stepIndex
                ? "bg-chalk-blue shadow-[0_0_6px_rgba(116,185,255,0.5)]"
                : "bg-chalk-board-light"
            }`}
          />
        ))}
      </div>

      {/* Step content */}
      <div className="relative z-10 flex items-center justify-center min-h-screen px-6">
        <AnimatePresence mode="wait" custom={direction}>
          <motion.div
            key={step}
            custom={direction}
            variants={variants}
            initial="enter"
            animate="center"
            exit="exit"
            transition={spring}
            className="w-full max-w-lg"
          >
            {error && (
              <motion.div
                initial={{ opacity: 0, y: -10 }}
                animate={{ opacity: 1, y: 0 }}
                className="mb-4 p-3 bg-bat-red/20 border border-bat-red/40 rounded-lg text-sm text-bat-red"
              >
                {error}
              </motion.div>
            )}

            {step === "welcome" && (
              <StepWelcome
                onNext={handleWelcomeNext}
                onSkip={onComplete}
                onRestore={onComplete}
                initialName={teacherName ?? ""}
                initialGrade={gradeLevel}
                initialSchool={schoolName}
              />
            )}
            {step === "school-calendar" && (
              <StepSchoolCalendar
                onNext={handleCalendarNext}
                onBack={() => goTo("welcome")}
                initialYearStart={calendarData?.yearStart}
                initialYearEnd={calendarData?.yearEnd}
                initialExceptions={calendarData?.exceptions}
              />
            )}
            {step === "daily-schedule" && (
              <StepDailySchedule
                onNext={handleDailyNext}
                onBack={() => goTo("school-calendar")}
                gradeLevel={gradeLevel}
                initialEvents={dailyEvents}
              />
            )}
            {step === "weekly-specials" && (
              <StepWeeklySpecials
                onNext={handleSpecialsNext}
                onBack={() => goTo("daily-schedule")}
                dailyEvents={dailyEvents}
                initialSpecials={specials}
              />
            )}
            {step === "schedule-review" && (
              <StepScheduleReview
                onNext={handleReviewNext}
                onBack={() => goTo("weekly-specials")}
                dailyEvents={dailyEvents}
                specials={specials}
                calendarData={calendarData}
              />
            )}
            {step === "google-auth" && (
              <StepGoogleAuth
                onNext={() => goTo("folder-select")}
                onBack={() => goTo("schedule-review")}
                setError={setError}
              />
            )}
            {step === "folder-select" && (
              <StepFolderSelect
                onNext={() => goTo("initial-digest")}
                onBack={() => goTo("google-auth")}
                setError={setError}
              />
            )}
            {step === "initial-digest" && (
              <StepInitialDigest
                onNext={() => goTo("complete")}
                onBack={() => goTo("folder-select")}
                setError={setError}
              />
            )}
            {step === "complete" && (
              <StepComplete onFinish={onComplete} teacherName={teacherName} />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}
