/**
 * How a spec starts work in the agent workspace: the home composer's send creates the task, starts it and opens its
 * conversation; Alt+Enter creates it without starting it. Every wait is on something the renderer shows.
 */
import { expect, type Locator, type Page } from "@playwright/test";

/** Shows the home composer (a task's conversation or a page may be open). */
export async function toNewTask(page: Page): Promise<void> {
  const goal = page.getByTestId("goal");
  if (await page.getByTestId("settings-nav").isVisible()) await page.getByTestId("nav-back").click();
  if (!(await goal.isVisible())) await page.getByTestId("agents-new-task").click();
  await expect(goal).toBeVisible();
}

async function compose(page: Page, goal: string, repo?: string): Promise<void> {
  await toNewTask(page);
  await page.getByTestId("goal").fill(goal);
  if (repo !== undefined) await page.getByTestId("workspace").fill(repo);
}

async function openedTask(page: Page): Promise<string> {
  const conv = page.getByTestId("conversation");
  await expect(conv).toHaveAttribute("data-task-id", /.+/, { timeout: 30_000 });
  return (await conv.getAttribute("data-task-id"))!;
}

/** Send: the task is created, started and open. Returns its id. */
export async function runTask(page: Page, goal: string, repo?: string): Promise<string> {
  await compose(page, goal, repo);
  await page.getByTestId("run").click();
  return openedTask(page);
}

/** Alt+Enter: the task is created and open but not started (the composer offers Start). Returns its id. */
export async function queueTask(page: Page, goal: string, repo?: string): Promise<string> {
  await compose(page, goal, repo);
  await page.getByTestId("goal").press("Alt+Enter");
  return openedTask(page);
}

/** A task's row in the agent list: `data-state` is the Core's task state, `data-task-id` its id. */
export const agentRow = (page: Page, taskId: string): Locator => page.locator(`[data-testid="agent-row"][data-task-id="${taskId}"]`);
export const expectTaskState = (page: Page, taskId: string, state: string | RegExp, timeout = 60_000): Promise<void> => expect(agentRow(page, taskId)).toHaveAttribute("data-state", state, { timeout });

/** Opens a task's conversation from its row. */
export async function openTask(page: Page, taskId: string): Promise<void> {
  await agentRow(page, taskId).getByTestId("agent-open").click();
  await expect(page.getByTestId("conversation")).toHaveAttribute("data-task-id", taskId);
}

/** The title bar's More, opened: the conversation's density, undo all, browser and cancel, and the details rail's body where the task area has no room for the rail (MWS-09). */
export async function openTaskMore(page: Page): Promise<Locator> {
  const more = page.getByTestId("task-more");
  if (!(await more.isVisible())) await page.getByTestId("task-details-toggle").click();
  await expect(more).toBeVisible();
  return more;
}

/**
 * The open task's details (state, phase, agents, generation, attachments, risk, security, next action, the context inspector and the
 * economics), opened behind Details in the rail's head. The rail's body is beside the conversation, or under More where the task
 * area is 800 px or narrower (More is then opened, and stays open). The density, Undo all, Browser and Cancel task are `openTaskMore`'s.
 */
export async function openTaskDetails(page: Page): Promise<Locator> {
  if ((await page.getByTestId("details-rail").count()) === 0) await openTaskMore(page);
  const panel = page.getByTestId("task-details");
  const toggle = page.getByTestId("rail-details-toggle");
  if ((await toggle.getAttribute("aria-expanded")) !== "true") await toggle.click();
  await expect(panel).toHaveAttribute("data-open", "true");
  return panel;
}

/** Settings (the gear), or one item of the person's menu at the foot of the left column: open-dashboard, open-worktrees, theme.dark and so on. */
export async function gearMenu(page: Page, itemId: string): Promise<void> {
  if (itemId === "settings.open") {
    await page.getByTestId("open-settings").click();
    await expect(page.getByTestId("settings-nav")).toBeVisible();
    return;
  }
  await page.getByTestId("overflow-menu").click();
  await page.getByTestId(`menu-item-${itemId}`).click();
}

