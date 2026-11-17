/**
 * The agent-first shell (REQ-PX-045): three regions, a top bar, the apps-panel
 * host, the tray host, the command palette and the shortcut registry, wired
 * over the renderer's existing state (`useApp`). The centre shows the home
 * composer, a conversation or one page at a time. The renderer holds no
 * authority (docs/81): every Core effect is a typed preload call made by those
 * screens.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { TrayHost, TrayStore, chordMatches, isMacPlatform, useEscapeLayers, useKeyDispatch, useKeyScope, type MenuItem } from "@modbit/ui";
import type { KeyScope } from "@modbit/ui/logic";
import { GEOMETRY, THEME_LABEL, THEME_PREFERENCES, type Density, type ThemePreference } from "@modbit/design-tokens";
import type { AppState } from "../state/use-app.ts";
import { AgentList, type TaskActions } from "../agents/agent-list.tsx";
import { ProjectDetail } from "../projects/project-detail.tsx";
import { useProjects } from "../projects/use-projects.ts";
import { WorktreesPanel } from "../worktrees/worktrees-panel.tsx";
import { Customize } from "../customize/customize.tsx";
import { useWorktrees } from "../worktrees/use-worktrees.ts";
import { Conversation } from "../conversation/conversation.tsx";
import { Browser } from "../browser/browser.tsx";
import { Dashboard } from "../dashboard/dashboard.tsx";
import { Home } from "../home/home.tsx";
import { Review } from "../review/review.tsx";
import { SettingsNav } from "../settings/settings-nav.tsx";
import { SettingsPage } from "../settings/settings-page.tsx";
import { Inbox } from "../approvals/inbox.tsx";
import { useNeedsYou } from "../approvals/use-needs-you.ts";
import { childrenOf } from "../model.ts";
import { taskState } from "../screens.ts";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import { StatusRegion } from "./status-region.tsx";
import { AgentRegion, PersonRow } from "./agent-region.tsx";
import { useTerminals } from "../apps/use-terminals.ts";
import { recoverySummary } from "../../main/lifecycle.ts";
import { AppsPanel } from "./apps-panel.tsx";
import { Automations } from "../automations/automations.tsx";
import { APP_LABEL, artifactsOf, panelTaskFor, panesOf } from "./artifacts.ts";
import { shellRegistry, type ShellActions } from "./commands.ts";
import { CommandPalette, type PaletteItem, type PaletteSearchProps } from "./palette.tsx";
import { clampZoom, EMPTY_DOCK, loadUiPrefs, saveUiPrefs, ZOOM_STEP, type AppKind, type PanelState, type UiPrefs } from "./prefs.ts";
import { dockedOf, openPane, paneStateOf, popOutPane, tiledOf, toggleDock, togglePane } from "./dock.ts";
import { paneView } from "./pane-view.ts";
import type { PaneWindowKind, PaneWindowRef } from "../../preload/preload.ts";
import { followDelivered } from "../apps/delivered.ts";
import { ShellFrame } from "./shell-frame.tsx";
import type { Layout } from "./layout.ts";
import { focusedTask, openInSplit, remainingAfterClose, requestSplit, splitRefusal, SPLIT_LAYOUT, SPLIT_NEED } from "./split.ts";
import { SplitView, type SplitColumn } from "./split-view.tsx";
import { ShortcutHelp } from "./shortcut-help.tsx";
import { TrayDirector } from "../context/tray-director.tsx";
import { TopBar, type PaneToggle } from "./top-bar.tsx";
import { PermissionsDialog } from "../computer/permissions-dialog.tsx";
import { PresenceBar, STOP_CHORD, useComputerStop } from "../computer/presence-bar.tsx";
import { holdsMachine, permissionsNeeded } from "../computer/model.ts";
import { useComputerPresence } from "../computer/use-computer.ts";
import { rowFactsOf } from "../agents/row-facts.ts";

/** A value that follows `value` at most once per `ms` (a task emits events in bursts; the apps read the Core once per beat). */
function useThrottled<T>(value: T, ms: number): T {
  const [out, setOut] = useState(value);
  useEffect(() => {
    if (Object.is(out, value)) return;
    const t = setTimeout(() => setOut(value), ms);
    return () => clearTimeout(t);
  }, [value, out, ms]);
  return out;
}

