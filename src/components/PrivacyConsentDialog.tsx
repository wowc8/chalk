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
        className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm"
      >
        <motion.div
          initial={{ opacity: 0, scale: 0.9, y: 20 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.9, y: 20 }}
          transition={{ type: "spring", stiffness: 300, damping: 30 }}
          className="bg-bat-charcoal border border-bat-purple/30 rounded-2xl p-8 max-w-md mx-4 shadow-2xl"
        >
          <div className="w-14 h-14 mx-auto mb-5 rounded-full bg-bat-navy border-2 border-bat-cyan/40 flex items-center justify-center">
            <svg
              className="w-7 h-7 text-bat-cyan"
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

          <p className="text-gray-400 text-sm text-center mb-6 leading-relaxed">
            Chalk can automatically send anonymous crash reports to help us fix
            bugs and improve the app. No student data, document content, or
            personal information is ever included.
          </p>

          <div className="text-xs text-gray-500 bg-bat-dark/50 rounded-lg p-3 mb-6">
            <p className="font-medium text-gray-400 mb-1">What we collect:</p>
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
              className="flex-1 px-4 py-2.5 border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors text-sm disabled:opacity-50"
            >
              No Thanks
            </motion.button>
            <motion.button
              whileHover={{ scale: 1.02 }}
              whileTap={{ scale: 0.98 }}
              disabled={submitting}
              onClick={() => handleChoice(true)}
              className="flex-1 px-4 py-2.5 bg-bat-cyan text-bat-dark font-semibold rounded-lg hover:bg-bat-cyan/90 transition-colors text-sm disabled:opacity-50"
            >
              Enable Reports
            </motion.button>
          </div>

          <p className="text-xs text-gray-600 text-center mt-4">
            You can change this anytime in Settings.
          </p>
        </motion.div>
      </motion.div>
    </AnimatePresence>
  );
}
