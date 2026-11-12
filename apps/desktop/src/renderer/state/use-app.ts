/** The renderer's one controller hook: the Core connection, the task model, the screens' states and the keyboard commands (moved out of the single index.tsx; behaviour unchanged). */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { applyEvent, columns, emptyModel, fromSnapshot, type Event, type Model, type Snapshot, type TaskCard } from "../model.ts";
import { fleetState, newTaskState, settingsState } from "../screens.ts";
import { DEFAULT_PREFERENCES, deriveNotifications, newSince, osDeliveryDue, type Notification as AppNotification, type NotificationPreferences } from "../notifications.ts";
import { commandFor, isActivatable, isEditable, type Command } from "../keyboard.ts";
import type { AttentionItem, CredentialHandle } from "../../preload/preload.ts";
import type { TaskModeId } from "../../shared/composer-types.ts";
import { draftPatch, type DraftModel } from "../composer/model.ts";
import { layers } from "@modbit/ui/logic";
import { hex32, loadPreferences, PREFS_KEY, type CoreStatus, type RecoveryInfo, type ScreenState } from "./types.ts";

export function useApp() {
  const [core, setCore] = useState<CoreStatus>({ state: "starting", restarts: 0 });
  const [model, setModel] = useState<Model>(emptyModel);
  const [screen, setScreen] = useState<ScreenState>("loading");
  // PX-030 / docs/76: what this platform has earned, from main, and nothing more.
  const [platform, setPlatform] = useState<{ state: string; statement: string } | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [recovered, setRecovered] = useState<string | null>(null);
  const [recovery, setRecovery] = useState<RecoveryInfo | null>(null);
  const [goal, setGoal] = useState("");
  // PX-010: a task made from a forge issue. The URL is the only input; the
  // Core reads the issue, names the task after it and attaches its text as
  // untrusted context. An unreadable issue is a refusal and no task.
  const [issueUrl, setIssueUrl] = useState("");
  const [languages, setLanguages] = useState<{ language: string; tier: string; label: string }[]>([]);
  // REQ-EV-0151 / 0275: the Core's attention items — derived from canonical
  // unresolved state, re-read after every task event, never invented here.
  const [attentionItems, setAttentionItems] = useState<AttentionItem[]>([]);
  // FIX-21: the attention count is the Core's attention view; until the Core
  // has answered once, the tasks' own next action stands in.
  const [attentionLoaded, setAttentionLoaded] = useState(false);
  const attentionTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const refreshAttention = useCallback((sessionId: string) => {
    if (attentionTimer.current) clearTimeout(attentionTimer.current);
    attentionTimer.current = setTimeout(() => {
      window.modbit
        .attention(sessionId)
        .then((a) => {
          setAttentionItems(a.items);
          setAttentionLoaded(true);
        })
        .catch(() => {});
    }, 150);
  }, []);
  const [workspaceRoot, setWorkspaceRoot] = useState("");
  // PX-118: the task works in its own worktree; the checkout is untouched until its result is applied.
  const [isolate, setIsolate] = useState(false);
  // What the home composer asks of the new task, held until the task exists: CreateTask has no such fields, so the Core hears them before StartTask.
  const [newMode, setNewMode] = useState<TaskModeId>("AGENT");
  const [newModel, setNewModel] = useState<DraftModel | null>(null);
  const [submitting, setSubmitting] = useState(false);
  const [reviewing, setReviewing] = useState<string | null>(null);
  // PX-045: the task the person last focused; the apps panel follows it.
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null);
  const [dashboardOpen, setDashboardOpen] = useState(false);
  // M7.1: the browser session shown in the centre (one task's live view).
  const [browsing, setBrowsing] = useState<{ taskId: string; browserSessionId: string | null; error: string | null } | null>(null);
  // Onboarding (REQ-PX-022, docs/39): three steps, each a working control.
  const [provider, setProvider] = useState<{ configured: boolean; endpoints: string[]; stored: boolean; provider: string; keychainAvailable: boolean } | null>(null);
  // M7.8: the credential broker's handles (never a value) and the add form.
  const [credentials, setCredentials] = useState<CredentialHandle[]>([]);
  const [credentialForm, setCredentialForm] = useState({ label: "", origin: "", username: "", secret: "" });
  const [credentialError, setCredentialError] = useState<string | null>(null);
  const refreshCredentials = useCallback(() => {
    void window.modbit.listCredentials().then(setCredentials).catch(() => {});
  }, []);
  const [providerKind, setProviderKind] = useState<"openai" | "anthropic">("openai");
  const [providerKey, setProviderKey] = useState("");
  const [providerUrl, setProviderUrl] = useState("");
  const [providerBusy, setProviderBusy] = useState(false);
  const [providerResult, setProviderResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [trustRoot, setTrustRoot] = useState("");
  const [trusted, setTrusted] = useState<string | null>(null);
  const [trustBusy, setTrustBusy] = useState(false);
  const [trustError, setTrustError] = useState<string | null>(null);
  const [starters, setStarters] = useState<{ stacks: string[]; tasks: { id: string; title: string; goalText: string; stack: string }[] } | null>(null);
  const [autoReview, setAutoReview] = useState<string | null>(null);
  const [createError, setCreateError] = useState<string | null>(null);
  // A request to the shell's view state: open a task, the approvals page or the palette.
  const [navRequest, setNavRequest] = useState<{ n: number; kind: "task"; taskId: string } | { n: number; kind: "inbox" | "palette" } | null>(null);
  const requestNav = useCallback((r: { kind: "task"; taskId: string } | { kind: "inbox" | "palette" }) => setNavRequest((c) => ({ ...r, n: (c?.n ?? 0) + 1 })), []);
  // PX-024: the keyboard model's state — the help panel, a pending confirmation
  // before an irreversible effect, the live region's announcement.
  const [helpOpen, setHelpOpen] = useState(false);
  const [confirm, setConfirm] = useState<{ kind: "approve" | "cancel"; taskId: string; text: string } | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const seenAttention = useRef<Set<string>>(new Set());
  const [prefs, setPrefs] = useState<NotificationPreferences>(loadPreferences);
  const [delivered, setDelivered] = useState<string[]>([]);
  const previousNotifications = useRef<AppNotification[]>([]);
  const modelRef = useRef(model);
  modelRef.current = model;
  const wasRestarting = useRef(false);

  /** docs/32 "Event consumption": snapshot, subscribe from its cursor, reduce events. */
  const load = useCallback(async () => {
    setScreen("loading");
    setError(null);
    try {
      const local = await window.modbit.localState();
      if (local.platform) setPlatform({ state: local.platform.state, statement: local.platform.statement });
      if (!local.sessionId) {
        setModel(emptyModel());
        setScreen("empty");
        return;
      }
      const snap = (await window.modbit.sessionSnapshot(local.sessionId)) as Snapshot;
      const prev = modelRef.current;
      if (prev.sessionId === snap.sessionId && prev.cursor !== "0") {
        // Reconnect by cursor (docs/32, PX-023): the cards stay as they are
        // and the stream resumes from the last offset this renderer applied,
        // so what the Core appended while away — its recovery's waits and
        // diagnostics included — is replayed, not lost behind a snapshot.
        setScreen(prev.tasks.size === 0 ? "empty" : "populated");
        await window.modbit.subscribe(snap.sessionId, prev.cursor);
        refreshAttention(snap.sessionId);
        return;
      }
      const m = fromSnapshot(snap);
      setModel(m);
      setScreen(m.tasks.size === 0 ? "empty" : "populated");
      // A snapshot carries states, not the typed diagnostics behind a wait
      // or a failure (REQ-EV-0073): read those from the Core's task status
      // so a degraded state after an app restart still names its cause.
      const statuses = await Promise.all([...m.tasks.values()].filter((t) => t.state === "Waiting" || t.state === "Failed").map(async (t) => [t.taskId, await window.modbit.taskStatus(t.taskId).catch(() => null)] as const));
      if (statuses.length > 0) {
        setModel((cur) => {
          const tasks = new Map(cur.tasks);
          for (const [id, st] of statuses) {
            const c = tasks.get(id);
            if (!st || !st.failureClass || !c || c.diagnostic) continue;
            tasks.set(id, { ...c, diagnostic: { class: st.failureClass, code: st.failureCode, detail: st.attentionReason, userAction: st.userAction, recoveryPath: st.recoveryPath, retryable: st.retryable, evidenceRefs: st.evidenceRefs }, nextAction: c.nextAction ?? st.attentionReason, lastOffset: st.lastOffset });
          }
          return { ...cur, tasks };
        });
      }
      await window.modbit.subscribe(snap.sessionId, snap.lastOffset);
      refreshAttention(snap.sessionId);
    } catch (e) {
      setError((e as Error).message);
      setScreen("error");
    }
  }, [refreshAttention]);

  useEffect(() => {
    const offEvent = window.modbit.onEvent((raw) => {
      const e = raw as Event & { taskId: string | null; aggregateType: string; eventType?: string };
      // Task events, and the run and approval aggregates' events the Core
      // stamps with the task id (phase, gate verdict, feasibility, the
      // approval's intent); everything else is not a card's business.
      if (!e.taskId || !["task", "run", "approval"].includes(e.aggregateType)) return;
      setModel((m) => applyEvent(m, e, e.taskId!));
      setScreen("populated");
      if (e.sessionId) refreshAttention(e.sessionId);
      // docs/39 step 5: the first result opens the Review on the diff.
      if (e.eventType === "TaskReadyForReview") setAutoReview(e.taskId);
    });
    const offRecovery = window.modbit.onRecovery((raw) => setRecovery(raw as RecoveryInfo));
    const offStatus = window.modbit.onCoreStatus((raw) => {
      const s = raw as CoreStatus;
      setCore(s);
      if (s.state === "restarting") wasRestarting.current = true;
      if (s.state === "connected") {
        // PX-026: the language labels are read once the Core is connected,
        // whether that happened before this screen mounted or after.
        void window.modbit.languages().then(setLanguages).catch(() => {});
        void window.modbit.providerStatus().then(setProvider).catch(() => {});
        refreshCredentials();
        if (wasRestarting.current) {
          wasRestarting.current = false;
          setRecovered(`Core reconnected after restart ${s.restarts}`);
          // Recovery: re-read the projection so nothing shown is invented.
          void load();
        } else {
          void load();
        }
      }
    });
    void window.modbit.coreStatus().then((s) => {
      setCore(s as CoreStatus);
      if ((s as CoreStatus).state === "connected") {
        void load();
        void window.modbit.languages().then(setLanguages).catch(() => {});
        void window.modbit.providerStatus().then(setProvider).catch(() => {});
      }
    });
    return () => {
      offEvent();
      offStatus();
      offRecovery();
    };
  }, [load]);

  /** Creates the task and opens it; `start` also starts it (send does; Alt+Enter leaves it queued). No provider leaves it queued too. */
  const submit = useCallback(
    async (ev: React.SyntheticEvent, start: boolean) => {
      ev.preventDefault();
      const text = goal.trim();
      const issue = issueUrl.trim();
      if ((!text && !issue) || submitting) return;
      setSubmitting(true);
      setError(null);
      // Stable command id: a retry after a crash replays instead of duplicating (docs/30).
      const commandId = hex32();
      try {
        let sessionId = modelRef.current.sessionId;
        if (!sessionId) {
          sessionId = await window.modbit.createSession();
          setModel((m) => ({ ...m, sessionId }));
          await window.modbit.subscribe(sessionId, "0");
        }
        const root = workspaceRoot.trim();
        // A desktop task runs only on a repository this session trusted
        // (REQ-PX-022); trusting is explicit and scoped to this root.
        if (root && trusted !== root) {
          const t = await window.modbit.trustRepository(sessionId, root);
          setTrusted(t.workspaceRoot);
        }
        const r = await window.modbit.createTask(sessionId, text, commandId, root, issue || undefined, isolate && root ? { isolation: "WORKTREE" } : {});
        // Render from the durable id the Core returned; the events will follow.
        setModel((m) => {
          if (m.tasks.has(r.taskId)) return m;
          const tasks = new Map(m.tasks);
          tasks.set(r.taskId, { taskId: r.taskId, goalText: r.goalText || text, state: "Queued", waitReason: "Capacity", generation: 2, createdAtMs: Date.now(), nextAction: null, attachments: issue ? 1 : 0, parentTaskId: null, origin: issue ? "forge_issue" : "desktop", agents: { total: 0, running: 0, background: 0, waiting: 0, done: 0, failed: 0 }, phase: "drafting", latestEvidence: null, risk: null, children: [], diagnostic: null, approval: null, feasibility: null, question: null, gate: null, lastOffset: "0" } as TaskCard);
          return { ...m, tasks };
        });
        setScreen("populated");
        setGoal("");
        setIssueUrl("");
        setCreateError(null);
        setNewMode("AGENT");
        setNewModel(null);
        requestNav({ kind: "task", taskId: r.taskId });
        try {
          if (newMode !== "AGENT") await window.modbit.composerSetMode(sessionId, r.taskId, newMode);
          if (newModel) await window.modbit.composerSetPreference(sessionId, r.taskId, draftPatch(newModel));
        } catch (e) {
          // The task exists, and it does not run on anything other than what was asked for: it stays unstarted and says why.
          setError(`The task was created and not started, because the Core refused what was chosen for it: ${(e as Error).message}`);
          return;
        }
        if (start && provider?.configured) await window.modbit.startTask(sessionId, r.taskId);
      } catch (e) {
        setError((e as Error).message);
        setCreateError((e as Error).message);
      } finally {
        setSubmitting(false);
      }
    },
    [goal, issueUrl, workspaceRoot, submitting, trusted, isolate, newMode, newModel, provider?.configured, requestNav],
  );

  const startTask = useCallback(async (taskId: string) => {
    setError(null);
    try {
      const sessionId = modelRef.current.sessionId;
      if (!sessionId) throw new Error("no session");
      await window.modbit.startTask(sessionId, taskId);
    } catch (e) {
      setError((e as Error).message);
    }
  }, []);

  // M7.1: open (or reuse) the task's browser session; the view is placed by
  // the Browser panel once it is on screen.
  const openBrowser = useCallback(async (taskId: string) => {
    const sessionId = modelRef.current.sessionId;
    if (!sessionId) return;
    setBrowsing({ taskId, browserSessionId: null, error: null });
    try {
      const r = await window.modbit.openBrowser(sessionId, taskId);
      setBrowsing({ taskId, browserSessionId: r.browserSessionId, error: null });
    } catch (e) {
      setBrowsing({ taskId, browserSessionId: null, error: (e as Error).message });
    }
  }, []);

  // PX-024 live region: every new attention item is announced once, in
  // words (kind, task, reason) — never by colour alone.
  useEffect(() => {
    const fresh = attentionItems.filter((i) => !seenAttention.current.has(`${i.kind}:${i.taskId}:${i.reference}`));
    for (const i of fresh) seenAttention.current.add(`${i.kind}:${i.taskId}:${i.reference}`);
    if (fresh.length === 0) return;
    const i = fresh[fresh.length - 1]!;
    const goal = modelRef.current.tasks.get(i.taskId)?.goalText ?? i.taskId.slice(0, 8);
    setAnnouncement(`${fresh.length > 1 ? `${fresh.length} tasks need attention; latest: ` : "Needs attention: "}${i.kind.toLowerCase().replace(/_/g, " ")} on ${goal}: ${i.reason}`);
  }, [attentionItems]);

  useEffect(() => {
    if (autoReview && !reviewing) {
      setReviewing(autoReview);
      setAutoReview(null);
    }
  }, [autoReview, reviewing]);

  /** docs/39 step 2: the key goes to main and the Core; the live test call
   *  confirms it or names the cause. The renderer keeps nothing. */
  const setupProvider = useCallback(async () => {
    if (providerBusy) return;
    setProviderBusy(true);
    setProviderResult(null);
    try {
      const r = await window.modbit.setupProvider(providerKind, providerKey, providerUrl.trim() || undefined);
      setProviderKey("");
      if (r.ok) {
        setProviderResult({ ok: true, text: `Provider confirmed: ${r.endpoint} answered on ${r.model}${r.persisted ? "; the key is in the OS keychain's custody" : r.keychainAvailable ? "" : "; this OS offers no keychain encryption, so the key was not saved and must be entered again after a restart"}.` });
        setProvider(await window.modbit.providerStatus());
      } else {
        const cause = r.errorCode === "AUTH_REJECTED" ? "the provider rejected the key" : r.errorCode.includes("CONNECT") || r.errorCode === "TIMEOUT" || r.errorCode === "TRANSPORT" ? "the provider could not be reached" : r.errorCode === "RATE_LIMITED" ? "the provider is rate limiting or out of quota" : `the test call failed (${r.errorCode})`;
        setProviderResult({ ok: false, text: `Not confirmed: ${cause}. ${r.errorMessage} Check the key and the network, then retry.` });
      }
    } catch (e) {
      setProviderResult({ ok: false, text: `Not confirmed: ${(e as Error).message}. Retry.` });
    } finally {
      setProviderBusy(false);
    }
  }, [providerBusy, providerKind, providerKey, providerUrl]);

  /** docs/39 step 3: explicit, scoped trust; then the starter tasks for the
   *  stack the Core detects. */
  const trustRepository = useCallback(async () => {
    const root = trustRoot.trim();
    if (!root || trustBusy) return;
    setTrustBusy(true);
    setTrustError(null);
    try {
      let sessionId = modelRef.current.sessionId;
      if (!sessionId) {
        sessionId = await window.modbit.createSession();
        setModel((m) => ({ ...m, sessionId }));
        await window.modbit.subscribe(sessionId, "0");
      }
      const t = await window.modbit.trustRepository(sessionId, root);
      setTrusted(t.workspaceRoot);
      setWorkspaceRoot(t.workspaceRoot);
      setStarters(await window.modbit.starterTasks(t.workspaceRoot));
    } catch (e) {
      setTrustError((e as Error).message);
    } finally {
      setTrustBusy(false);
    }
  }, [trustRoot, trustBusy]);

  /** docs/39 step 4: a starter task starts immediately on the direct baseline. */
  const runStarter = useCallback(
    async (goalText: string) => {
      if (submitting || !trusted) return;
      setSubmitting(true);
      setError(null);
      try {
        const sessionId = modelRef.current.sessionId;
        if (!sessionId) throw new Error("no session");
        const r = await window.modbit.createTask(sessionId, goalText, hex32(), trusted);
        setModel((m) => {
          if (m.tasks.has(r.taskId)) return m;
          const tasks = new Map(m.tasks);
          tasks.set(r.taskId, { taskId: r.taskId, goalText, state: "Queued", waitReason: "Capacity", generation: 2, createdAtMs: Date.now(), nextAction: null, attachments: 0, parentTaskId: null, origin: "desktop", agents: { total: 0, running: 0, background: 0, waiting: 0, done: 0, failed: 0 }, phase: "drafting", latestEvidence: null, risk: null, children: [], diagnostic: null, approval: null, feasibility: null, question: null, gate: null, lastOffset: "0" } as TaskCard);
          return { ...m, tasks };
        });
        setScreen("populated");
        requestNav({ kind: "task", taskId: r.taskId });
        await window.modbit.startTask(sessionId, r.taskId);
      } catch (e) {
        setError((e as Error).message);
      } finally {
        setSubmitting(false);
      }
    },
    [submitting, trusted, requestNav],
  );

  const cols = useMemo(() => columns(model, attentionLoaded ? attentionItems : undefined), [model, attentionItems, attentionLoaded]);
  const attention = cols.needsAttention.length;
  const taskOpener = (taskId: string) => document.querySelector<HTMLElement>(`[data-testid="agent-row"][data-task-id="${taskId}"] [data-testid="agent-open"]`);
  const runKeyCommand = useCallback(
    async (cmd: Command) => {
      const sessionId = modelRef.current.sessionId;
      // The focused task: its row or conversation, or an attention item about it (approve, deny, cancel and steer act on that task).
      const taskId = document.activeElement?.closest("[data-task-id]")?.getAttribute("data-task-id") ?? document.querySelector('[data-testid="conversation"]')?.getAttribute("data-task-id") ?? null;
      const task = taskId ? modelRef.current.tasks.get(taskId) : undefined;
      switch (cmd) {
        case "newTask":
          document.getElementById("goal")?.focus();
          return;
        case "search":
          requestNav({ kind: "palette" });
          return;
        case "help":
          setHelpOpen((h) => !h);
          return;
        case "jumpAttention":
          requestNav({ kind: "inbox" });
          return;
        case "jumpRunning":
          document.querySelector<HTMLElement>('[data-testid="agent-row"][data-class="RUNNING"] [data-testid="agent-open"]')?.focus();
          return;
        case "next":
        case "previous": {
          const rows = [...document.querySelectorAll<HTMLElement>('[data-testid="agent-open"]')];
          if (rows.length === 0) return;
          const i = rows.indexOf(document.activeElement as HTMLElement);
          rows[i < 0 ? 0 : Math.min(rows.length - 1, Math.max(0, i + (cmd === "next" ? 1 : -1)))]?.focus();
          return;
        }
        case "open":
          if (task && (task.state === "ReadyForReview" || task.state === "Completed")) setReviewing(task.taskId);
          return;
        case "approve":
        case "deny": {
          if (!task?.approval || !sessionId || task.state !== "Waiting" || task.waitReason !== "Approval") return;
          const irreversible = task.approval.effectClass === "Destructive" || task.approval.effectClass === "ExternalSideEffect";
          if (cmd === "approve" && irreversible) {
            setConfirm({ kind: "approve", taskId: task.taskId, text: `Approve ${task.approval.toolName} (${task.approval.effectClass}, intent ${task.approval.intentHash.slice(0, 12)}…) on “${task.goalText}”? This effect cannot be undone.` });
            return;
          }
          await window.modbit.resolveApproval(sessionId, task.approval.approvalId, cmd === "approve", `${cmd === "approve" ? "approved" : "denied"} from the keyboard`, task.approval.intentHash).catch((e: Error) => setError(e.message));
          return;
        }
        case "cancelTask":
          if (!task || !sessionId || task.state === "Completed" || task.state === "Cancelled" || task.state === "Failed") return;
          setConfirm({ kind: "cancel", taskId: task.taskId, text: `Cancel “${task.goalText}”? The run stops at its next safe boundary; the task cannot be resumed.` });
          return;
        case "steerTask":
          document.querySelector<HTMLElement>('[data-testid="composer-input"]')?.focus();
          return;
        case "confirm": {
          const c = confirm;
          if (!c || !sessionId) return;
          setConfirm(null);
          const t = modelRef.current.tasks.get(c.taskId);
          taskOpener(c.taskId)?.focus();
          try {
            if (c.kind === "cancel") await window.modbit.cancelTask(sessionId, c.taskId);
            else if (t?.approval) await window.modbit.resolveApproval(sessionId, t.approval.approvalId, true, "approved from the keyboard, confirmed", t.approval.intentHash);
          } catch (e) {
            setError((e as Error).message);
          }
          return;
        }
        case "back": {
          if (confirm) {
            setConfirm(null);
            taskOpener(confirm.taskId)?.focus();
            return;
          }
          if (helpOpen) {
            setHelpOpen(false);
            return;
          }
          if (reviewing) {
            setReviewing(null);
            return;
          }
          if (browsing) {
            setBrowsing(null);
            return;
          }
          if (isEditable(document.activeElement)) (document.activeElement as HTMLElement).blur();
          return;
        }
        default:
          return;
      }
    },
    [confirm, helpOpen, reviewing, browsing, requestNav],
  );
  // The listener reads what the latest render holds, not what held when an effect last ran: a key that arrives between a
  // commit and its effects (Escape pressed the moment the review opens by itself) is answered for the screen that is showing.
  const keyState = useRef({ runKeyCommand, reviewing, confirm, browsing });
  keyState.current = { runKeyCommand, reviewing, confirm, browsing };
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const { runKeyCommand, reviewing, confirm, browsing } = keyState.current;
      // A control that handled its own key keeps it (the queue tray's arrows, an open menu's).
      if (e.defaultPrevented || (e.target as Element | null)?.closest?.('[role="menu"]')) return;
      // A modal layer (the palette, the shortcut help) owns the keyboard while it is open (PX-045).
      if (layers.hasModal()) return;
      // Enter and Space on a button or a checkbox are that control's.
      if ((e.key === "Enter" || e.key === " ") && isActivatable(document.activeElement) && confirm === null) return;
      const cmd = commandFor(e, { editable: isEditable(document.activeElement), screen: reviewing ? "review" : "fleet", confirming: confirm !== null, isMac: navigator.platform.toUpperCase().includes("MAC") });
      if (!cmd) return;
      // Over the browser panel only the global shortcuts and Escape apply.
      if (browsing && !["back", "help", "newTask", "search", "confirm"].includes(cmd)) return;
      // The Review handles its own change navigation (hunk*, split, failing).
      if (["hunkNext", "hunkPrevious", "hunkToggle", "toggleSplit", "jumpFailing"].includes(cmd)) return;
      e.preventDefault();
      void runKeyCommand(cmd);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const tasks = useMemo(() => [...model.tasks.values()], [model]);
  // PX-023: the screen states, each derived from what the Core said.
  const fleet = useMemo(() => fleetState({ core, loaded: screen === "loading" ? "loading" : screen === "empty" ? "empty" : "populated", error, recovery: recovery ? { bootGeneration: Number(recovery.bootGeneration), eventsVerified: Number(recovery.eventsVerified), aggregatesVerified: Number(recovery.aggregatesVerified), sessions: Number(recovery.sessions), tasks: Number(recovery.tasks), projectionsRebuilt: recovery.projectionsRebuilt, notes: recovery.notes, recoveryMs: Number(recovery.recoveryMs) } : null, recoveredBanner: recovered !== null, tasks }), [core, screen, error, recovery, recovered, tasks]);
  const composer = useMemo(() => newTaskState({ core, provider: provider ? { configured: provider.configured } : null, trusted, workspaceRoot, lastError: createError }), [core, provider, trusted, workspaceRoot, createError]);
  const settings = useMemo(() => settingsState({ provider: provider ? { keychainAvailable: provider.keychainAvailable, configured: provider.configured } : null, organizationOverrides: [] }), [provider]);
  // PX-023: the notifications the tasks warrant, coalesced per task and
  // collapsed on a burst; OS delivery only for what is new, opted in and
  // outside quiet hours. Main logs what it delivered.
  const notifications = useMemo(() => deriveNotifications(tasks, attentionItems), [tasks, attentionItems]);
  useEffect(() => {
    const fresh = newSince(previousNotifications.current, notifications);
    previousNotifications.current = notifications;
    const due = fresh.filter((n) => osDeliveryDue(n, prefs, new Date().getHours()));
    if (due.length === 0) return;
    for (const n of due) {
      void window.modbit
        .deliverNotification(n.id, n.title, `${n.reason} — ${n.nextAction}`)
        .then(() => setDelivered((d) => [...d, `${n.id}:${n.reason}`]))
        .catch(() => {});
    }
  }, [notifications, prefs]);
  const savePrefs = useCallback((next: NotificationPreferences) => {
    setPrefs(next);
    try {
      localStorage.setItem(PREFS_KEY, JSON.stringify(next));
    } catch {
      // per-viewer convenience only; nothing depends on it persisting
    }
  }, []);
  const open = useCallback((link: AppNotification["deepLink"]) => {
    if (link.screen === "review" && link.taskId) {
      setReviewing(link.taskId);
      return;
    }
    setReviewing(null);
    if (link.screen === "task" && link.taskId) requestNav({ kind: "task", taskId: link.taskId });
    else requestNav({ kind: "inbox" });
  }, [requestNav]);
  // docs/39 "Empty states": no provider keeps the welcome up with step 2
  // highlighted whatever else exists; a provider with no trusted repository
  // and no tasks keeps it up with step 3.
  const onboarding = core.state === "connected" && provider !== null && (!provider.configured || model.tasks.size === 0);

  return { selectedTaskId, setSelectedTaskId, core, setCore, model, setModel, screen, setScreen, platform, setPlatform, error, setError, recovered, setRecovered, recovery, setRecovery, goal, setGoal, issueUrl, setIssueUrl, languages, setLanguages, attentionItems, setAttentionItems, attentionLoaded, setAttentionLoaded, attentionTimer, refreshAttention, workspaceRoot, setWorkspaceRoot, isolate, setIsolate, newMode, setNewMode, newModel, setNewModel, submitting, setSubmitting, reviewing, setReviewing, dashboardOpen, setDashboardOpen, browsing, setBrowsing, provider, setProvider, credentials, setCredentials, credentialForm, setCredentialForm, credentialError, setCredentialError, refreshCredentials, providerKind, setProviderKind, providerKey, setProviderKey, providerUrl, setProviderUrl, providerBusy, setProviderBusy, providerResult, setProviderResult, trustRoot, setTrustRoot, trusted, setTrusted, trustBusy, setTrustBusy, trustError, setTrustError, starters, setStarters, autoReview, setAutoReview, createError, setCreateError, navRequest, requestNav, helpOpen, setHelpOpen, confirm, setConfirm, announcement, setAnnouncement, seenAttention, prefs, setPrefs, delivered, setDelivered, previousNotifications, modelRef, wasRestarting, load, submit, startTask, openBrowser, setupProvider, trustRepository, runStarter, cols, attention, runKeyCommand, tasks, fleet, composer, settings, notifications, savePrefs, open, onboarding };
}

export type AppState = ReturnType<typeof useApp>;
