/**
 * Mock Tauri IPC responses for E2E smoke tests.
 *
 * This module provides a `setupTauriMocks` helper that injects a fake
 * `window.__TAURI_INTERNALS__` object into the page so that the
 * `@tauri-apps/api/core` `invoke()` function resolves with controlled
 * data instead of trying to talk to the Rust backend.
 */

import type { Page } from "@playwright/test";

/** Mock Google Drive folders returned by `list_drive_folders`. */
export const MOCK_FOLDERS = [
  {
    id: "folder-1",
    name: "Lesson Plans 2024",
    mime_type: "application/vnd.google-apps.folder",
  },
  {
    id: "folder-2",
    name: "English Department",
    mime_type: "application/vnd.google-apps.folder",
  },
  {
    id: "folder-3",
    name: "Master Plans",
    mime_type: "application/vnd.google-apps.folder",
  },
];

/** Default onboarding status (fresh start — nothing completed). */
export const FRESH_STATUS = {
  oauth_configured: false,
  tokens_stored: false,
  folder_selected: false,
  folder_accessible: false,
  initial_shred_complete: false,
  selected_folder_id: null,
  selected_folder_name: null,
};

/**
 * Inject Tauri IPC mocks into the page.
 *
 * The mocks intercept every `invoke()` call and return canned responses
 * that move the onboarding wizard forward without hitting any real APIs.
 *
 * @param page  Playwright page object.
 * @param opts  Override individual command responses if needed.
 */
export async function setupTauriMocks(
  page: Page,
  opts: {
    /** Status returned by `check_onboarding_status`. Mutated as the wizard progresses. */
    initialStatus?: typeof FRESH_STATUS;
    /** Folders returned by `list_drive_folders`. */
    folders?: typeof MOCK_FOLDERS;
    /** Message returned by `trigger_initial_shred`. */
    shredMessage?: string;
    /** Whether `test_folder_permissions_command` returns true. */
    folderAccessible?: boolean;
  } = {},
) {
  const status = opts.initialStatus ?? { ...FRESH_STATUS };
  const folders = opts.folders ?? MOCK_FOLDERS;
  const shredMessage =
    opts.shredMessage ?? "Found 7 documents with 14 lesson plans in 'Master Plans'.";
  const folderAccessible = opts.folderAccessible ?? true;

  await page.addInitScript(
    ({
      status,
      folders,
      shredMessage,
      folderAccessible,
    }: {
      status: typeof FRESH_STATUS;
      folders: typeof MOCK_FOLDERS;
      shredMessage: string;
      folderAccessible: boolean;
    }) => {
      // Track mutable status so subsequent calls reflect wizard progress.
      const onboardingStatus = { ...status };

      // Map of Tauri command name → handler returning a JSON-serialisable value.
      const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
        initialize_oauth: () => "OAuth initialized (mock)",
        log_frontend_error: () => null,

        check_onboarding_status: () => ({ ...onboardingStatus }),

        save_oauth_config: () => {
          onboardingStatus.oauth_configured = true;
          return "Config saved (mock)";
        },

        get_authorization_url: () =>
          "https://accounts.google.com/o/oauth2/v2/auth?mock=true",

        handle_oauth_callback: () => {
          onboardingStatus.tokens_stored = true;
          return "Tokens stored (mock)";
        },

        list_drive_folders: () => folders,

        test_folder_permissions_command: (args: Record<string, unknown>) => {
          if (folderAccessible) {
            onboardingStatus.folder_selected = true;
            onboardingStatus.folder_accessible = true;
            onboardingStatus.selected_folder_id = (args.folderId as string) ?? null;
            onboardingStatus.selected_folder_name = (args.folderName as string) ?? null;
          }
          return folderAccessible;
        },

        trigger_initial_shred: () => {
          onboardingStatus.initial_shred_complete = true;
          return shredMessage;
        },
      };

      // Expose the mock IPC handler that @tauri-apps/api/core reads.
      // The Tauri JS runtime resolves invoke() via this internal hook.
      (window as any).__TAURI_INTERNALS__ = {
        transformCallback: (cb: (...args: unknown[]) => void) => {
          const id = Math.random();
          (window as any)[`_${id}`] = cb;
          return id;
        },
        invoke: (cmd: string, args: Record<string, unknown> = {}) => {
          const handler = handlers[cmd];
          if (!handler) {
            console.warn(`[tauri-mock] unhandled command: ${cmd}`, args);
            return Promise.reject(`Unhandled mock command: ${cmd}`);
          }
          try {
            return Promise.resolve(handler(args));
          } catch (err) {
            return Promise.reject(err);
          }
        },
        metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
      };
    },
    { status, folders, shredMessage, folderAccessible },
  );
}
