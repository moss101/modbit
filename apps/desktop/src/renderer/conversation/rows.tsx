/**
 * The transcript's row components (REQ-PX-047; docs/65 AFW-C01..C08). Every
 * row is drawn from the Core's projection as plain text: React renders text
 * nodes, never markup, so a planted `<script>` or `<img onerror>` in a model
 * answer, a tool result or a repository file is inert, and a link is text (a
 * link opens only through main's allow-list, which no row can reach: nothing
 * here calls anything but the typed approval, question and steer controls the
 * Fleet already uses). Rows are memoised so a streaming delta re-renders only
 * the row that grew.
 */
import { memo, useEffect, useRef, useState, type ReactNode } from "react";
import { Button, StatusDot } from "@modbit/ui";
import type { PendingApprovalView, TranscriptRowView } from "../../shared/conversation-types.ts";
import type { TaskCard } from "../model.ts";
import { needsEyes, presentAssistant, streamKey, type LiveStreams, type MessageState, type PresentedRow } from "./model.ts";
import { sameBlock, splitBlocks, type Block } from "./blocks.ts";

export interface RowContext {
  sessionId: string | null;
  card: TaskCard | undefined;
  live: LiveStreams;
  /** The Core's open approvals for this task: the exact intent each decision must name. */
  approvals: readonly PendingApprovalView[];
  openGroups: ReadonlySet<string>;
  onToggleGroup: (rowId: string, open: boolean) => void;
  codeOpen: boolean;
  onCodeOpen: (open: boolean) => void;
  onResume: (taskId: string) => void;
  onCopyTurn: (turnId: string) => void;
  highlightRowId: string | null;
  /** The latest user message of the conversation stays pinned as a sticky card while its turn is on screen (AFW-C07). */
  latestUserRowId: string | null;
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  return m < 60 ? `${m} min ${s % 60} s` : `${Math.floor(m / 60)} h ${m % 60} min`;
}

const clock = (ms: number): string => (ms ? new Date(ms).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }) : "");
const fullDate = (ms: number): string => (ms ? new Date(ms).toLocaleString() : "");

// ----------------------------------------------------------------- streaming

/** The newest words fade from dim to full (AFW-C02); the fresh piece is keyed by length so each delta replays the fade on its own words only. */
function useFresh(text: string, streaming: boolean): { stable: string; fresh: string } {
  const seen = useRef(0);
  const from = streaming && text.length >= seen.current ? seen.current : text.length;
  useEffect(() => {
    seen.current = text.length;
  });
  return { stable: text.slice(0, from), fresh: text.slice(from) };
}

const BlockView = memo(function BlockView({ block, codeOpen, onCodeOpen }: { block: Block; codeOpen: boolean; onCodeOpen: (open: boolean) => void }) {
  if (block.kind === "code") {
    const lines = block.text.split("\n").length;
    return (
      <details className="conv-code" open={codeOpen} onToggle={(e) => onCodeOpen((e.currentTarget as HTMLDetailsElement).open)}>
        <summary>
          Code{block.lang ? ` (${block.lang})` : ""}, {lines} {lines === 1 ? "line" : "lines"}
          {block.open ? ", streaming" : ""}
        </summary>
        <pre>
          <code>{block.text}</code>
        </pre>
      </details>
    );
  }
  return <p className="conv-para">{block.text}</p>;
}, (a, b) => a.codeOpen === b.codeOpen && sameBlock(a.block, b.block));

/** The text of a message: paragraphs and collapsed code blocks. Finished blocks are memoised, so a delta re-renders the last one only. */
function MessageText({ text, streaming, codeOpen, onCodeOpen }: { text: string; streaming: boolean; codeOpen: boolean; onCodeOpen: (open: boolean) => void }) {
  const { stable, fresh } = useFresh(text, streaming);
  const blocks = splitBlocks(stable + fresh);
  // The fresh piece lies in the last block: draw it as its own span so it alone fades in.
  return (
    <div className="conv-text" data-testid="conv-text">
      {blocks.map((b, i) => {
        const last = i === blocks.length - 1;
        if (last && fresh && b.kind === "text" && b.text.length >= fresh.length) {
          const head = b.text.slice(0, b.text.length - fresh.length);
          return (
            <p className="conv-para" key={i}>
              {head}
              <span className="conv-fresh" key={text.length}>
                {fresh}
              </span>
            </p>
          );
        }
        return <BlockView key={i} block={b} codeOpen={codeOpen} onCodeOpen={onCodeOpen} />;
      })}
    </div>
  );
}

