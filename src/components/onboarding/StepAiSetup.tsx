import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

interface Props {
  onNext: () => void;
  onBack: () => void;
  onSkip: () => void;
  setError: (err: string | null) => void;
}

const inputCls =
  "w-full px-3 py-2 bg-chalk-board-dark/60 border border-chalk-white/8 rounded-lg text-sm text-chalk-white focus:outline-none focus:border-chalk-blue/40 transition-colors font-mono";

export function StepAiSetup({ onNext, onBack, onSkip, setError }: Props) {
  const [apiKey, setApiKey] = useState("");
  const [validating, setValidating] = useState(false);

  const handleContinue = async () => {
    const key = apiKey.trim();
    if (!key) return;

    setValidating(true);
    setError(null);
    try {
      const valid = await invoke<boolean>("validate_openai_key", {
        apiKey: key,
      });
      if (!valid) {
        setError("That API key was rejected by OpenAI. Please check and try again.");
        return;
      }
      await invoke("save_ai_config", {
        apiKey: key,
        baseUrl: null,
        model: null,
      });
      onNext();
    } catch (e) {
      setError(`Could not validate key: ${e}`);
    } finally {
      setValidating(false);
    }
  };

  return (
    <div>
      <h2 className="text-xl font-semibold text-chalk-blue mb-2">
        Power Up with AI
      </h2>
      <p className="text-chalk-muted text-sm mb-6">
        Chalk uses AI to understand your lesson plans, extract schedules, and
        build smart weekly planners. Add your OpenAI API key to unlock these
        features.
      </p>

      <div className="text-center py-6">
        <div className="w-16 h-16 mx-auto mb-5 rounded-2xl bg-chalk-board-dark border border-chalk-white/8 flex items-center justify-center">
          <svg
            className="w-8 h-8 text-chalk-blue"
            fill="none"
            stroke="currentColor"
            viewBox="0 0 24 24"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={2}
              d="M13 10V3L4 14h7v7l9-11h-7z"
            />
          </svg>
        </div>

        <div className="max-w-sm mx-auto space-y-4">
          <div className="text-left">
            <label className="block text-sm text-chalk-dust mb-1.5">
              OpenAI API Key
            </label>
            <input
              type="password"
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
              placeholder="sk-..."
              className={inputCls}
              onKeyDown={(e) => {
                if (e.key === "Enter" && apiKey.trim()) handleContinue();
              }}
            />
          </div>

          <button
            onClick={() =>
              openUrl("https://platform.openai.com/api-keys").catch(() => {})
            }
            className="text-xs text-chalk-blue hover:text-chalk-blue/80 transition-colors underline underline-offset-2"
          >
            How do I get an API key?
          </button>

          <button
            onClick={handleContinue}
            disabled={!apiKey.trim() || validating}
            className="btn btn-primary w-full py-3 text-base disabled:opacity-50 disabled:cursor-not-allowed"
          >
            {validating ? "Validating..." : "Continue"}
          </button>

          <button
            onClick={onSkip}
            className="text-xs text-chalk-muted hover:text-chalk-dust transition-colors"
          >
            Skip for now — AI features won't be available until you add a key
            in Settings
          </button>
        </div>
      </div>

      <div className="flex justify-between mt-6">
        <button onClick={onBack} className="btn btn-ghost">
          Back
        </button>
      </div>
    </div>
  );
}
