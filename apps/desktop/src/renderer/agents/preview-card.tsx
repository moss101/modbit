/**
 * A task's preview card (DR-PX-2026-11-11-021 MWS-04): it opens when the pointer rests on a row and on Space, beside the list.
 * It shows the Core's state with its reason or next action, the last line the assistant wrote (MWS-20, REQ-PX-218: the header's
 * own field, capped and redacted by the Core), the size of the change, the context in use and Open. Everything on it is the
 * task's header and the facts the list already holds: it makes no read of its own and opens no transcript. The pull request and
 * split view wait for the rows that deliver them (MWS-17, MWS-11).
 */
import { useEffect, useRef } from "react";
import { Button, StatusDot, useLayer } from "@modbit/ui";
import type { AgentHeaderView } from "../../shared/conversation-types.ts";
import { CLASS_META, changeSize, previewReason, relativeAge, type RowFacts } from "./list-model.ts";

export interface PreviewCardProps {
  header: AgentHeaderView;
  facts: RowFacts | undefined;
  nowMs: number;
  /** Opened with Space: focus moves to Open, and leaving the card closes it. Opened by the pointer it takes no focus. */
  byKeyboard: boolean;
  onOpen: () => void;
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
      </div>
    </div>
  );
}
