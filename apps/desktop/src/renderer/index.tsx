/**
 * Renderer entry (docs/32): app shell with the Home / Fleet screen and the New
 * Task composer (docs/10 screens 1 and 2). Runs sandboxed; all Core access goes
 * through `window.modbit` (preload bridge). Screen states per docs/39 PX-023:
 * empty, loading, populated, error, degraded (Core restarting) and recovery.
 */
import { StrictMode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { applyEvent, childrenOf, columns, emptyModel, fromSnapshot, type Event, type FleetColumn, type Model, type Snapshot, type TaskCard } from "./model.ts";
import { browserState, fleetState, newTaskState, reviewState, settingsState, taskState, type ScreenState as DerivedScreenState } from "./screens.ts";
import { DEFAULT_PREFERENCES, deriveNotifications, newSince, osDeliveryDue, type Notification as AppNotification, type NotificationKind, type NotificationPreferences } from "./notifications.ts";
import { commandFor, isActivatable, isEditable, SHORTCUTS, type Command } from "./keyboard.ts";
import { splitRows } from "./diff.ts";
import type { AttentionItem, BrowserSessionSummary, ContextInspectorSummary, CredentialHandle, ModbitBridge, PullRequestAckView, ReviewBundleView, TaskEconomicsSummary } from "../preload/preload.ts";

declare global {
  interface Window {
    modbit: ModbitBridge;
  }
}

type CoreStatus =
  | { state: "starting"; restarts: number }
  | { state: "connected"; pid: number; endpoint: string; restarts: number }
  | { state: "restarting"; reason: string; restarts: number; retryInMs: number }
  | { state: "failed"; reason: string; restarts: number };

type ScreenState = "loading" | "empty" | "populated" | "error";

interface RecoveryInfo {
  bootGeneration: string;
  lastOffset: string;
  eventsVerified: string;
  aggregatesVerified: string;
  projectionsRebuilt: boolean;
  sessions: string;
  tasks: string;
  notes: string[];
  recoveryMs: string;
}

const COLUMNS: { key: FleetColumn; title: string }[] = [
  { key: "needsAttention", title: "Needs Attention" },
  { key: "readyForReview", title: "Ready for Review" },
  { key: "running", title: "Running" },
  { key: "waiting", title: "Waiting" },
  { key: "completed", title: "Completed" },
  { key: "failed", title: "Failed" },
];

function hex32(): string {
  const b = new Uint8Array(16);
  crypto.getRandomValues(b);
  return Array.from(b, (x) => x.toString(16).padStart(2, "0")).join("");
}

const PREFS_KEY = "modbit.notifications";
function loadPreferences(): NotificationPreferences {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (!raw) return DEFAULT_PREFERENCES;
    const p = JSON.parse(raw) as Partial<NotificationPreferences>;
    return { os: { ...DEFAULT_PREFERENCES.os, ...(p.os ?? {}) }, quietHours: p.quietHours ?? null };
  } catch {
    return DEFAULT_PREFERENCES;
  }
}

/** PX-023: one screen's state, with cause, next action and evidence when it
 *  is not the plain populated state. Every screen renders one. */
const KIND_MARK: Record<DerivedScreenState["kind"], string> = { empty: "○", loading: "…", populated: "●", error: "✖", degraded: "⚠", recovery: "↻" };
function StateLine({ state, testid }: { state: DerivedScreenState; testid: string }) {
  // The kind is in the text (a mark and the word), never in colour alone.
  // `data-seen` keeps the labels this line has shown, in order: a state
  // that lasts a moment (a Core back in a second) is still on record.
  const seen = useRef<string[]>([]);
  if (seen.current[seen.current.length - 1] !== `${state.kind}:${state.label}`) seen.current = [...seen.current.slice(-19), `${state.kind}:${state.label}`];
  return (
    <div className="meta state" data-testid={testid} data-screen={state.screen} data-kind={state.kind} data-seen={seen.current.join("|")} role={state.kind === "error" ? "alert" : "status"} aria-label={`${state.kind}: ${state.label}`}>
      <span aria-hidden="true">{KIND_MARK[state.kind]} </span>
      <span className="sr-only">{state.kind}: </span>
      <span data-testid={`${testid}-label`}>{state.label}</span>
      {state.cause && (
        <>
          {" · cause: "}
          <span data-testid={`${testid}-cause`}>{state.cause}</span>
        </>
      )}
      {state.nextAction && (
        <>
          {" · next: "}
          <strong data-testid={`${testid}-next`}>{state.nextAction}</strong>
        </>
      )}
      {state.evidence && (
        <>
          {" · evidence: "}
          <span data-testid={`${testid}-evidence`}>{state.evidence}</span>
        </>
      )}
    </div>
  );
}

