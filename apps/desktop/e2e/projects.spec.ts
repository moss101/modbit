/**
 * REQ-PX-063 / REQ-PX-064 (QUAL-PX-063, QUAL-PX-064): projects in the agent
 * list against a real Core, its real event log and projections, real Git
 * repositories, the preload bridge and the renderer. The provider is a
 * scripted OpenAI-compatible server and is used for one thing only: to leave
 * one task waiting for an approval, so a project has a member that needs
 * attention. Everything a project shows is the Core's: membership, counts and
 * the typed refusal of a forbidden move are read back from the Core after
 * every action, and no wait here is on a clock - each is on a state the Core or
 * the page reports.
 */
import { expect, test, type Locator, type Page } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { accessible, closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";
import { streamingModel } from "./support/stream-model.ts";

const hex32 = () => Array.from(crypto.getRandomValues(new Uint8Array(16)), (x) => x.toString(16).padStart(2, "0")).join("");

const rowOf = (page: Page, taskId: string) => page.locator(`[data-testid="agent-row"][data-agent-id="${taskId}"]`);
const projectRow = (page: Page, projectId: string) => page.locator(`[data-testid="project-row"][data-project-id="${projectId}"]`);
const note = (page: Page) => page.getByTestId("agents-note");

async function createViaFleet(page: Page, goal: string, repo: string): Promise<string> {
  await page.getByTestId("goal").fill(goal);
  await page.getByTestId("workspace").fill(repo);
  await page.getByTestId("run").click();
  const card = page.getByTestId("task-card").filter({ hasText: goal }).first();
  await expect(card).toBeVisible();
  return (await card.getAttribute("data-task-id"))!;
}

async function session(page: Page): Promise<string> {
  return (await page.evaluate(() => window.modbit.localState())).sessionId!;
}

/** A task through the bridge; `stop` cancels it, which leaves a task that is no draft. */
async function makeTask(page: Page, goal: string, repo: string, stop: boolean): Promise<string> {
  const sid = await session(page);
  const id = hex32();
  const t = await page.evaluate(([s, g, c, r]) => window.modbit.createTask(s!, g!, c!, r!), [sid, goal, id, repo]);
  if (stop) await page.evaluate(([s, t2]) => window.modbit.cancelTask(s!, t2!), [sid, t.taskId]);
  return t.taskId;
}

/** The Core's own answer: the projects with their members. */
const coreProjects = (page: Page) => page.evaluate(() => window.modbit.listProjects({ includeArchived: true }));
const membersOf = async (page: Page, projectId: string) => (await coreProjects(page)).projects.find((p) => p.projectId === projectId)!.members.map((m) => m.taskId).sort();

async function dragOnto(page: Page, source: Locator, target: Locator): Promise<void> {
  const dt = await page.evaluateHandle(() => new DataTransfer());
  await source.dispatchEvent("dragstart", { dataTransfer: dt });
  await target.dispatchEvent("dragover", { dataTransfer: dt });
  await target.dispatchEvent("drop", { dataTransfer: dt });
}

async function newProject(page: Page, name: string, repo: string, color: string, icon: string): Promise<string> {
  await page.getByTestId("agents-new-project").click();
  const dialog = page.getByTestId("project-new-dialog");
  await expect(dialog).toBeVisible();
  await dialog.getByTestId("project-name").fill(name);
  await dialog.getByTestId("project-workspace").selectOption(repo);
  await dialog.getByTestId("project-color").selectOption(color);
  await dialog.getByTestId("project-icon").selectOption(icon);
  await dialog.getByTestId("project-new-create").click();
  await expect(dialog).toHaveCount(0);
  const page$ = page.getByTestId("project-page");
  await expect(page$).toHaveAttribute("data-state", "ready");
  return (await page$.getAttribute("data-project-id"))!;
}

test("PX-063/064: a project is created, filled by menu, drag and keyboard, refuses each forbidden move with the Core's reason, shows the Core's rollup, archives with an undo that leaves its tasks alone, and groups, filters and stores views", async () => {
  test.setTimeout(240_000);
  const repoA = makeRepo(mkdtempSync(join(tmpdir(), "modbit-proj-a-")));
  const repoB = makeRepo(mkdtempSync(join(tmpdir(), "modbit-proj-b-")));
  // One task will wait for an approval: it needs attention, which the project rolls up.
  const model = await streamingModel([{ parts: [{ call: { name: "shell.exec", args: { argv: ["git", "remote", "add", "origin", "https://example.invalid/x.git"] } } }] }]);
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-projects-"));
  let { app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    // ---- Tasks: three of repository A that can join (stopped, stopped, waiting for an approval), one draft, one of repository B.
    const a1 = await createViaFleet(page, "Alpha one", repoA);
    const sid = await session(page);
    await page.evaluate(([s, t]) => window.modbit.cancelTask(s!, t!), [sid, a1]);
    const a2 = await makeTask(page, "Alpha two", repoA, true);
    const draft = await makeTask(page, "Alpha draft", repoA, false);
    const b1 = await makeTask(page, "Beta one", repoB, true);
    const a3 = await makeTask(page, "Alpha three", repoA, false);
    await page.evaluate(([s, r]) => window.modbit.trustRepository(s!, r!), [sid, repoA]);
    await page.evaluate(([s, t]) => window.modbit.startTask(s!, t!), [sid, a3]);
    await expect(rowOf(page, a3)).toHaveAttribute("data-class", "NEEDS_ATTENTION", { timeout: 60_000 });
    await expect(rowOf(page, draft)).toHaveAttribute("data-class", "DRAFT");

    // ---- No project yet: the list is what it was; nothing is invented.
    await expect(page.getByTestId("project-row")).toHaveCount(0);
    await expect(page.locator('[data-testid="agents-section"][data-section="projects"]')).toHaveCount(0);
    expect((await coreProjects(page)).projects).toEqual([]);

    // ---- Create: name, workspace, palette colour and icon; the page opens on the Core's record.
    await page.getByTestId("agents-new-project").click();
    await accessible(page, "the new project dialog", '[data-testid="project-new-dialog"]');
    await page.keyboard.press("Escape");
    await expect(page.getByTestId("project-new-dialog")).toHaveCount(0);
    const pid = await newProject(page, "Release", repoA, "warn", "rocket");
    await expect(page.getByTestId("project-title")).toHaveText("Release");
    await expect(page.getByTestId("project-page").getByTestId("project-glyph")).toHaveAttribute("data-icon", "rocket");
    await expect(page.getByTestId("project-page").getByTestId("project-glyph")).toHaveAttribute("data-color", "warn");
    await expect(page.getByTestId("project-empty")).toBeVisible();
    await accessible(page, "an empty project page", '[data-testid="project-page"]');
    const core0 = (await coreProjects(page)).projects;
    expect(core0).toHaveLength(1);
    expect(core0[0]!.name).toBe("Release");
    expect(core0[0]!.workspaceRoot.endsWith(repoA.split("/").pop()!)).toBe(true);
    // The Projects group is separate from the repositories and has its New Project row.
    await expect(projectRow(page, pid)).toBeVisible();
    await expect(page.locator('[data-testid="agents-section"][data-section="projects"]')).toBeVisible();
    await expect(page.getByTestId("project-new-row")).toBeVisible();
    await expect(projectRow(page, pid).getByTestId("project-rollup")).toHaveText("0 tasks");
    // A name taken in the workspace is refused by the Core, in the dialog, and the typed text stays.
    await page.getByTestId("project-new-row").click();
    await page.getByTestId("project-name").fill("release");
    await page.getByTestId("project-new-create").click();
    await expect(page.getByTestId("project-problem")).toContainText("already used");
    await expect(page.getByTestId("project-name")).toHaveValue("release");
    await page.keyboard.press("Escape");
    expect((await coreProjects(page)).projects).toHaveLength(1);

    // ---- Add by the row's project menu.
    await rowOf(page, a1).hover();
    await rowOf(page, a1).getByTestId("agent-project-menu").click();
    await page.getByRole("menuitemcheckbox", { name: "Add to Release" }).click();
    await expect.poll(() => membersOf(page, pid)).toEqual([a1]);
    await expect(projectRow(page, pid).getByTestId("project-rollup")).toHaveText("1 task · none need attention");
    await expect(rowOf(page, a1)).toHaveAttribute("data-project-id", pid);
    // The same task is not listed a second time below.
    await expect(page.locator(`[data-testid="agent-row"][data-agent-id="${a1}"]`)).toHaveCount(1);

    // ---- Add by drag: the drop reaches the Core.
    await dragOnto(page, rowOf(page, a2), projectRow(page, pid));
    await expect.poll(() => membersOf(page, pid)).toEqual([a1, a2].sort());

    // ---- Each forbidden move shows the Core's typed reason and changes nothing.
    const before = await membersOf(page, pid);
    await dragOnto(page, rowOf(page, draft), projectRow(page, pid));
    await expect(note(page)).toContainText("A draft cannot join a project yet.");
    await expect(note(page)).toContainText("Alpha draft");
    await dragOnto(page, rowOf(page, b1), projectRow(page, pid));
    await expect(note(page)).toContainText("A project holds tasks of its own workspace only.");
    await expect(rowOf(page, b1)).not.toHaveAttribute("data-project-id", /./);
    const otherId = (await page.evaluate(([s, r]) => window.modbit.createProject(s!, { name: "Other", workspaceRoot: r! }), [sid, repoA])).project.projectId;
    await expect(projectRow(page, otherId)).toBeVisible();
    await dragOnto(page, rowOf(page, a1), projectRow(page, otherId));
    await expect(note(page)).toContainText("A task is in one project at a time.");
    await expect(note(page)).toContainText("remove it from there first");
    expect(await membersOf(page, pid)).toEqual(before);
    expect(await membersOf(page, otherId)).toEqual([]);
    await accessible(page, "the list with a refusal note", '[data-testid="agent-list"]');

    // ---- Keyboard only: add the task that needs attention through its menu.
    await rowOf(page, a3).getByTestId("agent-open").focus();
    await page.keyboard.press("Tab");
    await expect(rowOf(page, a3).getByTestId("agent-project-menu")).toBeFocused();
    await page.keyboard.press("Enter");
    // The menu lists projects newest first, as the Projects group does: Other, then Release.
    await expect(page.getByRole("menuitemcheckbox", { name: "Add to Other" })).toBeFocused();
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("menuitemcheckbox", { name: "Add to Release" })).toBeFocused();
    await page.keyboard.press("Enter");
    await expect.poll(() => membersOf(page, pid)).toEqual([a1, a2, a3].sort());

    // ---- The rollup is the Core's count of its members' headers: one task needs attention.
    await expect(projectRow(page, pid).getByTestId("project-rollup")).toHaveText("3 tasks · 1 needs attention · 1 approval waiting");
    await expect(projectRow(page, pid).getByTestId("project-attention-dot")).toBeVisible();
    const core1 = (await coreProjects(page)).projects.find((p) => p.projectId === pid)!;
    expect(core1.rollup.attentionTasks).toBe(1);
    expect(core1.rollup.members).toBe(3);
    expect(core1.rollup.byStatus.reduce((n, s) => n + s.count, 0)).toBe(3);
    await projectRow(page, pid).getByTestId("project-open").click();
    await expect(page.getByTestId("project-rollup-words")).toHaveText("3 tasks · 1 needs attention · 1 approval waiting");
    await expect(page.getByTestId("project-count")).not.toHaveCount(0);
    await expect(page.locator('[data-testid="project-count"][data-class="NEEDS_ATTENTION"]')).toHaveAttribute("data-count", "1");
    await expect(page.getByTestId("project-approvals")).toHaveText("1");
    await expect(page.getByTestId("project-prs")).toHaveText("none recorded");
    await expect(page.getByTestId("project-member")).toHaveCount(3);
    await accessible(page, "a project page with members", '[data-testid="project-page"]');
    // A member opens its conversation.
    await page.locator(`[data-testid="project-member"][data-task-id="${a2}"]`).getByTestId("project-member-open").click();
    await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", a2);
    await projectRow(page, pid).getByTestId("project-open").click();
    // Taking a task out is a Core command; the page follows the Core.
    await page.locator(`[data-testid="project-member"][data-task-id="${a2}"]`).getByTestId("project-member-remove").click();
    await expect(page.locator(`[data-testid="project-member"][data-task-id="${a2}"]`)).toHaveCount(0);
    expect(await membersOf(page, pid)).toEqual([a1, a3].sort());
    await expect(rowOf(page, a2)).not.toHaveAttribute("data-project-id", /./);

    // ---- Project grouping and the project chip, without opening a transcript.
    await page.getByTestId("agents-grouping").selectOption("project");
    await expect(page.locator(`[data-testid="agents-section"][data-section="project:${pid}"]`)).toBeVisible();
    await expect(page.locator(`[data-testid="agents-section"][data-section="project:${otherId}"]`)).toBeVisible();
    await expect(page.locator('[data-testid="agents-section"][data-section="project:none"]')).toBeVisible();
    await accessible(page, "the Project grouping", '[data-testid="agent-list"]');
    await page.getByTestId("agents-filter-toggle").click();
    await page.getByTestId(`filter-project-${pid}`).click();
    await expect(page.getByTestId("agent-row")).toHaveCount(2);
    await expect(rowOf(page, a1)).toBeVisible();
    await expect(rowOf(page, a2)).toHaveCount(0);

    // ---- A stored view keeps the grouping, the chip and the sort under a name, on this computer.
    await page.getByTestId("sort-title").click();
    await page.getByTestId("agents-views").click();
    await page.getByRole("menuitem", { name: "Save current view…" }).click();
    await accessible(page, "the save view dialog", '[data-testid="view-save-dialog"]');
    await page.getByTestId("view-name").fill("Release by title");
    await page.getByTestId("view-save").click();
    await expect(page.getByTestId("view-save-dialog")).toHaveCount(0);
    await page.getByTestId("filters-clear").click();
    await page.getByTestId("agents-grouping").selectOption("status");
    await expect(page.getByTestId("agent-row")).toHaveCount(5);
    await page.getByTestId("agents-views").click();
    await page.getByRole("menuitemradio", { name: "Release by title" }).click();
    await expect(page.getByTestId("agents-grouping")).toHaveValue("project");
    await expect(page.getByTestId("agent-row")).toHaveCount(2);
    // Collapse a section; then restart the whole app: grouping, chip, view and collapsed state come back.
    await page.locator(`[data-testid="agents-section"][data-section="project:${otherId}"] button`).click();
    await closeApp(app);
    ({ app, page } = await launch(dataDir, { env: { MODBIT_OPENAI_BASE_URL: model.url } }));
    await setContentSize(app, page, 1280, 800);
    await expect(page.getByTestId("agents-grouping")).toHaveValue("project");
    await expect(page.locator(`[data-testid="agents-section"][data-section="project:${otherId}"] button`)).toHaveAttribute("aria-expanded", "false");
    await expect(page.getByTestId("agent-row")).toHaveCount(2, { timeout: 30_000 });
    await page.getByTestId("agents-views").click();
    await expect(page.getByRole("menuitemradio", { name: "Release by title" })).toHaveAttribute("aria-checked", "true");
    await page.keyboard.press("Escape");
    await page.getByTestId("agents-filter-toggle").click();
    await page.getByTestId("filters-clear").click();
    await page.getByTestId("agents-grouping").selectOption("repository");
    await page.getByTestId("agents-filter-toggle").click();

    // ---- Archive by keyboard: the project leaves the list, its tasks do not move, Undo brings it back.
    const states = async () => (await page.evaluate((s) => window.modbit.agentHeaders(s, true), sid)).headers.map((h) => [h.taskId, h.taskState, h.archived, h.projectId] as const).sort();
    const tasksBefore = await states();
    await projectRow(page, pid).getByTestId("project-open").focus();
    await page.keyboard.press("Tab");
    await expect(projectRow(page, pid).getByTestId("project-rename")).toBeFocused();
    await page.keyboard.press("Tab");
    await expect(projectRow(page, pid).getByTestId("project-archive")).toBeFocused();
    await page.keyboard.press("Space");
    await expect(projectRow(page, pid)).toHaveCount(0);
    const undo = page.getByTestId("tray").filter({ hasText: "Archived the project" });
    await expect(undo).toBeVisible();
    await accessible(page, "the project undo notice", '[data-testid="tray-host"]');
    // The tasks are untouched and listed where they were.
    expect(await states()).toEqual(tasksBefore);
    await expect(rowOf(page, a1)).toBeVisible();
    await expect(rowOf(page, a3)).toBeVisible();
    await expect(page.getByTestId("project-row").filter({ hasText: "Release" })).toHaveCount(0);
    expect((await coreProjects(page)).projects.find((p) => p.projectId === pid)!.archived).toBe(true);
    await undo.getByRole("button", { name: "Undo" }).click();
    await expect(projectRow(page, pid)).toBeVisible();
    expect((await coreProjects(page)).projects.find((p) => p.projectId === pid)!.archived).toBe(false);
    expect(await membersOf(page, pid)).toEqual([a1, a3].sort());
    // An archived project accepts nothing: the Core says so.
    await page.evaluate(([s, p]) => window.modbit.archiveProject(s!, p!, true), [sid, pid]);
    await page.getByTestId("agents-filter-toggle").click();
    await page.getByTestId("filter-archived").click();
    await expect(projectRow(page, pid).getByTestId("project-name-text")).toContainText("(archived)");
    const refusal = await page.evaluate(([s, p, t]) => window.modbit.addProjectMember(s!, p!, t!).then(() => "accepted", (e: Error) => e.message), [sid, pid, a2]);
    expect(refusal).toContain("PROJECT_ARCHIVED");
    await projectRow(page, pid).hover();
    await projectRow(page, pid).getByTestId("project-restore").click();
    await expect(projectRow(page, pid).getByTestId("project-name-text")).not.toContainText("(archived)");
  } finally {
    await closeApp(app);
    model.server.close();
  }
});

