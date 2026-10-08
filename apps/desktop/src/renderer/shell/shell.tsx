/**
 * The agent-first shell (REQ-PX-045): three regions, a top bar, the apps-panel
 * host, a status row, the tray host, the command palette and the shortcut
 * registry, wired over the renderer's existing state (`useApp`). The Fleet,
 * Review, Browser and Dashboard screens live inside the conversation region
 * unchanged. The renderer holds no authority (docs/81): every Core effect is
 * still a typed preload call made by those screens.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { TrayHost, useEscapeLayers, useKeyDispatch, useKeyScope, type MenuItem } from "@modbit/ui";
import type { KeyScope } from "@modbit/ui/logic";
import { THEME_LABEL, THEME_PREFERENCES, type ThemePreference } from "@modbit/design-tokens";
import type { AppState } from "../state/use-app.ts";
import { FleetView } from "../fleet/fleet-view.tsx";
import { AgentRegion } from "./agent-region.tsx";
import { AppsPanel } from "./apps-panel.tsx";
import { artifactsOf, panelTaskFor } from "./artifacts.ts";
import { shellRegistry, type ShellActions } from "./commands.ts";
import { CommandPalette, type PaletteItem } from "./palette.tsx";
import { clampZoom, loadUiPrefs, saveUiPrefs, ZOOM_STEP, type AppKind, type PanelState, type UiPrefs } from "./prefs.ts";
import { ShellFrame } from "./shell-frame.tsx";
import { ShortcutHelp } from "./shortcut-help.tsx";
import { StatusRow } from "./status-row.tsx";
import { TopBar } from "./top-bar.tsx";

const LOCATION = "Local, trusted workspace";

function applyTheme(theme: ThemePreference): void {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

export function Shell({ app }: { app: AppState }) {
  const { core, platform, model, tasks, cols, reviewing, browsing, dashboardOpen, setDashboardOpen, setReviewing, setBrowsing, openBrowser, selectedTaskId, setSelectedTaskId, helpOpen, setHelpOpen, runFleetCommand, confirm } = app;
  const [prefs, setPrefsState] = useState<UiPrefs>(loadUiPrefs);
  const setPrefs = useCallback((update: (p: UiPrefs) => UiPrefs) => setPrefsState(update), []);
  const [paletteOpen, setPaletteOpen] = useState(false);
  const [notice, setNotice] = useState("");
  useEscapeLayers();
  useKeyDispatch();

  useEffect(() => saveUiPrefs(prefs), [prefs]);
  useEffect(() => applyTheme(prefs.theme), [prefs.theme]);
  useEffect(() => {
    document.documentElement.style.zoom = prefs.zoom === 1 ? "" : String(prefs.zoom);
  }, [prefs.zoom]);
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(""), 8000);
    return () => clearTimeout(t);
  }, [notice]);

  // The task whose artifacts the apps panel shows, and that task's remembered panel state (AFW-A09).
  const artifactCtx = useMemo(() => ({ browsingTaskId: browsing?.taskId ?? null }), [browsing?.taskId]);
  const panelTaskId = useMemo(() => panelTaskFor(tasks, reviewing ?? browsing?.taskId ?? selectedTaskId, artifactCtx), [tasks, reviewing, browsing?.taskId, selectedTaskId, artifactCtx]);
  const panelTask = panelTaskId ? model.tasks.get(panelTaskId) : undefined;
  const artifacts = useMemo(() => artifactsOf(panelTask, artifactCtx), [panelTask, artifactCtx]);
  const panelState: PanelState = (panelTaskId ? prefs.panelByTask[panelTaskId] : undefined) ?? { open: false, tab: artifacts[0] ?? "changes" };
  const panelOpen = panelTaskId !== null && panelState.open;
  const setPanelState = useCallback(
    (taskId: string, next: PanelState) => setPrefs((p) => {
      const rest = Object.fromEntries(Object.entries(p.panelByTask).filter(([id]) => id !== taskId));
      return { ...p, panelByTask: { ...rest, [taskId]: next } };
    }),
    [setPrefs],
  );

  const toFleet = useCallback(
    (then: () => void) => {
      const overlay = reviewing !== null || browsing !== null;
      setReviewing(null);
      setBrowsing(null);
      // The fleet is re-shown on the next render; act on it after that.
      if (overlay) setTimeout(then, 0);
      else then();
    },
    [reviewing, browsing, setReviewing, setBrowsing],
  );
  const focusTestId = (id: string) => {
    const el = document.querySelector<HTMLElement>(`[data-testid="${id}"]`);
    el?.scrollIntoView({ block: "center" });
    el?.focus();
  };

  const appAvailability = (kind: AppKind): true | string => {
    if (!panelTaskId) return "No task has an artifact to show yet";
    if ((kind === "changes" || kind === "browser") && !artifacts.includes(kind)) return `The task has no ${kind === "changes" ? "change set" : "browser session"} to show yet`;
    return true;
  };

  const actions: ShellActions = {
    focusNewTask: () => toFleet(() => void runFleetCommand("newTask")),
    focusFilter: () => toFleet(() => void runFleetCommand("search")),
    jumpAttention: () => toFleet(() => void runFleetCommand("jumpAttention")),
    jumpRunning: () => toFleet(() => void runFleetCommand("jumpRunning")),
    openPalette: () => setPaletteOpen(true),
    toggleHelp: () => setHelpOpen((h) => !h),
    openSettings: () => toFleet(() => focusTestId("settings")),
    focusSettingsSection: (id) => toFleet(() => (document.querySelector<HTMLElement>(`[data-testid="${id}"] input`) ?? document.querySelector<HTMLElement>(`[data-testid="${id}"]`))?.focus()),
    toggleAgentList: () => setPrefs((p) => ({ ...p, listCollapsed: !p.listCollapsed })),
    togglePanel: () => {
      if (panelTaskId) setPanelState(panelTaskId, { ...panelState, open: !panelState.open });
    },
    showApp: (kind) => {
      const ok = appAvailability(kind);
      if (ok === true && panelTaskId) setPanelState(panelTaskId, { open: kind === "terminal" ? !(panelState.open && panelState.tab === "terminal") : true, tab: kind });
      return ok;
    },
    openDashboard: () => setDashboardOpen(true),
    goBack: () => void runFleetCommand("back"),
    setTheme: (theme) => setPrefs((p) => ({ ...p, theme })),
    zoom: (delta) => setPrefs((p) => ({ ...p, zoom: delta === 0 ? 1 : clampZoom(p.zoom + delta * ZOOM_STEP) })),
    zoomAvailability: () => (browsing ? "Zoom is unavailable while the browser view is open" : true),
    appAvailability,
    canGoBack: () => reviewing !== null || browsing !== null || dashboardOpen,
  };
  const actionsRef = useRef(actions);
  actionsRef.current = actions;
  const confirmRef = useRef(confirm);
  confirmRef.current = confirm;

  const runCommand = useCallback((id: string) => {
    const why = shellRegistry.availability(id, actionsRef.current);
    if (why !== true) {
      setNotice(`${shellRegistry.get(id)?.title ?? id} is unavailable: ${why}`);
      return;
    }
    void shellRegistry.run(id, actionsRef.current);
  }, []);

  // The registry's chords are the shell's global key scope. A pending irreversible confirmation owns the keyboard.
  const globalScope = useMemo<KeyScope>(
    () => ({
      id: "shell",
      blocking: false,
      resolve: (e, mac) => {
        if (confirmRef.current) return null;
        const def = shellRegistry.forEvent(e, mac);
        return def ? () => runCommand(def.id) : null;
      },
    }),
    [runCommand],
  );
  useKeyScope(true, globalScope);

  const focusTask = useCallback(
    (taskId: string) => {
      setSelectedTaskId(taskId);
      toFleet(() => {
        const el = document.querySelector<HTMLElement>(`[data-testid="task-card"][data-task-id="${taskId}"]`);
        el?.scrollIntoView({ block: "center" });
        el?.focus();
      });
    },
    [setSelectedTaskId, toFleet],
  );

  const paletteItems = useMemo<PaletteItem[]>(() => {
    const agentItems: PaletteItem[] = tasks.map((t) => ({ id: `agent:${t.taskId}`, kind: "agent", title: t.goalText || t.taskId, detail: t.state, keywords: [t.taskId, t.state], run: () => focusTask(t.taskId) }));
    const commandItems: PaletteItem[] = shellRegistry
      .all()
      .filter((d) => d.palette !== false)
      .map((d) => {
        const why = shellRegistry.availability(d.id, actionsRef.current);
        const isTheme = d.id.startsWith("theme.");
        return {
          id: d.id,
          kind: d.group === "Settings" ? ("setting" as const) : ("action" as const),
          title: d.title,
          detail: isTheme && prefs.theme === d.id.slice(6) ? "current" : d.group,
          keywords: d.keywords,
          chord: d.keys?.[0],
          disabled: why === true ? undefined : why,
          run: () => runCommand(d.id),
        };
      });
    return [...agentItems, ...commandItems];
    // actionsRef is a ref: availability is re-read on each palette open via `paletteOpen`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tasks, prefs.theme, focusTask, runCommand, paletteOpen, panelTaskId, artifacts, browsing, reviewing, dashboardOpen]);

  const menuItems = useMemo<MenuItem[]>(() => {
    const fromRegistry = (id: string, extra: Partial<MenuItem> = {}): MenuItem => {
      const d = shellRegistry.get(id)!;
      const why = shellRegistry.availability(id, actionsRef.current);
      return { id, label: d.title, shortcut: d.keys?.[0], disabled: why !== true, onSelect: () => runCommand(id), ...extra };
    };
    return [
      fromRegistry("palette.open"),
      fromRegistry("help.shortcuts"),
      fromRegistry("settings.open"),
      ...THEME_PREFERENCES.map((t, i) => fromRegistry(`theme.${t}`, { label: THEME_LABEL[t], checked: prefs.theme === t, radio: true, separatorBefore: i === 0 })),
      fromRegistry("zoom.in", { separatorBefore: true }),
      fromRegistry("zoom.out"),
      fromRegistry("zoom.reset"),
    ];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefs.theme, runCommand, browsing]);

  const overlayTitle = reviewing ? model.tasks.get(reviewing)?.goalText : browsing ? model.tasks.get(browsing.taskId)?.goalText : undefined;
  const title = dashboardOpen ? "Dashboard" : (overlayTitle ?? "Fleet");
  const panelUnavailable = panelTaskId === null ? "no task has an artifact yet" : null;
  const coreLabel = core.state === "connected" ? `Core connected (pid ${core.pid})` : core.state === "restarting" ? "Core restarting…" : core.state === "failed" ? "Core failed" : "Core starting…";
  const coreStatus = core.state === "connected" ? "ok" : core.state === "failed" ? "danger" : "warn";

  return (
    <>
      <ShellFrame
        prefs={prefs}
        onPrefs={setPrefs}
        panelOpen={panelOpen}
        panel={
          panelTaskId && panelTask ? (
            <AppsPanel
              taskId={panelTaskId}
              taskTitle={panelTask.goalText}
              artifacts={artifacts}
              tab={panelState.tab}
              onTab={(tab) => setPanelState(panelTaskId, { ...panelState, tab })}
              onOpenReview={() => setReviewing(panelTaskId)}
              onOpenBrowser={() => void openBrowser(panelTaskId)}
            />
          ) : null
        }
        agents={(rail) => (
          <AgentRegion
            rail={rail}
            canExpand={prefs.listCollapsed}
            taskCount={tasks.length}
            attentionCount={cols.needsAttention.length}
            onNewTask={() => actions.focusNewTask()}
            onExpand={() => actions.toggleAgentList()}
          />
        )}
        topBar={
          <TopBar
            title={title}
            location={LOCATION}
            agentListCollapsed={prefs.listCollapsed}
            onToggleAgentList={() => actions.toggleAgentList()}
            canGoBack={actions.canGoBack()}
            onBack={() => actions.goBack()}
            menuItems={menuItems}
            dashboardOpen={dashboardOpen}
            dashboardDisabled={!model.sessionId}
            onToggleDashboard={() => setDashboardOpen((o) => !o)}
            panelOpen={panelOpen}
            panelUnavailable={panelUnavailable}
            onTogglePanel={() => actions.togglePanel()}
          />
        }
        tray={<TrayHost />}
        statusRow={
          <StatusRow
            coreLabel={coreLabel}
            coreStatus={coreStatus}
            platform={platform}
            location={LOCATION}
            extra={
              <span className="meta" role="status" aria-live="polite" data-testid="shell-notice">
                {notice}
              </span>
            }
          />
        }
      >
        <FleetView app={app} />
      </ShellFrame>
      <CommandPalette open={paletteOpen} items={paletteItems} onClose={() => setPaletteOpen(false)} announce={setNotice} />
      <ShortcutHelp open={helpOpen} onClose={() => setHelpOpen(false)} registry={shellRegistry} />
    </>
  );
}
