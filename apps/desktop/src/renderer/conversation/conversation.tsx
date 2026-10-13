/**
 * The conversation surface (REQ-PX-047), the shell's centre region for one
 * task: the Core's transcript projection with the live text of open streams,
 * a tail-status line with the stall ladder, pinned-to-bottom scrolling with an
 * "N new messages" pill, three Core-projected densities, folded work groups
 * that never hide something the person must act on, and the failure the Core
 * typed. The composer below it is PX-054..056's (composer/composer.tsx); the
 * inline approval and question cards live here.
 * Nothing in this file holds authority: every effect is a typed preload call.
 */
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Button, StatusDot, TrayHost } from "@modbit/ui";
import { DENSITIES, type Density, type TranscriptRowView } from "../../shared/conversation-types.ts";
import type { TaskCard } from "../model.ts";
import { INITIAL_FOLLOW, identityOf, messageIds, onDismiss, onJumpToBottom, onMessages, onScroll, pillText, stallLevel, withDivider, withLiveOnly, type Follow } from "./model.ts";
import { RowView, type RowContext } from "./rows.tsx";
import { loadConversationPrefs, saveConversationPrefs, type ConversationPrefs } from "./prefs.ts";
import { useConversation } from "./use-conversation.ts";
import { Composer } from "../composer/composer.tsx";
import { ApprovalDock } from "../approvals/approval-dock.tsx";
import { ModeProposalDock } from "../approvals/mode-proposal-dock.tsx";
import { RunModeControl } from "../approvals/run-mode.tsx";
import { useCheckpointSurface } from "./use-checkpoint-surface.tsx";

export interface ConversationProps {
  taskId: string;
  sessionId: string | null;
  connected: boolean;
  card: TaskCard | undefined;
  title: string;
  /** A transcript row to scroll to once it is loaded (a search hit). */
  focusRowId: string | null;
  onResume: (taskId: string) => void;
  onNewTask: () => void;
  /** Opens one of the task's terminals in the apps panel (the background-terminals tray). */
  onOpenTerminal?: ((taskId: string, terminalId: string) => void) | undefined;
  /** Open another task's conversation (a fork opens its child). */
  onOpenTask?: ((taskId: string) => void) | undefined;
  /** A task's name for the approvals of other agents shown above this composer. */
  titleOf?: ((taskId: string) => string) | undefined;
}

const DENSITY_LABEL: Record<Density, string> = { COMPACT: "Compact", BALANCED: "Balanced", DETAILED: "Detailed" };

const findRow = (rows: readonly TranscriptRowView[], id: string, path: string[] = []): string[] | null => {
  for (const r of rows) {
    if (r.rowId === id) return path;
    const inner = findRow(r.children, id, [...path, r.rowId]);
    if (inner) return inner;
  }
  return null;
};

function collectTurnText(rows: readonly TranscriptRowView[], turnId: string, out: string[]): void {
  for (const r of rows) {
    if (r.kind === "ASSISTANT_MESSAGE" && r.turnId === turnId && r.facts.type === "stream" && r.facts.phase === "COMPLETED") out.push(r.text);
    collectTurnText(r.children, turnId, out);
  }
}

