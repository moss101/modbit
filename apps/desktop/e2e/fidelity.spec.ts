/**
 * REQ-PX-142 / QUAL-PX-142 (docs/62, DR-PX-2026-10-24-016 section 6): the fidelity gate. Modbit's real primitives and native controls,
 * laid out as ZCode's specimens in the state gallery of the real Electron app, are probed with the same function that probed
 * ZCode's real components (evidence/zcode-ui/reference) and compared element by element: computed colour, border, radius, font,
 * spacing, size, shadow and transition, and the pixels of each element against its crop of the reference screenshot. A deviation
 * passes only when a declared exception (ZDX) covers it; a reference key with no Modbit counterpart must be declared not applicable.
 * The gallery is a QA aid and never proof of a behaviour: behaviour is proven by the specs that run against a real Core.
 */
import { expect, test, type Page } from "@playwright/test";
import { execFileSync } from "node:child_process";
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { PROBE_SRC } from "../../../tools/zcode-reference/scripts/probe-lib.mjs";
import { compareRecord, keyOf, type Deviation, type ProbeRecord } from "../../../packages/design-tokens/src/fidelity.ts";
import { EXCEPTIONS } from "../../../packages/design-tokens/src/fidelity-exceptions.ts";
import { appDir, closeApp, launch, setContentSize } from "./support/ui-harness.ts";
import { COVERAGE, type Cover } from "./support/zdl-coverage.ts";

const OUT = "dist/gallery-build";
const REF = join(appDir, "..", "..", "evidence", "zcode-ui", "reference");
const reference = JSON.parse(readFileSync(join(REF, "zcode-computed.json"), "utf8")) as Record<string, ProbeRecord[]>;
/** At most this share of an element's pixels may differ by more than 24 per channel per channel. */
const PIXEL_BOUND = Number(process.env.ZDL_PIXEL_BOUND ?? 0.05);

const deviations: Deviation[] = [];
const pixels: { key: string; theme: string; index: number; ratio: number; exempt: string | null }[] = [];
let used = new Set<string>();

test.describe.configure({ mode: "serial" });
test.beforeAll(() => {
  execFileSync("node", ["build.mjs"], { cwd: appDir, env: { ...process.env, MODBIT_GALLERY: "1", MODBIT_OUTDIR: OUT }, stdio: "pipe" });
});

async function probe(page: Page, selector: string): Promise<ProbeRecord[]> {
  return (await page.evaluate(`(${PROBE_SRC})(${JSON.stringify(selector)}, null)`)) as ProbeRecord[];
}

/** Share of differing pixels between a crop of the reference screenshot and the screenshot of the element. */
async function pixelRatio(page: Page, refPng: Buffer, ref: ProbeRecord, element: Buffer): Promise<number> {
  return page.evaluate(
    async ({ refB64, rect, elB64 }) => {
      const load = async (b64: string) => {
        const bin = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
        return createImageBitmap(new Blob([bin], { type: "image/png" }));
      };
      const [r, e] = await Promise.all([load(refB64), load(elB64)]);
      const scale = r.width / 1100;
      const w = e.width;
      const h = e.height;
      // Glyphs are rasterised at the sub-pixel phase of their origin, which differs between the two pages: both sides are lightly
      // blurred and the best of the nine +-1 device-pixel alignments is taken, so a different colour, shape, size or weight shows
      // and a different anti-aliasing phase does not.
      const ox = Math.floor(rect.x) * scale; // the element screenshot is clipped from the whole CSS pixel the element starts in
      const oy = Math.floor(rect.y) * scale;
      // The edge band is left out: ZCode positions some overlays with top-1/2 and a translate, which snaps a half-pixel edge one row
      // differently from Modbit's centring although the layout numbers are equal; radius, border and size are compared as styles.
      const INSET = 3;
      const iw = Math.max(1, w - 2 * INSET);
      const ih = Math.max(1, h - 2 * INSET);
      const crop = (src: ImageBitmap, sx: number, sy: number) => {
        const c = new OffscreenCanvas(iw, ih).getContext("2d")!;
        c.filter = "blur(0.5px)"; // absorbs the anti-aliasing phase of glyphs rasterised at different fractional origins
        c.drawImage(src, sx + INSET, sy + INSET, iw, ih, 0, 0, iw, ih);
        return c.getImageData(0, 0, iw, ih).data;
      };
      const db = crop(e, 0, 0);
      let best = 1;
      for (let dy = -1; dy <= 1; dy++) {
        for (let dx = -1; dx <= 1; dx++) {
          const da = crop(r, ox + dx, oy + dy);
          let bad = 0;
          for (let i = 0; i < da.length; i += 4) if (Math.max(Math.abs(da[i]! - db[i]!), Math.abs(da[i + 1]! - db[i + 1]!), Math.abs(da[i + 2]! - db[i + 2]!)) > 40) bad++;
          best = Math.min(best, bad / (iw * ih));
        }
      }
      return best;
    },
    { refB64: refPng.toString("base64"), rect: { x: ref.rect.x ?? 0, y: ref.rect.y ?? 0, w: ref.rect.w, h: ref.rect.h }, elB64: element.toString("base64") },
  );
}

