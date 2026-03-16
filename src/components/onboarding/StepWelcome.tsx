import { motion } from "framer-motion";

export function StepWelcome({ onNext }: { onNext: () => void }) {
  return (
    <div className="text-center">
      <motion.div
        initial={{ scale: 0.5, opacity: 0 }}
        animate={{ scale: 1, opacity: 1 }}
        transition={{ type: "spring", stiffness: 300, damping: 30 }}
        className="mb-8"
      >
        <div className="text-6xl mb-4">&#x270F;&#xFE0F;</div>
        <h1 className="text-4xl font-bold bg-gradient-to-r from-bat-cyan to-bat-purple bg-clip-text text-transparent">
          Welcome to Chalk
        </h1>
      </motion.div>

      <motion.p
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        className="text-gray-400 text-lg mb-8 leading-relaxed"
      >
        Your AI-powered lesson plan assistant. Let's connect your
        Google Drive so Chalk can learn from your teaching history.
      </motion.p>

      <motion.p
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.4 }}
        className="text-gray-500 text-sm mb-10"
      >
        This wizard will walk you through connecting your Google account,
        selecting your lesson plan folder, and importing your archive.
      </motion.p>

      <motion.button
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.6 }}
        whileHover={{ scale: 1.05 }}
        whileTap={{ scale: 0.95 }}
        onClick={onNext}
        className="px-8 py-3 bg-gradient-to-r from-bat-cyan to-bat-purple rounded-lg font-semibold text-white shadow-lg shadow-bat-cyan/20 hover:shadow-bat-cyan/40 transition-shadow"
      >
        Get Started
      </motion.button>
    </div>
  );
}
