/**
 * E2E smoke tests for the Chalk onboarding wizard (OnboardingWizard).
 *
 * These tests exercise the full React frontend against the Vite dev
 * server with mocked Tauri IPC, verifying that:
 *   1. The app launches and renders.
 *   2. The onboarding wizard progresses through every step.
 *   3. Google Auth + Import happen BEFORE schedule capture.
 *   4. AI-extracted schedule pre-fills the Daily Schedule step.
 *   5. Schedule capture steps work (daily, specials, review).
 *   6. The completion screen renders.
 *
 * No real Google API calls or Rust backend required — all Tauri
 * commands are intercepted by the mock layer in `tauri-mock.ts`.
 *
 * Flow: Welcome → School Calendar → AI Setup → Sign In → Select Source
 *       → Scan → Daily Schedule → Weekly Specials → Schedule Review → Done
 */

import { test, expect } from "@playwright/test";
import { setupTauriMocks, MOCK_ITEMS } from "./tauri-mock";

// ---------------------------------------------------------------------------
// Helper: navigate through AI Setup step (step 3)
// ---------------------------------------------------------------------------
async function completeAiSetupStep(page: import("@playwright/test").Page) {
  await expect(
    page.getByRole("heading", { name: "Power Up with AI" }),
  ).toBeVisible();
  // Skip for now (tests don't need a real key)
  await page.getByText(/Skip for now/i).click();
}

// ---------------------------------------------------------------------------
// Helper: navigate through sign-in + import steps (steps 4-6)
// ---------------------------------------------------------------------------
async function completeImportSteps(page: import("@playwright/test").Page) {
  // Step 4: Sign In
  await expect(
    page.getByRole("heading", { name: "Sign in with Google" }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Sign in with Google/i }).click();

  // Step 4: Select Source
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans/i }),
  ).toBeVisible();
  await page.getByText("Master Plans").click();
  await page.getByRole("button", { name: /Select & Continue/i }).click();

  // Step 5: Import
  await expect(
    page.getByRole("heading", { name: /Import Your Archive/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Start Import/i }).click();
  await page.getByRole("button", { name: /Continue/i }).click();
}

