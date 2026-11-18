/**
 * QUAL-PX-207 / PX-E2E-207 (DR-PX-2026-11-11-021 MWS-14): Automations and Customize in the page shape, on the real Electron
 * build, a real Core over its socket (its skill registry and trust file, its hook runner, its rule loader and its automation
 * scheduler on disk) and the repository's scripted OpenAI-compatible model, whose requests are recorded.
 *
 * One app, in order:
 * - Rules: a task runs in a trusted repository that carries AGENTS.md, a global rule, a rule scoped to a path the task never
 *   touches and a rule file that does not parse; the profile carries a rule of the person's own. Rules lists exactly the layers
 *   the Core recorded as loaded for the task (compared with what main answered on the bridge, from a tap the test puts on its
 *   handlers, and with what the model was sent), with their sources; the dormant rule is nowhere and the broken one is only
 *   among the files found and not loaded, with the Core's reason. Open source shows the file the Core read (relative to the
 *   task's folder for AGENTS.md); main refuses a source the Core did not load. Nothing in Rules can be edited.
 * - Search: one query finds a skill, a hook and a rule by their text; each result opens its own section with its own row focused.
 * - A skill that is not trusted offers no trust control of its own: Review and trust opens the review, which shows the content
 *   hash the decision is bound to; Cancel leaves it untrusted; trust from the review is the Core's record (every trust call on the
 *   bridge was made with the review open).
 * - Automations: the list with a details rail for the chosen one; enable (the approval), run now, pause, resume and kill go
 *   through the paths they used before and leave the same Core state and history.
 * - The page shape in both densities: a head the bar's height, rows of at least 44 px with a 1 px edge, the rail the task rail's
 *   width beside the list; no raw identifier in the pages' words; axe-core clean, the review open included.
 *
 * Each fail condition of QUAL-PX-207 has a retained mutation that the same check is shown to catch: a rule shown that the Core did
 * not load, a rule editable in place, a search result that opens the wrong section (and a trust made without the review).
 * With MODBIT_SHOTS_DIR set, the run writes the evidence screenshots (light and dark, comfortable and compact).
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { mkdirSync, mkdtempSync, realpathSync, writeFileSync } from "node:fs";
import type { Server } from "node:http";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { steppedModel, type Step } from "./support/apps-harness.ts";
import { runTask, setDensity } from "./support/new-task.ts";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

test.describe.configure({ mode: "serial" });

const SHOTS = process.env.MODBIT_SHOTS_DIR ?? "";

let app: ElectronApplication;
let page: Page;
let server: Server;
let repo: string;
let dataDir: string;
let sessionId = "";
let taskId = "";
const bodies: string[] = [];

// Markers the files carry, so the model's requests show which files reached the prompt.
const AGENTS_MARK = "AGENTS-MARK-7311 keep the notes short";
const STYLE_MARK = "STYLE-MARK-7312 write in plain words";
const DORMANT_MARK = "DORMANT-MARK-7313 only for docs";
const MINE_MARK = "MINE-MARK-7314 the person's own rule";

const skillText = (name: string, description: string): string => `---\nname: ${name}\nversion: 1.0.0\ndescription: ${description}\nprovenance.author: Acme Skills\nprovenance.source: https://example.test/${name}\nprovenance.license: MIT\n---\nInstructions of ${name}.\n`;
const skill = (name: string) => page.locator(`[data-testid="customize-skill"][data-name="${name}"]`);

// ------------------------------------------------------------------ the bridge's log (a tap the test puts on main's handlers)

interface BridgeCall {
  channel: string;
  args: unknown[];
  result: unknown;
  /** For a trust: whether the review was open in the window that asked, read from that window when main was asked. */
  reviewOpen?: boolean;
}

/** Taps the handlers this spec reads: each call is kept with its arguments and what main answered (the Core's answer, converted). */
async function tapBridge(): Promise<void> {
  await app.evaluate(({ ipcMain }) => {
    const g = globalThis as unknown as { __px207?: unknown[] };
    if (g.__px207) return;
    const log: unknown[] = (g.__px207 = []);
    const handlers = (ipcMain as unknown as { _invokeHandlers: Map<string, (...a: unknown[]) => unknown> })._invokeHandlers;
    for (const channel of ["context:inspector", "customize:skillTrust", "customize:openSource"]) {
      const fn = handlers.get(channel);
      if (!fn) throw new Error(`no handler for ${channel}`);
      handlers.set(channel, async (event: unknown, ...args: unknown[]) => {
        const entry: Record<string, unknown> = { channel, args };
        if (channel === "customize:skillTrust") {
          const sender = (event as { sender: { executeJavaScript: (code: string) => Promise<unknown> } }).sender;
          entry.reviewOpen = (await sender.executeJavaScript(`!!document.querySelector('[data-testid="customize-skill-review-dialog"]')`)) === true;
        }
        try {
          entry.result = await fn(event, ...args);
          return entry.result;
        } finally {
          log.push(entry);
        }
      });
    }
  });
}

