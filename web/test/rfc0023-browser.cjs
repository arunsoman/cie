// Run against a running development/production server; uses real Chromium/G6.
const { chromium } = require("playwright"),
  assert = require("node:assert/strict");
(async () => {
  const browser = await chromium.launch({
    headless: true,
    executablePath: process.env.CIE_CHROMIUM || "/usr/bin/chromium",
    args: ["--no-sandbox"],
  });
  try {
    const page = await browser.newPage({
      viewport: { width: 1600, height: 1000 },
      acceptDownloads: true,
    });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(process.env.CIE_TEST_URL || "http://127.0.0.1:3013");
    await page.getByText("24", { exact: true }).first().waitFor();
    await page.getByRole("button", { name: "Atlas", exact: true }).click();
    const chart = page.locator("[data-cie-chart=c4]").first();
    await chart.locator("canvas").first().waitFor({ timeout: 60000 });
    await chart.getByText(/Rendered: g6/).waitFor();
    assert.equal(await chart.locator("svg").count(), 0);
    console.log("PASS negotiated canvas and restored hero panel");
    await chart
      .getByRole("combobox", { name: "Chart layout" })
      .selectOption("elk-layered");
    await chart
      .getByText(/layout elk-layered \(Exact\)/)
      .waitFor({ timeout: 30000 });
    console.log("PASS actual ELK layout");
    const download = page.waitForEvent("download");
    await chart
      .getByRole("button", { name: "Export PNG", exact: true })
      .click();
    const image = await download;
    assert.equal(image.suggestedFilename(), "cie-chart.png");
    assert.equal(await image.failure(), null);
    const textDownload = page.waitForEvent("download");
    await chart
      .getByRole("button", { name: "Export Mermaid", exact: true })
      .click();
    assert.equal((await textDownload).suggestedFilename(), "cie-chart.mmd");
    console.log("PASS PNG and Mermaid export");
    await chart.locator("summary").filter({ hasText: "Text outline" }).click();
    const pick = chart.getByRole("button", { name: "billing", exact: true });
    await pick.click();
    await page
      .getByTestId("atlas-scope-note")
      .filter({ hasText: "billing" })
      .waitFor();
    await page
      .getByRole("button", {
        name: "Remove billing from answer context",
        exact: true,
      })
      .waitFor();
    console.log("PASS container selection drills and adds context");
    await page.getByRole("switch", { name: "Facts only", exact: true }).click();
    const input = page.getByPlaceholder("Ask about your selections…");
    await input.fill("Show me a UML diagram");
    await input.press("Enter");
    await page
      .getByText(/Drawn from \d+ files over \d+ named things/)
      .first()
      .waitFor();
    console.log("PASS selection-scoped local answer with honesty header");
    await page
      .getByRole("button", {
        name: "Answer from the whole project",
        exact: true,
      })
      .click();
    assert.equal(
      await page
        .getByRole("button", { name: /Remove .* from answer context/ })
        .count(),
      0,
    );
    assert.deepEqual(errors, []);
    console.log("PASS clear context and no browser errors");
  } finally {
    await browser.close();
  }
})().catch((e) => {
  console.error(e);
  process.exitCode = 1;
});
