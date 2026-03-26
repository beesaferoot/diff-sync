import { test, expect, type Page, type BrowserContext } from "@playwright/test";

// ─── Helpers ─────────────────────────────────────────────────────────────────

async function waitForConnection(page: Page) {
  // Use exact match to avoid matching textarea content that might contain "Connected"
  await expect(page.getByText("Connected", { exact: true })).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.locator("textarea")).toBeEnabled();
}

async function getTextareaValue(page: Page): Promise<string> {
  return page.locator("textarea").inputValue();
}

async function clearAndType(page: Page, text: string) {
  const textarea = page.locator("textarea");
  await textarea.fill(text);
}

// ─── Tests ───────────────────────────────────────────────────────────────────

test.describe("Collaborative Editor", () => {
  test("single client connects and sees document", async ({ page }) => {
    await page.goto("/");

    // Should show connecting state initially
    await expect(page.locator("textarea")).toBeVisible();

    // Wait for connection
    await waitForConnection(page);

    // Textarea should have content from server (default document)
    const value = await getTextareaValue(page);
    expect(value.length).toBeGreaterThan(0);

    // Server version should be displayed
    await expect(page.getByText(/Server v\d+/)).toBeVisible();

    // Client ID should be displayed
    await expect(page.locator("span.font-mono")).toBeVisible();
  });

  test("single client can edit and character count updates", async ({
    page,
  }) => {
    await page.goto("/");
    await waitForConnection(page);

    const testText = "Hello from the test harness!";
    await clearAndType(page, testText);

    // Character count should update
    await expect(
      page.getByText(`${testText.length} characters`)
    ).toBeVisible();
  });

  test("two clients see each other's edits", async ({ browser }) => {
    // Create two independent browser contexts (two separate users)
    const contextA = await browser.newContext();
    const contextB = await browser.newContext();
    const pageA = await contextA.newPage();
    const pageB = await contextB.newPage();

    try {
      // Both connect
      await pageA.goto("/");
      await pageB.goto("/");
      await waitForConnection(pageA);
      await waitForConnection(pageB);

      // User A types
      const messageFromA = `Hello from User A ${Date.now()}`;
      await clearAndType(pageA, messageFromA);

      // User B should see User A's text (within a few sync cycles)
      await expect(pageB.locator("textarea")).toHaveValue(messageFromA, {
        timeout: 5_000,
      });
    } finally {
      await contextA.close();
      await contextB.close();
    }
  });

  test("bidirectional sync works", async ({ browser }) => {
    const contextA = await browser.newContext();
    const contextB = await browser.newContext();
    const pageA = await contextA.newPage();
    const pageB = await contextB.newPage();

    try {
      await pageA.goto("/");
      await pageB.goto("/");
      await waitForConnection(pageA);
      await waitForConnection(pageB);

      // User A types first
      const textA = `From A: ${Date.now()}`;
      await clearAndType(pageA, textA);

      // Wait for sync to B
      await expect(pageB.locator("textarea")).toHaveValue(textA, {
        timeout: 5_000,
      });

      // User B overwrites with new text
      const textB = `From B: ${Date.now()}`;
      await clearAndType(pageB, textB);

      // Wait for sync back to A
      await expect(pageA.locator("textarea")).toHaveValue(textB, {
        timeout: 5_000,
      });

      // Both should have identical content
      const valueA = await getTextareaValue(pageA);
      const valueB = await getTextareaValue(pageB);
      expect(valueA).toBe(valueB);
    } finally {
      await contextA.close();
      await contextB.close();
    }
  });

  test("new client receives existing document state", async ({ browser }) => {
    const contextA = await browser.newContext();
    const pageA = await contextA.newPage();

    try {
      // User A connects and edits
      await pageA.goto("/");
      await waitForConnection(pageA);

      const setupText = `Existing content ${Date.now()}`;
      await clearAndType(pageA, setupText);

      // Wait for the edit to reach the server (at least one sync cycle)
      await pageA.waitForTimeout(1500);

      // User B joins late
      const contextB = await browser.newContext();
      const pageB = await contextB.newPage();

      try {
        await pageB.goto("/");
        await waitForConnection(pageB);

        // User B should immediately have the current document
        await expect(pageB.locator("textarea")).toHaveValue(setupText, {
          timeout: 5_000,
        });
      } finally {
        await contextB.close();
      }
    } finally {
      await contextA.close();
    }
  });
});
