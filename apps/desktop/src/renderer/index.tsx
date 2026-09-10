/**
 * Renderer entry (docs/32): app shell with the Home / Fleet screen and the New
 * Task composer (docs/10 screens 1 and 2). Runs sandboxed; all Core access goes
 * through `window.modbit` (preload bridge). Screen states per docs/39 PX-023:
 * empty, loading, populated, error, degraded (Core restarting) and recovery.
 */
import { StrictMode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { applyEvent, columns, emptyModel, fromSnapshot, type Event, type FleetColumn, type Model, type Snapshot, type TaskCard } from "./model.ts";
import type { ContextInspectorSummary, ModbitBridge, ReviewBundleView, TaskEconomicsSummary } from "../preload/preload.ts";

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

function App() {
  const [core, setCore] = useState<CoreStatus>({ state: "starting", restarts: 0 });
  const [model, setModel] = useState<Model>(emptyModel);
  const [screen, setScreen] = useState<ScreenState>("loading");
  const [error, setError] = useState<string | null>(null);
  const [recovered, setRecovered] = useState<string | null>(null);
  const [recovery, setRecovery] = useState<RecoveryInfo | null>(null);
  const [goal, setGoal] = useState("");
  const [languages, setLanguages] = useState<{ language: string; tier: string; label: string }[]>([]);
  const [inspector, setInspector] = useState<ContextInspectorSummary | null>(null);
  const [economics, setEconomics] = useState<TaskEconomicsSummary | null>(null);
  const [workspaceRoot, setWorkspaceRoot] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [reviewing, setReviewing] = useState<string | null>(null);
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
      const m = fromSnapshot(snap);
      setModel(m);
      setScreen(m.tasks.size === 0 ? "empty" : "populated");
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
    } catch (e) {
      setError((e as Error).message);
      setScreen("error");
    }
  }, []);

  useEffect(() => {
    const offEvent = window.modbit.onEvent((raw) => {
      const e = raw as Event & { taskId: string | null; aggregateType: string };
      if (e.aggregateType !== "task" || !e.taskId) return;
      setModel((m) => applyEvent(m, e, e.taskId!));
      setScreen("populated");
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
      if (!text || submitting) return;
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
        const r = await window.modbit.createTask(sessionId, text, commandId, workspaceRoot.trim());
        // Render from the durable id the Core returned; the events will follow.
        setModel((m) => {
          if (m.tasks.has(r.taskId)) return m;
          const tasks = new Map(m.tasks);
          tasks.set(r.taskId, { taskId: r.taskId, goalText: text, state: "Queued", waitReason: "Capacity", generation: 2, createdAtMs: Date.now(), nextAction: null } as TaskCard);
          return { ...m, tasks };
        });
        setScreen("populated");
        setGoal("");
      } catch (e) {
        setError((e as Error).message);
      } finally {
        setSubmitting(false);
      }
    },
    [goal, workspaceRoot, submitting],
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

  const cols = useMemo(() => columns(model), [model]);
  const attention = cols.needsAttention.length;

  return (
    <>
      <header>
        <h1>Modbit</h1>
        <span className="meta">Fleet</span>
        <span className="status" data-testid="core-status" aria-live="polite">
          {core.state === "connected" ? `Core connected (pid ${core.pid})` : core.state === "restarting" ? `Core restarting…` : core.state === "failed" ? "Core failed" : "Core starting…"}
        </span>
      </header>
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
      {reviewing && model.sessionId && <Review taskId={reviewing} sessionId={model.sessionId} onClose={() => setReviewing(null)} />}
      <main hidden={reviewing !== null}>
        <form className="composer" onSubmit={submit} aria-label="New Task">
          <h2 style={{ margin: 0, fontSize: 14 }}>New Task</h2>
          <label htmlFor="goal" className="meta">Goal</label>
          <textarea id="goal" data-testid="goal" value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="What should the agent achieve?" disabled={core.state !== "connected"} />
          <label htmlFor="workspace" className="meta">Workspace root (a local Git checkout; empty for a Work space)</label>
          <input id="workspace" data-testid="workspace" value={workspaceRoot} onChange={(e) => setWorkspaceRoot(e.target.value)} placeholder="/path/to/repo" disabled={core.state !== "connected"} />
          <div className="meta">Execution: local_trusted · Origin: desktop</div>
          <button type="submit" data-testid="run" disabled={core.state !== "connected" || submitting || !goal.trim()}>
            {submitting ? "Creating…" : "Run"}
          </button>
          {core.state !== "connected" && <div className="meta">Task creation is disabled until the Core is connected.</div>}
        </form>
        <div>
          <p className="sr-only" aria-live="polite">{attention} tasks need attention</p>
          {screen === "loading" && <p className="meta" data-testid="fleet-loading">Loading fleet from the Core…</p>}
          {screen === "empty" && <p className="empty" data-testid="fleet-empty">No tasks yet. Create one on the left.</p>}
          <div className="fleet" data-testid="fleet" data-screen={screen}>
            {COLUMNS.map((c) => (
              <section className="column" key={c.key} aria-label={c.title} data-testid={`column-${c.key}`}>
                <h2>
                  {c.title} <span aria-hidden="true">({cols[c.key].length})</span>
                </h2>
                {cols[c.key].length === 0 ? <p className="empty">None</p> : cols[c.key].map((t) => <Card key={t.taskId} card={t} onStart={startTask} onReview={(id) => setReviewing(id)} />)}
              </section>
            ))}
          </div>
        </div>
      </main>
    </>
  );
}

function Card({ card, onStart, onReview }: { card: TaskCard; onStart: (id: string) => void; onReview: (id: string) => void }) {
  const startable = card.state === "Queued" || (card.state === "Waiting" && card.waitReason === "UserInput");
  return (
    <article className="card" tabIndex={0} data-testid="task-card" data-task-id={card.taskId} data-state={card.state}>
      <div>{card.goalText}</div>
      <div className="meta">
        state: <span data-testid="task-state">{card.state}</span>
        {card.waitReason ? ` · waiting on ${card.waitReason}` : ""} · gen {card.generation} · local_trusted
        {" · attachments "}
        <span data-testid="task-attachments">{card.attachments ?? 0}</span>
      </div>
      {card.nextAction && (
        <div className="meta">
          next: <strong>{card.nextAction}</strong>
        </div>
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
function Review({ taskId, sessionId, onClose }: { taskId: string; sessionId: string; onClose: () => void }) {
  const [bundle, setBundle] = useState<ReviewBundleView | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [rejected, setRejected] = useState<Set<string>>(new Set());
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
  const hunkCount = bundle ? bundle.files.reduce((n, f) => n + f.hunks.length, 0) : 0;
  return (
    <section className="review" data-testid="review" aria-label="Review">
      <div className="review-head">
        <h2 style={{ margin: 0 }}>Review</h2>
        <span className="meta" data-testid="review-meta">
          {bundle ? `task ${taskId.slice(0, 8)} · ${bundle.taskState} · workspace revision ${bundle.workspaceRevision} · base ${bundle.baseCommit.slice(0, 8)} · ${bundle.files.length} file(s), ${hunkCount} hunk(s), ${bundle.receipts} receipt(s)` : "loading from the Core…"}
        </span>
        <button type="button" data-testid="review-close" onClick={onClose}>
          Back to fleet
        </button>
      </div>
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
      {bundle && (
        <div className="review-body">
          <div>
            {bundle.files.length === 0 && <p className="empty" data-testid="review-empty">The candidate has no changes.</p>}
            {bundle.files.map((f) => (
              <article className="file" key={f.path} data-testid="review-file" data-path={f.path}>
                <h3>
                  {f.path} <span className="meta">{f.status === "A" ? "added" : f.status === "D" ? "deleted" : f.status === "R" ? "renamed" : "modified"} · file revision {f.fileRevision.slice(0, 12)}</span>
                </h3>
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
