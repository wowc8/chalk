import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useErrorPipe } from "./hooks/useErrorPipe";
import { OnboardingWizard } from "./components/onboarding";
import type { OnboardingStatus } from "./components/onboarding/OnboardingWizard";
import "./index.css";

function App() {
  useErrorPipe();
  const [needsOnboarding, setNeedsOnboarding] = useState<boolean | null>(null);

  useEffect(() => {
    checkOnboarding();
  }, []);

  async function checkOnboarding() {
    try {
      await invoke("initialize_oauth");
      const status = await invoke<OnboardingStatus>("check_onboarding_status");
      setNeedsOnboarding(!status.initial_shred_complete);
    } catch {
      setNeedsOnboarding(true);
    }
  }

  if (needsOnboarding === null) {
    return (
      <div className="min-h-screen bg-bat-dark flex items-center justify-center">
        <div className="w-8 h-8 border-2 border-bat-cyan border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  if (needsOnboarding) {
    return (
      <OnboardingWizard onComplete={() => setNeedsOnboarding(false)} />
    );
  }

  return (
    <main className="min-h-screen bg-bat-dark text-white flex items-center justify-center">
      <div className="text-center">
        <h1 className="text-4xl font-bold bg-gradient-to-r from-bat-cyan to-bat-purple bg-clip-text text-transparent mb-4">
          Chalk
        </h1>
        <p className="text-gray-400">
          Your workspace is ready. Start building lesson plans!
        </p>
      </div>
    </main>
  );
}

export default App;
