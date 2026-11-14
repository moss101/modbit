/**
 * The pane dock's gate (REQ-PX-202; DR-PX-2026-11-11-021 MWS-10, MWS-01 and section 5.1), measured on the running app: every
 * pane the task has open and not minimized is on screen at once, tiled in at most two columns without overlap, none under
 * 280 by 100 px, each with a head the height of the top bar holding its name, Minimize and Close; the one-app tab strip is
 * gone; and while a pane is open there is no details rail. Empty when it holds. Used by px-202-pane-dock.spec.ts and awl.spec.ts.
 */
import type { Page } from "@playwright/test";

export const PANE_MIN = { width: 280, height: 100 } as const;

interface PaneBox {
  kind: string;
  x: number;
  y: number;
  width: number;
  height: number;
  head: { y: number; height: number } | null;
  name: string;
  minimize: boolean;
  close: boolean;
  shown: boolean;
}

export async function dockDrift(page: Page, topBar: number): Promise<string[]> {
  const bad: string[] = [];
  const dock = page.getByTestId("apps-panel");
  if ((await dock.count()) !== 1) return ["dock: not on screen"];
  const m = await dock.evaluate((panel) => {
    // The grid the panes tile in: the whole dock, less its own bar where the title bar does not hold the task's toggles.
    const el = panel.querySelector<HTMLElement>(".dock-grid") ?? panel;
    const r = el.getBoundingClientRect();
    const panes = [...el.querySelectorAll<HTMLElement>(".pane")].map((p) => {
      const b = p.getBoundingClientRect();
      const head = p.querySelector<HTMLElement>(".pane-head");
      const hb = head?.getBoundingClientRect();
      const kind = p.getAttribute("data-kind") ?? "";
      const style = getComputedStyle(p);
      return { kind, x: b.x, y: b.y, width: b.width, height: b.height, head: hb ? { y: hb.y - b.y, height: hb.height } : null, name: (p.querySelector(".pane-name")?.textContent ?? "").trim(), minimize: p.querySelector(`[data-testid="pane-minimize-${kind}"]`) !== null, close: p.querySelector(`[data-testid="pane-close-${kind}"]`) !== null, shown: style.display !== "none" && style.visibility !== "hidden" && b.width > 0 && b.height > 0 };
    });
    return { box: { x: r.x, y: r.y, width: r.width, height: r.height }, scrollHeight: el.scrollHeight, open: (panel.getAttribute("data-panes") ?? "").split(" ").filter(Boolean), columns: Number(panel.getAttribute("data-columns")), panes: panes as PaneBox[], tabs: panel.querySelectorAll('[role="tablist"], [role="tab"], .mb-tabs').length };
  });
  // The tab strip of one app at a time is gone.
  if (m.tabs !== 0 || (await page.getByTestId("apps-tabs").count()) !== 0) bad.push("dock: the one-app tab strip is back");
  const shown = m.panes.filter((p) => p.shown);
  // Every open pane is on screen together: one app at a time fails here.
  if (shown.length !== m.open.length || m.open.some((k) => !shown.some((p) => p.kind === k))) bad.push(`dock: ${m.open.length} pane(s) open (${m.open.join(", ")}) and ${shown.length} on screen (${shown.map((p) => p.kind).join(", ")})`);
  // At most two columns: the distinct left edges.
  const lefts = [...new Set(shown.map((p) => Math.round(p.x)))];
  if (lefts.length > 2) bad.push(`dock: ${lefts.length} columns`);
  if (lefts.length !== m.columns) bad.push(`dock: says ${m.columns} column(s) and draws ${lefts.length}`);
  for (const p of shown) {
    if (p.width < PANE_MIN.width - 0.5) bad.push(`dock: ${p.kind} is ${p.width} wide, under ${PANE_MIN.width}`);
    if (p.height < PANE_MIN.height - 0.5) bad.push(`dock: ${p.kind} is ${p.height} tall, under ${PANE_MIN.height}`);
    if (p.x < m.box.x - 1 || p.x + p.width > m.box.x + m.box.width + 1) bad.push(`dock: ${p.kind} is outside the dock`);
    // MWS-01: every pane head is the bar's height, at the top of its pane (a lower pane has its 1 px edge above it).
    if (!p.head) bad.push(`dock: ${p.kind} has no head`);
    else if (Math.abs(p.head.height - topBar) > 1 || p.head.y > 1.5) bad.push(`dock: pane head of ${p.kind} is ${p.head.height} tall at ${p.head.y}, not ${topBar} at the top`);
    if (!p.name) bad.push(`dock: ${p.kind} has no name in its head`);
    if (!p.minimize || !p.close) bad.push(`dock: ${p.kind} lacks ${!p.minimize ? "Minimize" : "Close"}`);
  }
  // Tiled: no two panes overlap, and together they fill the dock (or more, where it scrolls).
  for (let i = 0; i < shown.length; i++) {
    for (let j = i + 1; j < shown.length; j++) {
      const a = shown[i]!;
      const b = shown[j]!;
      const w = Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x);
      const h = Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y);
      if (w > 1 && h > 1) bad.push(`dock: ${a.kind} and ${b.kind} overlap by ${Math.round(w)} x ${Math.round(h)}`);
    }
  }
  const area = shown.reduce((sum, p) => sum + p.width * p.height, 0);
  const full = m.box.width * Math.max(m.box.height, m.scrollHeight);
  if (shown.length > 0 && Math.abs(area - full) > full * 0.01 + 2 * Math.max(m.box.height, m.scrollHeight)) bad.push(`dock: the panes cover ${Math.round(area)} of ${Math.round(full)} px²`);
  // MWS-10: with panes open the rail gives way.
  if (shown.length > 0 && (await page.getByTestId("details-rail").count()) !== 0) bad.push("dock: the details rail is beside open panes");
  // Each pane on screen has its toggle pressed in the title bar (where the title bar shows the task's toggles).
  for (const p of shown) {
    const toggle = page.getByTestId(`pane-toggle-${p.kind}`);
    if ((await toggle.count()) === 1 && (await toggle.getAttribute("aria-pressed")) !== "true") bad.push(`dock: ${p.kind} is open and its toggle does not say so`);
  }
  return bad;
}