async function bridgeLog(): Promise<BridgeCall[]> {
  return app.evaluate(() => ((globalThis as unknown as { __px207?: unknown[] }).__px207 ?? []) as never);
}

/** The instruction layers main last answered for the task (null when the bridge never answered one). */
async function lastInstructions(tid: string): Promise<{ source: string; loaded: boolean; contentHash: string; layer: string; notLoadedReason: string }[] | null> {
  const log = await bridgeLog();
  for (let i = log.length - 1; i >= 0; i--) if (log[i]!.channel === "context:inspector" && log[i]!.args[0] === tid && log[i]!.result) return (log[i]!.result as { instructions: never[] }).instructions;
  return null;
}

// ------------------------------------------------------------------ the checks (each shown failing on a retained mutation)

/** Every rule Rules shows must be one the Core recorded as loaded for the task, with its hash, in its order; and every one it loaded is shown. */
async function rulesDrift(tid: string): Promise<string[]> {
  const core = await lastInstructions(tid);
  if (!core) return ["the bridge never answered the task's instruction layers"];
  const loaded = core.filter((l) => l.loaded);
  const shown = await page.locator('[data-testid="customize-rules"] [data-testid="customize-rule"]').evaluateAll((els) => els.map((e) => ({ source: e.getAttribute("data-source") ?? "", hash: e.getAttribute("data-hash") ?? "", layer: e.getAttribute("data-layer") ?? "" })));
  const out: string[] = [];
  for (const s of shown) {
    const c = loaded.find((l) => l.source === s.source);
    if (!c) out.push(`rule ${s.source} is shown and the Core did not load it`);
    else if (c.contentHash !== s.hash || c.layer !== s.layer) out.push(`rule ${s.source} is shown with hash ${s.hash.slice(0, 12)} / ${s.layer}, the Core loaded ${c.contentHash.slice(0, 12)} / ${c.layer}`);
  }
  for (const c of loaded) if (!shown.some((s) => s.source === c.source)) out.push(`rule ${c.source} was loaded by the Core and is not shown`);
  if (out.length === 0 && shown.map((s) => s.source).join("\n") !== loaded.map((l) => l.source).join("\n")) out.push("the rules are not in the Core's order");
  return out;
}

/** Anything in Rules that edits: a field, an editable region, or a control other than Open source. */
async function editableRules(): Promise<string[]> {
  return page.getByTestId("customize-rules").evaluate((sec) => {
    const out: string[] = [];
    for (const el of sec.querySelectorAll<HTMLElement>("input, textarea, select, [contenteditable]:not([contenteditable='false'])")) out.push(`an editable ${el.tagName.toLowerCase()} in ${el.closest("[data-source]")?.getAttribute("data-source") ?? "Rules"}`);
    for (const b of sec.querySelectorAll<HTMLElement>("button, [role='button'], a[href]")) if (b.getAttribute("data-testid") !== "customize-rule-open") out.push(`a control "${(b.getAttribute("aria-label") ?? b.textContent ?? "").trim()}" in Rules`);
    return out;
  });
}

/** After a search result was opened: its section is the current one and its own row in that section has the focus. */
async function openedDrift(section: string, key: string): Promise<string[]> {
  return page.evaluate(
    ([sec, k]) => {
      const out: string[] = [];
      const current = document.querySelector('[data-testid="customize-nav"] [aria-current="true"]')?.getAttribute("data-testid") ?? "none";
      if (current !== `customize-nav-${sec}`) out.push(`the section shown is ${current.replace("customize-nav-", "")}, not ${sec}`);
      const a = document.activeElement as HTMLElement | null;
      const where = a?.closest("[data-section]")?.getAttribute("data-section") ?? "none";
      if (a?.getAttribute("data-key") !== k || where !== sec) out.push(`the focus is on ${a?.getAttribute("data-key") ?? a?.tagName ?? "nothing"} in ${where}, not on ${k} in ${sec}`);
      return out;
    },
    [section, key] as const,
  );
}

/** Trusts the bridge saw without the review open in the window that asked. */
async function trustedWithoutReview(): Promise<string[]> {
  return (await bridgeLog()).filter((c) => c.channel === "customize:skillTrust" && c.reviewOpen !== true).map((c) => `${String(c.args[0])} was trusted without its review open`);
}

/**
 * Raw identifiers in a page's own words, outside code and paths: a Core code (NEEDS_APPROVAL, TRUSTED_BY_OWNER), a person's id
 * (user:<uuid>), a tool's name (fs.read), a snake_case label (read_only, before_tool), a full 64-digit fingerprint.
 */