export function Conversation(props: ConversationProps) {
  const { taskId, sessionId, connected, card, title, focusRowId, onResume, onNewTask, onOpenTerminal, onOpenTask, titleOf } = props;
  const [prefs, setPrefs] = useState<ConversationPrefs>(loadConversationPrefs);
  useEffect(() => saveConversationPrefs(prefs), [prefs]);
  const conv = useConversation(taskId, sessionId, prefs.density, connected);
  const [openGroups, setOpenGroups] = useState<ReadonlySet<string>>(new Set());
  const [follow, setFollow] = useState<Follow>(INITIAL_FOLLOW);
  const [highlight, setHighlight] = useState<string | null>(null);
  const [copied, setCopied] = useState("");
  const scroller = useRef<HTMLDivElement>(null);
  const seen = useRef<Set<string> | null>(null);
  // REQ-PX-057 / 062: the run-mode dialog (also opened from an approval card) and the checkpoint surface.
  const [modeOpen, setModeOpen] = useState(false);
  const ckpt = useCheckpointSurface({ taskId, sessionId, connected, card, onResume, onOpenTask });

  const body = useMemo(() => withDivider(withLiveOnly(conv.rows, conv.live), conv.dividerOffset, (n) => `${n} new`), [conv.rows, conv.live, conv.dividerOffset]);
  const latestUser = useMemo(() => [...body].reverse().find((r) => r.kind === "USER_MESSAGE")?.rowId ?? null, [body]);

  // Follow the tail while pinned; scrolling up freezes it.
  // The view follows the tail when the content grows (a delta, a row, a row measured at its real height) and when the person returns to the tail;
  // a re-render that adds nothing never undoes a scroll the person just made.
  const pinnedRef = useRef(true);
  pinnedRef.current = follow.pinned;
  const inner = useRef<HTMLDivElement>(null);
  const wasPinned = useRef(true);
  useLayoutEffect(() => {
    const el = scroller.current;
    if (el && follow.pinned && !wasPinned.current) el.scrollTop = el.scrollHeight;
    wasPinned.current = follow.pinned;
  });
  useEffect(() => {
    const content = inner.current;
    const el = scroller.current;
    if (!content || !el || typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => {
      if (pinnedRef.current) el.scrollTop = el.scrollHeight;
    });
    ro.observe(content);
    return () => ro.disconnect();
  }, [conv.loaded]);
  const onScrollEvent = () => {
    const el = scroller.current;
    if (!el) return;
    setFollow((f) => onScroll(f, el.scrollHeight - el.scrollTop - el.clientHeight));
  };

  // New messages that land while scrolled up are counted, once each (a stream is one message from its first piece).
  useEffect(() => {
    if (!conv.loaded) return;
    const ids = messageIds(body);
    if (seen.current === null) {
      seen.current = new Set(ids);
      return;
    }
    const fresh = ids.filter((id) => !seen.current!.has(id));
    if (fresh.length === 0) return;
    for (const id of fresh) seen.current.add(id);
    setFollow((f) => onMessages(f, fresh.length));
  }, [body, conv.loaded]);

  // The person has seen the conversation up to the tip while the view follows it (the Core records it; the unread dot clears).
  useEffect(() => {
    if (!conv.loaded || !sessionId || !follow.pinned) return;
    if (BigInt(conv.lastOffset) <= BigInt(conv.readOffset)) return;
    const t = setTimeout(() => {
      if (document.visibilityState === "visible") void window.modbit.markRead(sessionId, taskId, conv.lastOffset).catch(() => {});
    }, 300);
    return () => clearTimeout(t);
  }, [conv.loaded, conv.lastOffset, conv.readOffset, follow.pinned, sessionId, taskId]);

  // A search hit: open the folds around the row, scroll to it and mark it for a moment.
  const focused = useRef<string | null>(null);
  useEffect(() => {
    if (!focusRowId || !conv.loaded || focused.current === focusRowId) return;
    const path = findRow(conv.rows, focusRowId);
    if (!path) return;
    focused.current = focusRowId;
    if (path.length > 0) setOpenGroups((g) => new Set([...g, ...path]));
    setFollow((f) => ({ ...f, pinned: false }));
    const t = setTimeout(() => {
      const el = scroller.current?.querySelector<HTMLElement>(`[data-row-id="${CSS.escape(focusRowId)}"]`);
      el?.scrollIntoView({ block: "center" });
      setHighlight(focusRowId);
    }, 0);
    const off = setTimeout(() => setHighlight(null), 3500);
    return () => {
      clearTimeout(t);
      clearTimeout(off);
    };
  }, [focusRowId, conv.loaded, conv.rows]);

  const toggleGroup = useCallback((rowId: string, open: boolean) => {
    setOpenGroups((g) => {
      if (g.has(rowId) === open) return g;
      const n = new Set(g);
      if (open) n.add(rowId);
      else n.delete(rowId);
      return n;
    });
  }, []);
  const copyTurn = useCallback(
    (turnId: string) => {
      const parts: string[] = [];
      collectTurnText(conv.rows, turnId, parts);
      const text = parts.join("\n\n");
      if (!text) {
        setCopied("Nothing to copy for this turn.");
        return;
      }
      navigator.clipboard
        .writeText(text)
        .then(() => setCopied("Copied the answer."))
        .catch(() => setCopied("Copying is unavailable here."));
    },
    [conv.rows],
  );

  const ctx = useMemo<RowContext>(
    () => ({ sessionId, card, live: conv.live, approvals: conv.approvals, openGroups, onToggleGroup: toggleGroup, codeOpen: prefs.codeOpen, onCodeOpen: (open) => setPrefs((p) => ({ ...p, codeOpen: open })), onResume, onCopyTurn: copyTurn, highlightRowId: highlight, latestUserRowId: latestUser, checkpoints: ckpt.context }),
    [sessionId, card, conv.live, conv.approvals, openGroups, toggleGroup, prefs.codeOpen, onResume, copyTurn, highlight, latestUser, ckpt.context],
  );

  const jumpUser = (dir: 1 | -1) => {
    const el = scroller.current;
    if (!el) return;
    const users = [...el.querySelectorAll<HTMLElement>('[data-kind="USER_MESSAGE"]')];
    const here = el.scrollTop + 8;
    const target = dir === 1 ? users.find((u) => u.offsetTop > here + 4) : [...users].reverse().find((u) => u.offsetTop < here - 4);
    if (target) {
      setFollow((f) => ({ ...f, pinned: false }));
      el.scrollTop = target.offsetTop - 4;
    } else if (dir === 1) {
      setFollow(onJumpToBottom());
    }
  };
  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.altKey && (e.key === "ArrowUp" || e.key === "ArrowDown") && !e.ctrlKey && !e.metaKey) {
      e.preventDefault();
      e.stopPropagation();
      jumpUser(e.key === "ArrowDown" ? 1 : -1);
    }
  };

  const tailFacts = conv.tail?.facts.type === "tail" ? conv.tail.facts : null;
  const streaming = [...conv.live.values()].some((l) => l.phase === "OPEN" && l.text.length > 0);
  const phase = tailFacts?.phase ?? "";
  const tailLabel = phase === "RUNNING" && streaming ? "Responding" : (tailFacts?.label ?? "");
  const failed = conv.taskState === "Failed" || phase === "FAILED" || phase === "NEEDS_ATTENTION";
  const lastUserText = useMemo(() => [...conv.rows].reverse().find((r) => r.kind === "USER_MESSAGE")?.text ?? "", [conv.rows]);

  return (
    <section className="conv" aria-label={`Conversation: ${title}`} data-testid="conversation" data-task-id={taskId} data-density={prefs.density} data-loaded={conv.loaded}>
      <div className="conv-head">
        <span className="conv-status" data-testid="conv-status">
          {card ? `${card.state}${card.waitReason ? `, waiting on ${card.waitReason}` : ""}` : conv.taskState}
        </span>
        <span className="conv-spacer" />
        {ckpt.head}
        <RunModeControl sessionId={sessionId} taskId={taskId} connected={connected} open={modeOpen} onOpenChange={setModeOpen} />
        <label className="conv-density">
          <span className="meta">Density</span>
          <select value={prefs.density} onChange={(e) => setPrefs((p) => ({ ...p, density: e.target.value as Density }))} data-testid="conv-density" aria-label="Conversation density">
            {DENSITIES.map((d) => (
              <option key={d} value={d}>
                {DENSITY_LABEL[d]}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="conv-body">
        <div ref={scroller} className="conv-scroll" role="region" aria-label="Messages" tabIndex={0} onScroll={onScrollEvent} onKeyDown={onKeyDown} data-testid="conv-scroll" data-pinned={follow.pinned} data-row-count={body.length}>
          <div ref={inner} className="conv-inner">
          {!conv.loaded && !conv.error && (
            <p className="meta conv-empty" role="status" data-testid="conv-loading">
              Loading the conversation…
            </p>
          )}
          {conv.error && !conv.loaded && (
            <p className="conv-empty" role="alert" data-testid="conv-error">
              The conversation could not be read: {conv.error}
            </p>
          )}
          {conv.loaded && body.length === 0 && (
            <p className="meta conv-empty" data-testid="conv-empty">
              Nothing has happened in this task yet.
            </p>
          )}
          {body.map((r) => (
            <RowView key={identityOf(r)} row={r} ctx={ctx} />
          ))}
          {failed && <FailureBlock taskId={taskId} card={card} onResume={onResume} promptText={lastUserText} />}
          {conv.tail && <TailStatus phase={phase} label={tailLabel} detail={tailFacts?.detail ?? ""} lastActivityMs={Math.max(conv.lastActivityMs, conv.refreshedAt)} />}
          </div>
        </div>
        {!follow.pinned && (
          <div className="conv-float">
            {follow.unseen > 0 && !follow.dismissed && (
              <div className="conv-pill" data-testid="conv-pill-wrap">
                <button type="button" className="conv-pill-btn" data-testid="conv-pill" onClick={() => setFollow(onJumpToBottom())}>
                  {pillText(follow.unseen)}
                </button>
                <button type="button" className="conv-pill-x" aria-label="Dismiss the new messages notice" data-testid="conv-pill-dismiss" onClick={() => setFollow(onDismiss)}>
                  ×
                </button>
              </div>
            )}
            <Button size="sm" onClick={() => setFollow(onJumpToBottom())} data-testid="conv-to-bottom">
              Scroll to latest
            </Button>
          </div>
        )}
      </div>
      <p className="mb-sr-only" role="status" aria-live="polite" data-testid="conv-live">
        {copied}
      </p>
      {ckpt.bar}
      <ModeProposalDock sessionId={sessionId} taskId={taskId} connected={connected} />
      <ApprovalDock sessionId={sessionId} taskId={taskId} connected={connected} onChangeMode={() => setModeOpen(true)} titleOf={titleOf ?? ((id) => id.slice(0, 8))} onOpenTask={onOpenTask ?? (() => {})} />
      {ckpt.overlay}
      {/* The one tray host: docked directly above the composer, never beside or below it (AFW-H01). */}
      <TrayHost />
      <Composer taskId={taskId} sessionId={sessionId} connected={connected} card={card} approvalPending={conv.approvals.length > 0} lastUserText={lastUserText} hasMessages={conv.rows.length > 0} onResume={onResume} onNewTask={onNewTask} onOpenTerminal={onOpenTerminal} />
    </section>
  );
}

/** The tail-status line: always says what the agent is doing while it runs, and says more the longer nothing happens (AFW-C03). */
export const TailStatus = memo(function TailStatus({ phase, label, detail, lastActivityMs }: { phase: string; label: string; detail: string; lastActivityMs: number }) {
  const running = phase === "RUNNING";
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!running) return;
    setNow(Date.now());
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, [running, lastActivityMs]);
  const idle = running && lastActivityMs > 0 ? Math.max(0, now - lastActivityMs) : 0;
  const s = running ? stallLevel(label, idle) : { level: 0 as const, text: label };
  const dotStatus = phase === "RUNNING" ? "running" : phase === "FAILED" ? "danger" : phase === "NEEDS_ATTENTION" || phase === "WAITING" ? "warn" : phase === "COMPLETED" || phase === "READY_FOR_REVIEW" ? "ok" : "idle";
  return (
    <div className="conv-tail" role="status" aria-live="polite" data-testid="conv-tail" data-phase={phase} data-stall={s.level}>
      <StatusDot status={dotStatus} label={label} />
      <span className={running ? "mb-shimmer conv-tail-text" : "conv-tail-text"} data-testid="conv-tail-text">
        {s.text}
      </span>
      {detail && <span className="meta">{detail.replace(/_/g, " ").toLowerCase()}</span>}
    </div>
  );
});

