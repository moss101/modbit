/**
 * The shell's typed command registry (AFW-A15, AFW-A16): the one list the
 * palette, the shortcut help and the key dispatcher read. Each command is data
 * plus a call into `ShellActions`, which the shell implements over the
 * renderer's state; a command holds no logic of its own and the renderer holds
 * no authority (docs/81): nothing here reaches the Core except through the
 * typed preload calls the existing screens already make.
 *
 * Chords (AFW-A16; `mod` is Cmd on macOS and Ctrl elsewhere): new task mod+N,
 * palette mod+K, changes mod+E, terminal mod+J, browser mod+Shift+B, files
 * mod+G, settings mod+comma, shortcut help Ctrl+Shift+slash, zoom mod+plus,
 * mod+minus, mod+0. The PX-024 single-letter keys stay in keyboard.ts.
 */
import { CommandRegistry, type CommandDef } from "@modbit/ui/logic";
import { THEME_LABEL, THEME_PREFERENCES, type ThemePreference } from "@modbit/design-tokens";
import type { AppKind } from "./prefs.ts";

/** What a command can ask the shell to do. */
export interface ShellActions {
  focusNewTask(): void;
  focusFilter(): void;
  jumpAttention(): void;
  jumpRunning(): void;
  openPalette(): void;
  toggleHelp(): void;
  openSettings(): void;
  focusSettingsSection(id: "notification-preferences" | "credentials"): void;
  toggleAgentList(): void;
  togglePanel(): void;
  /** Opens the apps panel on `kind`; false (with a reason) when the task has no artifact for it. */
  showApp(kind: AppKind): true | string;
  openDashboard(): void;
  openAutomations(): void;
  goBack(): void;
  setTheme(theme: ThemePreference): void;
  zoom(delta: 1 | -1 | 0): void;
  /** Why zoom is unavailable right now, or true. */
  zoomAvailability(): true | string;
  /** Whether an app can be shown for the current task, or the reason it cannot. */
  appAvailability(kind: AppKind): true | string;
  canGoBack(): boolean;
}

type Def = CommandDef<ShellActions>;

const appCommand = (kind: AppKind, title: string, keys: string[]): Def => ({
  id: `app.${kind}`,
  title,
  group: "Apps",
  keys,
  keywords: ["panel", kind],
  enabled: (a) => a.appAvailability(kind),
  run: (a) => void a.showApp(kind),
});

export function shellCommandDefs(): Def[] {
  const defs: Def[] = [
    { id: "task.new", title: "New task", group: "Tasks", keys: ["mod+n"], keywords: ["create", "goal"], run: (a) => a.focusNewTask() },
    { id: "palette.open", title: "Command palette", group: "General", keys: ["mod+k"], palette: false, run: (a) => a.openPalette() },
    { id: "tasks.filter", title: "Filter tasks", group: "Tasks", keywords: ["search", "find"], run: (a) => a.focusFilter() },
    { id: "tasks.attention", title: "Jump to the attention list", group: "Tasks", keywords: ["approval", "needs"], run: (a) => a.jumpAttention() },
    { id: "tasks.running", title: "Jump to running tasks", group: "Tasks", run: (a) => a.jumpRunning() },
    appCommand("changes", "Show changes", ["mod+e"]),
    appCommand("terminal", "Toggle terminal", ["mod+j"]),
    appCommand("browser", "Show browser", ["mod+shift+b"]),
    appCommand("files", "Show files", ["mod+g"]),
    { id: "panel.toggle", title: "Toggle apps panel", group: "Apps", keywords: ["right", "side"], enabled: (a) => a.appAvailability("changes") === true || a.appAvailability("browser") === true || "No task has an artifact to show yet", run: (a) => a.togglePanel() },
    { id: "list.toggle", title: "Toggle agent list", group: "General", keywords: ["sidebar", "rail"], run: (a) => a.toggleAgentList() },
    { id: "settings.open", title: "Settings", group: "General", keys: ["mod+comma"], keywords: ["preferences", "provider"], run: (a) => a.openSettings() },
    { id: "help.shortcuts", title: "Keyboard shortcuts", group: "General", keys: ["ctrl+shift+slash"], keywords: ["help", "keys"], run: (a) => a.toggleHelp() },
    { id: "view.automations", title: "Open automations", group: "General", keys: ["mod+shift+a"], keywords: ["schedule", "trigger", "cron", "webhook", "unattended"], run: (a) => a.openAutomations() },
    { id: "view.dashboard", title: "Open dashboard", group: "General", keywords: ["telemetry", "cost"], run: (a) => a.openDashboard() },
    { id: "nav.back", title: "Back to the fleet", group: "General", keywords: ["close", "return"], enabled: (a) => (a.canGoBack() ? true : "Already on the fleet"), run: (a) => a.goBack() },
    { id: "zoom.in", title: "Zoom in", group: "View", keys: ["mod+plus"], enabled: (a) => a.zoomAvailability(), run: (a) => a.zoom(1) },
    { id: "zoom.out", title: "Zoom out", group: "View", keys: ["mod+minus"], enabled: (a) => a.zoomAvailability(), run: (a) => a.zoom(-1) },
    { id: "zoom.reset", title: "Reset zoom", group: "View", keys: ["mod+0"], enabled: (a) => a.zoomAvailability(), run: (a) => a.zoom(0) },
    { id: "settings.notifications", title: "Notification preferences", group: "Settings", keywords: ["settings", "alerts"], run: (a) => a.focusSettingsSection("notification-preferences") },
    { id: "settings.credentials", title: "Login credentials", group: "Settings", keywords: ["settings", "secrets", "passwords"], run: (a) => a.focusSettingsSection("credentials") },
  ];
  for (const t of THEME_PREFERENCES) defs.push({ id: `theme.${t}`, title: `Theme: ${THEME_LABEL[t]}`, group: "Settings", keywords: ["settings", "appearance", "colour", "color", "dark", "light", "contrast"], run: (a) => a.setTheme(t) });
  return defs;
}

/** The registry, built once at module load: a duplicate id or chord throws here, which fails every import of the shell. */
export function buildShellRegistry(): CommandRegistry<ShellActions> {
  const registry = new CommandRegistry<ShellActions>();
  for (const d of shellCommandDefs()) registry.register(d);
  return registry;
}

export const shellRegistry = buildShellRegistry();

/** The chords the spec's default set (AFW-A16) requires, for the test that every one is bound. */
export const REQUIRED_CHORDS: Record<string, string> = {
  "task.new": "mod+n",
  "palette.open": "mod+k",
  "app.changes": "mod+e",
  "app.terminal": "mod+j",
  "app.browser": "mod+shift+b",
  "app.files": "mod+g",
  "settings.open": "mod+comma",
  "help.shortcuts": "ctrl+shift+slash",
  "zoom.in": "mod+plus",
  "zoom.out": "mod+minus",
  "zoom.reset": "mod+0",
};