const MESSAGE_WORDS: Record<MessageState, string> = { streaming: "Responding…", complete: "", aborted: "Stopped" };

function AssistantRow({ p, ctx }: { p: PresentedRow; ctx: RowContext }) {
  const state = p.message ?? "complete";
  const empty = p.text.length === 0;
  return (
    <article className="conv-row conv-assistant" data-testid="conv-assistant" data-row-id={p.row.rowId} data-kind="ASSISTANT_MESSAGE" data-message-state={state} data-highlight={ctx.highlightRowId === p.row.rowId} aria-label="Assistant message">
      {state === "streaming" && (
        <p className="meta conv-state" data-testid="conv-streaming">
          {MESSAGE_WORDS.streaming}
        </p>
      )}
      {p.row.hints.hasReasoning && (
        <details className="conv-reasoning">
          <summary>Reasoned before answering</summary>
          <p className="meta">The model's reasoning summary is kept out of the message and is not shown in the transcript.</p>
        </details>
      )}
      {state === "aborted" ? (
        <div className="conv-aborted" role="group" aria-label="Stopped response" data-testid="conv-aborted">
          <p>
            <StatusDot status="warn" label="Stopped" showLabel /> <strong>{p.abort}</strong> What was received is shown as a partial response; it is not a finished answer.
          </p>
          {!empty && (
            <details>
              <summary>Partial text received ({p.text.length.toLocaleString()} characters)</summary>
              <div className="conv-partial" data-testid="conv-partial">
                <MessageText text={p.text} streaming={false} codeOpen={ctx.codeOpen} onCodeOpen={ctx.onCodeOpen} />
              </div>
            </details>
          )}
        </div>
      ) : (
        <>
          <MessageText text={p.text} streaming={state === "streaming"} codeOpen={ctx.codeOpen} onCodeOpen={ctx.onCodeOpen} />
          {p.row.textTruncated && <p className="meta">The message is longer than the transcript shows here.</p>}
        </>
      )}
    </article>
  );
}

// ------------------------------------------------------------------ other rows

function UserRow({ row, ctx }: { row: TranscriptRowView; ctx: RowContext }) {
  const f = row.facts.type === "user" ? row.facts : null;
  const sticky = ctx.latestUserRowId === row.rowId;
  return (
    <article className="conv-row conv-user" data-testid="conv-user" data-row-id={row.rowId} data-kind="USER_MESSAGE" data-sticky={sticky} data-highlight={ctx.highlightRowId === row.rowId} aria-label="Your message">
      <p className="conv-user-text">{row.text}</p>
      <p className="meta">
        {f?.source === "answer" ? "Your answer" : f?.source === "queued_input" ? `Queued ${f.mode ? f.mode.toLowerCase() : "input"}` : "Task"}
        {f?.untrusted ? ` · from ${f.provenance.replace(/_/g, " ") || "outside"} (untrusted text)` : ""}
        {row.atMs ? ` · ${clock(row.atMs)}` : ""}
      </p>
    </article>
  );
}

const CLASS_WORD: Record<string, string> = { READ: "Read", SEARCH: "Search", SHELL: "Command", EDIT: "Edit", BROWSER: "Browser", OTHER: "Tool" };
const TOOL_STATUS_WORD: Record<string, string> = { PENDING: "Waiting to run", RUNNING: "Running", AWAITING_APPROVAL: "Waiting for approval", SUCCEEDED: "Done", FAILED: "Failed", DENIED: "Denied", CANCELLED: "Cancelled", UNKNOWN_OUTCOME: "Outcome unknown" };

