import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { BrowserRouter, Routes, Route } from "react-router-dom";
import { useErrorPipe } from "./hooks/useErrorPipe";
import { OnboardingWizard } from "./components/onboarding/OnboardingWizard";
import { AppLayout } from "./components/AppLayout";
import { Library } from "./components/Library";
import { PlanDetail } from "./components/PlanDetail";
import { CurriculumPage } from "./components/curriculum/CurriculumPage";
import { ToastProvider } from "./components/Toast";
import { PrivacyConsentDialog } from "./components/PrivacyConsentDialog";
import { initSentry } from "./sentry";
import "./App.css";

type AppView = "loading" | "onboarding" | "app";

function App() {
  useErrorPipe();
  const [view, setView] = useState<AppView>("loading");
  const [showConsentDialog, setShowConsentDialog] = useState(false);

  useEffect(() => {
    let settled = false;

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

      // Check if onboarding was already completed
      try {
        const status = (await invoke("check_onboarding_status")) as {
          initial_digest_complete: boolean;
          tokens_stored: boolean;
          folder_selected: boolean;
        };

        if (
          status.initial_digest_complete &&
          status.tokens_stored &&
          status.folder_selected
        ) {
          settled = true;
          setView("app");
          return;
        }

        // Also check if we have valid auth via connector status
        try {
          const connections = await invoke<
            Array<{ auth_status: string }>
          >("get_connection_details");
          const hasValidAuth = connections.some(
            (c) => c.auth_status === "connected"
          );

          if (hasValidAuth && status.folder_selected) {
            settled = true;
            setView("app");
            return;
          }
        } catch {
          // get_connection_details not available, fall through
        }

        settled = true;
        setView("onboarding");
      } catch {
        settled = true;
        setView("onboarding");
      }
    }

    init();

    // Safety timeout: if backend never responds, fall through to onboarding
    // after 4 seconds so the user isn't stuck on a spinner forever.
    const timeout = setTimeout(() => {
      if (!settled) {
        settled = true;
        setView("onboarding");
      }
    }, 4000);

    return () => clearTimeout(timeout);
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
      <div className="h-screen chalk-bg flex items-center justify-center">
        <div className="spinner" />
      </div>
    );
  }

  return (
    <ToastProvider>
      {showConsentDialog && (
        <PrivacyConsentDialog onConsent={handleConsent} />
      )}

      {view === "onboarding" && (
        <OnboardingWizard onComplete={() => setView("app")} />
      )}

      {view === "app" && (
        <BrowserRouter>
          <Routes>
            <Route
              element={
                <AppLayout onReconnect={() => setView("onboarding")} />
              }
            >
              <Route path="/" element={<Library />} />
              <Route path="/curriculum" element={<CurriculumPage />} />
              <Route path="/plan/:planId" element={<PlanDetail />} />
            </Route>
          </Routes>
        </BrowserRouter>
      )}
    </ToastProvider>
  );
}

export default App;
