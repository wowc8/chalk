import { useParams } from "react-router-dom";
import { motion } from "framer-motion";

export function PlanDetail() {
  const { planId } = useParams<{ planId: string }>();

  return (
    <div className="flex-1 px-6 py-8">
      <div className="max-w-6xl mx-auto">
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="text-center py-20"
        >
          <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-chalk-board-dark border-2 border-chalk-white/10 flex items-center justify-center">
            <svg
              className="w-10 h-10 text-chalk-blue/50"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={1.5}
                d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z"
              />
            </svg>
          </div>
          <h3 className="chalk-heading text-xl text-chalk-white mb-2">
            Plan Editor
          </h3>
          <p className="text-chalk-muted text-sm mb-2">
            Plan ID: <code className="text-chalk-blue/80 bg-chalk-board-dark px-2 py-0.5 rounded text-xs">{planId}</code>
          </p>
          <p className="text-chalk-muted/60 text-xs">
            The split-view editor with AI chat pane will be available in the next update.
          </p>
        </motion.div>
      </div>
    </div>
  );
}