function ToolRow({ row, ctx }: { row: TranscriptRowView; ctx: RowContext }) {
  const f = row.facts.type === "tool" ? row.facts : null;
  const status = row.hints.status;
  const bad = status === "FAILED" || status === "DENIED" || status === "UNKNOWN_OUTCOME" || status === "CANCELLED";
  const word = TOOL_STATUS_WORD[status] ?? status.toLowerCase();
  return (
    <div className="conv-row conv-tool" data-testid="conv-tool" data-row-id={row.rowId} data-kind="TOOL_CARD" data-tool-class={f?.toolClass} data-status={status} data-highlight={ctx.highlightRowId === row.rowId}>
      <span className="conv-tool-class">{CLASS_WORD[f?.toolClass ?? "OTHER"] ?? "Tool"}</span>
      <code className="conv-tool-text" title={row.text}>
        {row.text}
      </code>
      <span className="conv-tool-status" data-bad={bad}>
        {bad ? <StatusDot status={status === "FAILED" ? "danger" : "warn"} label={word} showLabel /> : status === "RUNNING" ? <StatusDot status="running" label={word} showLabel /> : <span className="meta">{word}</span>}
      </span>
      {row.hints.durationMs > 0 && <span className="meta">{formatDuration(row.hints.durationMs)}</span>}
      {(row.hints.linesAdded > 0 || row.hints.linesRemoved > 0) && (
        <span className="meta">
          +{row.hints.linesAdded} −{row.hints.linesRemoved}
        </span>
      )}
      {f && f.failureCode && <span className="conv-tool-failure">Failed: {f.failureCode.replace(/_/g, " ").toLowerCase()}</span>}
      {f && f.paths.length > 0 && <span className="meta conv-tool-paths">{f.paths.join(", ")}</span>}
    </div>
  );
}

function GroupRow({ row, ctx }: { row: TranscriptRowView; ctx: RowContext }) {
  const f = row.facts.type === "group" ? row.facts : null;
  // A fold that would hide something the person must act on or a failure stays open.
  const forced = needsEyes(row);
  const open = forced || ctx.openGroups.has(row.rowId);
  return (
    <details className="conv-row conv-group" data-testid="conv-group" data-row-id={row.rowId} data-kind="WORK_GROUP" data-open={open} data-forced={forced} open={open} onToggle={(e) => !forced && ctx.onToggleGroup(row.rowId, (e.currentTarget as HTMLDetailsElement).open)}>
      <summary data-testid="conv-group-summary">
        <span className="conv-group-label">{row.text || f?.label}</span>
        {f && f.steps > 0 && (
          <span className="meta">
            {" "}
            · {f.steps} {f.steps === 1 ? "step" : "steps"}
            {f.linesAdded + f.linesRemoved > 0 ? ` · +${f.linesAdded} −${f.linesRemoved}` : ""}
            {f.open ? " · in progress" : ""}
          </span>
        )}
      </summary>
      <div className="conv-group-body">
        {row.children.map((c) => (
          <RowView key={c.rowId} row={c} ctx={ctx} />
        ))}
      </div>
    </details>
  );
}

