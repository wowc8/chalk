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

/** Mock drive items (folders + documents) returned by `list_drive_items`. */
export const MOCK_ITEMS = [
  {
    id: "folder-1",
    name: "Lesson Plans 2024",
    mime_type: "application/vnd.google-apps.folder",
    is_folder: true,
  },
  {
    id: "folder-2",
    name: "English Department",
    mime_type: "application/vnd.google-apps.folder",
    is_folder: true,
  },
  {
    id: "folder-3",
    name: "Master Plans",
    mime_type: "application/vnd.google-apps.folder",
    is_folder: true,
  },
];

/** Default onboarding status (fresh start — nothing completed). */
export const FRESH_STATUS = {
  oauth_configured: false,
  tokens_stored: false,
  folder_selected: false,
  folder_accessible: false,
  initial_digest_complete: false,
  selected_folder_id: null,
  selected_folder_name: null,
};

/** Draft event shape for extracted schedule events. */
interface MockDraftEvent {
  id: string;
  name: string;
  event_type: string;
  occurrences: { day_of_week: number; start_time: string; end_time: string }[];
}

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
    /** Items returned by `list_drive_items`. */
    items?: typeof MOCK_ITEMS;
    /** Folders returned by `list_drive_folders` (fallback). */
    folders?: typeof MOCK_FOLDERS;
    /** Message returned by `trigger_initial_digest`. */
    digestMessage?: string;
    /** Whether `test_folder_permissions_command` returns true. */
    folderAccessible?: boolean;
    /** Extracted schedule events returned by `extract_schedule_from_imports`. */
    extractedEvents?: MockDraftEvent[];
  } = {},
) {
  const status = opts.initialStatus ?? { ...FRESH_STATUS };
  const items = opts.items ?? MOCK_ITEMS;
  const folders = opts.folders ?? MOCK_FOLDERS;
  const digestMessage =
    opts.digestMessage ?? "Found 7 documents with 14 lesson plans in 'Master Plans'.";
  const folderAccessible = opts.folderAccessible ?? true;
  const extractedEvents = opts.extractedEvents ?? [];

  await page.addInitScript(
    ({
      status,
      items,
      folders,
      digestMessage,
      folderAccessible,
      extractedEvents,
    }: {
      status: typeof FRESH_STATUS;
      items: typeof MOCK_ITEMS;
      folders: typeof MOCK_FOLDERS;
      digestMessage: string;
      folderAccessible: boolean;
      extractedEvents: MockDraftEvent[];
    }) => {
      // Track mutable status so subsequent calls reflect wizard progress.
      const onboardingStatus = { ...status };

      // Auto-incrementing ID for created entities
      let nextId = 1;

      // Map of Tauri command name → handler returning a JSON-serialisable value.
      const handlers: Record<string, (args: Record<string, unknown>) => unknown> = {
        initialize_oauth: () => "OAuth initialized (mock)",
        log_frontend_error: () => null,
        has_embedded_credentials: () => true,

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

        start_oauth_flow: () => {
          onboardingStatus.oauth_configured = true;
          onboardingStatus.tokens_stored = true;
          return "Authentication successful";
        },

        list_drive_items: () => items,
        list_drive_folders: () => folders,
        list_drive_subfolders: () => [],

        test_folder_permissions_command: (args: Record<string, unknown>) => {
          if (folderAccessible) {
            onboardingStatus.folder_selected = true;
            onboardingStatus.folder_accessible = true;
            onboardingStatus.selected_folder_id = (args.folderId as string) ?? null;
            onboardingStatus.selected_folder_name = (args.folderName as string) ?? null;
          }
          return folderAccessible;
        },

        select_single_document: (args: Record<string, unknown>) => {
          if (folderAccessible) {
            onboardingStatus.folder_selected = true;
            onboardingStatus.folder_accessible = true;
            onboardingStatus.selected_folder_id = (args.docId as string) ?? null;
            onboardingStatus.selected_folder_name = (args.docName as string) ?? null;
          }
          return folderAccessible;
        },

        trigger_initial_digest: () => {
          onboardingStatus.initial_digest_complete = true;
          return digestMessage;
        },

        // AI schedule extraction from imported LTPs
        extract_schedule_from_imports: () => extractedEvents,

        // --- Schedule / Calendar commands ---

        get_app_setting: () => null,
        set_app_setting: () => null,

        get_school_calendar: () => null,
        update_school_calendar: () => ({
          id: `cal-${nextId++}`,
          year_start: "2025-08-14",
          year_end: "2026-05-30",
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        }),

        add_calendar_exception: (args: Record<string, unknown>) => ({
          id: `exc-${nextId++}`,
          calendar_id: args.input ? (args.input as any).calendar_id : "cal-1",
          date: args.input ? (args.input as any).date : "2025-12-23",
          exception_type: args.input ? (args.input as any).exception_type : "no_school",
          label: args.input ? (args.input as any).label : "",
        }),
        delete_calendar_exception: () => null,
        list_calendar_exceptions: () => [],

        get_recurring_events: () => [],
        create_recurring_event: (args: Record<string, unknown>) => ({
          id: `evt-${nextId++}`,
          name: args.input ? (args.input as any).name : "Event",
          event_type: args.input ? (args.input as any).event_type : "fixed",
          linked_to: null,
          details_vary_daily: false,
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString(),
        }),
        update_recurring_event: () => null,
        delete_recurring_event: () => null,

        create_event_occurrence: (args: Record<string, unknown>) => ({
          id: `occ-${nextId++}`,
          event_id: args.input ? (args.input as any).event_id : "evt-1",
          day_of_week: args.input ? (args.input as any).day_of_week : 0,
          start_time: args.input ? (args.input as any).start_time : "08:00",
          end_time: args.input ? (args.input as any).end_time : "08:30",
        }),
        list_event_occurrences: () => [],
        delete_event_occurrence: () => null,
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
          // Handle Tauri event plugin commands (listen/unlisten/emit) as no-ops.
          if (cmd.startsWith("plugin:event|")) {
            return Promise.resolve(0);
          }

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
    { status, items, folders, digestMessage, folderAccessible, extractedEvents },
  );
}
