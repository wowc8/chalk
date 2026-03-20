import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { GRADE_LEVELS } from "../../types/schedule";
import { useTeacherName } from "../../hooks/useTeacherName";

interface Props {
  addToast: (msg: string, type: "success" | "error") => void;
}

export function SettingsMyProfile({ addToast }: Props) {
  const { name, setName: saveTeacherName, loading: nameLoading } = useTeacherName();
  const [nameInput, setNameInput] = useState("");
  const [gradeLevel, setGradeLevel] = useState("");
  const [schoolName, setSchoolName] = useState("");
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (name) setNameInput(name);
  }, [name]);

  useEffect(() => {
    Promise.all([
      invoke<string | null>("get_app_setting", { key: "grade_level" }),
      invoke<string | null>("get_app_setting", { key: "school_name" }),
    ])
      .then(([grade, school]) => {
        if (grade) setGradeLevel(grade);
        if (school) setSchoolName(school);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const handleSave = async () => {
    setSaving(true);
    try {
      const trimmedName = nameInput.trim();
      if (trimmedName) await saveTeacherName(trimmedName);
      if (gradeLevel) {
        await invoke("set_app_setting", { key: "grade_level", value: gradeLevel });
      }
      await invoke("set_app_setting", {
        key: "school_name",
        value: schoolName.trim(),
      });
      addToast("Profile saved", "success");
    } catch {
      addToast("Failed to save profile", "error");
    } finally {
      setSaving(false);
    }
  };

  if (loading || nameLoading) {
    return (
      <section className="mb-8">
        <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
          My Profile
        </h3>
        <div className="flex items-center gap-3 py-4 justify-center">
          <div className="w-4 h-4 border-2 border-chalk-blue border-t-transparent rounded-full animate-spin" />
        </div>
      </section>
    );
  }

  const inputCls =
    "w-full bg-chalk-board/50 border border-chalk-white/8 rounded-lg px-3 py-2 text-sm text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors";

  return (
    <section className="mb-8">
      <h3 className="text-xs font-semibold text-chalk-muted uppercase tracking-wider mb-4">
        My Profile
      </h3>

      <div className="bg-chalk-board-dark/60 rounded-lg border border-chalk-white/5 p-4 space-y-4">
        <div>
          <label className="block text-sm text-chalk-dust mb-1.5">Name</label>
          <input
            type="text"
            value={nameInput}
            onChange={(e) => setNameInput(e.target.value)}
            placeholder="Your first name"
            className={inputCls}
          />
        </div>

        <div>
          <label className="block text-sm text-chalk-dust mb-1.5">
            Grade Level
          </label>
          <select
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

        <div>
          <label className="block text-sm text-chalk-dust mb-1.5">
            School Name
          </label>
          <input
            type="text"
            value={schoolName}
            onChange={(e) => setSchoolName(e.target.value)}
            placeholder="e.g. Sunnydale Elementary"
            className={inputCls}
          />
        </div>

        <div className="flex justify-end">
          <motion.button
            whileHover={{ scale: 1.02 }}
            whileTap={{ scale: 0.98 }}
            disabled={saving}
            onClick={handleSave}
            className="px-5 py-2 bg-chalk-blue/10 border border-chalk-blue/30 rounded-lg text-chalk-blue text-sm hover:bg-chalk-blue/20 transition-colors disabled:opacity-50"
          >
            {saving ? "Saving..." : "Save Profile"}
          </motion.button>
        </div>
      </div>
    </section>
  );
}
