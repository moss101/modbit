/**
 * The composer's docked trays (REQ-PX-055, -056; docs/65 AFW-E02, E04, E05,
 * E07, E08, E09, H01, H02): the durable queue, the first-use education about
 * what a send during a turn does, the stopped state, the policy-blocked model
 * tray, the background-terminals tray, the side question and the send
 * behaviour settings. They show Core facts and call typed commands through
 * callbacks; there is no queue or interrupt logic in the renderer.
 */
import { useEffect, useRef, useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { SendBehaviorState } from "../../shared/composer-types.ts";
import type { TerminalViewJson } from "../../preload/preload.ts";
import { EDUCATION, elapsedLabel, queueHeader, sendNowLabel, type QueueRow } from "./model.ts";

// ------------------------------------------------------------------ queue

export interface QueueTrayProps {
  rows: readonly QueueRow[];
  behavior: SendBehaviorState | null;
  runAlive: boolean;
  busy: boolean;
  onSendNow: (id: string) => void;
  onEdit: (id: string, change: { text: string; mode: string }) => Promise<boolean>;
  onDelete: (id: string) => void;
  onMove: (id: string, dir: "up" | "down") => void;
  onClear: () => void;
  /** Escape leaves the tray for the composer. */
  onLeave: () => void;
}

/** The durable queue the Core holds, one row per message in the order it will run. */
export function QueueTray(p: QueueTrayProps) {
  const [collapsed, setCollapsed] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  const [editMode, setEditMode] = useState("FOLLOW_UP");
  const list = useRef<HTMLUListElement>(null);
  const send = sendNowLabel(p.behavior);
  const focusRow = (i: number) => list.current?.querySelectorAll<HTMLElement>('[data-testid="queue-row-main"]')[i]?.focus();
  const startEdit = (r: QueueRow) => {
    setEditing(r.inputId);
    setDraft(r.text);
    setEditMode(r.mode);
  };
  const save = async (id: string) => {
    if (await p.onEdit(id, { text: draft, mode: editMode })) setEditing(null);
  };
  if (p.rows.length === 0) return null;
  return (
    <section className="cmp-tray cmp-queue" aria-label="Queued messages" data-testid="queue-tray" data-count={p.rows.length}>
      <header className="cmp-tray-head">
        <strong data-testid="queue-header">{queueHeader(p.rows.length)}</strong>
        <span className="meta">{p.runAlive ? "They run in order after the current turn." : "They run when the task runs again."}</span>
        <span className="cmp-spacer" />
        <Button size="sm" variant="ghost" onClick={p.onClear} disabled={p.busy} data-testid="queue-clear">
          Clear
        </Button>
        <Button size="sm" variant="ghost" aria-expanded={!collapsed} onClick={() => setCollapsed((c) => !c)} data-testid="queue-collapse">
          {collapsed ? "Show" : "Collapse"}
        </Button>
      </header>
      {!collapsed && (
        <ul ref={list} className="cmp-qlist" aria-label="Queued messages in order">
          {p.rows.map((r, i) => (
            <li key={r.inputId} className="cmp-qrow" data-testid="queue-row" data-input-id={r.inputId} data-position={r.position}>
              {editing === r.inputId ? (
                <div className="cmp-qedit">
                  <textarea
                    aria-label={`Edit queued message ${r.position}`}
                    value={draft}
                    onChange={(e) => setDraft(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Escape") {
                        e.preventDefault();
                        e.stopPropagation();
                        e.nativeEvent.stopPropagation();
                        setEditing(null);
                      } else if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                        e.preventDefault();
                        void save(r.inputId);
                      }
                    }}
                    autoFocus
                    data-testid="queue-edit-input"
                    rows={2}
                  />
                  <select aria-label={`How queued message ${r.position} runs`} value={editMode} onChange={(e) => setEditMode(e.target.value)} data-testid="queue-edit-mode">
                    <option value="FOLLOW_UP">Queue: its own turn</option>
                    <option value="COLLECT">Collect: joined with others</option>
                    <option value="STEER">Steer: at the next safe point</option>
                  </select>
                  <Button size="sm" variant="primary" disabled={p.busy || draft.trim() === ""} onClick={() => void save(r.inputId)} data-testid="queue-edit-save">
                    Save
                  </Button>
                  <Button size="sm" onClick={() => setEditing(null)} data-testid="queue-edit-cancel">
                    Cancel
                  </Button>
                </div>
              ) : (
                <>
                  <button
                    type="button"
                    className="cmp-qmain"
                    data-testid="queue-row-main"
                    title={r.text}
                    onKeyDown={(e) => {
                      if (e.key === "ArrowDown") {
                        e.preventDefault();
                        focusRow(Math.min(p.rows.length - 1, i + 1));
                      } else if (e.key === "ArrowUp") {
                        e.preventDefault();
                        focusRow(Math.max(0, i - 1));
                      } else if (e.key === "ArrowRight" || e.key === " ") {
                        e.preventDefault();
                        startEdit(r);
                      } else if (e.key === "Backspace" && (e.metaKey || e.ctrlKey)) {
                        e.preventDefault();
                        p.onDelete(r.inputId);
                      } else if (e.key === "Escape") {
                        e.preventDefault();
                        // The shell's own Escape (back, blur) is not for a tray that has the key.
                        e.nativeEvent.stopPropagation();
                        p.onLeave();
                      }
                    }}
                    onClick={() => startEdit(r)}
                  >
                    <span className="cmp-qline" data-testid="queue-line">
                      {r.line}
                    </span>
                    {r.edited && <Badge tone="neutral">edited</Badge>}
                    {r.untrusted && <Badge tone="warn">untrusted</Badge>}
                    <span className="meta cmp-qmode" data-testid="queue-mode">
                      {r.modeLabel}
                    </span>
                  </button>
                  <span className="cmp-qactions">
                    <Button size="sm" disabled={p.busy} onClick={() => p.onSendNow(r.inputId)} title={send.title} data-testid="queue-send-now">
                      {send.label}
                    </Button>
                    <Button size="sm" disabled={p.busy} onClick={() => startEdit(r)} aria-label={`Edit queued message ${r.position}`} data-testid="queue-edit">
                      Edit
                    </Button>
                    <Button size="sm" variant="danger" disabled={p.busy} onClick={() => p.onDelete(r.inputId)} aria-label={`Delete queued message ${r.position}`} data-testid="queue-delete">
                      Delete
                    </Button>
                    <Button size="sm" variant="ghost" disabled={p.busy || !r.canMoveUp} onClick={() => p.onMove(r.inputId, "up")} aria-label={`Move queued message ${r.position} up`} data-testid="queue-up">
                      ↑
                    </Button>
                    <Button size="sm" variant="ghost" disabled={p.busy || !r.canMoveDown} onClick={() => p.onMove(r.inputId, "down")} aria-label={`Move queued message ${r.position} down`} data-testid="queue-down">
                      ↓
                    </Button>
                  </span>
                </>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

// -------------------------------------------------- attention-style trays

export function EducationTray({ onKeep, onSettings }: { onKeep: () => void; onSettings: () => void }) {
  return (
    <section className="cmp-tray cmp-attn" data-tone="info" aria-label={EDUCATION.title} data-testid="education-tray">
      <strong>{EDUCATION.title}</strong>
      <ul className="cmp-lines">
        {EDUCATION.lines.map((l) => (
          <li key={l}>{l}</li>
        ))}
      </ul>
      <div className="cmp-actions">
        <Button size="sm" variant="primary" onClick={onKeep} data-testid="education-keep">
          Keep queuing
        </Button>
        <Button size="sm" onClick={onSettings} data-testid="education-settings">
          Open settings
        </Button>
      </div>
    </section>
  );
}

export function StoppedTray({ text, onEdit, onContinue, onDismiss, canContinue }: { text: string; onEdit: () => void; onContinue: () => void; onDismiss: () => void; canContinue: boolean }) {
  return (
    <section className="cmp-tray cmp-attn" data-tone="warn" role="status" aria-label="Stopped" data-testid="stopped-tray">
      <strong>Stopped by you.</strong> <span className="meta">The turn ended at a safe point. Nothing was sent again.</span>
      {text && (
        <p className="cmp-stopped-text" data-testid="stopped-text">
          Your message: {text.length > 160 ? `${text.slice(0, 159)}…` : text}
        </p>
      )}
      <div className="cmp-actions">
        {text && (
          <Button size="sm" onClick={onEdit} data-testid="stopped-edit">
            Edit your message
          </Button>
        )}
        <Button size="sm" variant="primary" onClick={onContinue} disabled={!canContinue} data-testid="stopped-continue">
          Continue
        </Button>
        <Button size="sm" variant="ghost" onClick={onDismiss} data-testid="stopped-dismiss">
          Dismiss
        </Button>
      </div>
    </section>
  );
}

export function PolicyTray({ model, code, reason, onAuto, onCopy, onDismiss, note }: { model: string; code: string; reason: string; onAuto: () => void; onCopy: () => void; onDismiss: () => void; note: string }) {
  return (
    <section className="cmp-tray cmp-attn" data-tone="error" role="alert" aria-label="Model not available" data-testid="policy-tray" data-code={code}>
      <strong data-testid="policy-title">{model} is not available to you</strong>
      <p data-testid="policy-reason">{reason}</p>
      <p className="meta">
        Cause: <span data-testid="policy-code">{code.toLowerCase().replace(/_/g, " ")}</span>. Your message is still in the box and has not been sent.
      </p>
      <div className="cmp-actions">
        <Button size="sm" variant="primary" onClick={onAuto} data-testid="policy-auto">
          Switch to Auto
        </Button>
        <Button size="sm" onClick={onCopy} data-testid="policy-copy">
          Copy the policy rule for its owner
        </Button>
        <Button size="sm" variant="ghost" onClick={onDismiss} data-testid="policy-dismiss">
          Dismiss
        </Button>
        {note && (
          <span className="meta" role="status">
            {note}
          </span>
        )}
      </div>
    </section>
  );
}

// ------------------------------------------------------------- terminals

export function TerminalsChip({ count, open, onToggle }: { count: number; open: boolean; onToggle: () => void }) {
  if (count === 0) return null;
  return (
    <button type="button" className="cmp-chip" aria-expanded={open} onClick={onToggle} data-testid="terminals-chip" data-count={count}>
      {count} background {count === 1 ? "terminal" : "terminals"}
    </button>
  );
}

export function TerminalsTray({ terminals, onOpen, onKill, killing }: { terminals: readonly TerminalViewJson[]; onOpen: (terminalId: string) => void; onKill: (terminalId: string) => Promise<void>; killing: string | null }) {
  const [now, setNow] = useState(() => Date.now());
  const [confirm, setConfirm] = useState<string | null>(null);
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(t);
  }, []);
  return (
    <section className="cmp-tray" aria-label="Background terminals" data-testid="terminals-tray">
      <ul className="cmp-qlist">
        {terminals.map((t) => (
          <li key={t.terminalId} className="cmp-qrow" data-testid="terminal-tray-row" data-terminal-id={t.terminalId}>
            <span className="cmp-qmain cmp-static">
              <span className="cmp-qline" data-testid="terminal-tray-title">
                {t.title || "terminal"}
              </span>
              <Badge tone="neutral">{t.state}</Badge>
              <span className="meta" data-testid="terminal-tray-timer">
                {elapsedLabel(t.startedAtMs, now)}
              </span>
            </span>
            <span className="cmp-qactions">
              <Button size="sm" onClick={() => onOpen(t.terminalId)} data-testid="terminal-tray-open">
                Open in Terminal
              </Button>
              {confirm === t.terminalId ? (
                <>
                  <Button size="sm" variant="danger" disabled={killing === t.terminalId} onClick={() => void onKill(t.terminalId).then(() => setConfirm(null))} data-testid="terminal-tray-kill-confirm">
                    Stop it
                  </Button>
                  <Button size="sm" onClick={() => setConfirm(null)} data-testid="terminal-tray-kill-cancel">
                    Keep running
                  </Button>
                </>
              ) : (
                <Button size="sm" variant="danger" onClick={() => setConfirm(t.terminalId)} aria-label={`Stop ${t.title || "terminal"}`} data-testid="terminal-tray-kill">
                  Kill
                </Button>
              )}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

// ---------------------------------------------------------- side question

export function SideQuestionPanel({ onAsk, onAdd, onClose }: { onAsk: (q: string) => Promise<{ text: string; tokens: string } | string>; onAdd: (text: string) => void; onClose: () => void }) {
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [answer, setAnswer] = useState<{ text: string; tokens: string } | null>(null);
  const [error, setError] = useState("");
  const ask = async () => {
    if (!q.trim() || busy) return;
    setBusy(true);
    setError("");
    const r = await onAsk(q.trim());
    setBusy(false);
    if (typeof r === "string") setError(r);
    else setAnswer(r);
  };
  return (
    <section className="cmp-tray" aria-label="Side question" data-testid="side-panel">
      <header className="cmp-tray-head">
        <strong>Side question</strong>
        <span className="meta">Answered from a snapshot of this task. It is not added to the conversation.</span>
        <span className="cmp-spacer" />
        <Button size="sm" variant="ghost" onClick={onClose} data-testid="side-close">
          Close
        </Button>
      </header>
      <div className="cmp-side-row">
        <input
          aria-label="Your side question"
          value={q}
          onChange={(e) => setQ(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              void ask();
            }
          }}
          data-testid="side-input"
        />
        <Button size="sm" variant="primary" disabled={busy || !q.trim()} onClick={() => void ask()} data-testid="side-ask">
          {busy ? "Asking…" : "Ask"}
        </Button>
      </div>
      {error && (
        <p className="cmp-reason" role="alert" data-testid="side-error">
          {error}
        </p>
      )}
      {answer && (
        <div data-testid="side-answer-box" role="status">
          <p className="cmp-side-answer" data-testid="side-answer">
            {answer.text}
          </p>
          <Button size="sm" onClick={() => onAdd(answer.text)} data-testid="side-add">
            Add to my message
          </Button>
        </div>
      )}
    </section>
  );
}

// ------------------------------------------------------- send behaviour

const WHILE_LABEL: Record<string, string> = { QUEUE: "Queue", COLLECT: "Collect", STEER: "Steer", STOP_AND_SEND: "Stop and send" };
const NOW_LABEL: Record<string, string> = { INTERRUPT: "Interrupt", STEER: "Steer" };

export function SendBehaviorPanel({ behavior, onSet, onClose }: { behavior: SendBehaviorState; onSet: (whileRunning: string, sendNow: string) => Promise<void>; onClose: () => void }) {
  return (
    <section className="cmp-tray" aria-label="Send behavior" data-testid="behavior-panel">
      <header className="cmp-tray-head">
        <strong>What Enter does while a turn runs</strong>
        <span className="cmp-spacer" />
        <Button size="sm" variant="ghost" onClick={onClose} data-testid="behavior-close">
          Close
        </Button>
      </header>
      <fieldset className="cmp-fieldset">
        <legend className="meta">Plain Enter during a turn</legend>
        {behavior.whileRunningValues.map((v) => (
          <label key={v} className="cmp-radio">
            <input type="radio" name="while-running" checked={behavior.whileRunning === v} onChange={() => void onSet(v, "")} data-testid={`behavior-while-${v.toLowerCase()}`} />
            <span>{WHILE_LABEL[v] ?? v}</span>
          </label>
        ))}
        <p className="meta" data-testid="behavior-while-consequence">
          {behavior.whileRunningConsequence}
        </p>
      </fieldset>
      <fieldset className="cmp-fieldset">
        <legend className="meta">Send now on a queued message</legend>
        {behavior.sendNowValues.map((v) => (
          <label key={v} className="cmp-radio">
            <input type="radio" name="send-now" checked={behavior.sendNow === v} onChange={() => void onSet("", v)} data-testid={`behavior-now-${v.toLowerCase()}`} />
            <span>{NOW_LABEL[v] ?? v}</span>
          </label>
        ))}
        <p className="meta" data-testid="behavior-now-consequence">
          {behavior.sendNowConsequence}
        </p>
      </fieldset>
    </section>
  );
}
