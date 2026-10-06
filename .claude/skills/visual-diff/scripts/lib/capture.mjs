/**
 * capture.mjs - shared headless-Chromium screenshot capture.
 *
 * Extracted from screenshot.mjs so the single-shot CLI and the batch
 * visual-diff runner share one capture implementation (same viewport,
 * reduced-motion emulation, full-page scroll-trigger, and HTTP guard).
 */

import { chromium } from 'playwright';
import { existsSync } from 'fs';

// Use pinned binary when present; fall back to Playwright's managed Chromium.
const PINNED = '/opt/pw-browsers/chromium-1194/chrome-linux/chrome';
export const CHROMIUM = existsSync(PINNED) ? PINNED : undefined;

/** Launch a headless Chromium instance with the project's standard args. */
export async function launchBrowser() {
  return chromium.launch({
    ...(CHROMIUM ? { executablePath: CHROMIUM } : {}),
    headless: true,
    args: ['--no-sandbox', '--disable-setuid-sandbox', '--disable-dev-shm-usage'],
  });
}

/**
 * Capture a screenshot of `url` to `outPath`.
 *
 * @param {object} o
 * @param {string} o.url       Absolute URL to navigate to.
 * @param {string} o.outPath   Destination PNG path.
 * @param {boolean} [o.full]   Full-page capture (scrolls to trigger reveals).
 * @param {number} [o.width]   Viewport width (default 1280).
 * @param {number} [o.height]  Viewport height (default 800).
 * @param {number} [o.timeout] Navigation timeout ms (default 30000).
 * @param {string} [o.selector] CSS selector to clip to (captures just that element,
 *                              scrolled into view - used for section-by-section diffs).
 * @param {import('playwright').Browser} [o.browser] Reuse an existing browser.
 * @returns {Promise<{ width:number, height:number }>} viewport used.
 */
export async function captureScreenshot({
  url,
  outPath,
  full = false,
  width = 1280,
  height = 800,
  timeout = 30_000,
  selector = null,
  browser = null,
}) {
  const ownBrowser = !browser;
  const b = browser || (await launchBrowser());

  try {
    const page = await b.newPage();
    await page.setViewportSize({ width, height });

    // Emulate reduced-motion: freezes CSS animations so captures are deterministic
    // and IntersectionObserver-gated reveal sections are visible in full-page shots.
    await page.emulateMedia({ reducedMotion: 'reduce' });

    // 'load' (not 'networkidle'): networkidle is flaky on the single-threaded
    // python http.server and on animated pages, hanging until timeout. 'load'
    // plus document.fonts.ready gives deterministic, font-stable captures.
    const response = await page.goto(url, { waitUntil: 'load', timeout });

    // Abort on HTTP errors so we never silently save an error-page screenshot.
    if (response && !response.ok()) {
      await page.close();
      throw new Error(`Navigation failed: HTTP ${response.status()} for ${url}`);
    }

    // Wait for web fonts so text metrics match between local and live captures.
    await page.evaluate(() => document.fonts && document.fonts.ready).catch(() => {});
    await page.waitForTimeout(300);

    if (full || selector) {
      // Scroll through the page so IntersectionObserver reveal triggers for every section.
      await page.evaluate(async () => {
        await new Promise((resolve) => {
          let y = 0;
          const step = () => {
            window.scrollBy(0, window.innerHeight);
            y += window.innerHeight;
            if (y < document.body.scrollHeight) requestAnimationFrame(step);
            else { window.scrollTo(0, 0); resolve(); }
          };
          requestAnimationFrame(step);
        });
      });
      await page.waitForTimeout(300);
    }

    if (selector) {
      // Section-by-section: clip to a single element. scrollIntoView re-triggers its
      // reveal observer; the element-clip keeps both sides the same region/size.
      const el = page.locator(selector).first();
      if (!(await el.count())) {
        await page.close();
        throw new Error(`Selector not found: ${selector} on ${url}`);
      }
      await el.scrollIntoViewIfNeeded();
      await page.waitForTimeout(150);
      await el.screenshot({ path: outPath });
    } else {
      await page.screenshot({ path: outPath, fullPage: full });
    }
    await page.close();
    return { width, height };
  } finally {
    if (ownBrowser) await b.close();
  }
}
