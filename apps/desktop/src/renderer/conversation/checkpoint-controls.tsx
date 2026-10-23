/**
 * The checkpoint controls drawn in the conversation (REQ-PX-062; docs/65
 * AFW-G01..G07): the per-turn marker with Restore and Fork on the turn footer,
 * Restore and Edit on a user message, the editable message box, the Redo bar
 * and the Revert-or-Keep question when an older message is edited. Every
 * decision is made by the Core; these draw what it listed and ask the person.
 */
import { useEffect, useRef, useState } from "react";
import { Button, Dialog, Kbd } from "@modbit/ui";
import type { CheckpointInfo } from "../../shared/control-types.ts";
import type { TranscriptRowView } from "../../shared/conversation-types.ts";
import { normTurn } from "./checkpoints-model.ts";

/** What the rows need to draw the checkpoint surface and to ask for its actions. */
export interface CheckpointContext {
  /** The checkpoint that is each turn's boundary, by normalised turn id. */
  byTurn: ReadonlyMap<string, CheckpointInfo>;
  /** The checkpoint that was current when a message was sent (null for the first message). */
  before: (offset: string) => CheckpointInfo | null;
  /** Why the actions are unavailable right now ("" = available): the agent is working, or the Core is away. */
  disabled: string;
  /** Ask to restore a checkpoint; from a message, the message then becomes editable in place. */
  onRestore: (checkpoint: CheckpointInfo, heading: string, messageRowId?: string) => void;
  onFork: (turnId: string) => void;
  forking: boolean;
  /** The user message being edited in place. */
  editingRowId: string | null;
  onEdit: (rowId: string | null) => void;
  onSendEdit: (row: TranscriptRowView, text: string) => void;
}

const turnLabel = (c: CheckpointInfo): string => (c.turnOrdinal > 0 ? `turn ${c.turnOrdinal}` : "this turn");
const snippet = (t: string): string => (t.length > 48 ? `${t.slice(0, 48).trimEnd()}…` : t);

/** Restore and Edit on a user message (hover or focus reveals them). */
export function MessageActions({ row, cp }: { row: TranscriptRowView; cp: CheckpointContext }) {
  const f = row.facts.type === "user" ? row.facts : null;
  if (!f || f.source === "answer" || f.untrusted) return null;
  const before = cp.before(row.offset);
  const why = cp.disabled;
  return (
    <div className="ckpt-actions" data-hover-only="true" data-testid="message-actions">
      {before && (
        <Button size="sm" variant="ghost" disabled={why !== ""} title={why || undefined} onClick={() => cp.onRestore(before, `before your message “${snippet(row.text)}”`, row.rowId)} data-testid="restore-checkpoint" aria-label={`Restore checkpoint: files as they were before “${snippet(row.text)}”`}>
          Restore checkpoint
        </Button>
      )}
      <Button size="sm" variant="ghost" disabled={why !== ""} title={why || undefined} onClick={() => cp.onEdit(row.rowId)} data-testid="edit-message" aria-label={`Edit “${snippet(row.text)}”`}>
        Edit
      </Button>
    </div>
  );
}

/** The marker on a turn footer: a checkpoint was recorded at this turn's end; restore the files to it, or fork a new task from it. */
export function TurnMarker({ turnId, cp }: { turnId: string; cp: CheckpointContext }) {
  const c = cp.byTurn.get(normTurn(turnId));
  if (!c) return null;
  const why = cp.disabled;
  return (
    <span className="ckpt" data-testid="turn-checkpoint" data-checkpoint-id={c.checkpointId} data-turn-ordinal={c.turnOrdinal}>
      <span>
        Checkpoint · {turnLabel(c)}
        {c.name ? ` · ${c.name}` : ""}
      </span>
      <Button size="sm" variant="ghost" disabled={why !== ""} title={why || undefined} onClick={() => cp.onRestore(c, `at the end of ${turnLabel(c)}`)} data-testid="restore-turn" aria-label={`Restore files to the end of ${turnLabel(c)}`}>
        Restore files
      </Button>
      <Button size="sm" variant="ghost" disabled={cp.forking} onClick={() => cp.onFork(turnId)} data-testid="fork-turn" aria-label={`Fork a new task from the end of ${turnLabel(c)}`}>
        Fork from here
      </Button>
    </span>
  );
}

/** A message edited where it stands. It is not sent until the person sends it. */
export function EditBox({ row, cp }: { row: TranscriptRowView; cp: CheckpointContext }) {
  const [text, setText] = useState(row.text);
  const ref = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    ref.current?.focus();
  }, []);
  const send = () => {
    if (text.trim()) cp.onSendEdit(row, text.trim());
  };
  return (
    <div className="ckpt-edit" data-testid="edit-box">
      <textarea
        ref={ref}
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            e.preventDefault();
            send();
          } else if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            cp.onEdit(null);
          }
        }}
        aria-label="Edit your message"
        data-testid="edit-text"
      />
      <div className="ckpt-actions">
        <Button size="sm" variant="primary" disabled={!text.trim() || cp.disabled !== ""} onClick={send} data-testid="edit-send">
          Send <Kbd chord="mod+enter" />
        </Button>
        <Button size="sm" onClick={() => cp.onEdit(null)} data-testid="edit-cancel">
          Cancel <Kbd chord="esc" />
        </Button>
        <span className="meta">The message is not sent until you send it. Later turns stay in the log.</span>
      </div>
    </div>
  );
}

/** The Redo link after a restore. */
export function RedoBar({ checkpoint, label, busy, onRedo }: { checkpoint: CheckpointInfo; label: string; busy: boolean; onRedo: (c: CheckpointInfo) => void }) {
  return (
    <div className="ckpt-redo" role="status" data-testid="redo-bar" data-checkpoint-id={checkpoint.checkpointId}>
      <span data-testid="redo-text">{label}</span>
      <Button size="sm" variant="ghost" disabled={busy} onClick={() => onRedo(checkpoint)} data-testid="redo-checkpoint">
        Redo checkpoint
      </Button>
    </div>
  );
}

/** Editing an older message while the files differ from that message's checkpoint: revert them, or keep them. Keep is Shift+Enter. */
export function EditFilesDialog({ open, changedFiles, onRevert, onKeep, onCancel }: { open: boolean; changedFiles: number; onRevert: () => void; onKeep: () => void; onCancel: () => void }) {
  return (
    <Dialog open={open} onClose={onCancel} title="Revert the files, or keep them?" role="alertdialog" size="md" testId="edit-files-dialog">
      <div
        className="ckpt-dialog"
        onKeyDown={(e) => {
          if (e.key === "Enter" && e.shiftKey) {
            e.preventDefault();
            onKeep();
          }
        }}
      >
        <p data-testid="edit-files-text">
          The workspace has {changedFiles} {changedFiles === 1 ? "file" : "files"} that differ from how it was when you sent this message. Revert them to that checkpoint before the edited message is sent, or keep them as they are?
        </p>
        <div className="rm-actions">
          <Button onClick={onCancel} data-testid="edit-files-cancel">
            Cancel
          </Button>
          <Button onClick={onKeep} data-testid="edit-files-keep">
            Keep files <Kbd chord="shift+enter" />
          </Button>
          <Button variant="primary" onClick={onRevert} data-testid="edit-files-revert">
            Revert files
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
