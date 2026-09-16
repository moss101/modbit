/**
 * Renderer entry (docs/32): app shell with the Home / Fleet screen and the New
 * Task composer (docs/10 screens 1 and 2). Runs sandboxed; all Core access goes
 * through `window.modbit` (preload bridge). Screen states per docs/39 PX-023:
 * empty, loading, populated, error, degraded (Core restarting) and recovery.
 */
import { StrictMode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { applyEvent, childrenOf, columns, emptyModel, fromSnapshot, type Event, type FleetColumn, type Model, type Snapshot, type TaskCard } from "./model.ts";
import { fleetState, newTaskState, reviewState, settingsState, taskState, type ScreenState as DerivedScreenState } from "./screens.ts";
import { DEFAULT_PREFERENCES, deriveNotifications, newSince, osDeliveryDue, type Notification as AppNotification, type NotificationKind, type NotificationPreferences } from "./notifications.ts";
import type { AttentionItem, ContextInspectorSummary, ModbitBridge, PullRequestAckView, ReviewBundleView, TaskEconomicsSummary } from "../preload/preload.ts";

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
function StateLine({ state, testid }: { state: DerivedScreenState; testid: string }) {
  return (
    <div className="meta state" data-testid={testid} data-screen={state.screen} data-kind={state.kind} role={state.kind === "error" ? "alert" : "status"}>
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
  // Onboarding (REQ-PX-022, docs/39): three steps, each a working control.
  const [provider, setProvider] = useState<{ configured: boolean; endpoints: string[]; stored: boolean; provider: string; keychainAvailable: boolean } | null>(null);
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
      <StateLine state={fleet} testid="fleet-state" />
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
      {reviewing && model.sessionId && <Review taskId={reviewing} sessionId={model.sessionId} card={model.tasks.get(reviewing) ?? null} onClose={() => setReviewing(null)} />}
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
      <main hidden={reviewing !== null}>
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
          <p className="sr-only" aria-live="polite">{attention} tasks need attention</p>
          {attentionItems.length > 0 && (
            <section className="attention" data-testid="attention" aria-label="attention items">
              <h2>Attention ({attentionItems.length})</h2>
              <ul>
                {attentionItems.map((i) => (
                  <li key={`${i.kind}:${i.taskId}:${i.reference}`} data-testid="attention-item" data-kind={i.kind} data-task-id={i.taskId}>
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
              <section className="column" key={c.key} aria-label={c.title} data-testid={`column-${c.key}`}>
                <h2>
                  {c.title} <span aria-hidden="true">({cols[c.key].length})</span>
                </h2>
                {cols[c.key].length === 0 ? <p className="empty">None</p> : cols[c.key].map((t) => <Card key={t.taskId} card={t} children={childrenOf(model, t.taskId)} state={taskState({ card: t, core, attention: attentionItems })} sessionId={model.sessionId} onStart={startTask} onReview={(id) => setReviewing(id)} />)}
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
function Card({ card, children, state, sessionId, onStart, onReview }: { card: TaskCard; children?: TaskCard[]; state: DerivedScreenState; sessionId: string | null; onStart: (id: string) => void; onReview: (id: string) => void }) {
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
    <article className="card" tabIndex={0} data-testid="task-card" data-task-id={card.taskId} data-state={card.state} data-phase={card.phase}>
      <div>{card.goalText}</div>
      <div className="meta">
        state: <span data-testid="task-state">{card.state}</span>
        {card.waitReason ? ` · waiting on ${card.waitReason}` : ""} · gen {card.generation} · local_trusted
        {" · attachments "}
        <span data-testid="task-attachments">{card.attachments ?? 0}</span>
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
              <input data-testid="task-answer-text" value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="your answer" disabled={deciding} />
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
      </div>
    </article>
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
    <section className="review" data-testid="review" aria-label="Review">
      <div className="review-head">
        <h2 style={{ margin: 0 }}>Review</h2>
        <span className="meta" data-testid="review-meta">
          {selectionNote && (
            <span className="meta" data-testid="review-selection">
              {selectionNote}
            </span>
          )}
          {bundle ? `task ${taskId.slice(0, 8)} · ${bundle.taskState} · workspace revision ${bundle.workspaceRevision} · base ${bundle.baseCommit.slice(0, 8)} · ${bundle.files.length} file(s), ${hunkCount} hunk(s), ${bundle.receipts} receipt(s)` : "loading from the Core…"}
        </span>
        <button type="button" data-testid="review-close" onClick={onClose}>
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
                    <div className="hunk" key={key} data-testid="review-hunk" data-hunk={key} data-rejected={isRejected}>
                      <div className="hunk-head">
                        <code>{h.header}</code>
                        <label>
                          <input type="checkbox" data-testid="hunk-reject" checked={isRejected} onChange={() => toggle(key)} disabled={result !== null} /> reject
                        </label>
                        <label title="Give this hunk to retrieval as context. It cannot change any file.">
                          <input type="checkbox" data-testid="hunk-context" checked={context.has(key)} onChange={() => void toggleContext(key)} disabled={result !== null} /> context
                        </label>
                      </div>
                      <pre>
                        {h.lines.map((l, i) => (
                          <span key={i} className={l.startsWith("+") ? "add" : l.startsWith("-") ? "del" : "ctx"}>
                            {l}
                            {"\n"}
                          </span>
                        ))}
                      </pre>
                    </div>
                  );
                })}
              </article>
            ))}
          </div>
          <aside>
            <h3>Verification</h3>
            {bundle.verificationRuns.length === 0 && <p className="empty">No verification runs recorded.</p>}
            <ul data-testid="review-verification">
              {bundle.verificationRuns.map((v) => (
                <li key={v.id}>
                  <strong>{v.stage}</strong> {v.status} · {v.checks.filter((c) => c.status === "PASS").length}/{v.checks.length} pass · {v.candidateRevision}
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
            <pre className="small" data-testid="review-plan">{bundle.planJson || "(no plan recorded)"}</pre>
            <h3>Self-review</h3>
            <pre className="small">{bundle.selfReviewJson || "(none)"}</pre>
            <h3>Evidence</h3>
            <ul className="small" data-testid="review-evidence">
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
