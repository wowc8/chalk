import { Sentry } from "../sentry";
import { motion } from "framer-motion";

interface FallbackProps {
  error: Error;
  resetError: () => void;
}

function ErrorFallback({ error, resetError }: FallbackProps) {
  return (
    <div className="min-h-screen bg-bat-dark flex items-center justify-center text-white">
      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        className="bg-bat-charcoal border border-bat-red/30 rounded-2xl p-8 max-w-md mx-4 text-center"
      >
        <div className="w-14 h-14 mx-auto mb-5 rounded-full bg-bat-red/20 border-2 border-bat-red/40 flex items-center justify-center">
          <svg
            className="w-7 h-7 text-bat-red"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1.5}
              d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
            />
          </svg>
        </div>

        <h2 className="text-xl font-bold mb-2">Something went wrong</h2>
        <p className="text-gray-400 text-sm mb-4">
          An unexpected error occurred. This has been automatically reported.
        </p>
        <p className="text-xs text-gray-600 bg-bat-dark/50 rounded-lg p-3 mb-6 font-mono break-all">
          {error.message}
        </p>

        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={resetError}
          className="px-6 py-2.5 bg-bat-cyan text-bat-dark font-semibold rounded-lg hover:bg-bat-cyan/90 transition-colors text-sm"
        >
          Try Again
        </motion.button>
      </motion.div>
    </div>
  );
}

export function SentryErrorBoundary({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <Sentry.ErrorBoundary
      fallback={({ error, resetError }) => (
        <ErrorFallback
          error={error instanceof Error ? error : new Error(String(error))}
          resetError={resetError}
        />
      )}
    >
      {children}
    </Sentry.ErrorBoundary>
  );
}
