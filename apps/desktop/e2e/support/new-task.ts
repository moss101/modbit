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

/** The open conversation's details (state, phase, risk, evidence, context, economics, density, undo all, browser, cancel). */
export async function openTaskDetails(page: Page): Promise<Locator> {
  const panel = page.getByTestId("task-details");
  if (!(await panel.isVisible())) await page.getByTestId("task-details-toggle").click();
  await expect(panel).toBeVisible();
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
