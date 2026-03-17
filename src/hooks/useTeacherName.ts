import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

const SETTING_KEY = "teacher_name";

/**
 * Hook to get/set the teacher's name from app_settings.
 * Returns { name, setName, loading } where name is null if not yet set.
 */
export function useTeacherName() {
  const [name, setNameState] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    invoke<string | null>("get_app_setting", { key: SETTING_KEY })
      .then((val) => setNameState(val))
      .catch(() => setNameState(null))
      .finally(() => setLoading(false));
  }, []);

  const setName = useCallback(async (newName: string) => {
    const trimmed = newName.trim();
    if (!trimmed) return;
    await invoke("set_app_setting", { key: SETTING_KEY, value: trimmed });
    setNameState(trimmed);
  }, []);

  return { name, setName, loading };
}