async function rawWords(testId: string): Promise<string[]> {
  return page.getByTestId(testId).evaluate((root) => {
    const out = new Set<string>();
    const walk = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
    for (let n = walk.nextNode(); n; n = walk.nextNode()) {
      const el = n.parentElement;
      if (!el || el.closest("code, [role='alert'], [role='dialog']") || !el.getClientRects().length) continue;
      const t = n.textContent ?? "";
      for (const re of [/\b[A-Z][A-Z0-9]*_[A-Z0-9_]+\b/g, /\buser:[0-9a-z-]{4,}/g, /\b(?:fs|shell|git|change|net|browser|computer)\.[a-z_]+\b/g, /\b[a-z]+_[a-z_]+\b/g, /\b[0-9a-f]{40,}\b/g]) for (const m of t.matchAll(re)) out.add(m[0]);
    }
    return [...out];
  });
}

/** MWS-14's geometry: the head is the bar's height, list rows are at least 44 px with a 1 px edge, and the rail (where open) is the task rail's width beside the list. */
async function pageDrift(testId: string, density: "comfortable" | "compact"): Promise<string[]> {
  const bar = density === "comfortable" ? 45 : 40;
  return page.getByTestId(testId).evaluate(
    (root, [barPx]) => {
      const out: string[] = [];
      const head = root.querySelector<HTMLElement>(":scope > .pg-head");
      if (!head) out.push("no page head");
      else {
        const h = head.getBoundingClientRect().height;
        if (Math.abs(h - barPx!) > 0.5) out.push(`the head is ${h} px, not ${barPx}`);
        if (!head.querySelector("h2")) out.push("the head has no page name");
        const summary = head.querySelector<HTMLElement>('[data-testid="page-summary"]');
        if (!summary || !(summary.textContent ?? "").trim() || summary.getBoundingClientRect().left - head.getBoundingClientRect().left > 40) out.push("the head's left end is empty (no summary of the page)");
        if (head.querySelectorAll('.mb-btn[data-variant="primary"]').length > 1) out.push("the head has more than one primary action");
        if (getComputedStyle(head).borderBottomWidth !== "1px") out.push("the head has no 1 px edge");
      }
      for (const list of root.querySelectorAll<HTMLElement>(".pg-list")) {
        if (!list.getClientRects().length) continue;
        if (getComputedStyle(list).borderTopWidth !== "1px") out.push(`a list (${list.getAttribute("aria-label") ?? list.getAttribute("data-testid")}) has no 1 px edge`);
        for (const row of list.querySelectorAll<HTMLElement>(":scope > li")) {
          const h = row.getBoundingClientRect().height;
          if (h < 43.5) out.push(`a row of ${list.getAttribute("aria-label") ?? "a list"} is ${h} px`);
        }
      }
      const rail = root.querySelector<HTMLElement>(".autos-rail");
      if (rail) {
        const r = rail.getBoundingClientRect();
        const body = root.querySelector<HTMLElement>(".autos-body")!.getBoundingClientRect();
        // The task's details rail: 320 px (DR-PX-2026-11-11-021 section 5.1, layout.ts DETAILS_RAIL).
        const want = 320;
        if (Math.abs(r.width - want) > 0.5) out.push(`the rail is ${r.width} px, not ${want}`);
        if (r.left < body.right - 0.5) out.push("the rail is not beside the list");
      }
      return out;
    },
    [bar] as const,
  );
}

// ------------------------------------------------------------------ helpers

async function setTheme(theme: "light" | "dark"): Promise<void> {
  await page.evaluate((t) => document.documentElement.setAttribute("data-theme", t), theme);
  await page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => done()))));
}

/** Evidence screenshots of the window (only when asked for), light and dark, comfortable and compact. `reopen` puts the page back after a density change. */
async function shots(name: string, reopen: () => Promise<void>): Promise<void> {
  if (!SHOTS) return;
  mkdirSync(SHOTS, { recursive: true });
  for (const density of ["comfortable", "compact"] as const) {
    if ((await page.evaluate(() => document.documentElement.dataset.density ?? "comfortable")) !== density) {
      await closeDialog();
      await setDensity(page, density);
      await reopen();
    }
    for (const theme of ["light", "dark"] as const) {
      await setTheme(theme);
      await page.waitForTimeout(400);
      await page.screenshot({ path: join(SHOTS, `${name}-${theme}-${density}.png`) });
    }
  }
  await setTheme("light");
  await closeDialog();
  await setDensity(page, "comfortable");
  await reopen();
}

/** A dialog the picture was taken of is put away before Settings is opened for the density. */
async function closeDialog(): Promise<void> {
  if ((await page.getByRole("dialog").count()) === 0) return;
  await page.keyboard.press("Escape");
  await expect(page.getByRole("dialog")).toHaveCount(0);
}

async function openCustomize(): Promise<void> {
  if ((await page.getByTestId("customize").count()) === 0) await page.getByTestId("open-customize").click();
  await expect(page.getByTestId("customize")).toBeVisible();
  await page.getByTestId("customize-search").fill("");
  await page.getByTestId("customize-refresh").click();
  await expect(page.locator('[data-testid="customize-rules"] [data-testid="customize-rule"]').first()).toBeVisible();
}