// ---------------------------------------------------------------------------
// Helper: navigate through schedule steps (steps 6-8)
// ---------------------------------------------------------------------------
async function completeScheduleSteps(page: import("@playwright/test").Page) {
  // Step 6: Daily Schedule — pick "I'll Type It Out", then Next
  await expect(
    page.getByRole("heading", { name: "Daily Schedule" }),
  ).toBeVisible();
  // May show confirmation or method picker depending on whether events were extracted
  const typeItOut = page.getByText("I'll Type It Out");
  if (await typeItOut.isVisible().catch(() => false)) {
    await typeItOut.click();
  }
  // Click Next or Looks Good depending on mode
  const looksGood = page.getByRole("button", { name: /Looks Good/i });
  const next = page.getByRole("button", { name: "Next" });
  if (await looksGood.isVisible().catch(() => false)) {
    await looksGood.click();
  } else {
    await next.click();
  }

  // Step 7: Weekly Specials — click Next (no specials needed)
  await expect(
    page.getByRole("heading", { name: "Weekly Specials" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Next" }).click();

  // Step 8: Schedule Review — click "Looks Good!"
  await expect(
    page.getByRole("heading", { name: "Schedule Review" }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Looks Good/i }).click();
}

// ---------------------------------------------------------------------------
// 1. App launches and renders
// ---------------------------------------------------------------------------
test("app launches and shows the welcome heading", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk", level: 1 }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 2. Welcome step renders and can advance to School Calendar
// ---------------------------------------------------------------------------
test("welcome step renders and Get Started advances to school calendar", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk" }),
  ).toBeVisible();

  // New welcome text mentions classroom/schedule
  await expect(
    page.getByText("schedule"),
  ).toBeVisible();

  // Click "Get Started" to advance
  await page.getByRole("button", { name: "Get Started" }).click();

  // Should now show the school calendar step
  await expect(
    page.getByRole("heading", { name: "School Calendar" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 3. Calendar advances to AI Setup (new order)
// ---------------------------------------------------------------------------
test("school calendar advances to AI setup", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Advance past welcome
  await page.getByRole("button", { name: "Get Started" }).click();

  // Step 2: School Calendar — click Next
  await expect(
    page.getByRole("heading", { name: "School Calendar" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Next" }).click();

  // Should advance to AI Setup
  await expect(
    page.getByRole("heading", { name: "Power Up with AI" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 4. Import step advances to Daily Schedule
// ---------------------------------------------------------------------------
test("import step advances to daily schedule after scan", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through welcome + calendar + AI setup
  await page.getByRole("button", { name: "Get Started" }).click();
  await page.getByRole("button", { name: "Next" }).click();
  await completeAiSetupStep(page);

  // Complete import steps
  await completeImportSteps(page);

  // Should now be on Daily Schedule (not Complete)
  await expect(
    page.getByRole("heading", { name: "Daily Schedule" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 5. Daily Schedule shows pre-filled confirmation with extracted events
// ---------------------------------------------------------------------------
test("daily schedule shows pre-filled events from LTP extraction", async ({
  page,
}) => {
  await setupTauriMocks(page, {
    extractedEvents: [
      { id: "ext-0", name: "Lunch", event_type: "fixed", occurrences: [
        { day_of_week: 0, start_time: "11:30", end_time: "12:00" },
        { day_of_week: 1, start_time: "11:30", end_time: "12:00" },
        { day_of_week: 2, start_time: "11:30", end_time: "12:00" },
        { day_of_week: 3, start_time: "11:30", end_time: "12:00" },
        { day_of_week: 4, start_time: "11:30", end_time: "12:00" },
      ]},
      { id: "ext-1", name: "Recess", event_type: "fixed", occurrences: [
        { day_of_week: 0, start_time: "10:00", end_time: "10:20" },
        { day_of_week: 1, start_time: "10:00", end_time: "10:20" },
        { day_of_week: 2, start_time: "10:00", end_time: "10:20" },
        { day_of_week: 3, start_time: "10:00", end_time: "10:20" },
        { day_of_week: 4, start_time: "10:00", end_time: "10:20" },
      ]},
    ],
  });
  await page.goto("/");

  // Navigate through welcome + calendar + AI setup + import
  await page.getByRole("button", { name: "Get Started" }).click();
  await page.getByRole("button", { name: "Next" }).click();
  await completeAiSetupStep(page);
  await completeImportSteps(page);

  // Should show pre-filled confirmation
  await expect(
    page.getByRole("heading", { name: "Daily Schedule" }),
  ).toBeVisible();
  await expect(page.getByText("what we figured out")).toBeVisible();
  await expect(page.getByText("Lunch")).toBeVisible();
  await expect(page.getByText("Recess")).toBeVisible();

  // Confirm with "Looks Good!"
  await page.getByRole("button", { name: /Looks Good/i }).click();

  // Should advance to Weekly Specials
  await expect(
    page.getByRole("heading", { name: "Weekly Specials" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 6. Full flow — end-to-end through all steps to completion
// ---------------------------------------------------------------------------
test("full onboarding flow reaches completion", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Step 1: Welcome
  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Get Started" }).click();

  // Step 2: School Calendar
  await expect(
    page.getByRole("heading", { name: "School Calendar" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Next" }).click();

  // Step 3: AI Setup
  await completeAiSetupStep(page);

  // Steps 4-6: Sign In + Select Source + Import
  await completeImportSteps(page);

  // Steps 7-9: Daily Schedule + Specials + Review
  await completeScheduleSteps(page);

  // Step 10: Complete
  await expect(
    page.getByRole("heading", { name: /You're All Set/i }),
  ).toBeVisible();
  await expect(
    page.getByText(/Chalk is connected/i),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 7. Wizard progress indicators are rendered (10 steps)
// ---------------------------------------------------------------------------
test("progress indicators are displayed for each step", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // The OnboardingWizard renders 10 rounded-full indicator dots (one per step)
  const dots = page.locator(".rounded-full.w-2\\.5.h-2\\.5");
  await expect(dots).toHaveCount(10);
});

// ---------------------------------------------------------------------------
// 8. Back navigation works
// ---------------------------------------------------------------------------
test("back button returns to previous step", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Go to school calendar step
  await page.getByRole("button", { name: "Get Started" }).click();
  await expect(
    page.getByRole("heading", { name: "School Calendar" }),
  ).toBeVisible();

  // Go back to welcome
  await page.getByRole("button", { name: "Back" }).click();
  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 9. Status auto-resume — app resumes at correct step from saved status
// ---------------------------------------------------------------------------
test("app resumes at welcome step when oauth is already complete", async ({
  page,
}) => {
  await setupTauriMocks(page, {
    initialStatus: {
      oauth_configured: true,
      tokens_stored: true,
      folder_selected: false,
      folder_accessible: false,
      initial_digest_complete: false,
      selected_folder_id: null,
      selected_folder_name: null,
    },
  });
  await page.goto("/");

  // OnboardingWizard always starts at "welcome" (no auto-advance).
  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk" }),
  ).toBeVisible();
});
