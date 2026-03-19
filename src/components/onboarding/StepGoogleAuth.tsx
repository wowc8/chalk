import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";

interface Props {
  onNext: () => void;
  onBack: () => void;
  setError: (err: string | null) => void;
}

export function StepGoogleAuth({ onNext, onBack, setError }: Props) {
  const [signingIn, setSigningIn] = useState(false);

  const handleSignIn = async () => {
    setSigningIn(true);
    setError(null);
    try {
      await invoke("start_oauth_flow");
      onNext();
    } catch (e) {
      setError(`Sign-in failed: ${e}`);
    } finally {
      setSigningIn(false);
    }
  };

  const handleCancel = () => {
    setSigningIn(false);
    setError(null);
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
        {signingIn ? (
          <>
            <div className="text-chalk-muted text-sm mb-4">
              Waiting for Google sign-in...
            </div>
            <p className="text-xs text-chalk-muted mb-4">
              Complete sign-in in your browser, then return here.
            </p>
            <button
              onClick={handleCancel}
              className="btn btn-ghost text-sm"
            >
              Try Again
            </button>
          </>
        ) : (
          <>
            <button
              onClick={handleSignIn}
              className="btn btn-primary px-8 py-3 text-base"
            >
              Sign in with Google
            </button>
            <p className="mt-4 text-xs text-chalk-muted">
              Read-only access &middot; No student data sent to our servers
            </p>
          </>
        )}
      </div>

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