async function openAutomations(): Promise<void> {
  if ((await page.getByTestId("automations").count()) === 0) await page.getByTestId("open-automations").click();
  await expect(page.getByTestId("automations")).toHaveAttribute("data-loaded", "true");
}

const revealed = async (): Promise<string[]> => (await app.evaluate(() => (globalThis as unknown as { __revealed: string[] }).__revealed)).map((p) => realpathSync(p));

// ------------------------------------------------------------------ the run

test.beforeAll(async () => {
  repo = realpathSync(mkdtempSync(join(tmpdir(), "modbit-px207-repo-")));
  writeFileSync(join(repo, "notes.md"), "notes\n");
  writeFileSync(join(repo, "AGENTS.md"), `# Notes\n\n${AGENTS_MARK}\n`);
  mkdirSync(join(repo, ".modbit", "rules"), { recursive: true });
  writeFileSync(join(repo, ".modbit", "rules", "kestrel-style.md"), `---\nid: kestrel-style\n---\n${STYLE_MARK}\n`);
  writeFileSync(join(repo, ".modbit", "rules", "docs-only.md"), `---\nid: docs-only\npaths: [docs/**]\n---\n${DORMANT_MARK}\n`);
  writeFileSync(join(repo, ".modbit", "rules", "broken.md"), `---\nthis line is not a key and a value\n---\nnever loaded\n`);
  makeRepo(repo);
  dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px207-"));
  mkdirSync(join(dataDir, "rules"), { recursive: true });
  writeFileSync(join(dataDir, "rules", "mine.md"), `---\nid: mine\n---\n${MINE_MARK}\n`);
  for (const [name, description] of [["kestrel-notes", "Tidies the kestrel notes into one list"], ["release-draft", "Drafts release notes from merged work"]] as const) {
    mkdirSync(join(dataDir, "skills", name), { recursive: true });
    writeFileSync(join(dataDir, "skills", name, "SKILL.md"), skillText(name, description));
  }
  const hookScript = join(dataDir, "kestrel-watch.sh");
  writeFileSync(hookScript, "cat > /dev/null\n");
  writeFileSync(join(dataDir, "config.json"), JSON.stringify({ hooks: [JSON.stringify({ name: "kestrel-watch", point: "before_tool", mode: "observe", command: ["sh", hookScript], tools: ["fs.read"] })] }));

  const seen = (s: Step): Step => async (ctx) => {
    bodies.push(JSON.stringify(ctx.body.messages ?? []));
    return typeof s === "function" ? s(ctx) : s;
  };
  const m = await steppedModel([
    seen({ calls: [{ name: "plan.update", args: { outcome: "look at the notes", expected_files: ["notes.md"] } }] }),
    seen({ calls: [{ name: "fs.read", args: { path: "notes.md" } }] }),
    seen({ calls: [{ name: "task.complete", args: { summary: "read", self_review: { findings: [] } } }] }),
  ]);
  server = m.server;
  ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: m.url } }));
  await setContentSize(app, page, 1440, 900);
  // The file manager is the person's act: what main asks it to show is recorded here instead.
  await app.evaluate(({ shell }) => {
    (globalThis as unknown as { __revealed: string[] }).__revealed = [];
    (shell as unknown as { showItemInFolder: (p: string) => void }).showItemInFolder = (p) => (globalThis as unknown as { __revealed: string[] }).__revealed.push(p);
  });
  await tapBridge();

  taskId = await runTask(page, "Look at the kestrel notes", repo);
  sessionId = (await page.evaluate(() => window.modbit.localState()).then((s) => s.sessionId))!;
  await expect.poll(() => page.evaluate((t) => window.modbit.taskStatus(t).then((s) => s.loopAlive || s.state === "Queued" || s.state === "Created"), taskId), { timeout: 60_000 }).toBe(false);
});

test.afterAll(async () => {
  if (app) await closeApp(app);
  server?.close();
});