export function applyTheme(theme: ThemePreference): void {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

/** Comfortable is the token stylesheet's own block; Compact is its override block, chosen by this attribute (MWS-03). */
export function applyDensity(density: Density): void {
  const root = document.documentElement;
  if (density === "comfortable") delete root.dataset.density;
  else root.dataset.density = density;
}

export function Shell({ app }: { app: AppState }) {
  const { core, platform, model, tasks, cols, reviewing, browsing, dashboardOpen, setDashboardOpen, setReviewing, setBrowsing, openBrowser, selectedTaskId, setSelectedTaskId, helpOpen, setHelpOpen, runKeyCommand, confirm, startTask, setConfirm, attentionItems, navRequest, setWorkspaceRoot } = app;
  const [prefs, setPrefsState] = useState<UiPrefs>(loadUiPrefs);
  const setPrefs = useCallback((update: (p: UiPrefs) => UiPrefs) => setPrefsState(update), []);
  const [paletteOpen, setPaletteOpen] = useState(false);
  // REQ-PX-209: a file the palette asked the Files pane to show (n: each request is acted on once).
  const [fileRequest, setFileRequest] = useState<{ taskId: string; path: string; n: number } | null>(null);
  const [notice, setNotice] = useState("");
  // REQ-PX-086: the Automations surface fills the centre region while it is open.
  const [automationsOpen, setAutomationsOpen] = useState(false);
  const [customizeOpen, setCustomizeOpen] = useState(false);
  // AUT-E03: a task an automation created lives in the run's own session; its session id is the Core's (ListAutomationRuns by task).
  const [runSessions, setRunSessions] = useState<Record<string, string>>({});
  const [inboxOpen, setInboxOpen] = useState(false);
  const [inboxFocus, setInboxFocus] = useState(0);
  // A request to focus Inbox's first item belongs to one opening of the page.
  useEffect(() => {
    if (!inboxOpen) setInboxFocus(0);
  }, [inboxOpen]);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsQuery, setSettingsQuery] = useState("");
  const [settingsSection, setSettingsSection] = useState<string | null>(null);
  // PX-046/047: the conversation open in the centre region (null = the home composer). `rowId` is a search hit to scroll to.
  // REQ-PX-204: a split stored by the last session opens again with the task that had the keyboard in the centre.
  const [conversation, setConversation] = useState<{ taskId: string; rowId: string | null } | null>(() => (prefs.split ? { taskId: focusedTask(prefs.split), rowId: null } : null));
  // The conversation Back left, for Forward.
  const [leftConversation, setLeftConversation] = useState<string | null>(null);
  const [headers, setHeaders] = useState<readonly AgentHeaderView[]>([]);
  const [topEndSlot, setTopEndSlot] = useState<HTMLSpanElement | null>(null);
  // MWS-09: the details rail's element while the frame draws it; the open conversation fills it.
  const [railSlot, setRailSlot] = useState<HTMLElement | null>(null);
  // MWS-05: the open task's title menu pins and archives through the task list, which owns both.
  const [taskActions, setTaskActions] = useState<TaskActions | null>(null);
  const [userName, setUserName] = useState("");
  // PX-064: a project's page and PX-068: the worktree list take the centre like a conversation does.
  const [projectPage, setProjectPage] = useState<string | null>(null);
  const [worktreesOpen, setWorktreesOpen] = useState(false);
  const connected = core.state === "connected";
  // REQ-PX-206 (the owner's review, 2026-11-17): what waits on the person, read from the Core once, here. The task list's Inbox badge
  // and Inbox's "Needs you · N" are the same number: the rows Needs you draws. Before the Core's attention view is read, no item is counted.
  const needsYou = useNeedsYou(model.sessionId, connected, app.attentionLoaded ? attentionItems : [], inboxOpen);
  // PX-074: who holds the machine and which permissions the helper reports, as the Core says (nothing of it is kept here).
  const presence = useComputerPresence(connected);
  const computerStop = useComputerStop(model.sessionId, presence.refresh);
  const [permissionsOpen, setPermissionsOpen] = useState(false);
  const projects = useProjects(connected);
  const worktrees = useWorktrees(connected && worktreesOpen);
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    if (!worktreesOpen) return;
    setNowMs(Date.now());
    const t = setInterval(() => setNowMs(Date.now()), 30_000);
    return () => clearInterval(t);
  }, [worktreesOpen, worktrees.list]);
  useEscapeLayers();
  useKeyDispatch();
  useEffect(() => {
    void window.modbit.localState().then((l) => setUserName((l as { userName?: string }).userName ?? "")).catch(() => {});
  }, []);

  // Stop is never behind a dialog: this listener sits beside the key dispatcher and answers the chord even while a modal blocks the shell's other keys.
  const controlsRef = useRef(presence.info?.controls ?? []);
  controlsRef.current = presence.info?.controls ?? [];
  const stopAllRef = useRef(computerStop.stopAll);
  stopAllRef.current = computerStop.stopAll;
  useEffect(() => {
    const onKey = (e: KeyboardEvent): void => {
      if (e.defaultPrevented || !chordMatches(e, STOP_CHORD, isMacPlatform())) return;
      if (!controlsRef.current.some(holdsMachine)) return;
      e.preventDefault();
      e.stopImmediatePropagation();
      void stopAllRef.current(controlsRef.current);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  // CUC-E05: a call blocked by a missing permission routes the permission window once, in this (the root workspace) window only.
  const routedPermissions = useRef(false);
  const [blockedEpisodes, setBlockedEpisodes] = useState(0);
  useEffect(() => {
    const off = window.modbit.onEvent((raw) => {
      const e = raw as { eventType?: string; payload?: { failure_code?: string } | null };
      if (e.eventType === "ToolCallFailed" && e.payload?.failure_code === "PERMISSION_REQUIRED") setBlockedEpisodes((n) => n + 1);
    });
    return off;
  }, []);
  useEffect(() => {
    if (blockedEpisodes === 0) return;
    void presence.refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [blockedEpisodes]);
  useEffect(() => {
    if (blockedEpisodes === 0 || routedPermissions.current || !permissionsNeeded(presence.info)) return;
    routedPermissions.current = true;
    setPermissionsOpen(true);
  }, [blockedEpisodes, presence.info]);

  useEffect(() => saveUiPrefs(prefs), [prefs]);
  useEffect(() => applyTheme(prefs.theme), [prefs.theme]);
  useEffect(() => applyDensity(prefs.density), [prefs.density]);
  useEffect(() => {
    document.documentElement.style.zoom = prefs.zoom === 1 ? "" : String(prefs.zoom);
  }, [prefs.zoom]);
  useEffect(() => {
    if (!notice) return;
    const t = setTimeout(() => setNotice(""), 8000);
    return () => clearTimeout(t);
  }, [notice]);

  // The task whose artifacts the apps panel shows, and that task's remembered panel state (AFW-A09).
  const anyRunning = useMemo(() => tasks.some((t) => t.state === "Running"), [tasks]);
  const { byTask: terminalsByTask, refresh: refreshTerminals } = useTerminals(model.sessionId !== null, anyRunning, model.cursor);
  const terminalTaskIds = useMemo(() => new Set(terminalsByTask.keys()), [terminalsByTask]);
  const artifactCtx = useMemo(() => ({ browsingTaskId: browsing?.taskId ?? null, terminalTaskIds }), [browsing?.taskId, terminalTaskIds]);
  // The terminal each task's panel shows (a view choice; the Core owns the terminals).
  const [terminalPick, setTerminalPick] = useState<Record<string, string>>({});
  const panelTaskId = useMemo(() => panelTaskFor(tasks, reviewing ?? browsing?.taskId ?? selectedTaskId, artifactCtx), [tasks, reviewing, browsing?.taskId, selectedTaskId, artifactCtx]);
  const panelTask = panelTaskId ? model.tasks.get(panelTaskId) : undefined;
  const artifacts = useMemo(() => artifactsOf(panelTask, artifactCtx), [panelTask, artifactCtx]);
  // MWS-10: the task's dock. The panes it can show now, the ones it has open, and the ones in the grid (open, not minimized).
  const panelState: PanelState = (panelTaskId ? prefs.panelByTask[panelTaskId] : undefined) ?? EMPTY_DOCK;
  const available = useMemo(() => panesOf(artifacts), [artifacts]);
  // REQ-PX-203: the panes in windows of their own, as the main process lists them. A pane is in one place at a time: one of the
  // dock's task's that is in a window is not in the grid, and its toggle brings its window forward.
  const [paneWindows, setPaneWindows] = useState<PaneWindowRef[]>([]);
  useEffect(() => {
    let live = true;
    void window.modbit.paneWindows().then((l) => live && setPaneWindows(l), () => {});
    const off = window.modbit.onPaneWindows((l) => setPaneWindows(l));
    return () => {
      live = false;
      off();
    };
  }, []);
  const popped = useMemo(() => new Set<AppKind>(paneWindows.filter((w) => w.taskId === panelTaskId).map((w) => w.kind)), [paneWindows, panelTaskId]);
  const dockable = useMemo(() => available.filter((k) => !popped.has(k)), [available, popped]);
  const tiled = useMemo(() => dockedOf(tiledOf(panelState, available), popped), [panelState, available, popped]);
  const panelOpen = panelTaskId !== null && tiled.length > 0;
  const setPanelState = useCallback(
    (taskId: string, next: PanelState | ((s: PanelState) => PanelState)) => setPrefs((p) => {
      const rest = Object.fromEntries(Object.entries(p.panelByTask).filter(([id]) => id !== taskId));
      return { ...p, panelByTask: { ...rest, [taskId]: typeof next === "function" ? next(p.panelByTask[taskId] ?? EMPTY_DOCK) : next } };
    }),
    [setPrefs],
  );
  // The Activity pane lists what this window was given: the events are kept as they arrive, whether or not the pane is open.
  useEffect(() => followDelivered(), []);

  /** Closes every page; what is left is the conversation, or the home composer without one. */
  const closePages = useCallback(() => {
    setReviewing(null);
    setBrowsing(null);
    setDashboardOpen(false);
    setAutomationsOpen(false);
    setInboxOpen(false);
    setCustomizeOpen(false);
    setSettingsOpen(false);
    setProjectPage(null);
    setWorktreesOpen(false);
  }, [setReviewing, setBrowsing, setDashboardOpen]);
  /** Shows the home composer and acts on it once it is on screen. */
  const toHome = useCallback(
    (then: () => void) => {
      const elsewhere = reviewing !== null || browsing !== null || dashboardOpen || conversation !== null || automationsOpen || inboxOpen || customizeOpen || settingsOpen || projectPage !== null || worktreesOpen;
      closePages();
      if (conversation) setLeftConversation(conversation.taskId);
      setConversation(null);
      setPrefs((p) => (p.split ? { ...p, split: null } : p));
      if (elsewhere) setTimeout(then, 0);
      else then();
    },
    [reviewing, browsing, dashboardOpen, conversation, automationsOpen, inboxOpen, customizeOpen, settingsOpen, projectPage, worktreesOpen, closePages, setPrefs],
  );
  /** Opens a task's conversation (or, with null, the home composer). */
  const openConversation = useCallback(
    (taskId: string | null, rowId?: string) => {
      closePages();
      if (taskId === null) {
        setConversation(null);
        setPrefs((p) => (p.split ? { ...p, split: null } : p));
        return;
      }
      setSelectedTaskId(taskId);
      // REQ-PX-204: with two tasks side by side, a task already there takes the keyboard and another takes the focused column's place.
      setPrefs((p) => (p.split ? { ...p, split: openInSplit(p.split, taskId) } : p));
      setConversation({ taskId, rowId: rowId ?? null });
    },
    [closePages, setSelectedTaskId, setPrefs],
  );
  // REQ-PX-203: a pane window that closes hands back what it showed, and the pane is in the dock again, in its place; a pane
  // window that asks for the task's review, a call or the waiting approval has it shown here.
  useEffect(() => {
    const offReturned = window.modbit.onPaneReturned((r) => {
      paneView.put(r.taskId, r.kind, r.state);
      const tid = r.state.terminalId;
      if (r.kind === "terminal" && tid) setTerminalPick((m) => ({ ...m, [r.taskId]: tid }));
      setPanelState(r.taskId, (s) => openPane(s, r.kind));
      setPaneWindows((l) => l.filter((w) => !(w.taskId === r.taskId && w.kind === r.kind)));
    });
    const offShow = window.modbit.onPaneShow((r) => {
      if (r.what === "review") setReviewing(r.taskId);
      else if (r.what === "call" && r.toolCallId) openConversation(r.taskId, `tool:${r.toolCallId}`);
      else openConversation(r.taskId);
    });
    return () => {
      offReturned();
      offShow();
    };
  }, [setPanelState, setReviewing, openConversation]);
  useEffect(() => {
    if (!navRequest) return;
    if (navRequest.kind === "task") openConversation(navRequest.taskId);
    else if (navRequest.kind === "palette") setPaletteOpen(true);
    else {
      closePages();
      setInboxOpen(true);
      // Inbox focuses its first item once it has read the Core's lists (REQ-PX-206).
      setInboxFocus((n) => n + 1);
    }
    // Each request is acted on once.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [navRequest]);
  const openedTaskId = conversation?.taskId ?? null;
  useEffect(() => {
    if (!openedTaskId || model.tasks.has(openedTaskId) || runSessions[openedTaskId] !== undefined || core.state !== "connected") return;
    let live = true;
    window.modbit
      .automationRuns({ taskId: openedTaskId, limit: 1 })
      .then((r) => {
        const sid = r[0]?.sessionId;
        if (live && sid) setRunSessions((m) => ({ ...m, [openedTaskId]: sid }));
      })
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [openedTaskId, model.tasks, runSessions, core.state]);
  /** Opens a project's page in the centre region. */
  const openProject = useCallback(
    (projectId: string) => {
      closePages();
      setConversation(null);
      setProjectPage(projectId);
    },
    [closePages],
  );
  const openWorktrees = useCallback(() => {
    closePages();
    setConversation(null);
    setWorktreesOpen(true);
  }, [closePages]);
  const openSettings = useCallback((then?: () => void) => {
    closePages();
    setSettingsOpen(true);
    if (then) setTimeout(then, 0);
  }, [closePages]);
  const appAvailability = (kind: AppKind): true | string => {
    if (!panelTaskId) return "No task has an artifact to show yet";
    const started = artifacts.includes("files");
    if (kind === "activity" || kind === "background" || kind === "pull-request") return started ? true : "The task has not started yet";
    if (!artifacts.includes(kind)) return { changes: "The task has no change set to show yet", terminal: "The task has no terminal", browser: "The task has no browser session to show yet", files: "The task has not started, so it has no workspace to browse", evidence: "The task has not started, so it has no evidence yet" }[kind];
    return true;
  };

  const actions: ShellActions = {
    focusNewTask: () => toHome(() => void runKeyCommand("newTask")),
    focusFilter: () => setPaletteOpen(true),
    jumpAttention: () => void runKeyCommand("jumpAttention"),
    jumpRunning: () => void runKeyCommand("jumpRunning"),
    openPalette: () => setPaletteOpen(true),
    toggleHelp: () => setHelpOpen((h) => !h),
    openSettings: () => openSettings(() => document.querySelector<HTMLElement>('[data-testid="settings"]')?.focus()),
    focusSettingsSection: (id) => openSettings(() => (document.querySelector<HTMLElement>(`[data-testid="${id}"] input`) ?? document.querySelector<HTMLElement>(`[data-testid="${id}"]`))?.focus()),
    toggleAgentList: () => setPrefs((p) => ({ ...p, listCollapsed: !p.listCollapsed })),
    togglePanel: () => {
      if (panelTaskId) setPanelState(panelTaskId, (s) => toggleDock(s, dockable, dockable[0]));
    },
    showApp: (kind) => {
      const ok = appAvailability(kind);
      // A pane in a window of its own: its window comes forward.
      if (ok === true && panelTaskId && popped.has(kind)) void window.modbit.focusPaneWindow(panelTaskId, kind as PaneWindowKind);
      // The terminal's command toggles its pane; the others bring theirs into the grid.
      else if (ok === true && panelTaskId) setPanelState(panelTaskId, (s) => (kind === "terminal" ? togglePane(s, kind) : openPane(s, kind)));
      return ok;
    },
    openDashboard: () => {
      closePages();
      setDashboardOpen(true);
    },
    openAutomations: () => {
      closePages();
      setAutomationsOpen(true);
    },
    goBack: () => {
      // Back closes the page on top, then leaves the conversation for the home composer.
      if (settingsOpen) setSettingsOpen(false);
      else if (customizeOpen) setCustomizeOpen(false);
      else if (inboxOpen) setInboxOpen(false);
      else if (automationsOpen) setAutomationsOpen(false);
      else if (dashboardOpen) setDashboardOpen(false);
      else if (projectPage && !reviewing && !browsing) setProjectPage(null);
      else if (worktreesOpen && !reviewing && !browsing) setWorktreesOpen(false);
      else if (conversation && !reviewing && !browsing) {
        setLeftConversation(conversation.taskId);
        setConversation(null);
        setPrefs((p) => (p.split ? { ...p, split: null } : p));
      } else void runKeyCommand("back");
    },
    setTheme: (theme) => setPrefs((p) => ({ ...p, theme })),
    zoom: (delta) => setPrefs((p) => ({ ...p, zoom: delta === 0 ? 1 : clampZoom(p.zoom + delta * ZOOM_STEP) })),
    zoomAvailability: () => (browsing ? "Zoom is unavailable while the browser view is open" : true),
    appAvailability,
    stopComputer: () => void computerStop.stopAll(presence.info?.controls ?? []),
    computerStopAvailability: () => ((presence.info?.controls ?? []).some(holdsMachine) ? true : "Nothing is controlling this computer"),
    openComputerPermissions: () => setPermissionsOpen(true),
    canGoBack: () => reviewing !== null || browsing !== null || dashboardOpen || conversation !== null || automationsOpen || inboxOpen || customizeOpen || settingsOpen || projectPage !== null || worktreesOpen,
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
  // A pending confirmation owns Enter and Escape: a docked approval card behind it must not take them as Run or Skip,
  // even a card that docks after the confirmation opened.
  const confirmScope = useMemo<KeyScope>(
    () => ({
      id: "confirmation",
      blocking: false,
      priority: 1,
      resolve: (e) => (e.key === "Enter" ? () => void runKeyCommand("confirm") : e.key === "Escape" ? () => void runKeyCommand("back") : null),
    }),
    [runKeyCommand],
  );
  useKeyScope(confirm !== null, confirmScope);

  // The split opener is defined with the split's state below; the palette's items call it through this ref.
  const openSplitRef = useRef<(taskId: string) => void>(() => {});
  const rowFacts = useMemo(() => new Map(tasks.map((t) => [t.taskId, rowFactsOf(t, attentionItems)])), [tasks, attentionItems]);
  const paletteItems = useMemo<PaletteItem[]>(() => {
    // MWS-11: Alt+Enter opens the task beside the one in the centre, as Alt+click on its row does.
    const agentItems: PaletteItem[] = tasks.map((t) => ({ id: `agent:${t.taskId}`, kind: "agent", title: t.goalText || t.taskId, detail: t.state, keywords: [t.taskId, t.state], run: () => openConversation(t.taskId), runAlt: () => openSplitRef.current(t.taskId) }));
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
    // PX-064 / PX-068: the project and worktree surfaces, reachable by name.
    const surfaceItems: PaletteItem[] = [
      { id: "worktrees.open", kind: "action", title: "Open worktrees", detail: "Worktrees", keywords: ["worktree", "cleanup", "apply", "checkout"], run: () => openWorktrees() },
      ...projects.projects.filter((p) => !p.archived).map((p): PaletteItem => ({ id: `project:${p.projectId}`, kind: "action", title: `Open project ${p.name}`, detail: "Projects", keywords: ["project", p.name], run: () => openProject(p.projectId) })),
    ];
    return [...agentItems, ...surfaceItems, ...commandItems];
    // actionsRef is a ref: availability is re-read on each palette open via `paletteOpen`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [tasks, prefs.theme, openConversation, runCommand, paletteOpen, panelTaskId, artifacts, browsing, reviewing, dashboardOpen, projects.projects, openProject, openWorktrees]);

  const menuItems = useMemo<MenuItem[]>(() => {
    const fromRegistry = (id: string, extra: Partial<MenuItem> = {}): MenuItem => {
      const d = shellRegistry.get(id)!;
      const why = shellRegistry.availability(id, actionsRef.current);
      return { id, label: d.title, shortcut: d.keys?.[0], disabled: why !== true, onSelect: () => runCommand(id), ...extra };
    };
    return [
      { id: "open-dashboard", label: "Dashboard", disabled: !model.sessionId, onSelect: () => runCommand("view.dashboard") },
      { id: "open-worktrees", label: "Worktrees", onSelect: () => openWorktrees() },
      fromRegistry("palette.open"),
      fromRegistry("help.shortcuts"),
      ...THEME_PREFERENCES.map((t, i) => fromRegistry(`theme.${t}`, { label: THEME_LABEL[t], checked: prefs.theme === t, radio: true, separatorBefore: i === 0 })),
      fromRegistry("zoom.in", { separatorBefore: true }),
      fromRegistry("zoom.out"),
      fromRegistry("zoom.reset"),
    ];
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [prefs.theme, runCommand, browsing, model.sessionId, openWorktrees]);

  const overlayTitle = reviewing ? model.tasks.get(reviewing)?.goalText : browsing ? model.tasks.get(browsing.taskId)?.goalText : conversation ? model.tasks.get(conversation.taskId)?.goalText : undefined;
  const openedProject = projectPage ? projects.projects.find((p) => p.projectId === projectPage) : undefined;
  const title = settingsOpen ? "Settings" : customizeOpen ? "Customize" : inboxOpen ? "Inbox" : automationsOpen ? "Automations" : dashboardOpen ? "Dashboard" : reviewing || browsing ? (overlayTitle ?? "Task") : projectPage ? (openedProject?.name ?? "Project") : worktreesOpen ? "Worktrees" : (overlayTitle ?? "New task");
  const openedHeader = conversation ? headers.find((h) => h.taskId === conversation.taskId) : undefined;
  const panelRefreshKey = useThrottled(`${panelTask?.state ?? ""}:${panelTask?.lastOffset ?? ""}`, 1200);
  // MWS-05: one toggle per pane the task in the centre can show, each with its state. A page that is not a task, and a task that has not started, have none.
  const shownTaskId = settingsOpen || customizeOpen || inboxOpen || automationsOpen || dashboardOpen ? null : (reviewing ?? browsing?.taskId ?? (projectPage || worktreesOpen ? null : (conversation?.taskId ?? null)));
  const panes = useMemo<PaneToggle[]>(() => (shownTaskId !== null && shownTaskId === panelTaskId ? available.map((kind) => ({ kind, open: paneStateOf(panelState, kind, popped) === "open", state: paneStateOf(panelState, kind, popped) })) : []), [shownTaskId, panelTaskId, available, panelState, popped]);
  const onPane = (kind: AppKind) => {
    if (!panelTaskId) return;
    // A pane in a window of its own: its toggle brings the window forward.
    if (popped.has(kind)) void window.modbit.focusPaneWindow(panelTaskId, kind as PaneWindowKind);
    else setPanelState(panelTaskId, (s) => togglePane(s, kind));
  };
  // Where the title bar has no toggles for the dock's task (the centre shows home or another task's page), the dock carries them itself.
  const dockToggles = useMemo<PaneToggle[]>(() => (panes.length === 0 && panelTaskId !== null ? available.map((kind) => ({ kind, open: paneStateOf(panelState, kind, popped) === "open", state: paneStateOf(panelState, kind, popped) })) : []), [panes.length, panelTaskId, available, panelState, popped]);
  /** REQ-PX-203: Pop out. The pane leaves the grid for a window of its own, showing what it showed here. */
  const popOut = (taskId: string, title: string, kind: PaneWindowKind, size: { width: number; height: number } | undefined) => {
    const sid = model.sessionId;
    if (!sid) return;
    const terminals = terminalsByTask.get(taskId) ?? [];
    const state = kind === "terminal" ? { terminalId: terminalPick[taskId] ?? terminals[0]?.terminalId } : paneView.get(taskId, kind);
    setPanelState(taskId, (s) => popOutPane(s, kind));
    void window.modbit
      .popOutPane(sid, taskId, kind, Object.fromEntries(Object.entries(state).filter(([, v]) => v !== undefined && v !== "")), title, size)
      .then((l) => {
        setPaneWindows(l);
        setNotice(`${APP_LABEL[kind]} is in its own window. Closing that window brings it back here.`);
      })
      .catch((e: Error) => setNotice(`${APP_LABEL[kind]} could not be popped out: ${e.message}`));
  };
  // The title is the task's while its conversation is what the centre shows; then it opens the task's menu.
  const menuTaskId = conversation && shownTaskId === conversation.taskId && !reviewing && !browsing ? conversation.taskId : null;
  const taskMenu = useMemo<MenuItem[] | undefined>(() => {
    if (!menuTaskId || !taskActions) return undefined;
    const pinned = taskActions.pins.includes(menuTaskId);
    const items: MenuItem[] = [{ id: "task-pin", label: pinned ? "Unpin" : "Pin", onSelect: () => (pinned ? taskActions.unpin(menuTaskId) : taskActions.pin(menuTaskId)) }];
    if (openedHeader && !openedHeader.archived) items.push({ id: "task-archive", label: "Archive", separatorBefore: true, onSelect: () => taskActions.archive(menuTaskId) });
    return items;
  }, [menuTaskId, taskActions, openedHeader]);

  // ---- REQ-PX-204 (MWS-11): split view. Two tasks side by side in the centre, one with the keyboard. The split is a view choice
  // stored with the layout preferences; `conversation` is always the task whose column has the keyboard.
  const split = prefs.split;
  const pageOpen = settingsOpen || customizeOpen || inboxOpen || automationsOpen || dashboardOpen || reviewing !== null || browsing !== null || projectPage !== null || worktreesOpen;
  const splitShown = split !== null && conversation !== null && !pageOpen && split.tasks.includes(conversation.taskId);
  const layoutRef = useRef<Layout | null>(null);
  // REQ-PX-209: the palette searches the Core's conversation index and the folder of the task open in the centre, once it has one.
  const fileTaskId = !pageOpen && conversation && conversation.taskId === panelTaskId && artifacts.includes("files") ? conversation.taskId : null;
  const fileTaskTitle = fileTaskId ? (model.tasks.get(fileTaskId)?.goalText ?? "") || "this task" : "";
  const paletteSearch = useMemo<PaletteSearchProps>(
    () => ({
      sessionId: model.sessionId,
      fileTask: fileTaskId ? { taskId: fileTaskId, title: fileTaskTitle } : null,
      onOpenMatch: (taskId, rowId) => openConversation(taskId, rowId),
      onOpenFile: (taskId, path) => {
        if (taskId !== panelTaskId) return;
        if (popped.has("files")) {
          void window.modbit.focusPaneWindow(taskId, "files");
          setNotice(`The Files pane is in a window of its own: ${path} is there to open.`);
          return;
        }
        setPanelState(taskId, (s) => openPane(s, "files"));
        setFileRequest((r) => ({ taskId, path, n: (r?.n ?? 0) + 1 }));
      },
    }),
    [model.sessionId, fileTaskId, fileTaskTitle, openConversation, panelTaskId, popped, setPanelState],
  );
  // Each task in split view docks its approval card, queue and context into a tray store of its own (one task uses the app's).
  const splitStores = useRef(new Map<string, TrayStore<ReactNode>>());
  const storeFor = (id: string): TrayStore<ReactNode> => {
    let s = splitStores.current.get(id);
    if (!s) {
      s = new TrayStore<ReactNode>();
      splitStores.current.set(id, s);
    }
    return s;
  };
  useEffect(() => {
    for (const id of [...splitStores.current.keys()]) if (!split?.tasks.includes(id)) splitStores.current.delete(id);
  }, [split]);
  // The task with the keyboard is the selected one after a reload too (the dock and the keyboard's commands follow it).
  useEffect(() => {
    if (prefs.split) setSelectedTaskId(focusedTask(prefs.split));
    // Once, for the split the last session stored.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  // A stored split whose task the Core no longer lists closes, with the task that is left in the centre.
  useEffect(() => {
    if (!split || (app.screen !== "populated" && app.screen !== "empty")) return;
    const missing = split.tasks.filter((id) => !model.tasks.has(id) && runSessions[id] === undefined);
    if (missing.length === 0) return;
    const kept = split.tasks.find((id) => !missing.includes(id)) ?? null;
    setPrefs((p) => ({ ...p, split: null }));
    setConversation((c) => (c && missing.includes(c.taskId) ? (kept ? { taskId: kept, rowId: null } : null) : c));
    setNotice("Split view closed: a task in it is no longer listed.");
  }, [split, model.tasks, app.screen, runSessions, setPrefs]);
  /** Alt+click on a row and the preview card: the task beside the one in the centre, or the reason it cannot be. */
  const openSplit = (taskId: string) => {
    const r = requestSplit(split, !pageOpen && conversation ? conversation.taskId : null, taskId, split?.ratio);
    if (!r.ok) {
      setNotice(r.reason);
      return;
    }
    const l = layoutRef.current;
    const avail = l ? l.centerPx + l.panelPx : SPLIT_NEED;
    if (avail < SPLIT_NEED) {
      setNotice(splitRefusal(avail));
      return;
    }
    setPrefs((p) => ({ ...p, split: r.split }));
    setSelectedTaskId(taskId);
    setConversation({ taskId, rowId: null });
  };
  openSplitRef.current = openSplit;
  const focusColumn = (column: 0 | 1) => {
    if (!split || split.focus === column) return;
    const id = split.tasks[column];
    setPrefs((p) => (p.split ? { ...p, split: { ...p.split, focus: column } } : p));
    setSelectedTaskId(id);
    setConversation({ taskId: id, rowId: null });
  };
  /** A column's Close: split view ends and the other task is the centre's, with the keyboard. */
  const closeColumn = (column: 0 | 1) => {
    if (!split) return;
    const rest = remainingAfterClose(split, column);
    setPrefs((p) => ({ ...p, split: null }));
    setSelectedTaskId(rest);
    setConversation({ taskId: rest, rowId: null });
    setTimeout(() => document.querySelector<HTMLElement>('[data-testid="conversation"] [data-testid="conv-scroll"]')?.focus(), 0);
  };
  /** A task's title menu (pin, archive), for the single view's title and each column's. */
  const taskMenuOf = (id: string): MenuItem[] | undefined => {
    if (!taskActions) return undefined;
    const pinned = taskActions.pins.includes(id);
    const items: MenuItem[] = [{ id: "task-pin", label: pinned ? "Unpin" : "Pin", onSelect: () => (pinned ? taskActions.unpin(id) : taskActions.pin(id)) }];
    const h = headers.find((x) => x.taskId === id);
    if (h && !h.archived) items.push({ id: "task-archive", label: "Archive", separatorBefore: true, onSelect: () => taskActions.archive(id) });
    return items;
  };
  /** The pane toggles of a column's task: the panes it can show and where each is. A toggle gives its column the keyboard (the dock follows the focused task) and opens or closes the pane. */
  const togglesOf = (id: string): PaneToggle[] => {
    const st = prefs.panelByTask[id] ?? EMPTY_DOCK;
    const pop = new Set<AppKind>(paneWindows.filter((w) => w.taskId === id).map((w) => w.kind));
    return panesOf(artifactsOf(model.tasks.get(id), artifactCtx)).map((kind) => ({ kind, open: paneStateOf(st, kind, pop) === "open", state: paneStateOf(st, kind, pop) }));
  };
  const onColumnPane = (column: 0 | 1, id: string, kind: AppKind) => {
    focusColumn(column);
    if (paneWindows.some((w) => w.taskId === id && w.kind === kind)) void window.modbit.focusPaneWindow(id, kind as PaneWindowKind);
    else setPanelState(id, (s) => togglePane(s, kind));
  };
  // REQ-PX-049: what the Core recovered at start, as counts of its own task states, shown once its snapshot and attention list are in.
  const [recoveryNote, setRecoveryNote] = useState<{ boot: string; text: string } | null>(null);
  useEffect(() => {
    if (!app.recovery || app.screen !== "populated" || !app.attentionLoaded) return;
    if (recoveryNote?.boot === app.recovery.bootGeneration) return;
    const recovered = Number(app.recovery.tasks);
    if (recovered === 0) return;
    const resumed = tasks.filter((t) => t.state === "Running").length;
    setRecoveryNote({ boot: app.recovery.bootGeneration, text: recoverySummary({ recovered, resumed, needAttention: cols.needsAttention.length }) });
    // The counts are taken once per boot, when the first complete picture exists.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [app.recovery, app.screen, app.attentionLoaded]);
  const coreLabel = core.state === "connected" ? `Core connected (pid ${core.pid})` : core.state === "restarting" ? "Core restarting…" : core.state === "failed" ? "Core failed" : "Core starting…";
  const coreStatus = core.state === "connected" ? "ok" : core.state === "failed" ? "danger" : "warn";

  const splitColumn = (tasks: readonly [string, string], column: 0 | 1): SplitColumn => {
    const id = tasks[column];
    return { taskId: id, title: model.tasks.get(id)?.goalText ?? "Task", taskMenu: taskMenuOf(id), panes: togglesOf(id), onPane: (kind: AppKind) => onColumnPane(column, id, kind) };
  };
  /** A task's conversation: the one component, for the single view and for each column of split view (REQ-PX-204). */
  const conversationFor = (taskId: string, o: { topEndSlot: HTMLElement | null; railSlot: HTMLElement | null; keyFocus?: boolean; trayStore?: TrayStore<ReactNode>; hostsAppTrays?: boolean; shownBeside?: readonly string[] }) => (
    <Conversation
                  key={taskId}
                  taskId={taskId}
                  sessionId={model.tasks.has(taskId) ? model.sessionId : (runSessions[taskId] ?? model.sessionId)}
                  connected={core.state === "connected"}
                  card={model.tasks.get(taskId)}
                  title={model.tasks.get(taskId)?.goalText ?? "Task"}
                  focusRowId={conversation?.taskId === taskId ? conversation.rowId : null}
                  onResume={(id) => void startTask(id)}
                  onNewTask={() => actions.focusNewTask()}
                  onOpenTerminal={(tid, termId) => { setTerminalPick((m) => ({ ...m, [tid]: termId })); setPanelState(tid, (d) => openPane(d, "terminal")); }}
                  onOpenTask={(id) => openConversation(id)}
                  titleOf={(id) => model.tasks.get(id)?.goalText ?? "another task"}
                  topEndSlot={o.topEndSlot}
                  railSlot={o.railSlot}
                  subtasks={childrenOf(model, taskId)}
                  screenState={model.tasks.has(taskId) ? taskState({ card: model.tasks.get(taskId)!, core, attention: attentionItems }) : null}
                  header={headers.find((h) => h.taskId === taskId)}
                  terminals={terminalsByTask.get(taskId)}
                  onTerminalsChanged={refreshTerminals}
                  onReview={() => setReviewing(taskId)}
                  onBrowser={() => void openBrowser(taskId)}
                  onCancelTask={() => {
                    const t = model.tasks.get(taskId);
                    if (t) setConfirm({ kind: "cancel", taskId: t.taskId, text: `Cancel “${t.goalText}”? The run stops at its next safe boundary; the task cannot be resumed.` });
                  }}
                  onStopChild={(id) => {
                    const t = model.tasks.get(id);
                    if (t) setConfirm({ kind: "cancel", taskId: t.taskId, text: `Cancel “${t.goalText}”? The run stops at its next safe boundary; the task cannot be resumed.` });
                  }}
                  keyFocus={o.keyFocus}
                  trayStore={o.trayStore}
                  hostsAppTrays={o.hostsAppTrays}
                  shownBeside={o.shownBeside}
              nameLandmarks={o.shownBeside !== undefined}
                  onCustomize={() => {
                    closePages();
                    setCustomizeOpen(true);
                  }}
                />
  );

  return (
    <>
      <ShellFrame
        prefs={prefs}
        onPrefs={setPrefs}
        panelOpen={panelOpen}
        panel={
          panelTaskId && panelTask ? (
            <AppsPanel
              sessionId={model.sessionId ?? ""}
              taskId={panelTaskId}
              taskTitle={panelTask.goalText}
              taskState={panelTask.state}
              refreshKey={panelRefreshKey}
              terminals={terminalsByTask.get(panelTaskId) ?? []}
              selectedTerminalId={terminalPick[panelTaskId] ?? null}
              onSelectTerminal={(id) => setTerminalPick((m) => ({ ...m, [panelTaskId]: id }))}
              onTerminalsChanged={refreshTerminals}
              announce={setNotice}
              dock={panelState}
              ownToggles={dockToggles.length > 0 ? { panes: dockToggles, onPane } : undefined}
              tiled={tiled}
              onDock={(update) => setPanelState(panelTaskId, update)}
              onOpenReview={() => setReviewing(panelTaskId)}
              onOpenCall={(callId) => openConversation(panelTaskId, `tool:${callId}`)}
              onOpenBrowser={() => void openBrowser(panelTaskId)}
              subtasks={childrenOf(model, panelTaskId)}
              onOpenTask={(id) => openConversation(id)}
              onStopChild={(id) => {
                const t = model.tasks.get(id);
                if (t) setConfirm({ kind: "cancel", taskId: t.taskId, text: `Cancel “${t.goalText}”? The run stops at its next safe boundary; the task cannot be resumed.` });
              }}
              onOpenTerminal={(termId) => {
                setTerminalPick((m) => ({ ...m, [panelTaskId]: termId }));
                if (popped.has("terminal")) void window.modbit.focusPaneWindow(panelTaskId, "terminal");
                else setPanelState(panelTaskId, (d) => openPane(d, "terminal"));
              }}
              onPopOut={(kind, size) => popOut(panelTaskId, panelTask.goalText, kind, size)}
              fileRequest={fileRequest && fileRequest.taskId === panelTaskId ? fileRequest : null}
            />
          ) : null
        }
        agents={(rail) => !rail && settingsOpen ? (
          <SettingsNav
            query={settingsQuery}
            onQuery={setSettingsQuery}
            active={settingsSection}
            onPick={(id) => {
              setSettingsSection(id);
              document.getElementById(`settings-${id}`)?.scrollIntoView({ block: "start" });
            }}
            onBack={() => setSettingsOpen(false)}
            foot={<PersonRow userName={userName} coreLabel={coreLabel} coreStatus={coreStatus} menuItems={menuItems} settingsOpen onOpenSettings={() => setSettingsOpen(false)} onOpenHelp={() => setHelpOpen(true)} />}
          />
        ) : (
          <AgentRegion
            rail={rail}
            canExpand={prefs.listCollapsed}
            taskCount={tasks.length}
            attentionCount={needsYou.count}
            onNewTask={() => actions.focusNewTask()}
            homeOpen={!actions.canGoBack()}
            onExpand={() => actions.toggleAgentList()}
            onToggleAgentList={() => actions.toggleAgentList()}
            canGoBack={actions.canGoBack()}
            onBack={() => actions.goBack()}
            canGoForward={leftConversation !== null && conversation === null}
            onForward={() => {
              if (leftConversation) openConversation(leftConversation);
            }}
            automationsOpen={automationsOpen}
            automationsDisabled={core.state !== "connected"}
            onToggleAutomations={() => (automationsOpen ? setAutomationsOpen(false) : actions.openAutomations())}
            inboxOpen={inboxOpen}
            onToggleInbox={() => {
              const open = !inboxOpen;
              closePages();
              setInboxOpen(open);
            }}
            customizeOpen={customizeOpen}
            customizeDisabled={core.state !== "connected"}
            onToggleCustomize={() => {
              const open = !customizeOpen;
              closePages();
              setCustomizeOpen(open);
            }}
            userName={userName}
            coreLabel={coreLabel}
            coreStatus={coreStatus}
            menuItems={menuItems}
            settingsOpen={settingsOpen}
            onOpenSettings={() => actions.openSettings()}
            onOpenHelp={() => setHelpOpen(true)}
            list={
              rail ? undefined : (
                <AgentList
                  sessionId={model.sessionId}
                  connected={core.state === "connected"}
                  selectedTaskId={conversation?.taskId ?? null}
                  onSelect={openConversation}
                  onHeaderList={setHeaders}
                  facts={rowFacts}
                  projects={projects}
                  selectedProjectId={projectPage}
                  onOpenProject={openProject}
                  onProjectArchived={(id, archived) => {
                    if (archived && projectPage === id) setProjectPage(null);
                  }}
                  parents={model.parents}
                  density={prefs.density}
                  onSearch={() => setPaletteOpen(true)}
                  onNewTaskIn={(root) => {
                    setWorkspaceRoot(root);
                    actions.focusNewTask();
                  }}
                  onTaskActions={setTaskActions}
                  onSplit={openSplit}
                  splitIds={splitShown && split ? split.tasks : undefined}
                />
              )
            }
          />
        )}
        banner={
          <PresenceBar
            info={presence.info}
            titleOf={(id) => model.tasks.get(id)?.goalText ?? "another task"}
            onOpenTask={(id) => openConversation(id)}
            onStop={(id) => void computerStop.stop(id)}
            busy={computerStop.busy}
            note={computerStop.note}
          />
        }
        details={menuTaskId !== null && prefs.detailsOpen}
        detailsRef={setRailSlot}
        split={splitShown && split ? { ratio: split.ratio } : null}
        layoutRef={layoutRef}
        topBar={(layout) => layout.split ? null : (
          <TopBar
            title={title}
            taskMenu={taskMenu}
            endSlotRef={setTopEndSlot}
            panes={panes}
            onPane={onPane}
            details={
              menuTaskId !== null && layout.detailsFits
                ? {
                    shown: layout.detailsPx > 0,
                    // The rail gives way to an open panel: asking for it closes the panel, and hiding it is remembered.
                    onToggle: () => {
                      const shown = layout.detailsPx > 0;
                      setPrefs((p) => ({ ...p, detailsOpen: !shown }));
                      if (!shown && panelTaskId && panelOpen) setPanelState(panelTaskId, (d) => toggleDock(d, dockable, undefined));
                    },
                  }
                : undefined
            }
            onLeaveSplit={splitShown && split ? () => closeColumn(split.focus === 0 ? 1 : 0) : undefined}
          />
        )}
        tray={conversation && !reviewing && !browsing && !dashboardOpen ? null : <TrayHost />}
        statusRow={
          <section aria-label="App notices">
            {recoveryNote && (
              <p className="shell-toast meta" data-testid="recovery-summary">
                Recovered at start: {recoveryNote.text}
              </p>
            )}
            <p className={notice ? "shell-toast meta" : "mb-sr-only"} role="status" aria-live="polite" data-testid="shell-notice">
              {notice}
            </p>
          </section>
        }
      >
        {(layout: Layout) => (
        <div className="conv-wrap">
          <div className="conv-banners" data-testid="conv-banners" role="region" aria-label="Task status" tabIndex={-1}>
            <StatusRegion app={app} />
            {splitShown && split && !layout.split && (
              <p className="meta split-paused" role="status" data-testid="split-paused">
                Split view is paused: two tasks need {SPLIT_NEED} px beside the task list and this window gives {layout.centerPx + layout.panelPx}. {model.tasks.get(split.tasks[split.focus === 0 ? 1 : 0])?.goalText ?? "The other task"} is back beside this one when the window is wider; Close leaves split view.
              </p>
            )}
          </div>
          {settingsOpen ? (
            <SettingsPage
              app={app}
              query={settingsQuery}
              theme={prefs.theme}
              onTheme={(theme) => setPrefs((p) => ({ ...p, theme }))}
              zoom={prefs.zoom}
              onZoom={(delta) => actions.zoom(delta)}
              density={prefs.density}
              onDensity={(density) => setPrefs((p) => ({ ...p, density }))}
              onCustomize={() => {
                closePages();
                setCustomizeOpen(true);
              }}
              onAutomations={() => actions.openAutomations()}
              onWorktrees={openWorktrees}
              onDashboard={model.sessionId ? () => actions.openDashboard() : null}
              onShortcuts={() => setHelpOpen(true)}
              onComputerPermissions={() => setPermissionsOpen(true)}
            />
          ) : customizeOpen ? (
            <div className="autos-scroll" data-testid="customize-region">
              <Customize connected={core.state === "connected"} sessionId={model.sessionId} taskId={conversation?.taskId ?? selectedTaskId ?? [...model.tasks.keys()][0] ?? null} onClose={() => setCustomizeOpen(false)} />
            </div>
          ) : inboxOpen ? (
            <Inbox app={app} onOpenTask={(id) => openConversation(id)} onNotificationSettings={() => actions.focusSettingsSection("notification-preferences")} focusRequest={inboxFocus} needsYou={needsYou} />
          ) : automationsOpen ? (
            <div className="autos-scroll" data-testid="automations-region">
              <Automations connected={core.state === "connected"} onOpenTask={(id) => openConversation(id)} onClose={() => setAutomationsOpen(false)} />
            </div>
          ) : dashboardOpen && model.sessionId ? (
            <div className="autos-scroll">
              <Dashboard sessionId={model.sessionId} onClose={() => setDashboardOpen(false)} />
            </div>
          ) : reviewing && model.sessionId ? (
            <div className="autos-scroll">
              <Review taskId={reviewing} sessionId={model.sessionId} card={model.tasks.get(reviewing) ?? null} onClose={() => void runKeyCommand("back")} />
            </div>
          ) : browsing ? (
            <div className="autos-scroll">
              <Browser browsing={browsing} card={model.tasks.get(browsing.taskId) ?? null} sessionId={model.sessionId} onReopen={() => void openBrowser(browsing.taskId)} onClose={() => void runKeyCommand("back")} />
            </div>
          ) : projectPage ? (
            <ProjectDetail project={openedProject ?? null} loaded={projects.loaded} sessionId={model.sessionId} projects={projects} onOpenTask={(id) => openConversation(id)} announce={setNotice} />
          ) : worktreesOpen ? (
            <WorktreesPanel state={worktrees} sessionId={model.sessionId} titleOf={(id) => model.tasks.get(id)?.goalText ?? "another task"} taskStateOf={(id) => model.tasks.get(id)?.state} nowMs={nowMs} onOpenTask={(id) => openConversation(id)} announce={setNotice} />
          ) : conversation ? (
            splitShown && split && layout.split ? (
              <SplitView
                columns={[splitColumn(split.tasks, 0), splitColumn(split.tasks, 1)]}
                widths={layout.split}
                focus={split.focus}
                onFocus={focusColumn}
                onClose={closeColumn}
                onRatio={(ratio) => setPrefs((p) => (p.split ? { ...p, split: { ...p.split, ratio } } : p))}
                min={layout.panelPx > 0 ? SPLIT_LAYOUT.columnMinBesideDock : SPLIT_LAYOUT.columnMin}
                render={(column, endSlot, keyFocus) => conversationFor(split.tasks[column], { topEndSlot: endSlot, railSlot: null, keyFocus, trayStore: storeFor(split.tasks[column]), hostsAppTrays: keyFocus, shownBeside: [split.tasks[column === 0 ? 1 : 0]] })}
              />
            ) : (
              conversationFor(conversation.taskId, { topEndSlot, railSlot: menuTaskId === conversation.taskId ? railSlot : null })
            )
          ) : (
            <Home app={app} />
          )}
        </div>
        )}
      </ShellFrame>
      <TrayDirector core={{ state: core.state, reason: core.state === "restarting" || core.state === "failed" ? core.reason : "" }} sessionId={model.sessionId} taskId={conversation?.taskId ?? null} card={conversation ? model.tasks.get(conversation.taskId) : undefined} onResume={(id) => void startTask(id)} />
      <PermissionsDialog open={permissionsOpen} onClose={() => setPermissionsOpen(false)} info={presence.info} refresh={presence.refresh} />
      <CommandPalette open={paletteOpen} items={paletteItems} onClose={() => setPaletteOpen(false)} announce={setNotice} search={paletteSearch} />
      <ShortcutHelp open={helpOpen} onClose={() => setHelpOpen(false)} registry={shellRegistry} />
    </>
  );
}
