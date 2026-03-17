import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

interface UpdateStatus {
  available: boolean;
  current_version: string;
  latest_version: string | null;
  body: string | null;
}

export function UpdateBanner() {
  const [update, setUpdate] = useState<UpdateStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<string>("");
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    // Check on mount.
    invoke<UpdateStatus>("check_for_update")
      .then((status) => {
        if (status.available) setUpdate(status);
      })
      .catch(() => {});

    // Listen for periodic update events from the backend.
    const unlisten = listen<UpdateStatus>("update-available", (event) => {
      if (event.payload.available) {
        setUpdate(event.payload);
        setDismissed(false);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  if (!update?.available || dismissed) return null;

  const handleInstall = async () => {
    setInstalling(true);
    setProgress("Downloading update...");
    try {
      await invoke("install_update");
    } catch (e) {
      setProgress(`Update failed: ${e}`);
      setInstalling(false);
    }
  };

  return (
    <div className="fixed top-0 left-0 right-0 z-50 bg-chalk-blue/10 border-b border-chalk-blue/20 backdrop-blur-sm px-4 py-2 flex items-center justify-between text-sm">
      <div className="flex items-center gap-2">
        <span className="font-medium text-chalk-blue">
          Update available: v{update.latest_version}
        </span>
        {update.body && (
          <span className="text-white/60 hidden sm:inline">
            — {update.body}
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        {installing ? (
          <span className="text-white/60">{progress || "Installing..."}</span>
        ) : (
          <>
            <button
              onClick={handleInstall}
              className="px-3 py-1 bg-chalk-blue/20 hover:bg-chalk-blue/30 text-chalk-blue rounded-md transition-colors cursor-pointer"
            >
              Install & Restart
            </button>
            <button
              onClick={() => setDismissed(true)}
              className="px-2 py-1 text-white/40 hover:text-white/60 transition-colors cursor-pointer"
            >
              ✕
            </button>
          </>
        )}
      </div>
    </div>
  );
}
