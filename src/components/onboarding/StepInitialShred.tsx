import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";

interface Props {
  onNext: () => void;
  onBack: () => void;
  setError: (err: string | null) => void;
  setProcessing: (processing: boolean) => void;
}

export function StepInitialShred({
  onNext,
  onBack,
  setError,
  setProcessing,
}: Props) {
  const [result, setResult] = useState<string | null>(null);
  const [shredding, setShredding] = useState(false);

  const handleShred = async () => {
    setShredding(true);
    setProcessing(true);
    setError(null);
    try {
      const msg = await invoke<string>("trigger_initial_shred");
      setResult(msg);
    } catch (e) {
      setError(`Shred failed: ${e}`);
    } finally {
      setShredding(false);
      setProcessing(false);
    }
  };

  return (
    <div>
      <h2 className="text-2xl font-bold text-bat-cyan mb-2">
        Import Your Archive
      </h2>
      <p className="text-gray-400 text-sm mb-6">
        Chalk will scan your selected folder for lesson plan documents and
        begin indexing them for the AI to learn your teaching style.
      </p>

      {!result ? (
        <div className="text-center py-8">
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
            onClick={handleShred}
            disabled={shredding}
            className="px-8 py-3 bg-gradient-to-r from-bat-gold to-bat-cyan rounded-lg font-semibold text-bat-dark disabled:opacity-50 shadow-lg shadow-bat-gold/20"
          >
            {shredding ? "Scanning..." : "Start Import"}
          </motion.button>
        </div>
      ) : (
        <motion.div
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="text-center py-8"
        >
          <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-green/10 border-2 border-bat-green/40 flex items-center justify-center">
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
          </div>
          <p className="text-bat-green font-semibold mb-2">{result}</p>
          <p className="text-gray-500 text-sm">
            Your documents are queued for processing.
          </p>
        </motion.div>
      )}

      <div className="flex justify-between mt-8">
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={onBack}
          disabled={shredding}
          className="px-6 py-2.5 border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors disabled:opacity-50"
        >
          Back
        </motion.button>
        {result && (
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
      </div>
    </div>
  );
}