/** The title is never narrower than this while a pane toggle stands in the bar (REQ-PX-202, the owner's review). */
export const TITLE_FLOOR = 200;
/** The conversation keeps this width while the dock is open beside it (two pane minimums). */
export const CONVERSATION_MIN = 560;
/** The panes whose toggle may stand in the title bar whatever their state; others only while open or minimized. */
const ALWAYS_SHOWN = new Set(["changes", "terminal"]);

/**
 * What breaks the title bar and the conversation beside an open dock (empty when they hold): the title cut while a pane toggle
 * still stands in the bar, the title under its floor where it has more text than that, a toggle in the bar for a closed pane
 * other than Changes and Terminal, a control past the bar's end, and the conversation under 560 beside the dock.
 */
export async function titleBarDrift(page: Page): Promise<string[]> {
  const bad: string[] = [];
  const m = await page.getByTestId("top-bar").evaluate((bar) => {
    const b = bar.getBoundingClientRect();
    const text = bar.querySelector<HTMLElement>(".topbar-title-text");
    const toggles = [...bar.querySelectorAll<HTMLElement>('[data-testid^="pane-toggle-"]')].filter((t) => getComputedStyle(t).display !== "none" && t.getBoundingClientRect().width > 0).map((t) => ({ kind: (t.getAttribute("data-testid") ?? "").replace("pane-toggle-", ""), state: t.getAttribute("data-state") ?? "" }));
    const right = Math.max(...[...bar.querySelectorAll<HTMLElement>("button")].map((x) => x.getBoundingClientRect().right));
    return { barRight: b.right, right, title: text ? { width: text.getBoundingClientRect().width, natural: text.scrollWidth, cut: text.scrollWidth > text.clientWidth + 1 } : null, toggles };
  });
  if (m.title) {
    if (m.title.cut && m.toggles.length > 0) bad.push(`title bar: the title is cut (${Math.round(m.title.width)} of ${m.title.natural}) while ${m.toggles.length} pane toggle(s) stand in the bar`);
    if (m.toggles.length > 0 && m.title.width + 1 < Math.min(TITLE_FLOOR, m.title.natural)) bad.push(`title bar: the title is ${Math.round(m.title.width)} wide, under ${TITLE_FLOOR}`);
  }
  for (const t of m.toggles) if (!ALWAYS_SHOWN.has(t.kind) && t.state === "closed") bad.push(`title bar: ${t.kind} is closed and stands in the bar`);
  if (m.right > m.barRight + 1) bad.push(`title bar: a control ends ${Math.round(m.right - m.barRight)} px past the bar`);
  const shell = page.getByTestId("shell");
  const docked = (await page.getByTestId("region-apps").count()) === 1 && (await shell.getAttribute("data-stacked")) !== "true";
  if (docked) {
    const conv = (await page.getByTestId("region-conversation").boundingBox())!;
    if (conv.width + 0.5 < CONVERSATION_MIN) bad.push(`conversation: ${Math.round(conv.width)} wide beside the dock, under ${CONVERSATION_MIN}`);
  }
  return bad;
}

/**
 * Every pane the task can show can be reached from the Panes menu (empty when it can): the menu opens, lists each pane once, each
 * entry is visible and says the pane's state, and the menu closes with Escape. `expected` is the panes the task can show.
 */
export async function panesMenuDrift(page: Page, expected: readonly string[], states: Record<string, "open" | "minimized" | "closed">): Promise<string[]> {
  const bad: string[] = [];
  const trigger = page.getByTestId("panes-menu");
  if ((await trigger.count()) !== 1) return ["panes menu: no Panes menu"];
  await trigger.focus();
  await page.keyboard.press("Enter");
  const menu = page.getByRole("menu", { name: "Panes" });
  if ((await menu.count()) !== 1) return ["panes menu: Enter on it opens nothing"];
  const items = await menu.locator('[data-testid^="panes-menu-"]').evaluateAll((els) => els.map((e) => ({ kind: (e.getAttribute("data-testid") ?? "").replace("panes-menu-", ""), text: (e.textContent ?? "").trim(), checked: e.getAttribute("aria-checked"), shown: getComputedStyle(e).display !== "none" && getComputedStyle(e).visibility !== "hidden" && e.getBoundingClientRect().height > 0 })));
  const kinds = items.map((i) => i.kind);
  if (JSON.stringify(kinds) !== JSON.stringify(expected)) bad.push(`panes menu: lists ${kinds.join(", ")}, not ${expected.join(", ")}`);
  for (const i of items) {
    if (!i.shown) bad.push(`panes menu: ${i.kind} is listed and cannot be seen`);
    const want = states[i.kind] ?? "closed";
    const says = i.checked !== "true" ? "closed" : /minimized/.test(i.text) ? "minimized" : "open";
    if (says !== want) bad.push(`panes menu: ${i.kind} says ${says}, the pane is ${want}`);
  }
  await page.keyboard.press("Escape");
  if ((await menu.count()) !== 0) bad.push("panes menu: Escape does not close it");
  return bad;
}
