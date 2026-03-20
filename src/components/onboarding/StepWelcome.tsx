import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { motion } from "framer-motion";
import { GRADE_LEVELS } from "../../types/schedule";

interface Props {
  onNext: (data: { name: string; gradeLevel: string; schoolName: string }) => void;
  onSkip?: () => void;
  onRestore?: () => void;
  initialName?: string;
  initialGrade?: string;
  initialSchool?: string;
}

export function StepWelcome({
  onNext,
  onSkip,
  onRestore,
  initialName = "",
  initialGrade = "",
  initialSchool = "",
}: Props) {
  const [name, setName] = useState(initialName);
  const [gradeLevel, setGradeLevel] = useState(initialGrade);
  const [schoolName, setSchoolName] = useState(initialSchool);
  const [restoring, setRestoring] = useState(false);

  const handleNext = () => {
    onNext({ name: name.trim(), gradeLevel, schoolName: schoolName.trim() });
  };

  const handleRestore = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [
          { name: "Chalk Backup", extensions: ["chalk-backup.zip", "zip"] },
        ],
      });
      if (!path) return;
      setRestoring(true);
      await invoke("import_backup", { path });
      invoke("vectorize_all_plans").catch(() => {});
      onRestore?.();
    } catch (e) {
      console.error("Restore failed:", e);
      setRestoring(false);
    }
  };

  const inputCls =
    "w-full max-w-xs mx-auto block px-4 py-2.5 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-lg text-sm text-chalk-white caret-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors text-center";

  return (
    <div className="text-center">
      <motion.div
        initial={{ scale: 0.5, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ type: "spring", stiffness: 300, damping: 30 }}
        className="mb-8"
      >
        <div className="text-5xl mb-4">&#x270F;&#xFE0F;</div>
        <h1 className="text-3xl font-bold text-chalk-blue">
          Welcome to Chalk
        </h1>
      </motion.div>

      <motion.p
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        className="text-chalk-dust text-base mb-6 leading-relaxed"
      >
        Your AI-powered lesson plan assistant. Let's get to know your
        classroom so Chalk can build plans that fit your schedule.
      </motion.p>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.3 }}
        className="space-y-4 mb-8"
      >
        {/* Teacher name */}
        <div>
          <label
            htmlFor="teacher-name"
            className="block text-sm text-chalk-muted mb-2"
          >
            What should we call you?
          </label>
          <input
            id="teacher-name"
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleNext();
            }}
            placeholder="Your first name"
            className={inputCls}
            autoFocus
          />
        </div>

        {/* Grade level */}
        <div>
          <label
            htmlFor="grade-level"
            className="block text-sm text-chalk-muted mb-2"
          >
            What grade do you teach?
          </label>
          <select
            id="grade-level"
            value={gradeLevel}
            onChange={(e) => setGradeLevel(e.target.value)}
            className={`${inputCls} appearance-none`}
          >
            <option value="">Select grade level</option>
            {GRADE_LEVELS.map((g) => (
              <option key={g} value={g}>
                {g}
              </option>
            ))}
          </select>
        </div>

        {/* School name (optional) */}
        <div>
          <label
            htmlFor="school-name"
            className="block text-sm text-chalk-muted mb-2"
          >
            School name{" "}
            <span className="text-chalk-muted/60">(optional)</span>
          </label>
          <input
            id="school-name"
            type="text"
            value={schoolName}
            onChange={(e) => setSchoolName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") handleNext();
            }}
            placeholder="e.g. Sunnydale Elementary"
            className={inputCls}
          />
        </div>
      </motion.div>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.5 }}
      >
        <button
          onClick={handleNext}
          className="btn btn-primary px-8 py-3 text-base"
        >
          Get Started
        </button>
      </motion.div>

      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.7 }}
        className="flex items-center justify-center gap-4 mt-4"
      >
        {onSkip && (
          <button
            onClick={onSkip}
            className="text-sm text-chalk-muted hover:text-chalk-dust transition-colors"
          >
            Skip for now
          </button>
        )}
        {onRestore && (
          <button
            onClick={handleRestore}
            disabled={restoring}
            className="text-sm text-chalk-muted hover:text-chalk-dust transition-colors disabled:opacity-50"
          >
            {restoring ? "Restoring..." : "Restore Backup"}
          </button>
        )}
      </motion.div>
    </div>
  );
}