test("Rules lists the layers the Core loaded for the folder with their sources, Open source shows the file, and nothing in it is editable", async () => {
  // What the Core put in front of the model: the instruction files in force, never the dormant rule.
  const sent = bodies.join("\n");
  for (const mark of [AGENTS_MARK, STYLE_MARK, MINE_MARK]) expect(sent, mark).toContain(mark);
  expect(sent).not.toContain(DORMANT_MARK);

  await openCustomize();
  await page.getByTestId("customize-nav-rules").click();
  await expect(page.getByTestId("customize-nav-rules")).toHaveAttribute("aria-current", "true");
  const rules = page.locator('[data-testid="customize-rules"] [data-testid="customize-rule"]');
  await expect(rules).toHaveCount(3);
  expect(await rulesDrift(taskId)).toEqual([]);
  const sources = await rules.evaluateAll((els) => els.map((e) => e.getAttribute("data-source")!));
  expect(sources).toContain("AGENTS.md");
  expect(sources.some((s) => s.endsWith(join(".modbit", "rules", "kestrel-style.md")))).toBe(true);
  expect(sources.some((s) => s.endsWith(join("rules", "mine.md")))).toBe(true);
  // The layer and why it is in force, in words; the source as the Core names it.
  const agents = page.locator('[data-testid="customize-rule"][data-source="AGENTS.md"]');
  await expect(agents).toContainText("This repository's instructions");
  await expect(agents.getByTestId("customize-rule-when")).toContainText("Always in force");
  await expect(page.locator('[data-testid="customize-rule"][data-layer="user"]')).toContainText("Your rules, in every project");
  // Paths are short in the row (relative to the task's folder, or under home, or their last parts) and whole in its title.
  const style = page.locator('[data-testid="customize-rule"][data-source$="kestrel-style.md"]');
  await expect(style.getByTestId("customize-rule-path")).toHaveText(join(".modbit", "rules", "kestrel-style.md"));
  const minePath = (await page.locator('[data-testid="customize-rule"][data-layer="user"]').getByTestId("customize-rule-path").textContent())!;
  const mineSource = (await page.locator('[data-testid="customize-rule"][data-layer="user"]').getAttribute("data-source"))!;
  expect(minePath.length).toBeLessThan(mineSource.length);
  expect(minePath.endsWith(join("rules", "mine.md"))).toBe(true);
  expect(await page.locator('[data-testid="customize-rule"][data-layer="user"] [title]').first().getAttribute("title")).toContain(mineSource);
  // A hook says when and what it runs in words; the full command is its title.
  const hookRow = page.locator('[data-testid="customize-hook"][data-id$="/kestrel-watch"]');
  await expect(hookRow.getByTestId("customize-hook-when")).toHaveText("Before reading files · only watches · if it fails the step does not go ahead");
  await expect(hookRow.getByTestId("customize-hook-command")).toHaveText("kestrel-watch.sh");
  await expect(hookRow.getByTestId("customize-hook-last")).toContainText("Last ran fine in");
  await expect(page.getByTestId("page-summary")).toHaveText("2 skills · 3 rules · 1 hook · 0 extensions");
  // The dormant rule is nowhere; the broken one is only among the files found and not loaded, with the Core's reason.
  await expect(page.getByTestId("customize-rules")).not.toContainText("docs-only.md");
  const notLoaded = page.getByTestId("customize-rule-not-loaded");
  await expect(notLoaded).toHaveCount(1);
  await expect(notLoaded).toHaveAttribute("data-source", /broken\.md$/);
  await expect(notLoaded).toContainText("expected `key: value`");
  expect(await editableRules()).toEqual([]);

  // Open source shows the file the Core read: AGENTS.md in the task's folder, the rule file where it is.
  await agents.getByTestId("customize-rule-open").click();
  await expect.poll(revealed).toEqual([realpathSync(join(repo, "AGENTS.md"))]);
  await page.locator('[data-testid="customize-rule"][data-source$="kestrel-style.md"]').getByTestId("customize-rule-open").click();
  await expect.poll(revealed).toEqual([realpathSync(join(repo, "AGENTS.md")), realpathSync(join(repo, ".modbit", "rules", "kestrel-style.md"))]);
  // Main shows nothing the Core did not load: the dormant rule, a path out of the folder, a made-up file.
  for (const source of [join(repo, ".modbit", "rules", "docs-only.md"), "../../etc/passwd", "README.md"]) {
    const r = await page.evaluate(([s, sid, t]) => window.modbit.customizeOpenSource("rule", s!, sid!, t!).then(() => "shown", (e: Error) => e.message), [source, sessionId, taskId]);
    expect(r, source).toContain("the Core lists no such source");
  }
  expect((await revealed()).length).toBe(2);
  await accessible(page, "Customize at Rules", '[data-testid="customize"]');
  expect(await rawWords("customize")).toEqual([]);
  // Retained mutation 6: a raw person id and a tool name in the page's words are caught.
  await page.evaluate(() => {
    const p = document.createElement("p");
    p.setAttribute("data-mutation", "6");
    p.textContent = "Approved by user:b1b1b1b1-b1b1 for fs.read";
    document.querySelector('[data-testid="customize-rules"]')!.appendChild(p);
  });
  expect(await rawWords("customize")).toEqual(["user:b1b1b1b1-b1b1", "fs.read"]);
  await page.evaluate(() => document.querySelector('[data-mutation="6"]')?.remove());

  // Retained mutation 1: a rule shown that the Core did not load is caught.
  await page.evaluate(() => {
    const row = document.querySelector<HTMLElement>('[data-testid="customize-rule"]')!.cloneNode(true) as HTMLElement;
    row.setAttribute("data-source", "/tmp/never-loaded.md");
    row.setAttribute("data-mutation", "1");
    document.querySelector('[data-testid="customize-rule-list"]')!.appendChild(row);
  });
  expect(await rulesDrift(taskId)).toEqual(["rule /tmp/never-loaded.md is shown and the Core did not load it"]);
  await page.evaluate(() => document.querySelector('[data-mutation="1"]')?.remove());
  // Retained mutation 2: a rule editable in place is caught.
  await page.evaluate(() => {
    const t = document.createElement("textarea");
    t.setAttribute("data-mutation", "2");
    t.setAttribute("aria-label", "Edit the rule");
    document.querySelector('[data-testid="customize-rule"][data-source="AGENTS.md"] .pg-main')!.appendChild(t);
    document.querySelector<HTMLElement>('[data-testid="customize-rule"][data-source="AGENTS.md"] strong')!.contentEditable = "true";
  });
  expect(await editableRules()).toEqual(["an editable strong in AGENTS.md", "an editable textarea in AGENTS.md"]);
  await page.evaluate(() => {
    document.querySelector('[data-mutation="2"]')?.remove();
    document.querySelector<HTMLElement>('[data-testid="customize-rule"][data-source="AGENTS.md"] strong')!.removeAttribute("contenteditable");
  });
  expect(await editableRules()).toEqual([]);
  expect(await rulesDrift(taskId)).toEqual([]);

  await shots("customize-rules", async () => {
    await openCustomize();
    await page.getByTestId("customize-nav-rules").click();
  });
});

