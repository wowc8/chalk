import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { StepWelcome } from "./StepWelcome";
import { StepGoogleAuth } from "./StepGoogleAuth";
import { StepFolderSelect } from "./StepFolderSelect";
import { StepInitialShred } from "./StepInitialShred";
import { StepComplete } from "./StepComplete";
import { BatmanOverlay } from "./BatmanOverlay";

export interface OnboardingStatus {
  oauth_configured: boolean;
  tokens_stored: boolean;
  folder_selected: boolean;
  folder_accessible: boolean;
  initial_shred_complete: boolean;
  selected_folder_id: string | null;
  selected_folder_name: string | null;
}

/** Default flow skips the manual OAuth config step — embedded credentials
 *  are used automatically via PKCE. The oauth-config step is kept in the
 *  array so users can still be routed there from Settings (advanced). */
const STEPS = [
  "welcome",
  "google-auth",
  "folder-select",
  "initial-shred",
  "complete",
] as const;

type Step = (typeof STEPS)[number];

const spring = { type: "spring" as const, stiffness: 300, damping: 30 };

export function OnboardingWizard({
  onComplete,
}: {
  onComplete: () => void;
}) {
  const [step, setStep] = useState<Step>("welcome");
  const [direction, setDirection] = useState(1);
  const [processing, setProcessing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke("initialize_oauth").catch(() => {});
  }, []);

  const goTo = (next: Step) => {
    const curIdx = STEPS.indexOf(step);
    const nextIdx = STEPS.indexOf(next);
    setDirection(nextIdx > curIdx ? 1 : -1);
    setError(null);
    setStep(next);
  };

  const stepIndex = STEPS.indexOf(step);

  const variants = {
    enter: (d: number) => ({ x: d > 0 ? 300 : -300, opacity: 0 }),
    center: { x: 0, opacity: 1 },
    exit: (d: number) => ({ x: d > 0 ? -300 : 300, opacity: 0 }),
  };

  return (
    <div className="min-h-screen bg-bat-dark text-white relative overflow-hidden">
      {/* Animated background grid */}
      <div className="absolute inset-0 opacity-5">
        <div
          className="w-full h-full"
          style={{
            backgroundImage:
              "linear-gradient(rgba(0,212,255,0.3) 1px, transparent 1px), linear-gradient(90deg, rgba(0,212,255,0.3) 1px, transparent 1px)",
            backgroundSize: "40px 40px",
          }}
        />
      </div>

      {/* Progress bar */}
      <div className="absolute top-0 left-0 right-0 h-1 bg-bat-charcoal z-20">
        <motion.div
          className="h-full bg-gradient-to-r from-bat-cyan to-bat-purple"
          animate={{ width: `${((stepIndex + 1) / STEPS.length) * 100}%` }}
          transition={spring}
        />
      </div>

      {/* Step indicators */}
      <div className="absolute top-6 left-1/2 -translate-x-1/2 flex gap-3 z-20">
        {STEPS.map((s, i) => (
          <div
            key={s}
            className={`w-2.5 h-2.5 rounded-full transition-colors duration-300 ${
              i <= stepIndex
                ? "bg-bat-cyan shadow-[0_0_8px_rgba(0,212,255,0.6)]"
                : "bg-bat-charcoal"
            }`}
          />
        ))}
      </div>

      {/* Batman overlay for processing states */}
      <AnimatePresence>{processing && <BatmanOverlay />}</AnimatePresence>

      {/* Step content */}
      <div className="relative z-10 flex items-center justify-center min-h-screen px-6">
        <AnimatePresence mode="wait" custom={direction}>
          <motion.div
            key={step}
            custom={direction}
            variants={variants}
            initial="enter"
            animate="center"
            exit="exit"
            transition={spring}
            className="w-full max-w-lg"
          >
            {error && (
              <motion.div
                initial={{ opacity: 0, y: -10 }}
                animate={{ opacity: 1, y: 0 }}
                className="mb-4 p-3 bg-bat-red/20 border border-bat-red/40 rounded-lg text-sm text-bat-red"
              >
                {error}
              </motion.div>
            )}

            {step === "welcome" && (
              <StepWelcome
                onNext={() => goTo("google-auth")}
                onSkip={onComplete}
              />
            )}
            {step === "google-auth" && (
              <StepGoogleAuth
                onNext={() => goTo("folder-select")}
                onBack={() => goTo("welcome")}
                setError={setError}
              />
            )}
            {step === "folder-select" && (
              <StepFolderSelect
                onNext={() => goTo("initial-shred")}
                onBack={() => goTo("google-auth")}
                setError={setError}
              />
            )}
            {step === "initial-shred" && (
              <StepInitialShred
                onNext={() => goTo("complete")}
                onBack={() => goTo("folder-select")}
                setError={setError}
                setProcessing={setProcessing}
              />
            )}
            {step === "complete" && (
              <StepComplete onFinish={onComplete} />
            )}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}
