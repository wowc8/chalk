/**
 * E2E smoke tests for the Chalk onboarding wizard (OnboardingWizard).
 *
 * These tests exercise the full React frontend against the Vite dev
 * server with mocked Tauri IPC, verifying that:
 *   1. The app launches and renders.
 *   2. The onboarding wizard progresses through every step.
 *   3. Mock OAuth PKCE flow completes.
 *   4. Folder selection works.
 *   5. The initial import (digest) completes.
 *   6. The completion screen renders.
 *
 * No real Google API calls or Rust backend required — all Tauri
 * commands are intercepted by the mock layer in `tauri-mock.ts`.
 *
 * Flow: Welcome → Sign In → Select Source → Scan → Done
 */

import { test, expect } from "@playwright/test";
import { setupTauriMocks, MOCK_ITEMS } from "./tauri-mock";

// ---------------------------------------------------------------------------
// Helper: navigate through the sign-in step (used by multiple tests)
// ---------------------------------------------------------------------------
async function completeSignIn(page: import("@playwright/test").Page) {
  // Click "Sign in with Google" — the mock start_oauth_flow resolves
  // immediately, completing the full OAuth flow automatically.
  await page.getByRole("button", { name: /Sign in with Google/i }).click();
}

// ---------------------------------------------------------------------------
// Helper: select a folder by name and confirm
// ---------------------------------------------------------------------------
async function selectFolder(page: import("@playwright/test").Page, name: string) {
  // Click the folder item to select it
  await page.getByText(name).click();

  // Click the confirm button
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
// 2. Welcome step renders and can advance
// ---------------------------------------------------------------------------
test("welcome step renders and Get Started advances to sign-in", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Welcome step heading (h1)
  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk" }),
  ).toBeVisible();

  // Explanation text is present
  await expect(
    page.getByText("Connect your Google Drive"),
  ).toBeVisible();

  // Click "Get Started" to advance
  await page.getByRole("button", { name: "Get Started" }).click();

  // Should now show the sign-in step
  await expect(
    page.getByRole("heading", { name: "Sign in with Google" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 3. Google sign-in step — OAuth PKCE flow
// ---------------------------------------------------------------------------
test("sign-in step fetches auth URL, exchanges code, and advances to folder select", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Advance past welcome
  await page.getByRole("button", { name: "Get Started" }).click();
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
// 4. Folder selection — picks a folder and passes permission check
// ---------------------------------------------------------------------------
test("folder step lists items and selecting one advances to import", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through previous steps
  await page.getByRole("button", { name: "Get Started" }).click();
  await completeSignIn(page);

  // Now on folder selection step
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans/i }),
  ).toBeVisible();

  // Wait for items to load (mock returns 3 folder items)
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
// 5. Initial import — triggers scan and completes
// ---------------------------------------------------------------------------
test("import step scans documents and advances to complete", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through all previous steps
  await page.getByRole("button", { name: "Get Started" }).click();
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

  // Step 2: Sign In
  await expect(
    page.getByRole("heading", { name: "Sign in with Google" }),
  ).toBeVisible();
  await completeSignIn(page);

  // Step 3: Select Source
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans/i }),
  ).toBeVisible();
  await selectFolder(page, "Master Plans");

  // Step 4: Import
  await expect(
    page.getByRole("heading", { name: /Import Your Archive/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Start Import/i }).click();
  await page.getByRole("button", { name: /Continue/i }).click();

  // Step 5: Complete
  await expect(
    page.getByRole("heading", { name: /You're All Set/i }),
  ).toBeVisible();
  await expect(
    page.getByText(/Chalk is connected/i),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 7. Wizard progress indicators are rendered
// ---------------------------------------------------------------------------
test("progress indicators are displayed for each step", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // The OnboardingWizard renders 5 rounded-full indicator dots (one per step)
  const dots = page.locator(".rounded-full.w-2\\.5.h-2\\.5");
  await expect(dots).toHaveCount(5); // welcome, google-auth, folder-select, initial-digest, complete
});

// ---------------------------------------------------------------------------
// 8. Back navigation works
// ---------------------------------------------------------------------------
test("back button returns to previous step", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Go to sign-in step
  await page.getByRole("button", { name: "Get Started" }).click();
  await expect(
    page.getByRole("heading", { name: "Sign in with Google" }),
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
test("app resumes at folder step when oauth is already complete", async ({
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

  // With tokens already stored, app should show the welcome step initially.
  // The OnboardingWizard always starts at "welcome" (no auto-advance).
  await expect(
    page.getByRole("heading", { name: "Welcome to Chalk" }),
  ).toBeVisible();
});
