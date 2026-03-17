import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";

interface PrivacyConsentDialogProps {
  onConsent: (consented: boolean) => void;
}

export function PrivacyConsentDialog({ onConsent }: PrivacyConsentDialogProps) {
  const [submitting, setSubmitting] = useState(false);

  const handleChoice = (consented: boolean) => {
    setSubmitting(true);
    onConsent(consented);
  };

  return (
    <AnimatePresence>
      <motion.div
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        className="fixed inset-0 z-50 flex items-center justify-center settings-backdrop"
      >
        <motion.div
          initial={{ opacity: 0, scale: 0.9, y: 20 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.9, y: 20 }}
          transition={{ type: "spring", stiffness: 300, damping: 30 }}
          className="bg-chalk-board border border-chalk-white/10 rounded-2xl p-8 max-w-md mx-4 shadow-2xl"
        >
          <div className="w-14 h-14 mx-auto mb-5 rounded-full bg-chalk-board-dark border-2 border-chalk-blue/30 flex items-center justify-center">
            <svg
              className="w-7 h-7 text-chalk-blue"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={1.5}
                d="M9 12l2 2 4-4m5.618-4.016A11.955 11.955 0 0112 2.944a11.955 11.955 0 01-8.618 3.04A12.02 12.02 0 003 9c0 5.591 3.824 10.29 9 11.622 5.176-1.332 9-6.03 9-11.622 0-1.042-.133-2.052-.382-3.016z"
              />
            </svg>
          </div>

          <h2 className="text-xl font-bold text-white text-center mb-2">
            Help Improve Chalk
          </h2>

          <p className="text-chalk-muted text-sm text-center mb-6 leading-relaxed">
            Chalk can automatically send anonymous crash reports to help us fix
            bugs and improve the app. No student data, document content, or
            personal information is ever included.
          </p>

          <div className="text-xs text-chalk-muted bg-chalk-board-dark/50 rounded-lg p-3 mb-6">
            <p className="font-medium text-chalk-dust mb-1">What we collect:</p>
            <ul className="space-y-1 list-disc list-inside">
              <li>OS version and app version</li>
              <li>Error stack traces</li>
              <li>Recent app events (breadcrumbs)</li>
            </ul>
          </div>

          <div className="flex gap-3">
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              disabled={submitting}
              onClick={() => handleChoice(false)}
              className="flex-1 px-4 py-2.5 border border-chalk-white/10 rounded-lg text-chalk-muted hover:text-chalk-white hover:border-chalk-white/20 transition-colors text-sm disabled:opacity-50"
            >
              No Thanks
            </motion.button>
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              disabled={submitting}
              onClick={() => handleChoice(true)}
              className="flex-1 px-4 py-2.5 bg-chalk-blue text-chalk-board-dark font-semibold rounded-lg hover:bg-chalk-blue/90 transition-colors text-sm disabled:opacity-50"
            >
              Enable Reports
            </motion.button>
          </div>

          <p className="text-xs text-chalk-muted/60 text-center mt-4">
            You can change this anytime in Settings.
          </p>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}