/** The list's search, grouping, filters and views stay folded until asked for. */
export async function openListTools(page: Page): Promise<void> {
  if (!(await page.getByTestId("agents-controls").isVisible())) await page.getByTestId("agents-tools-toggle").click();
  await expect(page.getByTestId("agents-controls")).toBeVisible();
}

/** The list's More menu (New project, Row details), opened. */
export async function openListMore(page: Page, within: Locator | Page = page): Promise<void> {
  const more = within.getByTestId("agents-more");
  if ((await more.getAttribute("aria-expanded")) !== "true") await more.click();
  await expect(more).toHaveAttribute("aria-expanded", "true");
}

/** Chooses the density under Settings, Appearance (DR-PX-2026-11-11-021 MWS-03) with the real control, and returns to the page that was open. */
export async function setDensity(page: Page, density: "comfortable" | "compact"): Promise<void> {
  const inSettings = await page.getByTestId("settings-nav").isVisible();
  if (!inSettings) await gearMenu(page, "settings.open");
  await page.getByTestId(`settings-density-${density}`).click();
  await expect(page.getByTestId(`settings-density-${density}`)).toHaveAttribute("aria-checked", "true");
  await expect.poll(() => page.evaluate(() => document.documentElement.dataset.density ?? "comfortable")).toBe(density);
  if (!inSettings) {
    await page.getByTestId("nav-back").click();
    await expect(page.getByTestId("settings-nav")).toHaveCount(0);
    // The list and an open conversation are mounted again on the way back from Settings: both have read the Core before anything is measured.
    await expect(page.getByTestId("agents-loading")).toHaveCount(0);
    const conversation = page.getByTestId("conversation");
    if ((await conversation.count()) === 1) await expect(conversation).toHaveAttribute("data-loaded", "true", { timeout: 30_000 });
  }
}

export type PaneKind = "changes" | "terminal" | "browser" | "files" | "evidence" | "activity" | "background" | "pull-request";

/** Brings one pane into the dock from its toggle in the title bar (DR-PX-2026-11-11-021 MWS-10), beside whatever is open; a pane already in the grid is left as it is. */
export async function showPane(page: Page, kind: PaneKind): Promise<void> {
  const pane = page.getByTestId(`pane-${kind}`);
  if ((await pane.count()) === 0) {
    const toggle = page.getByTestId(`pane-toggle-${kind}`);
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
  }
  await expect(pane).toBeVisible();
}

/**
 * Shows `kind` as the only pane in the dock: it is opened if it is not, and every other pane in the grid is closed with its own
 * Close, as a person would. For the specs about one pane's content, which were written when the panel showed one app at a time.
 */
export async function soloPane(page: Page, kind: PaneKind): Promise<void> {
  await showPane(page, kind);
  for (;;) {
    const others = await page.getByTestId("apps-panel").locator(".pane").evaluateAll((els, k) => els.map((e) => e.getAttribute("data-kind") ?? "").filter((x) => x !== k), kind);
    if (others.length === 0) break;
    await page.getByTestId(`pane-close-${others[0]}`).click();
    await expect(page.getByTestId(`pane-${others[0]}`)).toHaveCount(0);
  }
  await expect(page.getByTestId("apps-panel")).toHaveAttribute("data-panes", kind);
}

/** Opens the dock from the title bar's pane toggles (DR-PX-2026-11-11-021 MWS-05) unless a pane is already open; `kind` is the pane it opens. */
export async function openPane(page: Page, kind: PaneKind = "changes"): Promise<void> {
  if ((await page.getByTestId("region-apps").count()) === 0) {
    const toggle = page.getByTestId(`pane-toggle-${kind}`);
    await expect(toggle).toHaveAttribute("aria-pressed", "false");
    await toggle.click();
    await expect(toggle).toHaveAttribute("aria-pressed", "true");
  }
  await expect(page.getByTestId("region-apps")).toBeVisible();
}
