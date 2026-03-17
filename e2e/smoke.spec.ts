/**
 * E2E smoke tests for the Chalk onboarding wizard (AdminWizard).
 *
 * These tests exercise the full React frontend against the Vite dev
 * server with mocked Tauri IPC, verifying that:
 *   1. The app launches and renders.
 *   2. The onboarding wizard progresses through every step.
 *   3. Mock OAuth flow completes.
 *   4. Folder selection works.
 *   5. The initial scan (shred) completes.
 *   6. The completion screen renders.
 *
 * No real Google API calls or Rust backend required — all Tauri
 * commands are intercepted by the mock layer in `tauri-mock.ts`.
 */

import { test, expect } from "@playwright/test";
import { setupTauriMocks, MOCK_FOLDERS } from "./tauri-mock";

// ---------------------------------------------------------------------------
// 1. App launches and renders
// ---------------------------------------------------------------------------
test("app launches and shows the wizard header", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  await expect(page.getByRole("heading", { name: "Chalk Setup", level: 1 })).toBeVisible();
});

// ---------------------------------------------------------------------------
// 2. Welcome step renders and can advance
// ---------------------------------------------------------------------------
test("welcome step renders and Get Started advances to credentials", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Welcome step heading (h2 inside the wizard)
  await expect(page.getByRole("heading", { name: "Welcome to Chalk" })).toBeVisible();

  // Explanation text is present
  await expect(
    page.getByText("Connect your Google account"),
  ).toBeVisible();

  // Click "Get Started" to advance
  await page.getByRole("button", { name: "Get Started" }).click();

  // Should now show the credentials step
  await expect(
    page.getByRole("heading", { name: "Google API Credentials" }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 3. OAuth config step — save credentials
// ---------------------------------------------------------------------------
test("credentials step saves config and advances to authorize", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Advance past welcome
  await page.getByRole("button", { name: "Get Started" }).click();
  await expect(
    page.getByRole("heading", { name: "Google API Credentials" }),
  ).toBeVisible();

  // Fill in the form
  await page
    .getByRole("textbox", { name: /client id/i })
    .fill("test-client-id.apps.googleusercontent.com");
  await page
    .locator('input[type="password"]')
    .fill("GOCSPX-test-secret");

  // Submit
  await page.getByRole("button", { name: /Save & Continue/i }).click();

  // Should advance to authorize step
  await expect(
    page.getByRole("heading", { name: /Authorize Google Access/i }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 4. Google auth step — mock OAuth flow completes
// ---------------------------------------------------------------------------
test("authorize step exchanges code and advances to folder select", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate: welcome → credentials → authorize
  await page.getByRole("button", { name: "Get Started" }).click();
  await page
    .getByRole("textbox", { name: /client id/i })
    .fill("test-client-id.apps.googleusercontent.com");
  await page.locator('input[type="password"]').fill("GOCSPX-test-secret");
  await page.getByRole("button", { name: /Save & Continue/i }).click();

  await expect(
    page.getByRole("heading", { name: /Authorize Google Access/i }),
  ).toBeVisible();

  // The authorize step auto-fetches the auth URL and shows the link
  await expect(page.getByText(/Open Google Sign-In/i)).toBeVisible();

  // Fill in the authorization code
  await page
    .getByRole("textbox", { name: /authorization code/i })
    .fill("mock-auth-code-123");

  // Submit the code
  await page.getByRole("button", { name: /Submit Code/i }).click();

  // Should advance to folder selection
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans Folder/i }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 5. Folder selection — picks a folder and passes permission check
// ---------------------------------------------------------------------------
test("folder step lists folders and selecting one advances to shred", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through previous steps
  await page.getByRole("button", { name: "Get Started" }).click();
  await page
    .getByRole("textbox", { name: /client id/i })
    .fill("test-client-id.apps.googleusercontent.com");
  await page.locator('input[type="password"]').fill("GOCSPX-test-secret");
  await page.getByRole("button", { name: /Save & Continue/i }).click();
  await page
    .getByRole("textbox", { name: /authorization code/i })
    .fill("mock-auth-code-123");
  await page.getByRole("button", { name: /Submit Code/i }).click();

  // Now on folder selection step
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans Folder/i }),
  ).toBeVisible();

  // Wait for folders to load (mock returns 3 folders)
  for (const folder of MOCK_FOLDERS) {
    await expect(page.getByRole("button", { name: folder.name })).toBeVisible();
  }

  // Select the "Master Plans" folder
  await page.getByRole("button", { name: "Master Plans" }).click();

  // Should advance to shred step
  await expect(
    page.getByRole("heading", { name: /Scan Your Documents/i }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 6. Initial shred — triggers scan and completes
// ---------------------------------------------------------------------------
test("shred step scans documents and advances to complete", async ({
  page,
}) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Navigate through all previous steps
  await page.getByRole("button", { name: "Get Started" }).click();
  await page
    .getByRole("textbox", { name: /client id/i })
    .fill("test-client-id.apps.googleusercontent.com");
  await page.locator('input[type="password"]').fill("GOCSPX-test-secret");
  await page.getByRole("button", { name: /Save & Continue/i }).click();
  await page
    .getByRole("textbox", { name: /authorization code/i })
    .fill("mock-auth-code-123");
  await page.getByRole("button", { name: /Submit Code/i }).click();
  await page.getByRole("button", { name: "Master Plans" }).click();

  // Now on shred step
  await expect(
    page.getByRole("heading", { name: /Scan Your Documents/i }),
  ).toBeVisible();

  // Trigger the scan
  await page.getByRole("button", { name: /Start Scan/i }).click();

  // Should advance to the complete step
  await expect(
    page.getByRole("heading", { name: /Setup Complete/i }),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 7. Full flow — end-to-end through all steps to completion
// ---------------------------------------------------------------------------
test("full onboarding flow reaches completion", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // Step 1: Welcome
  await expect(page.getByRole("heading", { name: "Chalk Setup" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Welcome to Chalk" })).toBeVisible();
  await page.getByRole("button", { name: "Get Started" }).click();

  // Step 2: Credentials
  await expect(
    page.getByRole("heading", { name: "Google API Credentials" }),
  ).toBeVisible();
  await page
    .getByRole("textbox", { name: /client id/i })
    .fill("test-client-id.apps.googleusercontent.com");
  await page.locator('input[type="password"]').fill("GOCSPX-test-secret");
  await page.getByRole("button", { name: /Save & Continue/i }).click();

  // Step 3: Authorize
  await expect(
    page.getByRole("heading", { name: /Authorize Google Access/i }),
  ).toBeVisible();
  await page
    .getByRole("textbox", { name: /authorization code/i })
    .fill("mock-auth-code-123");
  await page.getByRole("button", { name: /Submit Code/i }).click();

  // Step 4: Folder select
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans Folder/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Master Plans" }).click();

  // Step 5: Shred
  await expect(
    page.getByRole("heading", { name: /Scan Your Documents/i }),
  ).toBeVisible();
  await page.getByRole("button", { name: /Start Scan/i }).click();

  // Step 6: Complete
  await expect(
    page.getByRole("heading", { name: /Setup Complete/i }),
  ).toBeVisible();
  await expect(
    page.getByText(/connected to your Google Drive/i),
  ).toBeVisible();
});

// ---------------------------------------------------------------------------
// 8. Wizard progress dots are rendered
// ---------------------------------------------------------------------------
test("progress dots are displayed for each step", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  // The AdminWizard renders progress-dot spans for each step
  const dots = page.locator(".progress-dot");
  await expect(dots).toHaveCount(6); // welcome, credentials, authorize, folder, shred, complete
});

// ---------------------------------------------------------------------------
// 9. Error state — empty credentials prevented by form validation
// ---------------------------------------------------------------------------
test("credentials form requires both fields", async ({ page }) => {
  await setupTauriMocks(page);
  await page.goto("/");

  await page.getByRole("button", { name: "Get Started" }).click();

  // Both inputs should have the required attribute
  const clientIdInput = page.getByRole("textbox", { name: /client id/i });
  await expect(clientIdInput).toHaveAttribute("required", "");

  const clientSecretInput = page.locator('input[type="password"]');
  await expect(clientSecretInput).toHaveAttribute("required", "");
});

// ---------------------------------------------------------------------------
// 10. Status auto-resume — app resumes at correct step from saved status
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
      initial_shred_complete: false,
      selected_folder_id: null,
      selected_folder_name: null,
    },
  });
  await page.goto("/");

  // The useAdminSetup hook auto-advances to the folder step
  // since tokens_stored is true.
  await expect(
    page.getByRole("heading", { name: /Select Your Lesson Plans Folder/i }),
  ).toBeVisible();
});
