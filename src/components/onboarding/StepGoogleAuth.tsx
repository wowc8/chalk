import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";

interface Props {
  onNext: () => void;
  onBack: () => void;
  setError: (err: string | null) => void;
}

export function StepGoogleAuth({ onNext, onBack, setError }: Props) {
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const [authCode, setAuthCode] = useState("");
  const [exchanging, setExchanging] = useState(false);

  const handleGetUrl = async () => {
    setError(null);
    try {
      const url = await invoke<string>("get_authorization_url");
      setAuthUrl(url);
      window.open(url, "_blank");
    } catch (e) {
      setError(`Failed to get authorization URL: ${e}`);
    }
  };

  const handleExchange = async () => {
    if (!authCode.trim()) {
      setError("Please paste the authorization code from Google.");
      return;
    }

    setExchanging(true);
    setError(null);
    try {
      await invoke("handle_oauth_callback", { code: authCode.trim() });
      onNext();
    } catch (e) {
      setError(`Authentication failed: ${e}`);
    } finally {
      setExchanging(false);
    }
  };

  return (
    <div>
      <h2 className="text-2xl font-bold text-bat-cyan mb-2">
        Connect Google Account
      </h2>
      <p className="text-gray-400 text-sm mb-6">
        Authorize Chalk to read your Google Drive and Docs.
      </p>

      {!authUrl ? (
        <div className="text-center py-8">
          <div className="w-20 h-20 mx-auto mb-6 rounded-full bg-bat-charcoal border-2 border-bat-purple/40 flex items-center justify-center">
            <svg
              className="w-10 h-10 text-bat-cyan"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"
              />
            </svg>
          </div>
          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={handleGetUrl}
            className="px-8 py-3 bg-gradient-to-r from-bat-cyan to-bat-purple rounded-lg font-semibold text-white shadow-lg shadow-bat-cyan/20"
          >
            Open Google Sign-In
          </motion.button>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="p-3 bg-bat-charcoal rounded-lg border border-bat-purple/20">
            <p className="text-xs text-gray-500 mb-1">
              A browser window should have opened. Sign in with Google, then
              copy the authorization code and paste it below.
            </p>
          </div>

          <div>
            <label className="block text-sm text-gray-300 mb-1">
              Authorization Code
            </label>
            <input
              type="text"
              value={authCode}
              onChange={(e) => setAuthCode(e.target.value)}
              placeholder="Paste the code from Google here..."
              className="w-full px-4 py-2.5 bg-bat-charcoal border border-bat-purple/30 rounded-lg text-white placeholder-gray-600 focus:outline-none focus:border-bat-cyan transition-colors"
            />
          </div>

          <motion.button
            whileHover={{ scale: 1.05 }}
            whileTap={{ scale: 0.95 }}
            onClick={handleExchange}
            disabled={exchanging}
            className="w-full px-6 py-2.5 bg-gradient-to-r from-bat-gold to-bat-cyan rounded-lg font-semibold text-bat-dark disabled:opacity-50 shadow-lg"
          >
            {exchanging ? "Authenticating..." : "Complete Authentication"}
          </motion.button>
        </div>
      )}

      <div className="flex justify-between mt-8">
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={onBack}
          className="px-6 py-2.5 border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors"
        >
          Back
        </motion.button>
      </div>
    </div>
  );
}
