// XFILTER browser smoke — cross-chart slice & dice on the real production
// build. Run against a running server (CIE_TEST_URL, default :3013).
// Walks: Numbers → click the DeadCode kind bar → banner + sibling counts →
// the Fixes list filtered → release by a second click → a file filter
// reaching the Map → clear from the receiver. Zero page errors.
const { chromium } = require("playwright"),
  assert = require("node:assert/strict");
(async () => {
  const browser = await chromium.launch({
    headless: true,
    ...(process.env.CIE_CHROMIUM ? { executablePath: process.env.CIE_CHROMIUM } : {}),
    args: process.env.CIE_BROWSER_SINGLE_PROCESS === "1" ? ["--no-sandbox", "--no-zygote", "--single-process"] : [],
  });
  try {
    const page = await browser.newPage({ viewport: { width: 1600, height: 1000 } });
    const errors = [];
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(process.env.CIE_TEST_URL || "http://127.0.0.1:3013");
    // default workspace = the polyglot five-service sample
    await page.getByText(/five-service harbor system/).first().waitFor({ timeout: 60000 });

    const openNumbers = async () => {
      await page.getByRole("button", { name: /Charts/ }).click();
      await page.getByRole("menuitem", { name: "In pictures" }).click();
      await page.getByText("How urgent are the problems?").waitFor();
    };
    const openFixes = async () => {
      await page.getByRole("button", { name: /Explore/ }).click();
      await page.getByRole("menuitem", { name: "Things to look at" }).click();
    };
    const openMap = async () => {
      await page.getByRole("button", { name: /Explore/ }).click();
      await page.getByRole("menuitem", { name: "The map of your code" }).click();
      await page.locator(".map-node").first().waitFor({ timeout: 30000 });
    };

    await openNumbers();
    // sanity: the kind chart renders its bars before anything is clicked
    await page
      .locator('[data-chart-card="What kinds of problems?"] .recharts-bar-rectangle path')
      .first()
      .waitFor();

    // 1. click the DeadCode kind bar → banner + sibling surviving counts
    await page
      .locator('[data-chart-card="What kinds of problems?"] path[data-value="DeadCode"]')
      .click();
    const banner = page.getByTestId("chart-filter-banner");
    await banner.waitFor();
    assert.match(await banner.textContent(), /Filtered to/);
    // sibling severity chart discloses the surviving count (DeadCode = 36 of 79)
    await page
      .locator('[data-chart-card="How urgent are the problems?"]')
      .getByText(/36 of 79/)
      .waitFor();
    console.log("PASS kind bar click filters siblings with disclosed counts");

    // 2. the Fixes list is a receiver: same filter, only matching rows
    await openFixes();
    const recv = page.getByTestId("chart-filter-receiver");
    await recv.waitFor();
    assert.match(await recv.textContent(), /36 of 79/);
    await page.getByText("Good to know (36)").waitFor();
    assert.equal(await page.locator('section[aria-label="Fix soon"]').count(), 0, "non-matching groups vanish");
    console.log("PASS fixes list filtered with the receiver banner");

    // 3. release by a second click on the same bucket
    await openNumbers();
    await page
      .locator('[data-chart-card="What kinds of problems?"] path[data-value="DeadCode"]')
      .click();
    await banner.waitFor({ state: "detached" });
    // the surviving-count notes disappear with the filter
    await page
      .locator('[data-chart-card="How urgent are the problems?"]')
      .getByText(/of 79/)
      .waitFor({ state: "detached" });
    console.log("PASS second click releases the filter");

    // 4. a file filter reaches the Map (receiver) — kept bubbles or the
    //    named empty card, both honest
    await page
      .locator('[data-chart-card="Where do the problems live?"] .recharts-bar-rectangle path')
      .first()
      .click();
    await banner.waitFor();
    await openMap();
    await page.getByTestId("chart-filter-receiver").waitFor();
    const nodeCount = await page.locator(".map-node").count();
    const emptyCard = await page.getByText("No map bubbles match this filter").count();
    assert.ok(nodeCount < 75 || emptyCard === 1, `map filtered (${nodeCount} nodes) or honestly empty`);
    console.log(`PASS map receiver: ${emptyCard ? "named empty state" : `${nodeCount} of 75 bubbles`}`);

    // 5. clear from the receiver — the whole map returns
    await page.getByRole("button", { name: "Clear filter" }).click();
    await page.getByTestId("chart-filter-receiver").waitFor({ state: "detached" });
    await page.locator(".map-node").first().waitFor();
    assert.equal(await page.locator(".map-node").count(), 75, "full map restored");
    console.log("PASS clear restores the full map");

    assert.deepEqual(errors, [], "no page errors");
    console.log("PASS zero page errors");
  } finally {
    await browser.close();
  }
})();
