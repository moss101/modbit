/**
 * REQ-PX-106 / QUAL-PX-106, the Customize view, on the real stack: the
 * packaged Electron app, a real Core over its socket with its real skill
 * registry and trust file on disk, real hook processes (`sh` scripts) and the
 * repository's scripted OpenAI-compatible model (the live gateway leg is not
 * run here) whose requests are recorded.
 *
 * In order, on one app: each list shows the Core's scope, trust and
 * provenance; trusting a skill by the hash it shows writes the same trust
 * record the Core reads (so any client sees it) and the next request carries
 * the skill, a stale hash is refused, revoking removes it from the next
 * request; an administrator's hook cannot be switched off but the person's
 * can, and it does not fire on the next event; another agent's configuration
 * imports quarantined, its skill is absent until the person trusts the exact
 * manifest and then is still untrusted until trusted by hash; the view
 * offers no catalog, install or update; every control works by keyboard and
 * axe-core reports no violation.
 *
 * Every wait is for a condition the Core, the page or the file system reports.
 */
import { expect, test, type ElectronApplication, type Page } from "@playwright/test";
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, writeFileSync } from "node:fs";
import type { Server } from "node:http";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { steppedModel, type Step } from "./support/apps-harness.ts";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

test.describe.configure({ mode: "serial" });

let app: ElectronApplication;
let page: Page;
let server: Server;
let repo: string;
let dataDir: string;
let scripts: string;
let sessionId = "";
let lastTask = "";
const bodies: string[] = [];
let n = 0;

const ALPHA = "ALPHA-INDEX-LINE the owner will trust";
const BETA = "BETA-INDEX-LINE nobody vouches for";
const skillText = (name: string, description: string, extra = ""): string => `---\nname: ${name}\nversion: 1.0.0\ndescription: ${description}\nprovenance.author: Acme Skills\nprovenance.source: https://example.test/${name}\nprovenance.license: MIT\n${extra}---\nInstructions of ${name}.\n`;
const skill = (name: string) => page.locator(`[data-testid="customize-skill"][data-name="${name}"]`);
const hook = (suffix: string) => page.locator(`[data-testid="customize-hook"][data-id$="/${suffix}"]`);
const extension = (name: string) => page.locator(`[data-testid="customize-extension"]:has-text("${name}")`);
const fired = (log: string): number => {
  try {
    return readFileSync(log, "utf8").split("\n").filter(Boolean).length;
  } catch {
    return 0;
  }
};
const logOf = (name: string) => join(scripts, `${name}.log`);

/** Runs one scripted task to its end through the app's own bridge (a person would use the New task form) and returns what the model was sent. */
async function runTask(goal: string): Promise<string> {
  const mark = bodies.length;
  let taskId: string;
  if (sessionId === "") {
    // The first task is made with the New task form, which also makes the session.
    await page.getByTestId("goal").fill(goal);
    await page.getByTestId("workspace").fill(repo);
    await page.getByTestId("run").click();
    const card = page.getByTestId("task-card").first();
    taskId = (await card.getAttribute("data-task-id"))!;
    sessionId = (await page.evaluate(() => window.modbit.localState()).then((s) => s.sessionId))!;
    await card.getByTestId("task-start").click();
  } else {
    n += 1;
    const commandId = n.toString(16).padStart(32, "0");
    taskId = await page.evaluate(([s, g, c, w]) => window.modbit.createTask(s!, g!, c!, w).then((t) => t.taskId), [sessionId, goal, commandId, repo]);
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sessionId, taskId] as const);
  }
  lastTask = taskId;
  await expect.poll(() => page.evaluate((t) => window.modbit.taskStatus(t).then((s) => s.loopAlive || s.state === "Queued" || s.state === "Created"), taskId), { timeout: 60_000 }).toBe(false);
  return bodies.slice(mark).join("\n");
}

async function openCustomize(): Promise<void> {
  if ((await page.getByTestId("customize").count()) === 0) await page.getByTestId("open-customize").click();
  await expect(page.getByTestId("customize")).toBeVisible();
  await page.getByTestId("customize-refresh").click();
}

