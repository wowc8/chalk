import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useErrorPipe } from "./hooks/useErrorPipe";
import { OnboardingWizard } from "./components/onboarding/OnboardingWizard";
import { Dashboard } from "./components/Dashboard";
import { Settings } from "./components/Settings";
import { ToastProvider } from "./components/Toast";
import "./App.css";

type AppView = "loading" | "onboarding" | "dashboard" | "settings";

function App() {
  useErrorPipe();
  const [view, setView] = useState<AppView>("loading");

  useEffect(() => {
    invoke("check_onboarding_status")
      .then((status: unknown) => {
        const s = status as {
          initial_shred_complete: boolean;
          tokens_stored: boolean;
          folder_selected: boolean;
        };
        if (s.initial_shred_complete && s.tokens_stored && s.folder_selected) {
          setView("dashboard");
        } else {
          setView("onboarding");
        }
      })
      .catch(() => {
        setView("onboarding");
      });
  }, []);

  if (view === "loading") {
    return (
      <div className="min-h-screen bg-bat-dark flex items-center justify-center">
        <div className="w-8 h-8 border-2 border-bat-cyan border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <ToastProvider>
      {view === "onboarding" && (
        <OnboardingWizard onComplete={() => setView("dashboard")} />
      )}
      {view === "dashboard" && (
        <Dashboard
          onResetOnboarding={() => setView("onboarding")}
          onOpenSettings={() => setView("settings")}
        />
      )}
      {view === "settings" && (
        <Settings
          onBack={() => setView("dashboard")}
          onReconnect={() => setView("onboarding")}
        />
      )}
    </ToastProvider>
  );
}

export default App;