test("PX-063: the main process rejects invalid project arguments before the Core sees them, and a lease-less or foreign command is the Core's refusal", async () => {
  test.setTimeout(120_000);
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-proj-args-")));
  const dataDir = mkdtempSync(join(tmpdir(), "modbit-e2e-projargs-"));
  const { app, page } = await launch(dataDir);
  try {
    await createViaFleet(page, "Args task", repo);
    const sid = await session(page);
    const call = (expr: string) => page.evaluate(`(async () => { try { await (${expr}); return "accepted"; } catch (e) { return e.message; } })()`) as Promise<string>;
    expect(await call(`window.modbit.createProject("zz", { name: "A", workspaceRoot: "/w" })`)).toContain("BAD_ARGUMENT");
    expect(await call(`window.modbit.createProject("${sid}", { name: "  ", workspaceRoot: "/w" })`)).toContain("BAD_ARGUMENT: name");
    expect(await call(`window.modbit.createProject("${sid}", { name: "A", color: "#ff0000", workspaceRoot: "/w" })`)).toContain("BAD_ARGUMENT: color");
    expect(await call(`window.modbit.createProject("${sid}", { name: "A", icon: "skull", workspaceRoot: "/w" })`)).toContain("BAD_ARGUMENT: icon");
    expect(await call(`window.modbit.getProject("../../etc/passwd")`)).toContain("BAD_ARGUMENT: projectId");
    expect(await call(`window.modbit.addProjectMember("${sid}", "${"a".repeat(32)}", "nope")`)).toContain("BAD_ARGUMENT");
    // A well-formed command for a project that does not exist is the Core's typed refusal.
    expect(await call(`window.modbit.getProject("${"a".repeat(32)}")`)).toContain("UNKNOWN_PROJECT");
    // The Core's own validation: a name that is too long reaches it and is refused with its code.
    expect(await call(`window.modbit.createProject("${sid}", { name: "${"n".repeat(81)}", workspaceRoot: "${repo}" })`)).toContain("PROJECT_NAME_INVALID");
    expect((await coreProjects(page)).projects).toEqual([]);
  } finally {
    await closeApp(app);
  }
});