test("one search finds a skill, a hook and a rule by text, and each result opens its own section with its row focused", async () => {
  await openCustomize();
  const search = page.getByTestId("customize-search");
  await search.fill("kestrel");
  const results = page.getByTestId("customize-result");
  await expect(results).toHaveCount(3);
  await expect(page.getByTestId("page-summary")).toHaveText("3 matches");
  const found = await results.evaluateAll((els) => els.map((e) => `${e.getAttribute("data-section")}:${e.getAttribute("data-key")}`));
  expect(found.map((f) => f.split(":")[0])).toEqual(["skills", "rules", "hooks"]);
  expect(found[0]).toBe("skills:USER/kestrel-notes");
  expect(found[1]).toMatch(/^rules:.*kestrel-style\.md$/);
  expect(found[2]).toMatch(/^hooks:.*\/kestrel-watch$/);
  await accessible(page, "Customize with search results", '[data-testid="customize"]');
  await shots("customize-search", async () => {
    await openCustomize();
    await page.getByTestId("customize-search").fill("kestrel");
    await expect(page.getByTestId("customize-result")).toHaveCount(3);
  });

  // Each result, by pointer and by keyboard, opens its own section with its own row focused.
  for (const [i, f] of found.entries()) {
    const [section, ...rest] = f.split(":");
    const key = rest.join(":");
    await openCustomize();
    await page.getByTestId("customize-search").fill("kestrel");
    const hit = page.locator(`[data-testid="customize-result"][data-section="${section}"]`);
    if (i % 2 === 0) await hit.click();
    else {
      await hit.focus();
      await page.keyboard.press("Enter");
    }
    await expect(page.getByTestId("customize-results")).toHaveCount(0);
    await expect.poll(() => openedDrift(section!, key)).toEqual([]);
  }
  // Text inside a description finds a skill; a word nothing has finds nothing; Escape puts the search away.
  await search.fill("merged work");
  await expect(results).toHaveCount(1);
  await expect(results).toHaveAttribute("data-key", "USER/release-draft");
  await search.fill("no-such-word-anywhere");
  await expect(results).toHaveCount(0);
  await expect(page.getByTestId("customize-results")).toContainText("Nothing in skills, rules, hooks, extensions or memory matches");
  await search.press("Escape");
  await expect(search).toHaveValue("");

  // Retained mutation 3: a result that opens the wrong section is caught (the rule's result, then another section shown).
  await search.fill("kestrel");
  await page.locator('[data-testid="customize-result"][data-section="rules"]').click();
  const ruleKey = found[1]!.slice("rules:".length);
  await expect.poll(() => openedDrift("rules", ruleKey)).toEqual([]);
  await page.getByTestId("customize-nav-hooks").click();
  expect(await openedDrift("rules", ruleKey)).toEqual(["the section shown is hooks, not rules", `the focus is on BUTTON in none, not on ${ruleKey} in rules`]);
});

