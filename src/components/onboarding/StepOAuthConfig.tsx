import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";

interface Props {
  onNext: () => void;
  onBack: () => void;
  setError: (err: string | null) => void;
}

export function StepOAuthConfig({ onNext, onBack, setError }: Props) {
  const [clientId, setClientId] = useState("");
  const [clientSecret, setClientSecret] = useState("");
  const [saving, setSaving] = useState(false);

  const handleSave = async () => {
    if (!clientId.trim() || !clientSecret.trim()) {
      setError("Both Client ID and Client Secret are required.");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      await invoke("save_oauth_config", {
        clientId: clientId.trim(),
        clientSecret: clientSecret.trim(),
      });
      onNext();
    } catch (e) {
      setError(`Failed to save config: ${e}`);
    } finally {
      setSaving(false);
    }
  };

  return (
    <div>
      <h2 className="text-2xl font-bold text-bat-cyan mb-2">
        Google OAuth Setup
      </h2>
      <p className="text-gray-400 text-sm mb-6">
        Enter your Google Cloud OAuth credentials. You can create these in
        the Google Cloud Console under APIs &amp; Services &gt; Credentials.
      </p>

      <div className="space-y-4 mb-8">
        <div>
          <label className="block text-sm text-gray-300 mb-1">Client ID</label>
          <input
            type="text"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
            placeholder="xxxx.apps.googleusercontent.com"
            className="w-full px-4 py-2.5 bg-bat-charcoal border border-bat-purple/30 rounded-lg text-white placeholder-gray-600 focus:outline-none focus:border-bat-cyan transition-colors"
          />
        </div>
        <div>
          <label className="block text-sm text-gray-300 mb-1">
            Client Secret
          </label>
          <input
            type="password"
            value={clientSecret}
            onChange={(e) => setClientSecret(e.target.value)}
            placeholder="GOCSPX-..."
            className="w-full px-4 py-2.5 bg-bat-charcoal border border-bat-purple/30 rounded-lg text-white placeholder-gray-600 focus:outline-none focus:border-bat-cyan transition-colors"
          />
        </div>
      </div>

      <div className="flex justify-between">
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={onBack}
          className="px-6 py-2.5 border border-gray-600 rounded-lg text-gray-400 hover:text-white hover:border-gray-400 transition-colors"
        >
          Back
        </motion.button>
        <motion.button
          whileHover={{ scale: 1.05 }}
          whileTap={{ scale: 0.95 }}
          onClick={handleSave}
          disabled={saving}
          className="px-6 py-2.5 bg-gradient-to-r from-bat-cyan to-bat-purple rounded-lg font-semibold text-white disabled:opacity-50 shadow-lg shadow-bat-cyan/20"
        >
          {saving ? "Saving..." : "Save & Continue"}
        </motion.button>
      </div>
    </div>
  );
}