function ApprovalRow({ row, ctx }: { row: TranscriptRowView; ctx: RowContext }) {
  const f = row.facts.type === "approval" ? row.facts : null;
  const card = ctx.card;
  const requested = row.hints.status === "REQUESTED";
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [answer, setAnswer] = useState("");
  const [confirming, setConfirming] = useState(false);
  const isQuestion = f?.kind === "QUESTION";
  const decide = async (approve: boolean) => {
    if (!ctx.sessionId || !approval || busy) return;
    setBusy(true);
    try {
      const r = await window.modbit.resolveApproval(ctx.sessionId, approval.approvalId, approve, approve ? "approved in the conversation" : "denied in the conversation", approval.intentHash);
      setNote(`${r.status.toLowerCase()} at offset ${r.offset}`);
    } catch (e) {
      setNote(`refused: ${(e as Error).message}`);
    } finally {
      setBusy(false);
      setConfirming(false);
    }
  };
  const respond = async (optionId: string, text: string) => {
    if (!ctx.sessionId || !card || !f || busy) return;
    setBusy(true);
    try {
      const r = await window.modbit.respondToQuestion(ctx.sessionId, card.taskId, f.approvalId, optionId, text);
      setNote(r.alreadyAnswered ? "already answered" : "answered; resume to continue");
      setAnswer("");
    } catch (e) {
      setNote(`refused: ${(e as Error).message}`);
    } finally {
      setBusy(false);
    }
  };
  const norm = (id: string) => id.replace(/-/g, "").toLowerCase();
  // The Core's open approval list is the source of the exact intent; the card's own event-fed copy is the fallback.
  const approval = f && !isQuestion ? (ctx.approvals.find((a) => norm(a.approvalId) === norm(f.approvalId)) ?? (card?.approval && norm(card.approval.approvalId) === norm(f.approvalId) ? card.approval : null)) : null;
  const live = requested && approval !== null;
  const irreversible = approval !== null && /destructive|external/i.test(approval.effectClass);
  const known = requested && isQuestion && card?.question && card.question.questionId === f?.approvalId ? card.question : null;
  // After a reload the options are not in the renderer; the Core still takes a free-text answer to its question and decides whether it is acceptable.
  const question = known ?? (requested && isQuestion && f ? { questionId: f.approvalId, text: row.text, options: [] as { id: string; label: string }[], allowFreeText: true } : null);
  return (
    <article className="conv-row conv-approval" data-testid="conv-approval" data-row-id={row.rowId} data-kind="APPROVAL_CARD" data-state={row.hints.status} data-highlight={ctx.highlightRowId === row.rowId} aria-label={isQuestion ? "Question from the agent" : "Approval request"}>
      <p>
        <StatusDot status={requested ? "warn" : "ok"} label={requested ? "Needs you" : (row.hints.status || "Resolved").toLowerCase()} showLabel /> <strong>{isQuestion ? "The agent asks" : `Approval: ${f?.toolName || "a protected action"}`}</strong>
      </p>
      {row.text && <p className="conv-approval-text">{row.text}</p>}
      {!isQuestion && f && (
        <p className="meta">
          {f.effectClass ? `Effect class ${f.effectClass}` : ""}
          {f.resolver ? ` · resolved by ${f.resolver}` : ""}
        </p>
      )}
      {live && approval && (
        <div className="decision" data-testid="conv-approval-decision" data-approval-id={approval.approvalId}>
          <span className="meta">
            {approval.toolName} asks for a {approval.effectClass} effect · exact intent <code data-testid="conv-approval-intent" title={approval.intentHash}>{approval.intentHash.slice(0, 16)}</code>
          </span>
          {confirming ? (
            <>
              <span className="meta">This effect cannot be undone.</span>
              <Button size="sm" variant="danger" onClick={() => void decide(true)} disabled={busy} data-testid="conv-approve-confirm">
                Confirm approve
              </Button>
              <Button size="sm" onClick={() => setConfirming(false)} data-testid="conv-approve-back">
                Back
              </Button>
            </>
          ) : (
            <>
              <Button size="sm" variant="primary" onClick={() => (irreversible ? setConfirming(true) : void decide(true))} disabled={busy} data-testid="conv-approve">
                Approve
              </Button>
              <Button size="sm" onClick={() => void decide(false)} disabled={busy} data-testid="conv-deny">
                Deny
              </Button>
            </>
          )}
        </div>
      )}
      {question && (
        <div className="decision" data-testid="conv-question" data-question-id={question.questionId}>
          {question.options.map((o) => (
            <Button size="sm" key={o.id} onClick={() => void respond(o.id, "")} disabled={busy} data-testid="conv-answer-option" data-option-id={o.id}>
              {o.label}
            </Button>
          ))}
          {question.allowFreeText && (
            <>
              <input aria-label="Your answer" value={answer} onChange={(e) => setAnswer(e.target.value)} placeholder="your answer" disabled={busy} data-testid="conv-answer-text" />
              <Button size="sm" variant="primary" onClick={() => void respond("", answer)} disabled={busy || !answer.trim()} data-testid="conv-answer-send">
                Answer
              </Button>
            </>
          )}
        </div>
      )}
      {card && card.state === "Waiting" && card.waitReason === "UserInput" && !requested && (
        <Button size="sm" variant="primary" onClick={() => ctx.onResume(card.taskId)} data-testid="conv-resume-after-answer">
          Resume
        </Button>
      )}
      {note && (
        <p className="meta" role="status" data-testid="conv-decision-note">
          {note}
        </p>
      )}
    </article>
  );
}

