import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";

interface Props {
  onNext: () => void;
  onBack: () => void;
  setError: (err: string | null) => void;
  setProcessing: (processing: boolean) => void;
}

type ScanState = "idle" | "scanning" | "success" | "empty" | "error" | "success_no_key";

const PROGRESS_MESSAGES = [
  "Connecting to Google Drive...",
  "Searching for documents...",
  "Scanning folder and subfolders...",
  "Analyzing document contents...",
  "Extracting lesson plans from tables...",
  "Processing large documents (this may take a moment)...",
];

export function StepInitialDigest({
  onNext,
  onBack,
  setError,
  setProcessing,
}: Props) {
  const [scanState, setScanState] = useState<ScanState>("idle");
  const [result, setResult] = useState<string | null>(null);
  const [errorDetail, setErrorDetail] = useState<string | null>(null);
  const [progressIndex, setProgressIndex] = useState(0);
  const [progressPercent, setProgressPercent] = useState(0);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    return () => {
      if (intervalRef.current) clearInterval(intervalRef.current);
    };
  }, []);

  const startProgressSimulation = () => {
    setProgressIndex(0);
    setProgressPercent(0);
    let step = 0;
    intervalRef.current = setInterval(() => {
      step++;
      const messageIdx = Math.min(
        Math.floor(step / 3),
        PROGRESS_MESSAGES.length - 1
      );
      setProgressIndex(messageIdx);
      // Slow down as we approach 90% (never reaches 100 until actually done)
      setProgressPercent((prev) => Math.min(prev + (90 - prev) * 0.15, 90));
    }, 800);
  };

  const stopProgressSimulation = () => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }
  };

  // Track whether the scan was cancelled so we can ignore late results.
  const cancelledRef = useRef(false);

  const handleDigest = async () => {
    cancelledRef.current = false;
    setScanState("scanning");
    setProcessing(true);
    setError(null);
    setErrorDetail(null);
    startProgressSimulation();

    try {
      const msg = await invoke<string>("trigger_initial_digest");
      stopProgressSimulation();

      // If the user cancelled while we were waiting, ignore the result.
      if (cancelledRef.current) return;

      setProgressPercent(100);

      // Parse document count from result message
      const countMatch = msg.match(/found (\d+) document/);
      const extractedMatch = msg.match(/extracted (\d+) lesson/);
      const count = countMatch ? parseInt(countMatch[1], 10) : 0;
      const extracted = extractedMatch ? parseInt(extractedMatch[1], 10) : 0;
      const embeddingsSkipped = msg.includes("embeddings_skipped");

      if (count === 0 && extracted === 0) {
        setScanState("empty");
        setResult(msg.split("|")[0]);
      } else if (embeddingsSkipped) {
        setScanState("success_no_key");
        setResult(msg.split("|")[0]);
      } else {
        setScanState("success");
        setResult(msg.split("|")[0]);
      }
    } catch (e) {
      stopProgressSimulation();

      if (cancelledRef.current) return;

      const errorMsg = `${e}`;
      setScanState("error");
      setErrorDetail(errorMsg);

      // Show user-friendly message based on error type
      if (errorMsg.includes("Not authenticated")) {
        setError(
          "Authentication expired. Go back to re-authorize with Google."
        );
      } else if (errorMsg.includes("No folder selected")) {
        setError("No folder selected. Go back to choose a folder.");
      } else if (errorMsg.includes("network") || errorMsg.includes("fetch")) {
        setError(
          "Network error. Check your internet connection and try again."
        );
      } else {
        setError("Scan failed. You can retry or skip for now.");
      }
    } finally {
      if (!cancelledRef.current) {
        setProcessing(false);
      }
    }
  };

  const handleCancel = () => {
    cancelledRef.current = true;
    stopProgressSimulation();
    setScanState("idle");
    setProcessing(false);
    setError(null);
    setErrorDetail(null);
  };

  return (
    <div>
      <h2 className="text-2xl font-bold text-bat-cyan mb-2">
        Import Your Archive
      </h2>
      <p className="text-gray-400 text-sm mb-6">
        Chalk will scan your selected folder for lesson plan documents and begin
        indexing them for the AI to learn your teaching style.
      </p>

      <AnimatePresence mode="wait">
        {/* Idle state - show start button */}
        {scanState === "idle" && (
          <motion.div
            key="idle"
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="text-center py-8"
          >
            <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-charcoal border-2 border-bat-gold/40 flex items-center justify-center">
              <svg
                className="w-10 h-10 text-bat-gold"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M19 11H5m14 0a2 2 0 012 2v6a2 2 0 01-2 2H5a2 2 0 01-2-2v-6a2 2 0 012-2m14 0V9a2 2 0 00-2-2M5 11V9a2 2 0 012-2m0 0V5a2 2 0 012-2h6a2 2 0 012 2v2M7 7h10"
                />
              </svg>
            </div>

            <motion.button
              whileHover={{ scale: 1.05 }}
              whileTap={{ scale: 0.95 }}
              onClick={handleDigest}
              className="px-8 py-3 bg-gradient-to-r from-bat-gold to-bat-cyan rounded-lg font-semibold text-bat-dark shadow-lg shadow-bat-gold/20"
            >
              Start Import
            </motion.button>
          </motion.div>
        )}

        {/* Scanning state - progress indicator */}
        {scanState === "scanning" && (
          <motion.div
            key="scanning"
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -10 }}
            className="py-8"
          >
            <div className="flex items-center justify-center mb-6">
              <motion.div
                animate={{ rotate: 360 }}
                transition={{
                  duration: 1.5,
                  repeat: Infinity,
                  ease: "linear",
                }}
                className="w-12 h-12 border-2 border-bat-cyan border-t-transparent rounded-full"
              />
            </div>

            {/* Progress bar */}
            <div className="w-full h-2 bg-bat-charcoal rounded-full overflow-hidden mb-3">
              <motion.div
                className="h-full bg-gradient-to-r from-bat-cyan to-bat-purple rounded-full"
                animate={{ width: `${progressPercent}%` }}
                transition={{ duration: 0.5 }}
              />
            </div>

            {/* Progress message */}
            <AnimatePresence mode="wait">
              <motion.p
                key={progressIndex}
                initial={{ opacity: 0, y: 5 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0, y: -5 }}
                className="text-center text-sm text-gray-400"
              >
                {PROGRESS_MESSAGES[progressIndex]}
              </motion.p>
            </AnimatePresence>

            <div className="flex justify-center mt-4">
              <motion.button
                whileHover={{ scale: 1.05 }}
                whileTap={{ scale: 0.95 }}
                onClick={handleCancel}
                className="px-4 py-2 border border-gray-600 rounded-lg text-gray-400 text-sm hover:text-white hover:border-gray-400 transition-colors"
              >
                Cancel
              </motion.button>
            </div>
          </motion.div>
        )}

        {/* Success state */}
        {scanState === "success" && (
          <motion.div
            key="success"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="text-center py-8"
          >
            <motion.div
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              transition={{ type: "spring", stiffness: 300, damping: 20 }}
              className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-green/10 border-2 border-bat-green/40 flex items-center justify-center"
            >
              <svg
                className="w-10 h-10 text-bat-green"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M5 13l4 4L19 7"
                />
              </svg>
            </motion.div>
            <p className="text-bat-green font-semibold mb-2">{result}</p>
            <p className="text-gray-500 text-sm">
              Your documents are queued for processing.
            </p>
          </motion.div>
        )}

        {/* Success but no API key — embeddings skipped */}
        {scanState === "success_no_key" && (
          <motion.div
            key="success_no_key"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="text-center py-8"
          >
            <motion.div
              initial={{ scale: 0 }}
              animate={{ scale: 1 }}
              transition={{ type: "spring", stiffness: 300, damping: 20 }}
              className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-green/10 border-2 border-bat-green/40 flex items-center justify-center"
            >
              <svg
                className="w-10 h-10 text-bat-green"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M5 13l4 4L19 7"
                />
              </svg>
            </motion.div>
            <p className="text-bat-green font-semibold mb-2">{result}</p>
            <div className="mt-3 p-3 bg-bat-gold/10 border border-bat-gold/30 rounded-lg">
              <p className="text-bat-gold text-sm">
                AI-powered search requires an OpenAI API key. Add one in{" "}
                <span className="font-semibold">Settings</span> to enable
                smart search across your lesson plans.
              </p>
            </div>
          </motion.div>
        )}

        {/* Empty state - scan succeeded but no docs found */}
        {scanState === "empty" && (
          <motion.div
            key="empty"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="text-center py-8"
          >
            <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-gold/10 border-2 border-bat-gold/40 flex items-center justify-center">
              <svg
                className="w-10 h-10 text-bat-gold"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M12 9v2m0 4h.01m-6.938 4h13.856c1.54 0 2.502-1.667 1.732-3L13.732 4c-.77-1.333-2.694-1.333-3.464 0L3.34 16c-.77 1.333.192 3 1.732 3z"
                />
              </svg>
            </div>
            <p className="text-bat-gold font-semibold mb-2">
              No documents found
            </p>
            <p className="text-gray-500 text-sm mb-4">
              The selected folder doesn't contain any Google Docs yet. You can
              add documents later and Chalk will pick them up.
            </p>
            <div className="flex justify-center gap-3">
              <motion.button
                whileHover={{ scale: 1.05 }}
                whileTap={{ scale: 0.95 }}
                onClick={() => {
                  setScanState("idle");
                  setError(null);
                }}
                className="px-4 py-2 border border-bat-gold/40 rounded-lg text-bat-gold text-sm hover:bg-bat-gold/10 transition-colors"
              >
                Retry Scan
              </motion.button>
            </div>
          </motion.div>
        )}

        {/* Error state */}
        {scanState === "error" && (
          <motion.div
            key="error"
            initial={{ opacity: 0, y: 20 }}
            animate={{ opacity: 1, y: 0 }}
            className="text-center py-8"
          >
            <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-red/10 border-2 border-bat-red/40 flex items-center justify-center">
              <svg
                className="w-10 h-10 text-bat-red"
                fill="none"
                stroke="currentColor"
                viewBox="0 0 24 24"
              >
                <path
                  strokeLinecap="round"
                  strokeLinejoin="round"
                  strokeWidth={2}
                  d="M6 18L18 6M6 6l12 12"
                />
              </svg>
            </div>
            <p className="text-bat-red font-semibold mb-2">Scan failed</p>
            {errorDetail && (
              <p className="text-gray-600 text-xs mb-4 font-mono max-w-sm mx-auto truncate">
                {errorDetail}
              </p>
            )}
            <div className="flex justify-center gap-3">
              <motion.button
                whileHover={{ scale: 1.05 }}
                whileTap={{ scale: 0.95 }}
                onClick={() => {
                  setScanState("idle");
                  setError(null);
                  setErrorDetail(null);
                }}
                className="px-4 py-2 border border-bat-red/40 rounded-lg text-bat-red text-sm hover:bg-bat-red/10 transition-colors"
              >
                Try Again
              </motion.button>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      <div className="flex justify-between mt-8">
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={() => {
            if (scanState === "scanning") handleCancel();
            onBack();
          }}
          className="px-6 py-2.5 border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors"
        >
          Back
        </motion.button>
        {(scanState === "success" || scanState === "success_no_key" || scanState === "empty") && (
          <motion.button
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={onNext}
            className="px-6 py-2.5 bg-gradient-to-r from-bat-cyan to-bat-purple rounded-lg font-semibold text-white shadow-lg shadow-bat-cyan/20"
          >
            Continue
          </motion.button>
        )}
        {scanState === "error" && (
          <motion.button
            initial={{ opacity: 0, x: 20 }}
            animate={{ opacity: 1, x: 0 }}
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={onNext}
            className="px-6 py-2.5 border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors"
          >
            Skip for now
          </motion.button>
        )}
      </div>
    </div>
  );
}
