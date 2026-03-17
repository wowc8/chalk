import { motion } from "framer-motion";

interface Props {
  onNext: () => void;
  onSkip?: () => void;
}

export function StepWelcome({ onNext, onSkip }: Props) {
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

      <motion.p
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.4 }}
        className="text-chalk-muted text-sm mb-10"
      >
        Sign in with Google to get started. Chalk only needs read-only
        access to find your lesson plans &mdash; we never modify your documents.
      </motion.p>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.6 }}
      >
        <button
          onClick={onNext}
          className="btn btn-primary px-8 py-3 text-base"
        >
          Get Started
        </button>
      </motion.div>

      {onSkip && (
        <motion.button
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.8 }}
          onClick={onSkip}
          className="block mx-auto mt-4 text-sm text-chalk-muted hover:text-chalk-dust transition-colors"
        >
          Skip for now
        </motion.button>
      )}
    </div>
  );
}