test.beforeAll(async () => {
  repo = realpathSync(mkdtempSync(join(tmpdir(), "modbit-px106-repo-")));
  writeFileSync(join(repo, "notes.md"), "notes\n");
  makeRepo(repo);
  dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-px106-"));
  scripts = mkdtempSync(join(tmpdir(), "modbit-px106-hooks-"));
  // Fixture skills in the profile, hooks in the person's layer and in the administrator's.
  for (const [name, description] of [["alpha-skill", ALPHA], ["beta-skill", BETA]] as const) {
    mkdirSync(join(dataDir, "skills", name), { recursive: true });
    writeFileSync(join(dataDir, "skills", name, "SKILL.md"), skillText(name, description));
  }
  const recorder = (name: string): string[] => {
    const path = join(scripts, `${name}.sh`);
    writeFileSync(path, `cat > /dev/null\necho fired >> '${logOf(name)}'\n`);
    return ["sh", path];
  };
  const layer = (name: string) => ({ name, point: "before_tool", mode: "observe", command: recorder(name), tools: ["fs.read"] });
  writeFileSync(join(dataDir, "config.json"), JSON.stringify({ hooks: [JSON.stringify(layer("user-rec"))] }));
  writeFileSync(join(dataDir, "admin-config.json"), JSON.stringify({ hooks: [JSON.stringify(layer("admin-rec"))] }));

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
  await setContentSize(app, page, 1280, 800);
  // Two OS acts the view leaves to the person: the folder dialog and the file manager. Both are recorded here.
  await app.evaluate(({ dialog, shell }, fixture) => {
    (globalThis as unknown as { __revealed: string[] }).__revealed = [];
    (dialog as unknown as { showOpenDialog: () => Promise<unknown> }).showOpenDialog = () => Promise.resolve({ canceled: false, filePaths: [fixture] });
    (shell as unknown as { showItemInFolder: (p: string) => void }).showItemInFolder = (p) => (globalThis as unknown as { __revealed: string[] }).__revealed.push(p);
  }, join(tmpdir(), "px106-import-source"));
});

test.afterAll(async () => {
  if (app) await closeApp(app);
  server?.close();
});

test("each list shows the Core's scope, trust and provenance, and the first task's hook run is in the journal", async () => {
  const req = await runTask("Look at the notes");
  expect(req).not.toContain("ALPHA-INDEX-LINE");
  await openCustomize();
  // Skills.
  await expect(page.getByTestId("customize-skill")).toHaveCount(2);
  for (const name of ["alpha-skill", "beta-skill"]) {
    await expect(skill(name)).toHaveAttribute("data-scope", "USER");
    await expect(skill(name)).toHaveAttribute("data-trust", "UNTRUSTED");
    await expect(skill(name)).toHaveAttribute("data-enabled", "false");
    await expect(skill(name).getByTestId("customize-skill-provenance")).toContainText("from Acme Skills, https://example.test/" + name + ", MIT");
    await expect(skill(name).getByTestId("customize-skill-facts")).toContainText("hash ");
    expect((await skill(name).getAttribute("data-hash"))!).toMatch(/^[0-9a-f]{64}$/);
  }
  await expect(page.getByTestId("customize-skills-budget")).toContainText("index tokens used");
  // Hooks: scope, what a person may switch, and the journal's last result.
  await expect(hook("user-rec")).toHaveAttribute("data-scope", "USER");
  await expect(hook("user-rec")).toHaveAttribute("data-enabled", "true");
  await expect(hook("user-rec").getByTestId("customize-hook-last")).toContainText("OK after");
  await expect(hook("admin-rec")).toHaveAttribute("data-scope", "ADMIN");
  await expect(hook("admin-rec")).toHaveAttribute("data-can-disable", "false");
  await expect(hook("admin-rec").getByTestId("customize-hook-locked")).toContainText("above you");
  await expect(hook("admin-rec").getByTestId("customize-hook-toggle")).toBeDisabled();
  // Extensions: none yet.
  await expect(page.getByTestId("customize-extensions")).toContainText("No extension is loaded");
  await accessible(page, "the Customize view", '[data-testid="customize"]');
});

test("the view offers no catalog, no install from the network and no update flow", async () => {
  const names = await page.locator('[data-testid="customize"] button, [data-testid="customize"] a, [data-testid="open-customize"]').evaluateAll((els) => els.map((e) => (e.getAttribute("aria-label") ?? e.textContent ?? "").trim()));
  expect(names.length).toBeGreaterThan(5);
  expect(names.filter((t) => /\b(marketplace|catalog(ue)?|browse|install|download|update|upgrade|discover|store|publish)\b/i.test(t))).toEqual([]);
  expect(await page.locator('[data-testid="customize"] a[href]').count()).toBe(0);
  const api = await page.evaluate(() => Object.keys(window.modbit).filter((k) => /catalog|marketplace|install|download|upgrade|browseExt/i.test(k)));
  expect(api).toEqual([]);
});

