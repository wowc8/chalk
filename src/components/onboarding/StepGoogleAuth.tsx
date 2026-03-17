import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";

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

      // Open in the system default browser via Tauri opener plugin
      try {
        await openUrl(url);
      } catch {
        // Opener failed (e.g. dev mode without Tauri runtime) — the
        // fallback link below lets the user open it manually.
      }
    } catch (e) {
      setError(`Failed to start sign-in: ${e}`);
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
      <h2 className="text-xl font-semibold text-chalk-blue mb-2">
        Sign in with Google
      </h2>
      <p className="text-chalk-muted text-sm mb-6">
        Chalk needs read-only access to your Google Drive to find your
        lesson plans. We never modify your documents.
      </p>

      {!authUrl ? (
        <div className="text-center py-8">
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
                d="M13.828 10.172a4 4 0 00-5.656 0l-4 4a4 4 0 105.656 5.656l1.102-1.101m-.758-4.899a4 4 0 005.656 0l4-4a4 4 0 00-5.656-5.656l-1.1 1.1"
              />
            </svg>
          </div>
          <button
            onClick={handleGetUrl}
            className="btn btn-primary px-8 py-3 text-base"
          >
            Sign in with Google
          </button>
          <p className="mt-4 text-xs text-chalk-muted">
            Read-only access &middot; No student data sent to our servers
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          <div className="p-3 bg-chalk-board-dark rounded-lg border border-chalk-white/8">
            <p className="text-xs text-chalk-muted mb-2">
              A browser window should have opened. Sign in with Google, then
              copy the authorization code and paste it below.
            </p>
            <p className="text-xs text-chalk-muted">
              Didn't open?{" "}
              <a
                href={authUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="text-chalk-blue underline hover:no-underline"
              >
                Click here to open manually
              </a>
            </p>
          </div>

          <div>
            <label className="block text-sm text-chalk-dust mb-1">
              Authorization Code
            </label>
            <input
              type="text"
              value={authCode}
              onChange={(e) => setAuthCode(e.target.value)}
              placeholder="Paste the code from Google here..."
              className="w-full px-4 py-2.5 bg-chalk-board-dark border border-chalk-white/10 rounded-lg text-chalk-white placeholder-chalk-muted focus:outline-none focus:border-chalk-blue/40 transition-colors"
            />
          </div>

          <button
            onClick={handleExchange}
            disabled={exchanging}
            className="btn btn-primary w-full justify-center py-2.5"
          >
            {exchanging ? "Authenticating..." : "Complete Sign-In"}
          </button>
        </div>
      )}

      <div className="flex justify-between mt-8">
        <button
          onClick={onBack}
          className="btn btn-ghost"
        >
          Back
        </button>
      </div>
    </div>
  );
}
