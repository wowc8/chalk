import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface OnboardingStatus {
  oauth_configured: boolean;
  tokens_stored: boolean;
  folder_selected: boolean;
  folder_accessible: boolean;
  initial_shred_complete: boolean;
  selected_folder_id: string | null;
  selected_folder_name: string | null;
}

export interface DriveFolder {
  id: string;
  name: string;
  mime_type: string;
}

export type SetupStep =
  | "welcome"
  | "credentials"
  | "authorize"
  | "folder"
  | "shred"
  | "complete";

export function useAdminSetup() {
  const [step, setStep] = useState<SetupStep>("welcome");
  const [status, setStatus] = useState<OnboardingStatus | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      const s = await invoke<OnboardingStatus>("check_onboarding_status");
      setStatus(s);

      // Auto-advance to the correct step based on status.
      if (s.initial_shred_complete) {
        setStep("complete");
      } else if (s.folder_selected && s.folder_accessible) {
        setStep("shred");
      } else if (s.tokens_stored) {
        setStep("folder");
      } else if (s.oauth_configured) {
        setStep("authorize");
      }
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    invoke<string>("initialize_oauth").then(() => refreshStatus());
  }, [refreshStatus]);

  const saveCredentials = useCallback(
    async (clientId: string, clientSecret: string) => {
      setLoading(true);
      setError(null);
      try {
        await invoke<string>("save_oauth_config", {
          clientId,
          clientSecret,
        });
        await refreshStatus();
        setStep("authorize");
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [refreshStatus],
  );

  const getAuthUrl = useCallback(async (): Promise<string | null> => {
    setError(null);
    try {
      return await invoke<string>("get_authorization_url");
    } catch (e) {
      setError(String(e));
      return null;
    }
  }, []);

  const submitAuthCode = useCallback(
    async (code: string) => {
      setLoading(true);
      setError(null);
      try {
        await invoke<string>("handle_oauth_callback", { code });
        await refreshStatus();
        setStep("folder");
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [refreshStatus],
  );

  const listFolders = useCallback(async (): Promise<DriveFolder[]> => {
    setError(null);
    try {
      return await invoke<DriveFolder[]>("list_drive_folders");
    } catch (e) {
      setError(String(e));
      return [];
    }
  }, []);

  const selectFolder = useCallback(
    async (folderId: string, folderName: string) => {
      setLoading(true);
      setError(null);
      try {
        const accessible = await invoke<boolean>(
          "test_folder_permissions_command",
          {
            folderId,
            folderName,
          },
        );
        await refreshStatus();
        if (accessible) {
          setStep("shred");
        } else {
          setError("Cannot access the selected folder. Check permissions.");
        }
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    [refreshStatus],
  );

  const triggerShred = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await invoke<string>("trigger_initial_shred");
      await refreshStatus();
      setStep("complete");
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [refreshStatus]);

  return {
    step,
    setStep,
    status,
    error,
    loading,
    saveCredentials,
    getAuthUrl,
    submitAuthCode,
    listFolders,
    selectFolder,
    triggerShred,
    refreshStatus,
  };
}