function App() {
  const [core, setCore] = useState<CoreStatus>({ state: "starting", restarts: 0 });
  const [model, setModel] = useState<Model>(emptyModel);
  const [screen, setScreen] = useState<ScreenState>("loading");
  const [error, setError] = useState<string | null>(null);
  const [recovered, setRecovered] = useState<string | null>(null);
  const [recovery, setRecovery] = useState<RecoveryInfo | null>(null);
  const [goal, setGoal] = useState("");
  // PX-010: a task made from a forge issue. The URL is the only input; the
  // Core reads the issue, names the task after it and attaches its text as
  // untrusted context. An unreadable issue is a refusal and no task.
  const [issueUrl, setIssueUrl] = useState("");
  const [languages, setLanguages] = useState<{ language: string; tier: string; label: string }[]>([]);
  const [inspector, setInspector] = useState<ContextInspectorSummary | null>(null);
  const [economics, setEconomics] = useState<TaskEconomicsSummary | null>(null);
  // REQ-EV-0151 / 0275: the Core's attention items — derived from canonical
  // unresolved state, re-read after every task event, never invented here.
  const [attentionItems, setAttentionItems] = useState<AttentionItem[]>([]);
  const attentionTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const refreshAttention = useCallback((sessionId: string) => {
    if (attentionTimer.current) clearTimeout(attentionTimer.current);
    attentionTimer.current = setTimeout(() => {
      window.modbit
        .attention(sessionId)
        .then((a) => setAttentionItems(a.items))
        .catch(() => {});
    }, 150);
  }, []);
  const [workspaceRoot, setWorkspaceRoot] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [reviewing, setReviewing] = useState<string | null>(null);
  // M7.1: the browser session shown over the fleet (one task's live view).
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
  // PX-024: the keyboard model's state — the type-ahead filter, the help
  // panel, a pending confirmation before an irreversible effect, the card
  // focus to retain across state changes, the live region's announcement.
  const [filter, setFilter] = useState("");
  const [helpOpen, setHelpOpen] = useState(false);
  const [confirm, setConfirm] = useState<{ kind: "approve" | "cancel"; taskId: string; text: string } | null>(null);
  const focusedTask = useRef<string | null>(null);
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
      // Context Inspector for the newest task, when it has compiled a pack.
      const newest = [...m.tasks.values()].sort((a, b) => b.createdAtMs - a.createdAtMs)[0];
      if (newest) {
        try {
          setInspector(await window.modbit.contextInspector(newest.taskId));
        } catch {
          setInspector(null);
        }
        try {
          setEconomics(await window.modbit.taskEconomics(newest.taskId));
        } catch {
          setEconomics(null);
        }
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

  const submit = useCallback(
    async (ev: React.FormEvent) => {
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
        const r = await window.modbit.createTask(sessionId, text, commandId, root, issue || undefined);
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
      } catch (e) {
        setError((e as Error).message);
        setCreateError((e as Error).message);
      } finally {
        setSubmitting(false);
      }
    },
    [goal, issueUrl, workspaceRoot, submitting, trusted],
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

  // PX-024 focus retention: a card that moves between columns on a Core
  // event is re-mounted by React; the focus it held comes back to it.
  useEffect(() => {
    const id = focusedTask.current;
    if (!id || reviewing) return;
    const active = document.activeElement;
    if (active && active !== document.body && active.closest('[data-testid="task-card"]')) return;
    if (active && active !== document.body && active.getAttribute("data-testid") !== "task-card") return;
    const el = document.querySelector<HTMLElement>(`[data-testid="task-card"][data-task-id="${id}"]`);
    el?.focus({ preventScroll: true });
  }, [model, reviewing]);
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
        await window.modbit.startTask(sessionId, r.taskId);
      } catch (e) {
        setError((e as Error).message);
      } finally {
        setSubmitting(false);
      }
    },
    [submitting, trusted],
  );

  const cols = useMemo(() => columns(model), [model]);
  const attention = cols.needsAttention.length;
  const visible = useCallback((cards: TaskCard[]) => (filter.trim() ? cards.filter((t) => t.goalText.toLowerCase().includes(filter.trim().toLowerCase())) : cards), [filter]);
  const focusedCard = () => document.activeElement?.closest<HTMLElement>('[data-testid="task-card"]') ?? null;
  const cardsOf = (column: Element) => [...column.querySelectorAll<HTMLElement>(':scope > [data-testid="task-card"]')];
  const runFleetCommand = useCallback(
    async (cmd: Command) => {
      const sessionId = modelRef.current.sessionId;
      // The focused task: its card, or an attention item or notification
      // about it (approve, deny, cancel and steer act on that task).
      const taskId = document.activeElement?.closest("[data-task-id]")?.getAttribute("data-task-id") ?? null;
      const card = focusedCard() ?? (taskId ? document.querySelector<HTMLElement>(`[data-testid="task-card"][data-task-id="${taskId}"]`) : null);
      const task = taskId ? modelRef.current.tasks.get(taskId) : undefined;
      switch (cmd) {
        case "newTask":
          document.getElementById("goal")?.focus();
          return;
        case "search":
          document.querySelector<HTMLElement>('[data-testid="search"]')?.focus();
          return;
        case "help":
          setHelpOpen((h) => !h);
          return;
        case "jumpAttention": {
          const target = document.querySelector<HTMLElement>('[data-testid="attention-item"]') ?? document.querySelector<HTMLElement>('[data-testid="attention"]') ?? document.querySelector<HTMLElement>('[data-testid="column-needsAttention"]');
          target?.focus();
          return;
        }
        case "jumpRunning": {
          const column = document.querySelector<HTMLElement>('[data-testid="column-running"]');
          (column ? (cardsOf(column)[0] ?? column) : null)?.focus();
          return;
        }
        case "next":
        case "previous": {
          const column = card?.parentElement ?? document.querySelector<HTMLElement>('[data-testid="task-card"]')?.parentElement;
          if (!column) return;
          const cards = cardsOf(column);
          const i = card ? cards.indexOf(card) : -1;
          const target = cards[i < 0 ? 0 : Math.min(cards.length - 1, Math.max(0, i + (cmd === "next" ? 1 : -1)))];
          target?.focus();
          return;
        }
        case "columnNext":
        case "columnPrevious": {
          const columns = [...document.querySelectorAll<HTMLElement>('[data-testid^="column-"]')];
          const focused = focusedCard();
          const current = focused?.parentElement ?? null;
          const step = cmd === "columnNext" ? 1 : -1;
          // From outside the board: the first (→) or last (←) column with cards.
          const from = current ? columns.indexOf(current) : step > 0 ? -1 : columns.length;
          for (let j = from + step; j >= 0 && j < columns.length; j += step) {
            const cards = cardsOf(columns[j]!);
            if (cards.length) {
              const index = current && focused ? cardsOf(current).indexOf(focused) : 0;
              cards[Math.min(cards.length - 1, Math.max(0, index))]!.focus();
              return;
            }
          }
          return;
        }
        case "open":
          if (!task) return;
          if (task.state === "ReadyForReview" || task.state === "Completed") setReviewing(task.taskId);
          else card?.querySelector<HTMLElement>("button:not(:disabled)")?.focus();
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
          card?.querySelector<HTMLElement>('[data-testid="task-steer"]')?.focus();
          return;
        case "confirm": {
          const c = confirm;
          if (!c || !sessionId) return;
          setConfirm(null);
          const t = modelRef.current.tasks.get(c.taskId);
          // The focus belongs to this task through the state change its
          // effect causes (the card re-mounts in another column).
          focusedTask.current = c.taskId;
          document.querySelector<HTMLElement>(`[data-testid="task-card"][data-task-id="${c.taskId}"]`)?.focus();
          try {
            if (c.kind === "cancel") await window.modbit.cancelTask(sessionId, c.taskId);
            else if (t?.approval) await window.modbit.resolveApproval(sessionId, t.approval.approvalId, true, "approved from the keyboard, confirmed", t.approval.intentHash);
          } catch (e) {
            setError((e as Error).message);
          }
          document.querySelector<HTMLElement>(`[data-testid="task-card"][data-task-id="${c.taskId}"]`)?.focus();
          return;
        }
        case "back": {
          if (confirm) {
            setConfirm(null);
            document.querySelector<HTMLElement>(`[data-testid="task-card"][data-task-id="${confirm.taskId}"]`)?.focus();
            return;
          }
          if (helpOpen) {
            setHelpOpen(false);
            return;
          }
          if (reviewing) {
            // The retention effect focuses the card once the fleet is shown again.
            focusedTask.current = reviewing;
            setReviewing(null);
            return;
          }
          if (browsing) {
            focusedTask.current = browsing.taskId;
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
    [confirm, helpOpen, reviewing, browsing],
  );
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      // Enter and Space on a button or a checkbox are that control's.
      if ((e.key === "Enter" || e.key === " ") && isActivatable(document.activeElement) && confirm === null) return;
      const cmd = commandFor(e, { editable: isEditable(document.activeElement), screen: reviewing ? "review" : "fleet", confirming: confirm !== null, isMac: navigator.platform.toUpperCase().includes("MAC") });
      if (!cmd) return;
      // Over the browser panel only the global shortcuts and Escape apply.
      if (browsing && !["back", "help", "newTask", "search", "confirm"].includes(cmd)) return;
      // The Review handles its own change navigation (hunk*, split, failing).
      if (["hunkNext", "hunkPrevious", "hunkToggle", "toggleSplit", "jumpFailing"].includes(cmd)) return;
      e.preventDefault();
      void runFleetCommand(cmd);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [runFleetCommand, reviewing, confirm, browsing]);
  const tasks = useMemo(() => [...model.tasks.values()], [model]);
  // PX-023: the screen states, each derived from what the Core said.
  const fleet = useMemo(() => fleetState({ core, loaded: screen === "loading" ? "loading" : screen === "empty" ? "empty" : "populated", error, recovery: recovery ? { bootGeneration: Number(recovery.bootGeneration), eventsVerified: Number(recovery.eventsVerified), aggregatesVerified: Number(recovery.aggregatesVerified), sessions: Number(recovery.sessions), tasks: Number(recovery.tasks), projectionsRebuilt: recovery.projectionsRebuilt, notes: recovery.notes, recoveryMs: Number(recovery.recoveryMs) } : null, recoveredBanner: recovered !== null, tasks }), [core, screen, error, recovery, recovered, tasks]);
  const composer = useMemo(() => newTaskState({ core, provider: provider ? { configured: provider.configured } : null, trusted, workspaceRoot, lastError: createError }), [core, provider, trusted, workspaceRoot, createError]);
  const settings = useMemo(() => settingsState({ provider: provider ? { keychainAvailable: provider.keychainAvailable, configured: provider.configured } : null, organizationOverrides: [] }), [provider]);
  // PX-023: the notifications the fleet warrants, coalesced per task and
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
    const target = link.screen === "task" && link.taskId ? document.querySelector<HTMLElement>(`[data-testid="task-card"][data-task-id="${link.taskId}"]`) : document.querySelector<HTMLElement>('[data-testid="attention"]');
    target?.scrollIntoView({ block: "center" });
    target?.focus();
  }, []);
  // docs/39 "Empty states": no provider keeps the welcome up with step 2
  // highlighted whatever else exists; a provider with no trusted repository
  // and no tasks keeps it up with step 3.
  const onboarding = core.state === "connected" && provider !== null && (!provider.configured || model.tasks.size === 0);

  return (
    <>
      <header>
        <h1>Modbit</h1>
        <span className="meta">Fleet</span>
        <span className="status" data-testid="core-status" aria-live="polite">
          {core.state === "connected" ? `Core connected (pid ${core.pid})` : core.state === "restarting" ? `Core restarting…` : core.state === "failed" ? "Core failed" : "Core starting…"}
        </span>
      </header>
      <div role="region" aria-label="Status and notifications">
      <StateLine state={fleet} testid="fleet-state" />
      <p className="sr-only" role="status" aria-live="polite" data-testid="live-attention">{announcement}</p>
      {confirm && (
        <div className="banner" data-kind="error" role="alertdialog" aria-modal="false" aria-labelledby="confirm-text" data-testid="confirm" data-confirm-kind={confirm.kind} data-task-id={confirm.taskId}>
          <strong id="confirm-text">{confirm.text}</strong>{" "}
          <span className="meta">Enter confirms · Escape cancels</span>{" "}
          <button type="button" className="small" data-testid="confirm-yes" onClick={() => void runFleetCommand("confirm")}>Confirm</button>{" "}
          <button type="button" className="small" data-testid="confirm-no" onClick={() => void runFleetCommand("back")}>Cancel</button>
        </div>
      )}
      {helpOpen && (
        <section className="banner" data-kind="info" role="dialog" aria-label="Keyboard shortcuts" data-testid="help">
          <strong>Keyboard</strong>
          <ul>
            {SHORTCUTS.map((s) => (
              <li key={s.command + s.keys}>
                <kbd>{s.keys}</kbd> — {s.what}
              </li>
            ))}
          </ul>
          <button type="button" className="small" onClick={() => setHelpOpen(false)}>Close</button>
        </section>
      )}
      {notifications.length > 0 && (
        <section className="notifications" data-testid="notifications" aria-label="notifications" aria-live="polite">
          <ul>
            {notifications.map((n) => (
              <li key={n.id} data-testid="notification" data-kind={n.kind} data-id={n.id} data-coalesced={n.coalesced} data-count={n.count ?? 1} data-delivered={delivered.includes(`${n.id}:${n.reason}`) ? "true" : "false"}>
                <strong>{n.title}</strong> · {n.reason} · <em>{n.nextAction}</em>{" "}
                <button type="button" className="small" data-testid="notification-open" onClick={() => open(n.deepLink)}>
                  Open {n.deepLink.screen}
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      {inspector && inspector.packId !== "" && (
        <div className="banner" data-kind="info" data-testid="context-inspector">
          <strong>Context</strong> — {inspector.entries.filter((e) => e.injected).length} of {inspector.entries.length} fragment(s) injected, {inspector.tokenUsed}/{inspector.tokenBudget} tokens, {inspector.omittedCount} omitted
          {inspector.rejectedRefs.length > 0 ? `, ${inspector.rejectedRefs.length} refused for missing provenance` : ""}.
          {inspector.compactionEpochs > 0 && (
            <div data-testid="context-compaction">
              <strong>Compaction</strong> — epoch {inspector.compactionEpoch} of {inspector.compactionEpochs}, {String(inspector.compactedEntries)} earlier entr{Number(inspector.compactedEntries) === 1 ? "y" : "ies"} summarised and still in the log; prompt prefix reused on {inspector.prefixCacheHits} turn(s), rebuilt on {inspector.prefixCacheMisses}.
            </div>
          )}
          <ul>
            {inspector.entries.map((e) => (
              <li key={e.entryId} data-testid="context-entry">
                {e.sourceRef}
                {e.lineEnd > 0 ? ` L${e.lineStart}-${e.lineEnd}` : ""} — {e.reason}; {e.freshness}; {e.tokenCost} tokens
                {e.injected ? "; injected" : "; not injected"}
                {e.used ? "; used" : ""}
                {e.stub ? "; stub" : ""}
              </li>
            ))}
            {inspector.omittedPaths.map((p) => (
              <li key={p} data-testid="context-omitted">
                {p} — omitted for the token budget
              </li>
            ))}
          </ul>
        </div>
      )}
      {economics && economics.modelCalls > 0 && (
        <div className="banner" data-kind="info" data-testid="task-economics">
          <strong>Economics</strong> — {economics.verified ? "verified" : "not verified"}, {economics.checksPassed}/{economics.checksPassed + economics.checksFailed} check(s) passed; {economics.modelCalls} model call(s) on {economics.model || "no model"}, {economics.inputTokens} in ({economics.cachedInputTokens} cached) and {economics.outputTokens} out{economics.pricingKnown ? `, $${economics.costUsd.toFixed(4)} at catalog list prices` : ", price unknown"}; {economics.toolCalls} tool call(s); {economics.wallMs} ms wall, {economics.modelMs} ms in the model, {economics.toolMs} ms in tools; prompt prefix reused {economics.prefixCacheHits} of {economics.prefixCacheHits + economics.prefixCacheMisses} time(s).
        </div>
      )}
      {languages.length > 0 && (
        <div className="banner" data-kind="info" data-testid="language-labels" title={languages.map((l) => `${l.language}: ${l.label}`).join("\n")}>
          <strong>Languages</strong> — {languages.filter((l) => l.tier === "ALPHA_BASELINE").map((l) => l.language).join(", ")} at the Alpha baseline (Tier C plus compile and test evidence; structural intelligence not yet claimed); everything else unsupported.
        </div>
      )}
      {core.state === "restarting" && (
        <div className="banner" data-kind="restarting" role="status" data-testid="banner-restarting">
          <strong>Core restarting</strong> — {core.reason}. Showing the last persisted state; retrying in {Math.round(core.retryInMs / 1000)}s. Nothing shown here is new progress.
        </div>
      )}
      {core.state === "failed" && (
        <div className="banner" data-kind="error" role="alert" data-testid="banner-failed">
          <strong>Core unavailable</strong> — {core.reason}. Last durable state is shown; restart the app to retry.
        </div>
      )}
      {recovered && core.state === "connected" && (
        <div className="banner" data-kind="recovered" role="status" data-testid="banner-recovered">
          <strong>Recovered</strong> — {recovered}
          {recovery ? ` (boot ${recovery.bootGeneration}): the Core verified ${recovery.eventsVerified} events across ${recovery.aggregatesVerified} aggregates and recovered ${recovery.sessions} session(s) and ${recovery.tasks} task(s) in ${recovery.recoveryMs} ms${recovery.projectionsRebuilt ? "; projections were rebuilt from the log" : ""}${recovery.notes.length ? `; still to reconcile: ${recovery.notes.join("; ")}` : "; nothing left to reconcile"}.` : "."}{" "}
          <button type="button" onClick={() => setRecovered(null)}>Dismiss</button>
        </div>
      )}
      {error && (
        <div className="banner" data-kind="error" role="alert" data-testid="banner-error">
          <strong>Error</strong> — {error}. <button type="button" onClick={() => void load()}>Retry</button>
        </div>
      )}
      {onboarding && (
        <section className="welcome" data-testid="welcome" aria-label="Welcome">
          <h2 style={{ margin: 0, fontSize: 16 }}>Welcome to Modbit</h2>
          <p className="meta">Three steps to a first reviewed change. Nothing here is stored outside the Core and your OS keychain.</p>
          <ol className="steps">
            <li data-testid="welcome-step-core" data-done="true">Core running (pid {core.state === "connected" ? core.pid : "…"})</li>
            <li data-testid="welcome-step-provider" data-done={provider?.configured ? "true" : "false"} data-active={!provider?.configured ? "true" : "false"}>
              <strong>Provider.</strong>{" "}
              {provider?.configured ? (
                <span data-testid="provider-ready">
                  Ready: {provider.endpoints.join(", ")}.
                  {providerResult?.ok && (
                    <span className="meta" role="status" data-testid="provider-result" data-ok="true">
                      {" "}
                      {providerResult.text}
                    </span>
                  )}
                </span>
              ) : (
                <form
                  onSubmit={(ev) => {
                    ev.preventDefault();
                    void setupProvider();
                  }}
                  aria-label="Provider setup"
                >
                  <label htmlFor="provider-kind" className="meta">Provider</label>
                  <select id="provider-kind" data-testid="provider-kind" value={providerKind} onChange={(e) => setProviderKind(e.target.value as "openai" | "anthropic")}>
                    <option value="openai">OpenAI (or a compatible endpoint)</option>
                    <option value="anthropic">Anthropic</option>
                  </select>
                  <label htmlFor="provider-key" className="meta">API key (goes to the OS keychain through the app; never shown again)</label>
                  <input id="provider-key" data-testid="provider-key" type="password" autoComplete="off" value={providerKey} onChange={(e) => setProviderKey(e.target.value)} />
                  <label htmlFor="provider-url" className="meta">Endpoint URL (optional; a compatible endpoint)</label>
                  <input id="provider-url" data-testid="provider-url" value={providerUrl} onChange={(e) => setProviderUrl(e.target.value)} placeholder="https://" />
                  <button type="submit" data-testid="provider-test" disabled={providerBusy}>
                    {providerBusy ? "Testing…" : "Test and save"}
                  </button>
                  {providerResult && (
                    <div className="meta" role="status" data-testid="provider-result" data-ok={providerResult.ok ? "true" : "false"}>
                      {providerResult.text}
                    </div>
                  )}
                </form>
              )}
            </li>
            <li data-testid="welcome-step-repository" data-done={trusted ? "true" : "false"} data-active={provider?.configured && !trusted ? "true" : "false"}>
              <strong>Repository.</strong>{" "}
              {trusted ? (
                <span data-testid="repository-trusted">Trusted: {trusted}. Indexing runs in the background; you can start now.</span>
              ) : (
                <form
                  onSubmit={(ev) => {
                    ev.preventDefault();
                    void trustRepository();
                  }}
                  aria-label="Repository trust"
                >
                  <label htmlFor="trust-root" className="meta">Local repository (a Git checkout on this machine)</label>
                  <input id="trust-root" data-testid="trust-root" value={trustRoot} onChange={(e) => setTrustRoot(e.target.value)} placeholder="/path/to/repo" disabled={!provider?.configured} />
                  <button type="submit" data-testid="trust-confirm" disabled={!provider?.configured || trustBusy || !trustRoot.trim()}>
                    {trustBusy ? "Trusting…" : "Trust this repository"}
                  </button>
                  <div className="meta">Trust is scoped to this folder only: Modbit may read it, index it and run its checks. Nothing else on this machine is in scope.</div>
                  {trustError && (
                    <div className="meta" role="alert" data-testid="trust-error">
                      {trustError}
                    </div>
                  )}
                </form>
              )}
            </li>
          </ol>
          {trusted && starters && (
            <div data-testid="starter-tasks">
              <strong>First task</strong>
              <span className="meta"> — detected: {starters.stacks.join(", ")}. Pick one, or write your own goal below.</span>
              <ul>
                {starters.tasks.map((t) => (
                  <li key={t.id}>
                    <button type="button" data-testid="starter-task" data-starter-id={t.id} disabled={submitting} onClick={() => void runStarter(t.goalText)}>
                      {t.title}
                    </button>{" "}
                    <span className="meta">{t.goalText}</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </section>
      )}
      </div>
      {reviewing && model.sessionId && <Review taskId={reviewing} sessionId={model.sessionId} card={model.tasks.get(reviewing) ?? null} onClose={() => void runFleetCommand("back")} />}
      {browsing && !reviewing && <Browser browsing={browsing} card={model.tasks.get(browsing.taskId) ?? null} sessionId={model.sessionId} onReopen={() => void openBrowser(browsing.taskId)} onClose={() => void runFleetCommand("back")} />}
      <main hidden={reviewing !== null || browsing !== null}>
        <div className="side">
          <form className="composer" onSubmit={submit} aria-label="New Task">
            <h2 style={{ margin: 0, fontSize: 14 }}>New Task</h2>
            <label htmlFor="goal" className="meta">Goal</label>
            <textarea id="goal" data-testid="goal" value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="What should the agent achieve?" disabled={core.state !== "connected"} />
            <label htmlFor="issue-url" className="meta">Or from an issue (a GitHub issue URL; its text enters as untrusted context)</label>
            <input id="issue-url" data-testid="issue-url" value={issueUrl} onChange={(e) => setIssueUrl(e.target.value)} placeholder="https://github.com/owner/repo/issues/123" disabled={core.state !== "connected"} />
            <label htmlFor="workspace" className="meta">Workspace root (a local Git checkout; empty for a Work space)</label>
            <input id="workspace" data-testid="workspace" value={workspaceRoot} onChange={(e) => setWorkspaceRoot(e.target.value)} placeholder="/path/to/repo" disabled={core.state !== "connected"} />
            <div className="meta">Execution: local_trusted · Origin: desktop · Running here trusts this repository, scoped to it, if it is not trusted yet.</div>
            {provider !== null && !provider.configured && (
              <div className="meta" role="status" data-testid="composer-no-provider">
                No provider is set up: a task can be created but will not start until step 2 above is done.
              </div>
            )}
            <button type="submit" data-testid="run" disabled={core.state !== "connected" || submitting || (!goal.trim() && !issueUrl.trim())}>
              {submitting ? "Creating…" : "Run"}
            </button>
            {core.state !== "connected" && <div className="meta">Task creation is disabled until the Core is connected.</div>}
            <StateLine state={composer} testid="composer-state" />
          </form>
          <section className="settings" data-testid="settings" aria-label="Settings">
            <h2 style={{ margin: 0, fontSize: 14 }}>Settings</h2>
            <div className="meta">
              Credentials: {provider === null ? "loading…" : provider.configured ? `${provider.stored ? "provider key in the OS keychain" : "provider key held for this session only"} (${provider.provider})` : "no provider key"}; the Core holds keys in memory only and never shows one again.
            </div>
            <StateLine state={settings} testid="settings-state" />
            <fieldset data-testid="credentials">
              <legend className="meta">Login credentials (bound to one origin; the agent fills them by handle and never sees a value)</legend>
              {credentials.length === 0 ? (
                <div className="meta" data-testid="credentials-empty">None yet.</div>
              ) : (
                <ul data-testid="credential-list">
                  {credentials.map((c) => (
                    <li key={c.handle} className="meta" data-testid="credential" data-handle={c.handle} data-origin={c.origin}>
                      <code>{c.handle}</code> · {c.label} · {c.origin} · {c.username || "(no account name)"} · {c.persisted ? "in the OS keychain" : "held for this session only"}{" "}
                      <button type="button" data-testid="credential-remove" onClick={() => void window.modbit.removeCredential(c.handle).then(refreshCredentials)}>
                        Forget
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <form
                data-testid="credential-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  setCredentialError(null);
                  const f = credentialForm;
                  void window.modbit
                    .addCredential(f.label, f.origin, f.username, f.secret)
                    .then(() => {
                      setCredentialForm({ label: "", origin: "", username: "", secret: "" });
                      refreshCredentials();
                    })
                    .catch((err: Error) => setCredentialError(err.message));
                }}
              >
                <label className="meta">
                  Label <input data-testid="credential-label" value={credentialForm.label} onChange={(e) => setCredentialForm({ ...credentialForm, label: e.target.value })} />
                </label>{" "}
                <label className="meta">
                  Origin <input data-testid="credential-origin" placeholder="https://host[:port]" value={credentialForm.origin} onChange={(e) => setCredentialForm({ ...credentialForm, origin: e.target.value })} />
                </label>{" "}
                <label className="meta">
                  Account <input data-testid="credential-username" value={credentialForm.username} onChange={(e) => setCredentialForm({ ...credentialForm, username: e.target.value })} />
                </label>{" "}
                <label className="meta">
                  Secret <input data-testid="credential-secret" type="password" value={credentialForm.secret} onChange={(e) => setCredentialForm({ ...credentialForm, secret: e.target.value })} />
                </label>{" "}
                <button type="submit" data-testid="credential-add" disabled={core.state !== "connected" || !credentialForm.label.trim() || !credentialForm.origin.trim() || !credentialForm.secret}>
                  Add credential
                </button>
                {credentialError && (
                  <div className="meta" role="alert" data-testid="credential-error">
                    {credentialError}
                  </div>
                )}
              </form>
            </fieldset>
            <fieldset data-testid="notification-preferences">
              <legend className="meta">OS notifications (off until you opt in; the in-app list is always on)</legend>
              {(["attention", "completion", "failure"] as NotificationKind[]).map((k) => (
                <label key={k} className="meta">
                  <input type="checkbox" data-testid={`notify-${k}`} checked={prefs.os[k]} onChange={(e) => savePrefs({ ...prefs, os: { ...prefs.os, [k]: e.target.checked } })} /> {k}
                </label>
              ))}
              <label className="meta">
                <input type="checkbox" data-testid="notify-quiet" checked={prefs.quietHours !== null} onChange={(e) => savePrefs({ ...prefs, quietHours: e.target.checked ? { start: 22, end: 7 } : null })} /> quiet hours
              </label>
              {prefs.quietHours && (
                <span className="meta">
                  {" from "}
                  <input type="number" min={0} max={23} data-testid="notify-quiet-start" value={prefs.quietHours.start} onChange={(e) => savePrefs({ ...prefs, quietHours: { start: Number(e.target.value), end: prefs.quietHours!.end } })} />
                  {" to "}
                  <input type="number" min={0} max={23} data-testid="notify-quiet-end" value={prefs.quietHours.end} onChange={(e) => savePrefs({ ...prefs, quietHours: { start: prefs.quietHours!.start, end: Number(e.target.value) } })} />
                </span>
              )}
            </fieldset>
          </section>
        </div>
        <div>
          <label htmlFor="search" className="sr-only">Filter tasks</label>
          <input id="search" data-testid="search" type="search" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter tasks (/)" aria-label="Filter tasks by goal" />
          <p className="sr-only" aria-live="polite">{attention} tasks need attention</p>
          {attentionItems.length > 0 && (
            <section className="attention" data-testid="attention" aria-label="attention items" tabIndex={-1}>
              <h2>Attention ({attentionItems.length})</h2>
              <ul>
                {attentionItems.map((i) => (
                  <li key={`${i.kind}:${i.taskId}:${i.reference}`} data-testid="attention-item" data-kind={i.kind} data-task-id={i.taskId} tabIndex={-1}>
                    <strong>{i.kind}</strong> · {i.reason} · <em>{i.action}</em>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {screen === "loading" && <p className="meta" data-testid="fleet-loading">Loading fleet from the Core…</p>}
          {screen === "empty" && <p className="empty" data-testid="fleet-empty">No tasks yet. Create one on the left.</p>}
          <div className="fleet" data-testid="fleet" data-screen={screen}>
            {COLUMNS.map((c) => (
              <section className="column" key={c.key} aria-label={`${c.title}, ${cols[c.key].length} task(s)`} data-testid={`column-${c.key}`} tabIndex={-1}>
                <h2>
                  {c.title} <span aria-hidden="true">({cols[c.key].length})</span>
                </h2>
                {cols[c.key].length === 0 ? <p className="empty">None</p> : visible(cols[c.key]).map((t) => <Card key={t.taskId} card={t} children={childrenOf(model, t.taskId)} state={taskState({ card: t, core, attention: attentionItems })} sessionId={model.sessionId} onFocus={(id) => (focusedTask.current = id)} onStart={startTask} onReview={(id) => setReviewing(id)} onBrowser={(id) => void openBrowser(id)} />)}
              </section>
            ))}
          </div>
        </div>
      </main>
    </>
  );
}

const PHASE_LABEL: Record<TaskCard["phase"], string> = {
  drafting: "drafting",
  verifying: "verifying",
  reviewing: "reviewing",
  escalating: "escalating",
  awaitingHuman: "awaiting human",
  waitingCapacity: "waiting for capacity",
  delegating: "delegating",
  done: "done",
};

/** PRD "Home / Fleet" card (M6.6): goal, state, phase, active agents, risk,
 *  latest evidence, next required action, and the subagents nested under
 *  their parent — every fact from a Core event. */
function Card({ card, children, state, sessionId, onFocus, onStart, onReview, onBrowser }: { card: TaskCard; children?: TaskCard[]; state: DerivedScreenState; sessionId: string | null; onFocus: (id: string) => void; onStart: (id: string) => void; onReview: (id: string) => void; onBrowser: (id: string) => void }) {
  // PX-024: one line of steering input on the card (QueueInput STEER).
  const [steer, setSteer] = useState("");
  const [steerNote, setSteerNote] = useState<string | null>(null);
  const sendSteer = async () => {
    if (!sessionId || !steer.trim()) return;
    try {
      const r = await window.modbit.steerTask(sessionId, card.taskId, steer.trim());
      setSteerNote(`steered at offset ${r.offset}`);
      setSteer("");
    } catch (e) {
      setSteerNote(`refused: ${(e as Error).message}`);
    }
  };
  // PX-023 "awaiting approval with the exact intent" / "awaiting your
  // answer": the decision names the intent hash the Core showed (the Core
  // refuses any other), the answer names the question; both are the
  // Core's records, not the renderer's.
  const [deciding, setDeciding] = useState(false);
  const [decisionNote, setDecisionNote] = useState<string | null>(null);
  const [answer, setAnswer] = useState("");
  const decide = async (approve: boolean) => {
    if (!sessionId || !card.approval || deciding) return;
    setDeciding(true);
    try {
      const r = await window.modbit.resolveApproval(sessionId, card.approval.approvalId, approve, approve ? "approved on the task card" : "denied on the task card", card.approval.intentHash);
      setDecisionNote(`${r.status.toLowerCase()} at offset ${r.offset}`);
    } catch (e) {
      setDecisionNote(`refused: ${(e as Error).message}`);
    } finally {
      setDeciding(false);
    }
  };
  const respond = async (optionId: string, text: string) => {
    if (!sessionId || !card.question || deciding) return;
    setDeciding(true);
    try {
      const r = await window.modbit.respondToQuestion(sessionId, card.taskId, card.question.questionId, optionId, text);
      setDecisionNote(r.alreadyAnswered ? "already answered" : `answered question ${r.questionId.slice(0, 8)}; resume to continue`);
      setAnswer("");
    } catch (e) {
      setDecisionNote(`refused: ${(e as Error).message}`);
    } finally {
      setDeciding(false);
    }
  };
  // A queued task starts; a waiting task resumes with StartTask unless it
  // waits on an approval (decide it) or capacity (the Core grants it).
  const startable = card.state === "Queued" || (card.state === "Waiting" && card.waitReason !== "Approval" && card.waitReason !== "Capacity");
  const active = card.agents.running + card.agents.background;
  return (
    <article className="card" tabIndex={0} data-testid="task-card" data-task-id={card.taskId} data-state={card.state} data-phase={card.phase} aria-label={`${card.goalText}: ${card.state}${card.waitReason ? ` waiting on ${card.waitReason}` : ""}, ${PHASE_LABEL[card.phase]}`} onFocus={() => onFocus(card.taskId)}>
      <div>{card.goalText}</div>
      <div className="meta">
        state: <span data-testid="task-state">{card.state}</span>
        {card.waitReason ? ` · waiting on ${card.waitReason}` : ""} · gen {card.generation} · local_trusted
        {" · attachments "}
        <span data-testid="task-attachments">{card.attachments ?? 0}</span>
        {card.security?.length ? (
          <>
            {" · security "}
            <span data-testid="task-security" data-count={card.security.length} title={card.security.map((s) => `${s.kind} in ${s.toolName}: ${s.patterns.join(", ")} (${s.action.toLowerCase()})`).join("\n")}>
              {card.security.filter((s) => s.action === "BLOCKED").length} blocked, {card.security.filter((s) => s.action === "MARKED").length} marked
            </span>
          </>
        ) : null}
      </div>
      <div className="meta">
        phase: <span data-testid="task-phase">{PHASE_LABEL[card.phase]}</span>
        {" · agents "}
        <span data-testid="task-agents" aria-label={`${active} active of ${card.agents.total} agents`}>
          {active}/{card.agents.total}
        </span>
        {card.risk ? (
          <>
            {" · risk "}
            <span data-testid="task-risk">{card.risk}</span>
          </>
        ) : null}
      </div>
      {card.latestEvidence && (
        <div className="meta">
          evidence: <span data-testid="task-evidence">{card.latestEvidence}</span>
        </div>
      )}
      {card.nextAction && (
        <div className="meta">
          next: <strong>{card.nextAction}</strong>
        </div>
      )}
      <StateLine state={state} testid="task-screen-state" />
      {card.state === "Waiting" && card.waitReason === "Approval" && card.approval && (
        <div className="decision" data-testid="task-approval" data-approval-id={card.approval.approvalId}>
          <span className="meta">
            {card.approval.toolName} asks for a {card.approval.effectClass} effect · intent <code data-testid="task-approval-intent">{card.approval.intentHash.slice(0, 16)}</code>
          </span>{" "}
          <button type="button" className="small" data-testid="task-approve" onClick={() => void decide(true)} disabled={deciding}>
            Approve
          </button>{" "}
          <button type="button" className="small" data-testid="task-deny" onClick={() => void decide(false)} disabled={deciding}>
            Deny
          </button>
        </div>
      )}
      {card.question && (
        <div className="decision" data-testid="task-question" data-question-id={card.question.questionId}>
          <span className="meta" data-testid="task-question-text">{card.question.text}</span>{" "}
          {card.question.options.map((o) => (
            <button type="button" className="small" key={o.id} data-testid="task-answer-option" data-option-id={o.id} onClick={() => void respond(o.id, "")} disabled={deciding}>
              {o.label}
            </button>
          ))}
          {card.question.allowFreeText && (
            <>
              <input data-testid="task-answer-text" aria-label="Your answer" value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="your answer" disabled={deciding} />
              <button type="button" className="small" data-testid="task-answer-send" onClick={() => void respond("", answer)} disabled={deciding || !answer.trim()}>
                Answer
              </button>
            </>
          )}
        </div>
      )}
      {decisionNote && (
        <div className="meta" role="status" data-testid="task-decision">
          {decisionNote}
        </div>
      )}
      {(card.state === "Running" || card.state === "Waiting" || card.state === "Queued") && (
        <div className="decision">
          <input data-testid="task-steer" aria-label={`Steer ${card.goalText}`} value={steer} onChange={(e) => setSteer(e.target.value)} onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); void sendSteer(); } }} placeholder="steer (s), Enter sends" />
          {steerNote && (
            <span className="meta" role="status" data-testid="task-steer-note">
              {steerNote}
            </span>
          )}
        </div>
      )}
      {children && children.length > 0 && (
        <ul className="children" data-testid="task-children" aria-label="subagents">
          {children.map((c) => (
            <li key={c.taskId} data-testid="child-card" data-task-id={c.taskId} data-state={c.state}>
              <span data-testid="child-goal">{c.goalText}</span> · <span data-testid="child-state">{c.state}</span>
              {c.latestEvidence ? ` · ${c.latestEvidence}` : ""}
            </li>
          ))}
        </ul>
      )}
      <div className="actions">
        {startable && (
          <button type="button" data-testid="task-start" onClick={() => onStart(card.taskId)}>
            {card.state === "Waiting" ? "Resume" : "Start"}
          </button>
        )}
        {card.state === "ReadyForReview" && (
          <button type="button" data-testid="task-review" onClick={() => onReview(card.taskId)}>
            Review
          </button>
        )}
        {card.state !== "Completed" && card.state !== "Cancelled" && card.state !== "Failed" && (
          <button type="button" data-testid="task-browser" onClick={() => onBrowser(card.taskId)}>
            Browser
          </button>
        )}
      </div>
    </article>
  );
}

/** Browser panel (M7.1, docs/22): the task's live Chromium session — main's
 *  sandboxed view placed over the placeholder below; the URL, title and
 *  state version are what the host reports, the lease what the Core
 *  records. The page's content never reaches this renderer. */
function Browser({ browsing, card, sessionId, onReopen, onClose }: { browsing: { taskId: string; browserSessionId: string | null; error: string | null }; card: TaskCard | null; sessionId: string | null; onReopen: () => void; onClose: () => void }) {
  const [host, setHost] = useState<{ attached: boolean; shown: boolean; url: string; title: string; stateVersion: number; leaseGeneration: number; controller: "AGENT" | "USER"; stopped?: string | null; humanInputAt?: number } | null>(null);
  const [controlBusy, setControlBusy] = useState(false);
  const [stopped, setStopped] = useState<string | null>(null);
  // IMP-EV-0085: the emergency stop — the host's input halts at once, the
  // Core blocks every new effect; the reason is on the log.
  const emergencyStop = async () => {
    if (!sessionId || controlBusy) return;
    setControlBusy(true);
    try {
      await window.modbit.emergencyStop(sessionId, "stopped from the Browser panel");
      setStopped("stopped from the Browser panel");
    } finally {
      setControlBusy(false);
    }
  };
  // M7.6: take or return control — the lease moves, the same session stays.
  const setControl = async (controller: "AGENT" | "USER") => {
    if (!bsid || controlBusy) return;
    setControlBusy(true);
    try {
      await window.modbit.setBrowserControl(bsid, controller);
    } finally {
      setControlBusy(false);
    }
  };
  const [core, setCore] = useState<BrowserSessionSummary | null>(null);
  const [gone, setGone] = useState<string | null>(null);
  const [probe, setProbe] = useState<{ node_reachable: boolean; partition: string; sandboxed: boolean; context_isolated: boolean } | null>(null);
  const placeholder = useRef<HTMLDivElement>(null);
  const bsid = browsing.browserSessionId;
  const taskId = card?.taskId ?? null;
  // Place the view over the placeholder and keep it there through resizes.
  useEffect(() => {
    if (!bsid) return;
    const place = () => {
      const r = placeholder.current?.getBoundingClientRect();
      if (!r) return;
      void window.modbit.showBrowser(bsid, { x: r.left, y: r.top, width: r.width, height: r.height }).catch(() => {});
    };
    place();
    const ro = new ResizeObserver(place);
    if (placeholder.current) ro.observe(placeholder.current);
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    const refresh = () => {
      void window.modbit.describeBrowser(bsid).then(setHost).catch(() => setHost(null));
      if (taskId) void window.modbit.browserSession(bsid, taskId).then(setCore).catch(() => {});
    };
    refresh();
    void window.modbit.probeBrowser(bsid).then((p) => setProbe(p ? { node_reachable: p.node_reachable, partition: p.partition, sandboxed: p.sandboxed, context_isolated: p.context_isolated } : null)).catch(() => {});
    const off = window.modbit.onBrowserState((raw) => {
      const s = raw as { browserSessionId: string; gone?: string };
      if (s.browserSessionId !== bsid) return;
      if (s.gone) setGone(s.gone);
      refresh();
    });
    return () => {
      off();
      ro.disconnect();
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
      void window.modbit.hideBrowser(bsid).catch(() => {});
    };
  }, [bsid, taskId]);
  const state = browserState({ opening: !bsid && !browsing.error, error: browsing.error, host, gone, controller: host?.controller ?? core?.controller ?? "AGENT" });
  return (
    <section className="review browser" data-testid="browser" aria-label="Browser" role="main" data-browser-session-id={bsid ?? ""}>
      <div className="review-head">
        <h2 style={{ margin: 0 }}>Browser</h2>
        <span className="meta" data-testid="browser-meta">
          {card ? `task ${card.taskId.slice(0, 8)} · ${card.goalText}` : ""}
          {core ? ` · ${core.controller.toLowerCase()} holds control (generation ${core.leaseGeneration})` : ""}
        </span>
        <button type="button" data-testid="browser-close" onClick={onClose} aria-keyshortcuts="Escape">
          Back to fleet
        </button>
      </div>
      <StateLine state={state} testid="browser-state" />
      <div className="actions" data-testid="browser-control" data-controller={host?.controller ?? "AGENT"} data-lease-generation={host?.leaseGeneration ?? 0}>
        {host?.controller === "USER" ? (
          <button type="button" data-testid="browser-return-control" onClick={() => void setControl("AGENT")} disabled={controlBusy || !bsid}>
            Return control to the agent
          </button>
        ) : (
          <button type="button" data-testid="browser-take-control" onClick={() => void setControl("USER")} disabled={controlBusy || !bsid}>
            Take control
          </button>
        )}
        <span className="meta">
          {host?.controller === "USER" ? "you hold control: the agent's input is blocked, it can still observe" : "the agent holds control: your typing into the page is yours to do, its actions are its own"} · lease generation {host?.leaseGeneration ?? 0}
        </span>
        <button type="button" data-testid="browser-emergency-stop" onClick={() => void emergencyStop()} disabled={controlBusy || !sessionId || !!(stopped ?? host?.stopped)}>
          Emergency stop
        </button>
        {(stopped ?? host?.stopped) && (
          <span className="meta" role="status" data-testid="browser-stopped">
            ⚠ emergency stop: {stopped ?? host?.stopped} — no agent input runs until a new session lease is taken
          </span>
        )}
      </div>
      {(gone || (host && !host.attached)) && (
        <div className="actions">
          <button type="button" data-testid="browser-reopen" onClick={onReopen}>
            Reopen session
          </button>
        </div>
      )}
      <div className="meta" data-testid="browser-page">
        {"url: "}
        <span data-testid="browser-url">{host?.url ?? ""}</span>
        {" · title: "}
        <span data-testid="browser-title">{host?.title ?? ""}</span>
        {" · state "}
        <span data-testid="browser-version">{host?.stateVersion ?? 0}</span>
        {core?.url ? ` · recorded on the Core: ${core.url} (state ${core.stateVersion}, ${core.fingerprint.slice(0, 12)})` : ""}
        {" · page content is untrusted and never enters this window"}
      </div>
      {probe && (
        <div className="meta" data-testid="browser-isolation" data-node-reachable={probe.node_reachable ? "true" : "false"}>
          isolation: partition {probe.partition} · sandbox {probe.sandboxed ? "on" : "off"} · context isolation {probe.context_isolated ? "on" : "off"} · Node reachable from the page: {probe.node_reachable ? "YES" : "no"}
        </div>
      )}
      <div ref={placeholder} className="browser-view" data-testid="browser-view" aria-label="the live page (rendered by the host's sandboxed view)" />
    </section>
  );
}

/** Review screen (docs/20 "Trusted Code Surface", REQ-EV-0036): every fact on it
 *  is a revision-bound payload from the Core; the renderer holds no buffers. */
function Review({ taskId, sessionId, card, onClose }: { taskId: string; sessionId: string; card: TaskCard | null; onClose: () => void }) {
  const [bundle, setBundle] = useState<ReviewBundleView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // PX-007: the pull request from this candidate. The ack's states are the
  // screen's: APPROVAL_PENDING (decide the intent shown), OPENED/UPDATED
  // (the URL), DENIED (the branch stays local; a receipt exists).
  const [pr, setPr] = useState<PullRequestAckView | null>(null);
  const [prBusy, setPrBusy] = useState(false);
  const [prError, setPrError] = useState<string | null>(null);
  // The revision the pull request names: the one this review showed, which
  // is the one an acceptance records as the candidate.
  const candidateRevision = useRef<string | null>(null);
  const openPullRequest = async (update: boolean) => {
    if (!bundle || prBusy) return;
    setPrBusy(true);
    setPrError(null);
    try {
      candidateRevision.current ??= bundle.workspaceRevision;
      setPr(await window.modbit.openPullRequest(sessionId, taskId, candidateRevision.current, update));
    } catch (e) {
      setPrError((e as Error).message);
    } finally {
      setPrBusy(false);
    }
  };
  const decidePullRequest = async (approve: boolean) => {
    if (!pr || pr.status !== "APPROVAL_PENDING" || prBusy) return;
    setPrBusy(true);
    setPrError(null);
    try {
      const r = await window.modbit.resolveApproval(sessionId, pr.approvalId, approve, approve ? "approved from the review" : "denied from the review", pr.intentHash);
      if (approve) {
        // Approved: the same call now pushes and opens, under that approval.
        setPr(await window.modbit.openPullRequest(sessionId, taskId, candidateRevision.current ?? bundle!.workspaceRevision, false));
      } else {
        // Denied: terminal for this attempt; the branch stays local. A later
        // "Try again" is a new attempt with a new approval.
        setPr({ ...pr, status: "DENIED", detail: `approval ${r.approvalId.slice(0, 8)} denied at offset ${r.offset}: the branch stays local, nothing reached the forge`, receipts: 0 });
      }
    } catch (e) {
      setPrError((e as Error).message);
    } finally {
      setPrBusy(false);
    }
  };
  const [rejected, setRejected] = useState<Set<string>>(new Set());
  // REQ-EV-0141: hunks the reviewer marks as context. This changes what
  // retrieval prefers and nothing else — it cannot edit or accept anything.
  const [context, setContext] = useState<Set<string>>(new Set());
  const [selectionError, setSelectionError] = useState<string | null>(null);
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const load = useCallback(async () => {
    setErr(null);
    try {
      setBundle(await window.modbit.reviewBundle(taskId));
    } catch (e) {
      setErr((e as Error).message);
    }
  }, [taskId]);
  useEffect(() => {
    void load();
  }, [load]);
  // PX-024: the Review takes the focus when it opens (the fleet is hidden
  // behind it); Escape or "Back to fleet" returns it to the card.
  const section = useRef<HTMLElement>(null);
  useEffect(() => {
    section.current?.focus();
  }, []);
  const toggle = (key: string) =>
    setRejected((r) => {
      const n = new Set(r);
      if (n.has(key)) n.delete(key);
      else n.add(key);
      return n;
    });
  const toggleContext = async (key: string) => {
    const next = new Set(context);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    setContext(next);
    const hunks = Array.from(next);
    try {
      setSelectionError(null);
      await window.modbit.setTaskSelection(sessionId, taskId, {
        paths: Array.from(new Set(hunks.map((k) => k.split("#")[0] ?? k))),
        reviewHunks: hunks,
        source: "review",
      });
    } catch (e) {
      setSelectionError((e as Error).message);
    }
  };
  const decide = async (decision: "ACCEPT" | "RETURN") => {
    if (!bundle || busy) return;
    setBusy(true);
    setErr(null);
    try {
      const rej = Array.from(rejected).map((k) => {
        const i = k.lastIndexOf("#");
        return { path: k.slice(0, i), index: Number(k.slice(i + 1)) };
      });
      const d = await window.modbit.decideReview(sessionId, taskId, decision, rej, note, bundle.workspaceRevision);
      setResult(decision === "ACCEPT" ? `Accepted: commit ${d.commit.slice(0, 12)}, ${d.reverted.length} hunk(s) reverted` : "Returned to work");
    } catch (e) {
      setErr((e as Error).message);
    } finally {
      setBusy(false);
    }
  };
  // PX-005 (docs/20, docs/29): a constrained inline patch. The two fields
  // are the command's arguments and nothing more — no buffer of the file
  // lives here; the Core applies the edit at the revisions this review
  // showed, or refuses it, and the review is re-read from the Core.
  const [patching, setPatching] = useState<string | null>(null);
  // PX-024 review navigation: next/previous change (focus moves between
  // hunks), expand or collapse the focused hunk, unified or split, jump to
  // the failing check. Every hunk and check is focusable; nothing is
  // reachable by mouse only.
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [split, setSplit] = useState(false);
  const hunkElements = () => [...document.querySelectorAll<HTMLElement>('[data-testid="review-hunk"]')];
  const focusedHunk = () => document.activeElement?.closest<HTMLElement>('[data-testid="review-hunk"]') ?? null;
  const toggleCollapsed = (key: string) =>
    setCollapsed((c) => {
      const next = new Set(c);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const cmd = commandFor(e, { editable: isEditable(document.activeElement), screen: "review", confirming: false, isMac: navigator.platform.toUpperCase().includes("MAC") });
      if (!cmd) return;
      switch (cmd) {
        case "hunkNext":
        case "hunkPrevious": {
          e.preventDefault();
          const hunks = hunkElements();
          const i = hunks.indexOf(focusedHunk()!);
          hunks[i < 0 ? 0 : Math.min(hunks.length - 1, Math.max(0, i + (cmd === "hunkNext" ? 1 : -1)))]?.focus();
          return;
        }
        case "hunkToggle": {
          const h = focusedHunk();
          // Enter on a control inside the hunk (a checkbox, a button) is that control's.
          if (!h || (e.key === "Enter" && document.activeElement !== h)) return;
          e.preventDefault();
          toggleCollapsed(h.getAttribute("data-hunk") ?? "");
          return;
        }
        case "toggleSplit":
          e.preventDefault();
          setSplit((v) => !v);
          return;
        case "jumpFailing": {
          e.preventDefault();
          const failing = document.querySelector<HTMLElement>('[data-testid="review-check"][data-status="FAIL"]');
          (failing ?? document.querySelector<HTMLElement>('[data-testid="review-verification"]'))?.focus();
          return;
        }
        default:
          return;
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);
  const [patchOld, setPatchOld] = useState("");
  const [patchNew, setPatchNew] = useState("");
  const [patchNote, setPatchNote] = useState<string | null>(null);
  const openPatch = (path: string) => {
    setPatching(patching === path ? null : path);
    setPatchOld("");
    setPatchNew("");
    setPatchNote(null);
  };
  const applyPatch = async (path: string, fileRevision: string) => {
    if (!bundle || busy) return;
    setBusy(true);
    setPatchNote(null);
    try {
      const r = await window.modbit.applyUserPatch(sessionId, taskId, { path, old: patchOld, new: patchNew, expectedWorkspaceRevision: bundle.workspaceRevision, expectedFileRevision: fileRevision });
      setPatchNote(`applied at workspace revision ${r.workspaceRevision} (was ${r.previousRevision}) · file revision ${r.fileRevision.slice(0, 12)}`);
      setPatching(null);
      setPatchOld("");
      setPatchNew("");
      await load();
    } catch (e) {
      setPatchNote(`refused: ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  };
  const hunkCount = bundle ? bundle.files.reduce((n, f) => n + f.hunks.length, 0) : 0;
  const state = reviewState({
    loading: bundle === null && err === null,
    // A refused patch (STALE_REVISION: the review shown is behind the
    // workspace) is this screen's error until the review is reloaded.
    error: err ?? prError ?? (patchNote?.startsWith("refused") ? patchNote : null),
    bundle,
    gate: card?.gate ?? null,
    pullRequest: pr ? { status: pr.status, detail: pr.detail, evidence: pr.status === "DENIED" && pr.receipts === 0 ? `approval ${pr.approvalId.slice(0, 8)}` : `${pr.receipts} receipt(s)${pr.url ? `; ${pr.url}` : ""}` } : null,
  });
  const selectionNote = selectionError
    ? `selection not recorded: ${selectionError}`
    : context.size > 0
      ? `${context.size} hunk(s) given to retrieval as context`
      : null;
  return (
    <section className="review" data-testid="review" aria-label="Review" role="main" tabIndex={-1} ref={section}>
      <div className="review-head">
        <h2 style={{ margin: 0 }}>Review</h2>
        <button type="button" className="small" data-testid="review-split" aria-pressed={split} onClick={() => setSplit((v) => !v)}>
          {split ? "Unified (u)" : "Split (u)"}
        </button>
        <span className="meta" data-testid="review-meta">
          {selectionNote && (
            <span className="meta" data-testid="review-selection">
              {selectionNote}
            </span>
          )}
          {bundle ? `task ${taskId.slice(0, 8)} · ${bundle.taskState} · workspace revision ${bundle.workspaceRevision} · base ${bundle.baseCommit.slice(0, 8)} · ${bundle.files.length} file(s), ${hunkCount} hunk(s), ${bundle.receipts} receipt(s)` : "loading from the Core…"}
        </span>
        <button type="button" data-testid="review-close" onClick={onClose} aria-keyshortcuts="Escape">
          Back to fleet
        </button>
      </div>
      <StateLine state={state} testid="review-state" />
      {err && (
        <div className="banner" data-kind="error" role="alert" data-testid="review-error">
          <strong>Error</strong> — {err} <button type="button" onClick={() => void load()}>Reload</button>
        </div>
      )}
      {result && (
        <div className="banner" data-kind="recovered" role="status" data-testid="review-result">
          {result}
        </div>
      )}
      {patchNote && (
        <div className="banner" data-kind={patchNote.startsWith("refused") ? "error" : "recovered"} role="status" data-testid="patch-result">
          {patchNote}
        </div>
      )}
      {bundle && (
        <div className="review-body">
          <div>
            {bundle.files.length === 0 && <p className="empty" data-testid="review-empty">The candidate has no changes.</p>}
            {bundle.files.map((f) => (
              <article className="file" key={f.path} data-testid="review-file" data-path={f.path}>
                <h3>
                  {f.path} <span className="meta">{f.status === "A" ? "added" : f.status === "D" ? "deleted" : f.status === "R" ? "renamed" : "modified"} · file revision {f.fileRevision.slice(0, 12)}</span>
                  {!f.binary && f.status !== "D" && (
                    <button type="button" className="small" data-testid="file-patch" onClick={() => openPatch(f.path)} disabled={busy || result !== null || bundle.taskState !== "ReadyForReview"}>
                      {patching === f.path ? "Cancel edit" : "Edit"}
                    </button>
                  )}
                </h3>
                {patching === f.path && (
                  <div className="patch" data-testid="patch-panel" data-path={f.path}>
                    <label>
                      Replace
                      <textarea data-testid="patch-old" value={patchOld} onChange={(e) => setPatchOld(e.target.value)} placeholder="the exact text to replace (one place)" />
                    </label>
                    <label>
                      With
                      <textarea data-testid="patch-new" value={patchNew} onChange={(e) => setPatchNew(e.target.value)} placeholder="its replacement" />
                    </label>
                    <div className="actions">
                      <button type="button" data-testid="patch-apply" onClick={() => void applyPatch(f.path, f.fileRevision)} disabled={busy || patchOld.length === 0}>
                        Apply at revision {bundle.workspaceRevision}
                      </button>
                      <span className="meta">through the Core's change transaction; nothing is kept here</span>
                    </div>
                  </div>
                )}
                {f.binary && <p className="meta">binary file</p>}
                {f.hunks.map((h) => {
                  const key = `${f.path}#${h.index}`;
                  const isRejected = rejected.has(key);
                  return (
                    <div className="hunk" key={key} data-testid="review-hunk" data-hunk={key} data-rejected={isRejected} data-collapsed={collapsed.has(key)} tabIndex={0} role="group" aria-label={`${f.path} ${h.header}${isRejected ? ", rejected" : ""}${collapsed.has(key) ? ", collapsed" : ""}`}>
                      <div className="hunk-head">
                        <code>{h.header}</code>
                        {isRejected && <span className="meta">rejected</span>}
                        <button type="button" className="small" data-testid="hunk-toggle" aria-expanded={!collapsed.has(key)} onClick={() => toggleCollapsed(key)}>
                          {collapsed.has(key) ? "Expand" : "Collapse"}
                        </button>
                        <label>
                          <input type="checkbox" data-testid="hunk-reject" checked={isRejected} onChange={() => toggle(key)} disabled={result !== null} /> reject
                        </label>
                        <label title="Give this hunk to retrieval as context. It cannot change any file.">
                          <input type="checkbox" data-testid="hunk-context" checked={context.has(key)} onChange={() => void toggleContext(key)} disabled={result !== null} /> context
                        </label>
                      </div>
                      {collapsed.has(key) ? (
                        <p className="meta" data-testid="hunk-collapsed">
                          {h.lines.filter((l) => l.startsWith("+")).length} added, {h.lines.filter((l) => l.startsWith("-")).length} removed line(s) — collapsed
                        </p>
                      ) : split ? (
                        <table className="split" data-testid="hunk-split" aria-label="split view">
                          <tbody>
                            {splitRows(h.lines).map((r, i) => (
                              <tr key={i}>
                                <td className={r.left === null ? "gap" : r.left.startsWith("-") ? "del" : "ctx"}>{r.left ?? ""}</td>
                                <td className={r.right === null ? "gap" : r.right.startsWith("+") ? "add" : "ctx"}>{r.right ?? ""}</td>
                              </tr>
                            ))}
                          </tbody>
                        </table>
                      ) : (
                        <pre data-testid="hunk-unified">
                          {h.lines.map((l, i) => (
                            <span key={i} className={l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : "ctx"}>
                              {l}
                              {"\n"}
                            </span>
                          ))}
                        </pre>
                      )}
                    </div>
                  );
                })}
              </article>
            ))}
          </div>
          <aside role="region" aria-label="Evidence and decision">
            <h3>Verification</h3>
            {bundle.verificationRuns.length === 0 && <p className="empty">No verification runs recorded.</p>}
            <ul data-testid="review-verification" tabIndex={-1} aria-label="verification runs">
              {bundle.verificationRuns.map((v) => (
                <li key={v.id}>
                  <strong>{v.stage}</strong> {v.status} · {v.checks.filter((c) => c.status === "PASS").length}/{v.checks.length} pass · {v.candidateRevision}
                  {v.checks.some((c) => c.status !== "PASS") && (
                    <ul>
                      {v.checks.filter((c) => c.status !== "PASS").map((c) => (
                        <li key={c.id} data-testid="review-check" data-status={c.status} tabIndex={-1}>
                          {c.status === "FAIL" ? "✖ failed: " : `${c.status.toLowerCase()}: `}
                          {c.id}
                        </li>
                      ))}
                    </ul>
                  )}
                </li>
              ))}
            </ul>
            {bundle.attributions.length > 0 && (
              <>
                <h3>Attribution</h3>
                <ul data-testid="review-attribution">
                  {bundle.attributions.map((a) => (
                    <li key={a}>{a}</li>
                  ))}
                </ul>
              </>
            )}
            {bundle.quarantined.length > 0 && (
              <>
                <h3>Quarantined (flaky)</h3>
                <ul>
                  {bundle.quarantined.map((q) => (
                    <li key={q}>{q}</li>
                  ))}
                </ul>
              </>
            )}
            {bundle.invariantFindings.length > 0 && (
              <>
                <h3>Diff invariants</h3>
                <ul>
                  {bundle.invariantFindings.map((q) => (
                    <li key={q}>{q}</li>
                  ))}
                </ul>
              </>
            )}
            <h3>Plan</h3>
            <pre className="small" data-testid="review-plan" tabIndex={0} aria-label="plan">{bundle.planJson || "(no plan recorded)"}</pre>
            <h3>Self-review</h3>
            <pre className="small" tabIndex={0} aria-label="self-review">{bundle.selfReviewJson || "(none)"}</pre>
            <h3>Evidence</h3>
            <ul className="small" data-testid="review-evidence" tabIndex={0} aria-label="evidence links">
              {bundle.evidenceLinks.map((e) => (
                <li key={e}>{e}</li>
              ))}
            </ul>
            <h3>Decision</h3>
            <textarea data-testid="review-note" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Commit message / feedback" disabled={result !== null} />
            <div className="actions">
              <button type="button" data-testid="review-accept" onClick={() => void decide("ACCEPT")} disabled={busy || result !== null || bundle.taskState !== "ReadyForReview"}>
                Accept{rejected.size ? ` (${rejected.size} rejected)` : ""}
              </button>
              <button type="button" data-testid="review-return" onClick={() => void decide("RETURN")} disabled={busy || result !== null || bundle.taskState !== "ReadyForReview"}>
                Return to work
              </button>
            </div>
            <h3>Pull request</h3>
            <div className="meta">The push and the forge write are one protected effect: it runs only after you approve the exact intent shown, with the token the Core holds.</div>
            <div className="actions" data-testid="pull-request" data-status={pr?.status ?? "NONE"}>
              {(!pr || pr.status === "DENIED" || pr.status === "FAILED") && (
                <button type="button" data-testid="pr-open" onClick={() => void openPullRequest(false)} disabled={prBusy || (bundle.taskState !== "ReadyForReview" && !result?.startsWith("Accepted") && bundle.taskState !== "Completed")}>
                  {prBusy ? "Asking the Core…" : pr ? "Try again" : "Open pull request"}
                </button>
              )}
              {pr?.status === "APPROVAL_PENDING" && (
                <span data-testid="pr-approval">
                  <span className="meta">
                    approve intent <code data-testid="pr-intent">{pr.intentHash.slice(0, 16)}</code> (branch {pr.branch}, revision {pr.candidateRevision})?
                  </span>{" "}
                  <button type="button" data-testid="pr-approve" onClick={() => void decidePullRequest(true)} disabled={prBusy}>
                    Approve and push
                  </button>{" "}
                  <button type="button" data-testid="pr-deny" onClick={() => void decidePullRequest(false)} disabled={prBusy}>
                    Deny
                  </button>
                </span>
              )}
              {(pr?.status === "OPENED" || pr?.status === "UPDATED") && (
                <span className="meta" data-testid="pr-result">
                  {pr.status === "OPENED" ? "opened" : "updated"} #{pr.number} at {pr.url} · head {pr.headSha.slice(0, 12)} · {pr.receipts} receipt(s){pr.replayed ? " · replayed" : ""}{" "}
                  <button type="button" className="small" data-testid="pr-update" onClick={() => void openPullRequest(true)} disabled={prBusy}>
                    Update
                  </button>
                </span>
              )}
              {pr?.status === "DENIED" && (
                <span className="meta" role="status" data-testid="pr-denied">
                  denied: {pr.detail}
                </span>
              )}
              {prError && (
                <span className="meta" role="alert" data-testid="pr-error">
                  {prError}
                </span>
              )}
            </div>
          </aside>
        </div>
      )}
    </section>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
