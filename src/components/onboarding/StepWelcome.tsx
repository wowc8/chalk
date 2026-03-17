import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { motion } from "framer-motion";

interface Props {
  onNext: (name: string) => void;
  onSkip?: () => void;
  onRestore?: () => void;
}

export function StepWelcome({ onNext, onSkip, onRestore }: Props) {
  const [name, setName] = useState("");
  const [restoring, setRestoring] = useState(false);

  const handleNext = () => {
    onNext(name.trim());
  };

  const handleRestore = async () => {
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "Chalk Backup", extensions: ["chalk-backup.zip", "zip"] }],
      });
      if (!path) return;
      setRestoring(true);
      await invoke("import_backup", { path });
      // Re-vectorize in background
      invoke("vectorize_all_plans").catch(() => {});
      onRestore?.();
    } catch (e) {
      console.error("Restore failed:", e);
      setRestoring(false);
    }
  };

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
        Your AI-powered lesson plan assistant. Connect your
        Google Drive so Chalk can learn from your teaching history.
      </motion.p>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.4 }}
        className="mb-8"
      >
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
          className="w-full max-w-xs mx-auto block px-4 py-2.5 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-lg text-sm text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors text-center"
          autoFocus
        />
      </motion.div>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.6 }}
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
        transition={{ delay: 0.8 }}
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
