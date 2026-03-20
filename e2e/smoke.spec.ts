/**
 * E2E smoke tests for the Chalk onboarding wizard (OnboardingWizard).
 *
 * These tests exercise the full React frontend against the Vite dev
 * server with mocked Tauri IPC, verifying that:
 *   1. The app launches and renders.
 *   2. The onboarding wizard progresses through every step.
 *   3. Schedule capture steps work (calendar, daily, specials, review).
 *   4. Mock OAuth PKCE flow completes.
 *   5. Folder selection works.
 *   6. The initial import (digest) completes.
 *   7. The completion screen renders.
 *
 * No real Google API calls or Rust backend required — all Tauri
 * commands are intercepted by the mock layer in `tauri-mock.ts`.
 *
 * Flow: Welcome → School Calendar → Daily Schedule → Weekly Specials
 *       → Schedule Review → Sign In → Select Source → Scan → Done
 */

import { test, expect } from "@playwright/test";
import { setupTauriMocks, MOCK_ITEMS } from "./tauri-mock";

// ---------------------------------------------------------------------------
// Helper: navigate through schedule steps (steps 2-5)
// ---------------------------------------------------------------------------
async function completeScheduleSteps(page: import("@playwright/test").Page) {
  // Step 2: School Calendar — click Next (dates optional)
  await expect(
    page.getByRole("heading", { name: "School Calendar" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Next" }).click();

  // Step 3: Daily Schedule — pick "I'll Type It Out", then Next
  await expect(
    page.getByRole("heading", { name: "Daily Schedule" }),
  ).toBeVisible();
  await page.getByText("I'll Type It Out").click();
  await page.getByRole("button", { name: "Next" }).click();

  // Step 4: Weekly Specials — click Next (no specials needed)
  await expect(
    page.getByRole("heading", { name: "Weekly Specials" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Next" }).click();

  // Step 5: Schedule Review — click "Looks Good!"
  await expect(
    page.getByRole("heading", { name: "Schedule Review" }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Looks Good/i }).click();
}

// ---------------------------------------------------------------------------
// Helper: navigate through the sign-in step
// ---------------------------------------------------------------------------
async function completeSignIn(page: import("@playwright/test").Page) {
  await page.getByRole("button", { name: /Sign in with Google/i }).click();
}

// ---------------------------------------------------------------------------
// Helper: select a folder by name and confirm
// ---------------------------------------------------------------------------
async function selectFolder(page: import("@playwright/test").Page, name: string) {
  await page.getByText(name).click();
  await page.getByRole("button", { name: /Select & Continue/i }).click();
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
// 3. Schedule steps flow through to Google sign-in
// ---------------------------------------------------------------------------
test("schedule steps advance through calendar, daily, specials, review to sign-in", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Advance past welcome
  await page.getByRole("button", { name: "Get Started" }).click();

  // Complete all schedule steps
  await completeScheduleSteps(page);

  // Should advance to sign-in
  await expect(
    page.getByRole("heading", { name: "Sign in with Google" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 4. Google sign-in step — OAuth PKCE flow
// ---------------------------------------------------------------------------
test("sign-in step fetches auth URL, exchanges code, and advances to folder select", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through welcome + schedule steps
  await page.getByRole("button", { name: "Get Started" }).click();
  await completeScheduleSteps(page);

  await expect(
    page.getByRole("heading", { name: "Sign in with Google" }),
  ).toBeVisible();

  // Complete the sign-in flow
  await completeSignIn(page);

  // Should advance to folder selection
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans/i }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 5. Folder selection — picks a folder and passes permission check
// ---------------------------------------------------------------------------
test("folder step lists items and selecting one advances to import", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through previous steps
  await page.getByRole("button", { name: "Get Started" }).click();
  await completeScheduleSteps(page);
  await completeSignIn(page);

  // Now on folder selection step
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans/i }),
  ).toBeVisible();

  // Wait for items to load
  for (const item of MOCK_ITEMS) {
    await expect(page.getByText(item.name)).toBeVisible();
  }

  // Select the "Master Plans" folder and confirm
  await selectFolder(page, "Master Plans");

  // Should advance to import step
  await expect(
    page.getByRole("heading", { name: /Import Your Archive/i }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 6. Initial import — triggers scan and completes
// ---------------------------------------------------------------------------
test("import step scans documents and advances to complete", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through all previous steps
  await page.getByRole("button", { name: "Get Started" }).click();
  await completeScheduleSteps(page);
  await completeSignIn(page);
  await selectFolder(page, "Master Plans");

  // Now on import step
  await expect(
    page.getByRole("heading", { name: /Import Your Archive/i }),
  ).toBeVisible();

  // Trigger the import
  await page.getByRole("button", { name: /Start Import/i }).click();

  // Wait for scan to complete and show Continue button
  await page.getByRole("button", { name: /Continue/i }).click();

  // Should advance to the complete step
  await expect(
    page.getByRole("heading", { name: /You're All Set/i }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 7. Full flow — end-to-end through all steps to completion
// ---------------------------------------------------------------------------
test("full onboarding flow reaches completion", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Step 1: Welcome
  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk" }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Get Started" }).click();

  // Steps 2-5: Schedule capture
  await completeScheduleSteps(page);

  // Step 6: Sign In
  await expect(
    page.getByRole("heading", { name: "Sign in with Google" }),
  ).toBeVisible();
  await completeSignIn(page);

  // Step 7: Select Source
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans/i }),
  ).toBeVisible();
  await selectFolder(page, "Master Plans");

  // Step 8: Import
  await expect(
    page.getByRole("heading", { name: /Import Your Archive/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Start Import/i }).click();
  await page.getByRole("button", { name: /Continue/i }).click();

  // Step 9: Complete
  await expect(
    page.getByRole("heading", { name: /You're All Set/i }),
  ).toBeVisible();
  await expect(
    page.getByText(/Chalk is connected/i),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 8. Wizard progress indicators are rendered (9 steps now)
// ---------------------------------------------------------------------------
test("progress indicators are displayed for each step", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // The OnboardingWizard renders 9 rounded-full indicator dots (one per step)
  const dots = page.locator(".rounded-full.w-2\\.5.h-2\\.5");
  await expect(dots).toHaveCount(9);
});

// ---------------------------------------------------------------------------
// 9. Back navigation works
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
// 10. Status auto-resume — app resumes at correct step from saved status
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