test("trusting a skill by the hash it shows is the Core's record, reaches the next request, and revoking removes it", async () => {
  const hash = (await skill("alpha-skill").getAttribute("data-hash"))!;
  // By keyboard: the button names the version it trusts.
  const trust = skill("alpha-skill").getByTestId("customize-skill-trust");
  await expect(trust).toHaveAccessibleName(`Trust alpha-skill at ${hash.slice(0, 12)}`);
  await trust.focus();
  await page.keyboard.press("Enter");
  await expect(skill("alpha-skill")).toHaveAttribute("data-trust", "TRUSTED_BY_OWNER");
  await expect(skill("alpha-skill")).toHaveAttribute("data-enabled", "true");
  await expect(skill("beta-skill")).toHaveAttribute("data-trust", "UNTRUSTED");
  // The same record any other client reads: the trust file in the profile holds that hash for that name only.
  const trusted = JSON.parse(readFileSync(join(dataDir, "skills", "trusted.json"), "utf8")) as { name: string; content_hash: string }[] | { records: { name: string; content_hash: string }[] };
  const records = Array.isArray(trusted) ? trusted : trusted.records;
  expect(records.map((r) => [r.name, r.content_hash])).toEqual([["alpha-skill", hash]]);
  // The next request names it, and only it.
  const req = await runTask("Look at the notes again");
  expect(req).toContain("ALPHA-INDEX-LINE");
  expect(req).not.toContain("BETA-INDEX-LINE");
  // Trust by name only does not exist; a hash other than the current one is refused with the Core's code.
  const noHash = await page.evaluate(() => window.modbit.customizeSkillTrust("beta-skill", "").then(() => "accepted", (e: Error) => e.message));
  expect(noHash).toContain("BAD_ARGUMENT");
  const wrong = await page.evaluate(() => window.modbit.customizeSkillTrust("beta-skill", "0".repeat(64)).then(() => "accepted", (e: Error) => e.message));
  expect(wrong).toContain("HASH_MISMATCH");
  await expect(skill("beta-skill")).toHaveAttribute("data-trust", "UNTRUSTED");
  // Revoke.
  await skill("alpha-skill").getByTestId("customize-skill-revoke").click();
  await expect(skill("alpha-skill")).toHaveAttribute("data-trust", "UNTRUSTED");
  expect(await runTask("Look once more")).not.toContain("ALPHA-INDEX-LINE");
});

test("a skill that changed under the view is refused with the Core's code, and trust is bound to the new hash", async () => {
  const shown = (await skill("beta-skill").getAttribute("data-hash"))!;
  writeFileSync(join(dataDir, "skills", "beta-skill", "SKILL.md"), skillText("beta-skill", BETA, "triggers: [never]\n"));
  await skill("beta-skill").getByTestId("customize-skill-trust").click();
  await expect(page.getByTestId("customize-refusal")).toHaveAttribute("data-code", "HASH_MISMATCH");
  await expect(skill("beta-skill")).toHaveAttribute("data-trust", "UNTRUSTED");
  // Refreshed, the row shows the new hash, and trusting that works.
  await page.getByTestId("customize-refresh").click();
  const now = (await skill("beta-skill").getAttribute("data-hash"))!;
  expect(now).not.toBe(shown);
  await skill("beta-skill").getByTestId("customize-skill-trust").click();
  await expect(skill("beta-skill")).toHaveAttribute("data-trust", "TRUSTED_BY_OWNER");
  await skill("beta-skill").getByTestId("customize-skill-revoke").click();
  await expect(skill("beta-skill")).toHaveAttribute("data-trust", "UNTRUSTED");
});

test("an administrator's hook cannot be switched off; the person's can, and it does not fire on the next event", async () => {
  const [user0, admin0] = [fired(logOf("user-rec")), fired(logOf("admin-rec"))];
  expect(user0).toBeGreaterThan(0);
  const forbidden = await page.evaluate(([s, t]) => {
    const id = document.querySelector('[data-testid="customize-hook"][data-scope="ADMIN"]')!.getAttribute("data-id")!;
    return window.modbit.customizeHookEnabled(s!, t!, id, false).then(() => "accepted", (e: Error) => e.message);
  }, [sessionId, lastTask] as const);
  expect(forbidden).toContain("HOOK_SCOPE_FORBIDS");
  await hook("user-rec").getByTestId("customize-hook-toggle").click();
  await expect(hook("user-rec")).toHaveAttribute("data-enabled", "false");
  await expect(hook("user-rec")).toContainText("Off");
  await expect(hook("admin-rec")).toHaveAttribute("data-enabled", "true");
  await runTask("Read with a hook off");
  expect(fired(logOf("user-rec")), "the switched-off hook did not fire").toBe(user0);
  expect(fired(logOf("admin-rec")), "the administrator's still does").toBeGreaterThan(admin0);
  // Back on by keyboard: it fires again.
  const toggle = hook("user-rec").getByTestId("customize-hook-toggle");
  await toggle.focus();
  await page.keyboard.press("Enter");
  await expect(hook("user-rec")).toHaveAttribute("data-enabled", "true");
  await runTask("Read with the hook on");
  expect(fired(logOf("user-rec"))).toBeGreaterThan(user0);
});

