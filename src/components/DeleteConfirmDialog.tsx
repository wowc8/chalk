import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface DeleteConfirmDialogProps {
  planTitle: string;
  onConfirm: () => Promise<void>;
  onCancel: () => void;
}

export function DeleteConfirmDialog({
  planTitle,
  onConfirm,
  onCancel,
}: DeleteConfirmDialogProps) {
  const [deleting, setDeleting] = useState(false);

  const handleConfirm = async () => {
    setDeleting(true);
    await onConfirm();
  };

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-50 flex items-center justify-center settings-backdrop"
        onClick={(e) => {
          if (e.target === e.currentTarget && !deleting) onCancel();
        }}
      >
        <motion.div
          initial={{ opacity: 0, scale: 0.9, y: 20 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.9, y: 20 }}
          transition={{ type: "spring", stiffness: 300, damping: 30 }}
          className="bg-chalk-board border border-chalk-white/10 rounded-2xl p-6 max-w-sm mx-4 shadow-2xl"
        >
          <div className="w-12 h-12 mx-auto mb-4 rounded-full bg-chalk-red/10 border border-chalk-red/20 flex items-center justify-center">
            <svg
              className="w-6 h-6 text-chalk-red"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={1.5}
                d="M19 7l-.867 12.142A2 2 0 0116.138 21H7.862a2 2 0 01-1.995-1.858L5 7m5 4v6m4-6v6m1-10V4a1 1 0 00-1-1h-4a1 1 0 00-1 1v3M4 7h16"
              />
            </svg>
          </div>

          <h3 className="text-base font-semibold text-chalk-white text-center mb-1">
            Delete Plan
          </h3>
          <p className="text-sm text-chalk-muted text-center mb-5 leading-relaxed">
            Are you sure you want to delete{" "}
            <span className="text-chalk-white font-medium">"{planTitle}"</span>?
            This action cannot be undone.
          </p>

          <div className="flex gap-3">
            <button
              disabled={deleting}
              onClick={onCancel}
              className="flex-1 px-4 py-2 border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-white hover:border-chalk-white/20 transition-colors text-sm disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              disabled={deleting}
              onClick={handleConfirm}
              className="flex-1 px-4 py-2 bg-chalk-red/80 text-white font-medium rounded-lg hover:bg-chalk-red transition-colors text-sm disabled:opacity-50"
            >
              {deleting ? "Deleting..." : "Delete"}
            </button>
          </div>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}