test("a skill that is not trusted cannot be enabled without its review; trust from the review is the Core's record", async () => {
  await openCustomize();
  const row = skill("kestrel-notes");
  await expect(row).toHaveAttribute("data-trust", "UNTRUSTED");
  await expect(row).toHaveAttribute("data-enabled", "false");
  // The row offers the review and the source, and nothing that trusts by itself.
  const controls = await row.locator("button").evaluateAll((els) => els.map((e) => e.getAttribute("data-testid")));
  expect(controls).toEqual(["customize-skill-review", "customize-skill-open"]);
  const hash = (await row.getAttribute("data-hash"))!;

  // Cancel leaves it as it was.
  await row.getByTestId("customize-skill-review").click();
  const review = page.getByTestId("customize-skill-review-dialog");
  await expect(review).toBeVisible();
  await expect(review.getByTestId("customize-review-hash")).toHaveText(hash);
  await expect(review).toContainText("Acme Skills");
  await accessible(page, "the review of a skill that is not trusted");
  await shots("customize-review", async () => {
    await openCustomize();
    await skill("kestrel-notes").getByTestId("customize-skill-review").click();
    await expect(page.getByTestId("customize-skill-review-dialog")).toBeVisible();
  });
  await page.getByTestId("customize-review-cancel").click();
  await expect(review).toHaveCount(0);
  await expect(row).toHaveAttribute("data-trust", "UNTRUSTED");

  // Trust from the review, by keyboard: the Core's record, for the hash the review showed.
  await row.getByTestId("customize-skill-review").click();
  const trust = page.getByTestId("customize-skill-trust");
  await expect(trust).toHaveAccessibleName(`Trust kestrel-notes at ${hash.slice(0, 12)}`);
  await trust.focus();
  await page.keyboard.press("Enter");
  await expect(row).toHaveAttribute("data-trust", "TRUSTED_BY_OWNER");
  await expect(row).toHaveAttribute("data-enabled", "true");
  const trusts = (await bridgeLog()).filter((c) => c.channel === "customize:skillTrust");
  expect(trusts.map((c) => [c.args[0], c.args[1], c.reviewOpen])).toEqual([["kestrel-notes", hash, true]]);
  expect(await trustedWithoutReview()).toEqual([]);
  await row.getByTestId("customize-skill-revoke").click();
  await expect(row).toHaveAttribute("data-trust", "UNTRUSTED");

  // Retained mutation 4: a control that trusts from the row, without the review, is caught on the bridge.
  await page.evaluate(([n, h]) => {
    const b = document.createElement("button");
    b.textContent = "Trust";
    b.setAttribute("data-mutation", "4");
    b.onclick = () => void window.modbit.customizeSkillTrust(n!, h!);
    document.querySelector(`[data-testid="customize-skill"][data-name="${n}"] .pg-actions`)!.appendChild(b);
  }, ["kestrel-notes", hash]);
  await page.locator('[data-mutation="4"]').click();
  await expect.poll(trustedWithoutReview).toEqual(["kestrel-notes was trusted without its review open"]);
  await page.evaluate(() => document.querySelector('[data-mutation="4"]')?.remove());
  await page.evaluate((n) => window.modbit.customizeSkillRevoke(n, ""), "kestrel-notes");
  await page.getByTestId("customize-refresh").click();
  await expect(skill("kestrel-notes")).toHaveAttribute("data-trust", "UNTRUSTED");
});