test("another agent's configuration imports quarantined; its skill is absent until the exact manifest is trusted, then still untrusted until trusted by hash", async () => {
  const source = join(tmpdir(), "px106-import-source");
  cpSync(join(dirname(fileURLToPath(import.meta.url)), "..", "..", "..", "tests", "fixtures", "agent-configs", "mixed"), source, { recursive: true });
  writeFileSync(join(source, ".mcp.json"), JSON.stringify({ mcpServers: { helper: { command: "true", args: [], env: { API_KEY: "sk-not-for-import", MODE: "fast" } } } }));
  await page.getByTestId("customize-import-name").fill("mixed");
  await page.getByTestId("customize-import").click();
  const report = page.getByTestId("customize-import-report");
  await expect(report).toHaveAttribute("data-quarantined", "true");
  await expect(report).toContainText("Quarantined");
  const ext = extension("mixed");
  await expect(ext).toHaveAttribute("data-state", "QUARANTINED");
  await expect(ext.getByTestId("customize-extension-state")).toContainText("signature UNSIGNED");
  // Its tool servers are declared and inert: the Core never starts a quarantined extension's server.
  await expect(ext.locator('[data-testid="customize-server"][data-name="helper"]')).toHaveAttribute("data-state", "INERT");
  await expect(ext.locator('[data-testid="customize-server"][data-name="helper"]')).toContainText("inert (never started)");
  for (const s of await ext.getByTestId("customize-server").all()) await expect(s).toHaveAttribute("data-state", "INERT");
  await expect(skill("pdf-extract")).toHaveCount(0);
  // Released for the exact manifest shown.
  const digest = (await ext.getAttribute("data-digest"))!;
  await expect(ext.getByTestId("customize-extension-trust")).toHaveAccessibleName(`Trust mixed at manifest ${digest.slice(0, 12)}`);
  await ext.getByTestId("customize-extension-trust").click();
  await expect(ext).toHaveAttribute("data-state", "ACTIVE");
  // Its skill is listed now, as the extension's own, and cannot reach the model until trusted by hash.
  await expect(skill("pdf-extract")).toHaveAttribute("data-scope", "EXTENSION");
  await expect(skill("pdf-extract")).toHaveAttribute("data-enabled", "false");
  await skill("pdf-extract").getByTestId("customize-skill-trust").click();
  await expect(skill("pdf-extract")).toHaveAttribute("data-trust", "TRUSTED_BY_OWNER");
  await skill("pdf-extract").getByTestId("customize-skill-revoke").click();
  // Unloaded, none of it remains.
  await ext.getByTestId("customize-extension-unload").click();
  await expect(page.getByTestId("customize-extension")).toHaveCount(0);
  await expect(skill("pdf-extract")).toHaveCount(0);
});

test("open source shows the folder the Core listed; every control works by keyboard", async () => {
  await skill("alpha-skill").getByTestId("customize-skill-open").focus();
  await page.keyboard.press("Enter");
  await expect.poll(async () => (await app.evaluate(() => (globalThis as unknown as { __revealed: string[] }).__revealed)).map((p) => realpathSync(p))).toEqual([realpathSync(join(dataDir, "skills", "alpha-skill"))]);
  // Tab order reaches every action of the first skill row.
  await expect(skill("alpha-skill").getByTestId("customize-skill-trust")).toBeEnabled();
  await skill("alpha-skill").getByTestId("customize-skill-trust").focus();
  await page.keyboard.press("Tab");
  expect(await page.evaluate(() => document.activeElement?.getAttribute("data-testid"))).toBe("customize-skill-open");
  await accessible(page, "the Customize view after the actions", '[data-testid="customize"]');
  expect(readdirSync(join(dataDir, "skills")).sort()).toContain("trusted.json");
});
