/**
 * Renderer entry (docs/32): app shell with the Home / Fleet screen and the New
 * Task composer (docs/10 screens 1 and 2). Runs sandboxed; all Core access goes
 * through `window.modbit` (preload bridge). Screen states per docs/39 PX-023:
 * empty, loading, populated, error, degraded (Core restarting) and recovery.
 */
import { StrictMode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { createRoot } from "react-dom/client";
import { applyEvent, columns, emptyModel, fromSnapshot, type Event, type FleetColumn, type Model, type Snapshot, type TaskCard } from "./model.ts";
import type { ModbitBridge } from "../preload/preload.ts";

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
  const [submitting, setSubmitting] = useState(false);
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
      if ((s as CoreStatus).state === "connected") void load();
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
        const r = await window.modbit.createTask(sessionId, text, commandId);
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
    [goal, submitting],
  );

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
      <main>
        <form className="composer" onSubmit={submit} aria-label="New Task">
          <h2 style={{ margin: 0, fontSize: 14 }}>New Task</h2>
          <label htmlFor="goal" className="meta">Goal</label>
          <textarea id="goal" data-testid="goal" value={goal} onChange={(e) => setGoal(e.target.value)} placeholder="What should the agent achieve?" disabled={core.state !== "connected"} />
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
                {cols[c.key].length === 0 ? <p className="empty">None</p> : cols[c.key].map((t) => <Card key={t.taskId} card={t} />)}
              </section>
            ))}
          </div>
        </div>
      </main>
    </>
  );
}

function Card({ card }: { card: TaskCard }) {
  return (
    <article className="card" tabIndex={0} data-testid="task-card" data-task-id={card.taskId} data-state={card.state}>
      <div>{card.goalText}</div>
      <div className="meta">
        state: <span data-testid="task-state">{card.state}</span>
        {card.waitReason ? ` · waiting on ${card.waitReason}` : ""} · gen {card.generation} · local_trusted
      </div>
      {card.nextAction && (
        <div className="meta">
          next: <strong>{card.nextAction}</strong>
        </div>
      )}
    </article>
  );
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