test("Automations: the list with a details rail; enable, run now, pause and kill behave as before on the new layout", async () => {
  test.setTimeout(240_000);
  const def = JSON.stringify({ schema: "modbit.automation/1", name: "kestrel-report", description: "Reports on the kestrel notes.", prompt: "Report on the notes. Change nothing.", triggers: [{ kind: "manual", id: "now" }] }, null, 2);
  await page.evaluate(([j, r]) => window.modbit.createAutomation(j!, r!), [def, repo]);
  await page.evaluate(([j, r]) => window.modbit.createAutomation(j!, r!), [JSON.stringify({ schema: "modbit.automation/1", name: "kestrel-tick", prompt: "Tick.", triggers: [{ kind: "schedule", id: "tick", every_minutes: 60 }] }), repo]);
  await openAutomations();
  const row = page.locator('[data-testid="automation-row"][data-name="kestrel-report"]');
  await expect(row).toHaveAttribute("data-state", "NEEDS_APPROVAL");
  await expect(page.getByTestId("automation-detail")).toHaveCount(0);

  // The rail opens for the chosen one, beside the list.
  await row.getByTestId("automation-select").click();
  const rail = page.getByTestId("automation-detail");
  await expect(rail.getByRole("heading", { name: "kestrel-report" })).toBeVisible();
  await expect(rail).toContainText("Reports on the kestrel notes.");
  await expect(rail.getByTestId("detail-trigger")).toHaveAttribute("data-kind", "manual");
  await expect(rail.getByTestId("detail-effects")).toHaveText("Reads only");
  await expect(rail.getByTestId("detail-run")).toBeDisabled();
  expect(await pageDrift("automations", "comfortable")).toEqual([]);

  // Enable: the approval of the exact version (the dialog the surface had).
  await rail.getByTestId("detail-enable").click();
  await page.getByTestId("enable-confirm").click();
  await expect(row).toHaveAttribute("data-state", "ENABLED");
  await expect(page.getByTestId("page-summary")).toHaveText("2 automations · 1 enabled");
  // The approver is the person at this machine: "you", with the Core's id as data and title only.
  await expect(rail.getByTestId("detail-approver")).toHaveText("you");
  expect(await rail.getByTestId("detail-approver").getAttribute("data-approver")).toMatch(/^user:/);
  await expect(rail.getByTestId("detail-approved")).not.toContainText("user:");
  // The fingerprint is 12 characters in the rail; Copy puts the whole one on the clipboard.
  const fullHash = (await rail.getByTestId("detail-hash").getAttribute("data-hash"))!;
  expect(fullHash).toMatch(/^[0-9a-f]{64}$/);
  await expect(rail.getByTestId("detail-hash")).toHaveText(fullHash.slice(0, 12));
  await rail.getByTestId("detail-hash-copy").click();
  await expect(rail.getByTestId("detail-hash-copy")).toHaveText("Copied");
  expect(await app.evaluate(({ clipboard }) => clipboard.readText())).toBe(fullHash);
  await expect(rail.getByTestId("detail-trigger")).toHaveText("By hand");
  // Run now: an ordinary task the Core records on the definition, with its history in the rail.
  await rail.getByTestId("detail-run").click();
  const run = rail.getByTestId("run-row").first();
  await expect(run).toHaveAttribute("data-trigger-kind", "manual");
  await expect(run).toHaveAttribute("data-status", "succeeded", { timeout: 120_000 });
  expect(await run.getAttribute("data-task-id")).toMatch(/^[0-9a-f]{32}$/);
  await expect(row.getByTestId("automation-last-run")).toHaveAttribute("data-status", "succeeded");
  const listed = await page.evaluate(() => window.modbit.automations());
  const core = listed.automations.find((a) => a.name === "kestrel-report")!;
  expect(core.state).toBe("ENABLED");
  expect(core.lastRunStatus).toBe("succeeded");

  // Pause and resume from the row: the Core's state.
  await row.getByTestId("row-pause").click();
  await expect(row).toHaveAttribute("data-state", "PAUSED");
  expect((await page.evaluate(() => window.modbit.automations())).automations.find((a) => a.name === "kestrel-report")!.paused).toBe(true);
  await row.getByTestId("row-pause").click();
  await expect(row).toHaveAttribute("data-state", "ENABLED");

  await accessible(page, "Automations with the rail open", '[data-testid="automations"]');
  expect(await rawWords("automations")).toEqual([]);
  await shots("automations-rail", async () => {
    await openAutomations();
    await page.locator('[data-testid="automation-row"][data-name="kestrel-report"]').getByTestId("automation-select").click();
    await expect(page.getByTestId("automation-detail")).toBeVisible();
  });

  // Kill from the rail asks first, then pauses it and reports the Core's counts.
  await rail.getByTestId("detail-kill").click();
  await expect(page.getByTestId("kill-dialog")).toContainText("cancels the runs in progress");
  await page.getByTestId("kill-confirm").click();
  await expect(page.getByTestId("kill-report")).toHaveAttribute("data-paused", "true");
  await page.getByTestId("kill-close").click();
  await expect(row).toHaveAttribute("data-state", "PAUSED");
  // The rail closes and the list takes the room.
  await rail.getByTestId("automation-rail-close").click();
  await expect(page.getByTestId("automation-detail")).toHaveCount(0);
});

test("the page shape holds in both densities and at the narrower width", async () => {
  for (const density of ["comfortable", "compact"] as const) {
    await setDensity(page, density);
    await openAutomations();
    await page.locator('[data-testid="automation-row"][data-name="kestrel-report"]').getByTestId("automation-select").click();
    expect(await pageDrift("automations", density), `Automations in ${density}`).toEqual([]);
    await openCustomize();
    expect(await pageDrift("customize", density), `Customize in ${density}`).toEqual([]);
  }
  await setDensity(page, "comfortable");
  // At 1100 x 760 the rail still stands beside the list; nothing is cut.
  await setContentSize(app, page, 1100, 760);
  await openAutomations();
  await page.locator('[data-testid="automation-row"][data-name="kestrel-report"]').getByTestId("automation-select").click();
  expect(await pageDrift("automations", "comfortable")).toEqual([]);
  // Retained mutation 5: a page head pushed back to the superseded 40 px is caught.
  await page.evaluate(() => document.querySelector<HTMLElement>('[data-testid="automations"] > .pg-head')!.style.setProperty("height", "40px"));
  expect(await pageDrift("automations", "comfortable")).toEqual(["the head is 40 px, not 45"]);
  await page.evaluate(() => document.querySelector<HTMLElement>('[data-testid="automations"] > .pg-head')!.style.removeProperty("height"));
  await setContentSize(app, page, 1440, 900);
});