/** A failed run in the Core's typed words: what happened, what only the person can do, how the system recovers, and a retry when the Core says it can. */
export function FailureBlock({ taskId, card, onResume, promptText }: { taskId: string; card: TaskCard | undefined; onResume: (taskId: string) => void; promptText: string }) {
  const [status, setStatus] = useState<{ failureClass: string; failureCode: string; attentionReason: string; userAction: string; recoveryPath: string; retryable: boolean } | null>(null);
  const [note, setNote] = useState("");
  useEffect(() => {
    if (card?.diagnostic) return;
    let live = true;
    window.modbit
      .taskStatus(taskId)
      .then((s) => live && setStatus(s))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [taskId, card?.diagnostic, card?.state]);
  const d = card?.diagnostic
    ? { failureClass: card.diagnostic.class, failureCode: card.diagnostic.code, attentionReason: card.diagnostic.detail, userAction: card.diagnostic.userAction, recoveryPath: card.diagnostic.recoveryPath, retryable: card.diagnostic.retryable }
    : status;
  const words = (x: string) => x.replace(/_/g, " ").toLowerCase();
  return (
    <div className="conv-failure" role="alert" data-testid="conv-failure" aria-label="The task needs attention">
      <p>
        <StatusDot status="danger" label="Problem" showLabel /> <strong>{d?.attentionReason || "The run stopped and needs attention."}</strong>
      </p>
      {d && (d.failureClass || d.failureCode) && (
        <p className="meta" data-testid="conv-failure-cause">
          Cause: {words(d.failureClass)}
          {d.failureCode ? ` (${words(d.failureCode)})` : ""}
        </p>
      )}
      {d?.userAction && (
        <p data-testid="conv-failure-next">
          <strong>What you can do:</strong> {d.userAction}
        </p>
      )}
      {d?.recoveryPath && <p className="meta">How Modbit recovers: {d.recoveryPath}</p>}
      <div className="decision">
        {d?.retryable === true && (
          <Button size="sm" variant="primary" onClick={() => onResume(taskId)} data-testid="conv-retry">
            Retry
          </Button>
        )}
        {promptText && (
          <Button
            size="sm"
            onClick={() => {
              navigator.clipboard
                .writeText(promptText)
                .then(() => setNote("Your prompt is copied."))
                .catch(() => setNote("Copying is unavailable here."));
            }}
            data-testid="conv-copy-prompt"
          >
            Copy your prompt
          </Button>
        )}
        {note && (
          <span className="meta" role="status">
            {note}
          </span>
        )}
      </div>
    </div>
  );
}
