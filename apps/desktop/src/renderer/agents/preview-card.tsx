/**
 * A task's preview card (DR-PX-2026-11-11-021 MWS-04): it opens when the pointer rests on a row and on Space, beside the list.
 * It shows the Core's state with its reason or next action, the last line the assistant wrote (MWS-20, REQ-PX-218: the header's
 * own field, capped and redacted by the Core), the size of the change, the context in use and Open. Everything on it is the
 * task's header and the facts the list already holds: it makes no read of its own and opens no transcript. Open in split view
 * puts the task beside the one in the centre (MWS-11, REQ-PX-204), or says why it cannot. The task's pull request is the header's
 * own field (MWS-17, REQ-PX-211): its number, the state and checks the Core last read, and when.
 */
import { useEffect, useRef } from "react";
import { Button, StatusDot, useLayer } from "@modbit/ui";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import { CLASS_META, changeSize, previewReason, relativeAge, type RowFacts } from "./list-model.ts";
import { requestStateWords } from "../apps/pull-request-model.ts";

export interface PreviewCardProps {
  header: AgentHeaderView;
  facts: RowFacts | undefined;
  nowMs: number;
  /** Opened with Space: focus moves to Open, and leaving the card closes it. Opened by the pointer it takes no focus. */
  byKeyboard: boolean;
  onOpen: () => void;
  /** Opens the task in split view beside the one in the centre; the shell refuses it with its reason where it cannot. */
  onSplit?: (() => void) | undefined;
  /** `restoreFocus` is true when the card was dismissed with Escape from the keyboard: focus goes back to the row. */
  onClose: (restoreFocus: boolean) => void;
  onPointerEnter: () => void;
  onPointerLeave: () => void;
}

export function PreviewCard(p: PreviewCardProps) {
  const h = p.header;
  const open = useRef<HTMLButtonElement>(null);
  // Escape closes the card and nothing else (WCAG 1.4.13), like every other layer.
  useLayer(true, "agents-preview", false, () => p.onClose(p.byKeyboard));
  useEffect(() => {
    if (p.byKeyboard) open.current?.focus({ preventScroll: true });
  }, [p.byKeyboard, h.taskId]);
  const meta = CLASS_META[h.statusClass];
  const words = h.statusLabel || meta.short;
  const title = h.title || "Untitled task";
  const reason = previewReason(p.facts);
  const change = changeSize(h);
  const age = relativeAge(h.updatedAtMs, p.nowMs);
  const pr = requestStateWords(h.pullRequest);
  const prAge = h.pullRequest ? relativeAge(h.pullRequest.readAtMs, p.nowMs) : "";
  return (
    <div
      className="agents-preview"
      role="dialog"
      aria-label={`Preview: ${title}`}
      data-testid="agent-preview"
      data-task-id={h.taskId}
      data-by={p.byKeyboard ? "keyboard" : "pointer"}
      onMouseEnter={p.onPointerEnter}
      onMouseLeave={p.onPointerLeave}
      onBlur={(e) => {
        if (p.byKeyboard && !e.currentTarget.contains(e.relatedTarget as Node | null)) p.onClose(false);
      }}
    >
      <div className="agents-preview-head">
        <span className="agents-preview-state" data-testid="agent-preview-state">
          <StatusDot status={meta.status} label={words} showLabel />
        </span>
        {age && (
          <span className="agents-preview-age" data-testid="agent-preview-age">
            {age === "now" ? "now" : `${age} ago`}
          </span>
        )}
      </div>
      <p className="agents-preview-title" data-testid="agent-preview-title">
        {title}
      </p>
      {reason && (
        <p className="agents-preview-reason" data-testid="agent-preview-reason">
          {reason}
        </p>
      )}
      {h.lastLine && (
        <p className="agents-preview-last" data-testid="agent-preview-last">
          <span className="mb-sr-only">The assistant's last line: </span>
          {h.lastLine}
        </p>
      )}
      {pr && (
        <p className="agents-preview-pr" data-testid="agent-preview-pr" data-state={h.pullRequest?.state || "UNREAD"} data-checks={h.pullRequest?.checks || "UNREAD"}>
          Pull request {pr.number} · {pr.state}
          {pr.checks ? ` · ${pr.checks}` : ""}
          {pr.read && prAge ? ` · read ${prAge === "now" ? "just now" : `${prAge} ago`}` : ""}
        </p>
      )}
      {(change || h.contextPercent > 0) && (
        <dl className="agents-preview-facts">
          {change && (
            <div className="agents-preview-fact">
              <dt>Changes</dt>
              <dd data-testid="agent-preview-changes">
                <span className="conv-add">+{change.added}</span> <span className="conv-del">−{change.removed}</span> · {change.files}
              </dd>
            </div>
          )}
          {h.contextPercent > 0 && (
            <div className="agents-preview-fact">
              <dt>Context</dt>
              <dd data-testid="agent-preview-context">{h.contextPercent}% used</dd>
            </div>
          )}
        </dl>
      )}
      <div className="agents-preview-actions">
        <Button ref={open} size="sm" onClick={p.onOpen} data-testid="agent-preview-open">
          Open
        </Button>
        {p.onSplit && (
          <Button size="sm" variant="ghost" onClick={p.onSplit} data-testid="agent-preview-split">
            Open in split view
          </Button>
        )}
      </div>
    </div>
  );
}
