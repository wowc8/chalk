import { motion } from "framer-motion";

interface Props {
  onFinish: () => void;
  teacherName?: string | null;
}

export function StepComplete({ onFinish, teacherName }: Props) {
  const heading = teacherName
    ? `You're all set, ${teacherName}!`
    : "You're All Set!";

  return (
    <div className="text-center">
      <motion.div
        initial={{ scale: 0 }}
        animate={{ scale: 1 }}
        transition={{ type: "spring", stiffness: 300, damping: 20 }}
        className="w-24 h-24 mx-auto mb-6 rounded-full bg-gradient-to-br from-bat-cyan to-bat-purple flex items-center justify-center shadow-lg shadow-bat-cyan/30"
      >
        <svg
          className="w-12 h-12 text-white"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2.5}
            d="M5 13l4 4L19 7"
          />
        </svg>
      </motion.div>

      <motion.h2
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.2 }}
        className="text-3xl font-bold bg-gradient-to-r from-bat-gold to-bat-cyan bg-clip-text text-transparent mb-4"
      >
        {heading}
      </motion.h2>

      <motion.p
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.4 }}
        className="text-gray-400 text-lg mb-8"
      >
        Chalk is connected and your lesson plan archive is being indexed.
        Time to start creating!
      </motion.p>

      <motion.button
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ delay: 0.6 }}
        whileHover={{ scale: 1.05 }}
        whileTap={{ scale: 0.95 }}
        onClick={onFinish}
        className="px-10 py-3 bg-gradient-to-r from-bat-gold to-bat-cyan rounded-lg font-bold text-bat-dark text-lg shadow-lg shadow-bat-gold/30 hover:shadow-bat-gold/50 transition-shadow"
      >
        Launch Chalk
      </motion.button>
    </div>
  );
}