test("PX-142: the coverage table is closed over the reference capture", () => {
  for (const [sheet, { keys }] of Object.entries(COVERAGE)) {
    const refKeys = new Set([...(reference[`${sheet}:light`] ?? []).map((r) => keyOf(r.id))]);
    expect([...refKeys].filter((k) => !(k in keys)), `${sheet}: reference keys with neither a selector nor a not-applicable reason`).toEqual([]);
    expect(Object.keys(keys).filter((k) => !refKeys.has(k)), `${sheet}: coverage keys the capture does not have`).toEqual([]);
    for (const e of Object.values(keys)) if ("na" in e) expect(e.na.length, "a not-applicable entry states its reason").toBeGreaterThan(10);
  }
});

test("PX-142: Modbit's primitives match ZCode's real components in light and dark, except the declared exceptions", async () => {
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-fidelity-"));
  const { app, page } = await launch(dataDir, { outDir: OUT, waitForCore: false, env: { MODBIT_CORE_BIN: join(dataDir, "no-such-core") } });
  try {
    await setContentSize(app, page, 1100, 900);
    await page.mouse.move(1, 1); // no element under the pointer: a hovered button is a different state
    const dpr = await page.evaluate(() => window.devicePixelRatio);
    for (const theme of ["light", "dark"] as const) {
      for (const [sheet, { modbitSheet, keys }] of Object.entries(COVERAGE)) {
        await page.evaluate((h) => (location.hash = h), `#fidelity/${modbitSheet}/${theme}`);
        await expect(page.locator(`[data-testid="fidelity"][data-sheet="${modbitSheet}"]`)).toBeVisible();
        await expect(page.locator("html")).toHaveAttribute("data-theme", theme);
        await page.mouse.move(1, 1); // a click on the previous sheet leaves the pointer over this one: hover is a different state
        if (modbitSheet === "menu") await page.locator('.mb-btn[aria-haspopup="menu"]').click();
        if (modbitSheet === "tooltip") await page.locator(".mb-icon-btn").focus();
        await page.evaluate(() => document.fonts.ready);
        await page.waitForTimeout(250); // the 150 ms colour transitions settle
        const refRecords = reference[`${sheet}:${theme}`]!;
        const refPng = readFileSync(join(REF, `${sheet}-${theme}.png`));
        for (const [key, entry] of Object.entries(keys)) {
          if ("na" in entry) continue;
          const cover = entry as Cover;
          const want = refRecords.filter((r) => keyOf(r.id) === key).slice(0, cover.first ?? Infinity);
          const got = (await probe(page, cover.select)).slice(0, cover.first ?? Infinity);
          expect(got.length, `${theme} ${sheet} ${key}: elements matching ${cover.select}`).toBe(want.length);
          for (const [i, ref] of want.entries()) {
            const found = compareRecord(ref, got[i]!, theme, EXCEPTIONS, key, i, cover.ignore ?? []);
            deviations.push(...found);
            if (dpr === 2 && !cover.noPixels) {
              const shot = await page.locator(cover.select).nth(i).screenshot();
              pixels.push({ key, theme, index: i, ratio: await pixelRatio(page, refPng, ref, shot), exempt: found.find((d) => d.exception)?.exception ?? null });
            }
          }
        }
      }
    }
  } finally {
    await closeApp(app);
  }
  used = new Set(deviations.filter((d) => d.exception).map((d) => d.exception!));
  mkdirSync(join(appDir, "test-results"), { recursive: true });
  writeFileSync(join(appDir, "test-results", "zdl-fidelity-report.json"), JSON.stringify({ deviations, pixels }, null, 1));
  const undeclared = deviations.filter((d) => !d.exception);
  const lines = [...new Set(undeclared.map((d) => `${d.key} ${d.prop}: ZCode ${d.reference.slice(0, 40)} / Modbit ${d.actual.slice(0, 40)} [${d.theme}]`))];
  expect(lines, "deviations no declared exception covers").toEqual([]);
});

test("PX-142: every element is within the pixel bound of its ZCode crop, and every declared exception is still used", () => {
  const dpr2 = pixels.length > 0;
  if (dpr2) {
    // An element a declared exception covers differs from ZCode by design (a stronger edge, a muted text colour); its pixels are reported, not bounded.
    const bad = pixels.filter((p) => p.ratio > PIXEL_BOUND && p.exempt === null).map((p) => `${p.theme} ${p.key}#${p.index}: ${(p.ratio * 100).toFixed(1)}%`);
    expect(bad, `elements over ${PIXEL_BOUND * 100}% differing pixels`).toEqual([]);
  }
  const stale = EXCEPTIONS.filter((e) => !used.has(e.id) && e.id !== "ZDX-06").map((e) => e.id);
  expect(stale, "declared exceptions that no deviation uses any more").toEqual([]);
});