function FooterRow({ row, ctx }: { row: TranscriptRowView; ctx: RowContext }) {
  const f = row.facts.type === "footer" ? row.facts : null;
  if (!f) return null;
  const failed = f.outcome === "FAILED";
  return (
    <div className="conv-row conv-footer" data-testid="conv-footer" data-row-id={row.rowId} data-kind="TURN_FOOTER" data-outcome={f.outcome}>
      <span className="meta">
        <StatusDot status={failed ? "danger" : f.outcome === "INTERRUPTED" ? "warn" : "ok"} label={f.outcome === "COMPLETED" ? "Turn complete" : f.outcome === "INTERRUPTED" ? "Turn interrupted" : failed ? `Turn failed${f.failureCode ? `: ${f.failureCode.replace(/_/g, " ").toLowerCase()}` : ""}` : f.outcome.toLowerCase()} showLabel />
        {f.durationMs > 0 ? ` · ${formatDuration(f.durationMs)}` : ""}
        {Number(f.outputTokens) > 0 ? ` · ${Number(f.inputTokens).toLocaleString()} in, ${Number(f.outputTokens).toLocaleString()} out tokens` : ""}
        {row.atMs ? (
          <>
            {" · "}
            <time dateTime={new Date(row.atMs).toISOString()} title={fullDate(row.atMs)}>
              {clock(row.atMs)}
            </time>
          </>
        ) : null}
      </span>
      <Button size="sm" variant="ghost" onClick={() => ctx.onCopyTurn(f.turnId)} data-testid="conv-copy-turn">
        Copy answer
      </Button>
    </div>
  );
}

function BoundaryRow({ row }: { row: TranscriptRowView }) {
  const f = row.facts.type === "boundary" ? row.facts : null;
  return (
    <div className="conv-row conv-boundary meta" role="separator" aria-label={`Time boundary, ${f?.day ?? row.text}`} data-testid="conv-boundary" data-row-id={row.rowId} data-kind="TIME_BOUNDARY">
      <span>{f?.day ?? row.text}</span>
    </div>
  );
}

function DividerRow({ row }: { row: TranscriptRowView }) {
  return (
    <div className="conv-row conv-divider" role="separator" aria-label={`Unread messages: ${row.text}`} data-testid="conv-unread-divider" data-row-id={row.rowId} data-kind="UNREAD_DIVIDER">
      <span>{row.text || "New"}</span>
    </div>
  );
}

// ------------------------------------------------------------------- the row

function RowInner({ row, ctx }: { row: TranscriptRowView; ctx: RowContext }): ReactNode {
  switch (row.kind) {
    case "USER_MESSAGE":
      return <UserRow row={row} ctx={ctx} />;
    case "ASSISTANT_MESSAGE":
      return <AssistantRow p={presentAssistant(row, ctx.live)} ctx={ctx} />;
    case "TOOL_CARD":
      return <ToolRow row={row} ctx={ctx} />;
    case "WORK_GROUP":
      return <GroupRow row={row} ctx={ctx} />;
    case "APPROVAL_CARD":
      return <ApprovalRow row={row} ctx={ctx} />;
    case "TURN_FOOTER":
      return <FooterRow row={row} ctx={ctx} />;
    case "TIME_BOUNDARY":
      return <BoundaryRow row={row} />;
    case "UNREAD_DIVIDER":
      return <DividerRow row={row} />;
    default:
      return null;
  }
}

/** Memoised: a row re-renders when its own data, the state of its stream or the controls it depends on change. */
export const RowView = memo(RowInner, (a, b) => {
  if (a.row !== b.row) return false;
  const x = a.ctx;
  const y = b.ctx;
  if (x.sessionId !== y.sessionId || x.codeOpen !== y.codeOpen || x.highlightRowId !== y.highlightRowId || x.latestUserRowId !== y.latestUserRowId || x.openGroups !== y.openGroups) return false;
  // A row reads its own stream and the task card only if it is one that needs them: a delta of another stream re-renders nothing here.
  if (a.row.facts.type === "stream") {
    const k = streamKey(a.row.facts.streamId);
    if (x.live.get(k) !== y.live.get(k)) return false;
  }
  if (a.row.kind === "APPROVAL_CARD" || a.row.kind === "WORK_GROUP") return x.card === y.card && x.approvals === y.approvals;
  return true;
});
