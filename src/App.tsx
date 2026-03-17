import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useErrorPipe } from "./hooks/useErrorPipe";
import { OnboardingWizard } from "./components/onboarding/OnboardingWizard";
import { Dashboard } from "./components/Dashboard";
import { Settings } from "./components/Settings";
import { ToastProvider } from "./components/Toast";
import { UpdateBanner } from "./components/UpdateBanner";
import { PrivacyConsentDialog } from "./components/PrivacyConsentDialog";
import { initSentry } from "./sentry";
import "./App.css";

type AppView = "loading" | "onboarding" | "dashboard" | "settings";

function App() {
  useErrorPipe();
  const [view, setView] = useState<AppView>("loading");
  const [showConsentDialog, setShowConsentDialog] = useState(false);

  useEffect(() => {
    async function init() {
      // Check privacy consent status
      try {
        const consent = await invoke<{
          consent_shown: boolean;
          crash_reporting_enabled: boolean;
        }>("get_privacy_consent_status");

        if (consent.crash_reporting_enabled) {
          initSentry();
        }

        if (!consent.consent_shown) {
          setShowConsentDialog(true);
        }
      } catch {
        // Consent commands may not be available in dev mode; continue
      }

      // Check onboarding status
      try {
        const status = (await invoke("check_onboarding_status")) as {
          initial_shred_complete: boolean;
          tokens_stored: boolean;
          folder_selected: boolean;
        };
        if (
          status.initial_shred_complete &&
          status.tokens_stored &&
          status.folder_selected
        ) {
          setView("dashboard");
        } else {
          setView("onboarding");
        }
      } catch {
        setView("onboarding");
      }
    }
    init();
  }, []);

  const handleConsent = async (consented: boolean) => {
    try {
      await invoke("save_privacy_consent", { consented });
      if (consented) {
        initSentry();
      }
    } catch {
      // Best-effort save
    }
    setShowConsentDialog(false);
  };

  if (view === "loading") {
    return (
      <div className="min-h-screen bg-bat-dark flex items-center justify-center">
        <div className="w-8 h-8 border-2 border-bat-cyan border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  return (
    <ToastProvider>
      {showConsentDialog && (
        <PrivacyConsentDialog onConsent={handleConsent} />
      )}

      {view === "onboarding" && (
        <OnboardingWizard onComplete={() => setView("dashboard")} />
      )}
      {view === "dashboard" && (
        <>
          <UpdateBanner />
          <Dashboard
            onResetOnboarding={() => setView("onboarding")}
            onOpenSettings={() => setView("settings")}
          />
        </>
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
