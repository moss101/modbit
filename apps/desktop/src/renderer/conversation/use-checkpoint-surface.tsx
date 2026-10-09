/**
 * The checkpoint surface's state for one open conversation (REQ-PX-062): the
 * Core's checkpoint list, the restore dialog, the message being edited in place,
 * the Revert-or-Keep question, the Redo bar, Undo all and fork. It owns no
 * authority: it asks the Core to preview, restore and fork through typed
 * preload calls and shows what the Core answers (including every typed refusal).
 */
import { useCallback, useMemo, useState, type ReactNode } from "react";
import { Button } from "@modbit/ui";
import type { CheckpointInfo, RestoreOutcome, RewindPreviewInfo } from "../../shared/control-types.ts";
import type { TranscriptRowView } from "../../shared/conversation-types.ts";
import type { TaskCard } from "../model.ts";
import { EditFilesDialog, RedoBar, type CheckpointContext } from "./checkpoint-controls.tsx";
import { byTurn, changeCounts, changedEntries, checkpointBefore, editedAndAffected, expectedHashes, newCommandId, redoCandidate, refusalWords, restoreSentence, undoAllTarget } from "./checkpoints-model.ts";
import { RestoreDialog, type RestoreRequest } from "./restore-dialog.tsx";
import { useCheckpoints } from "./use-checkpoints.ts";

export interface CheckpointSurfaceInput {
  taskId: string;
  sessionId: string | null;
  connected: boolean;
  card: TaskCard | undefined;
  onResume: (taskId: string) => void;
  onOpenTask?: ((taskId: string) => void) | undefined;
}

const clean = (m: string): string => m.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");

export function useCheckpointSurface(p: CheckpointSurfaceInput): { context: CheckpointContext; overlay: ReactNode; bar: ReactNode; head: ReactNode } {
  const { taskId, sessionId, connected, card, onResume, onOpenTask } = p;
  const { list, reload } = useCheckpoints(taskId, connected);
  const [request, setRequest] = useState<RestoreRequest | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [pendingEdit, setPendingEdit] = useState<{ row: TranscriptRowView; text: string; changed: number; before: CheckpointInfo } | null>(null);
  const [redone, setRedone] = useState<ReadonlySet<string>>(new Set());
  const [note, setNote] = useState("");
  const [forking, setForking] = useState(false);
  const [undo, setUndo] = useState<{ target: CheckpointInfo; preview: RewindPreviewInfo } | null>(null);
  const [undoBusy, setUndoBusy] = useState(false);

  const running = card?.state === "Running";
  const disabled = !connected ? "The Core is not connected." : !sessionId ? "No session is open." : running ? "The agent is working. Stop or pause it before changing files." : "";

  const startable = !!card && (card.state === "Queued" || (card.state === "Waiting" && card.waitReason !== "Capacity" && card.waitReason !== "Approval" && card.waitReason !== "UserInput"));
  const send = useCallback(
    async (text: string) => {
      if (!sessionId) return;
      try {
        await window.modbit.steerTask(sessionId, taskId, text);
        setEditing(null);
        setNote("Your edited message was sent as a new message.");
        if (startable) onResume(taskId);
      } catch (e) {
        setNote(`The Core did not take the message: ${clean((e as Error).message)}`);
      }
    },
    [sessionId, taskId, startable, onResume],
  );

  /** What a finished restore (a dialog's, or Undo all's) changes on screen. */
  const restored = useCallback(
    (out: RestoreOutcome, req: RestoreRequest) => {
      setRequest(null);
      void reload();
      const n = out.filesWritten + out.filesReverted;
      if (req.redo) {
        // The restore a redo makes is itself undoable in the Core, but the person just asked for the other direction: no bar for it.
        setRedone((s) => new Set([...s, req.checkpointId, ...(out.preRestoreCheckpointId ? [out.preRestoreCheckpointId] : [])]));
        setNote(`Redone: the files are back exactly as they were before the restore (${n} changed).`);
      } else {
        setNote(`Restored ${n} ${n === 1 ? "file" : "files"}${out.keptPaths.length ? `, kept ${out.keptPaths.length} you edited` : ""}. Redo puts them back.`);
        if (req.messageRowId) setEditing(req.messageRowId);
      }
      if (req.thenSend) void send(req.thenSend);
    },
    [reload, send],
  );

  const onSendEdit = useCallback(
    async (row: TranscriptRowView, text: string) => {
      const before = checkpointBefore(list, row.offset);
      if (before) {
        try {
          const pv = await window.modbit.previewRestore(taskId, { checkpointId: before.checkpointId });
          const n = pv.refusal ? 0 : changedEntries(pv.entries).length;
          if (n > 0) {
            setPendingEdit({ row, text, changed: n, before });
            return;
          }
        } catch {
          // Without a preview the files are kept: nothing is reverted that was not shown.
        }
      }
      await send(text);
    },
    [list, taskId, send],
  );

  const onFork = useCallback(
    async (turnId: string) => {
      if (!sessionId) return;
      setForking(true);
      try {
        const f = await window.modbit.forkFromTurn(sessionId, taskId, { turnId });
        setNote(`Forked a new task from turn ${f.turnOrdinal || "this turn"}${f.approvalsDropped ? ` (${f.approvalsDropped} pending approval${f.approvalsDropped === 1 ? "" : "s"} not carried)` : ""}.`);
        onOpenTask?.(f.taskId);
      } catch (e) {
        setNote(`The Core did not fork: ${clean((e as Error).message)}`);
      } finally {
        setForking(false);
      }
    },
    [sessionId, taskId, onOpenTask],
  );

  const context = useMemo<CheckpointContext>(
    () => ({
      byTurn: byTurn(list),
      before: (offset) => checkpointBefore(list, offset),
      disabled,
      onRestore: (c, heading, messageRowId) => setRequest({ checkpointId: c.checkpointId, heading, redo: false, messageRowId }),
      onFork: (turnId) => void onFork(turnId),
      forking,
      editingRowId: editing,
      onEdit: setEditing,
      onSendEdit: (row, text) => void onSendEdit(row, text),
    }),
    [list, disabled, forking, editing, onFork, onSendEdit],
  );

  // Undo all: the first click asks the Core what it would do and arms; the second click does it.
  const undoTarget = undoAllTarget(list);
  const armUndo = async () => {
    if (!undoTarget) return;
    setUndoBusy(true);
    try {
      const pv = await window.modbit.previewRestore(taskId, { checkpointId: undoTarget.checkpointId });
      if (pv.refusal) setNote(refusalWords(pv.refusal, pv.detail));
      else if (changedEntries(pv.entries).length === 0) setNote("Nothing in the workspace differs from how it was before the agent started.");
      else setUndo({ target: undoTarget, preview: pv });
    } catch (e) {
      setNote(`The Core could not preview this: ${clean((e as Error).message)}`);
    } finally {
      setUndoBusy(false);
    }
  };
  const confirmUndo = async () => {
    if (!undo || !sessionId) return;
    const { target, preview } = undo;
    const req: RestoreRequest = { checkpointId: target.checkpointId, heading: "before the agent changed anything", redo: false };
    // A file the person edited needs their own choice, which the dialog asks for.
    if (editedAndAffected(preview).length > 0) {
      setUndo(null);
      setRequest(req);
      return;
    }
    setUndoBusy(true);
    try {
      const out = await window.modbit.restoreCheckpoint(sessionId, taskId, { checkpointId: target.checkpointId }, { expected: expectedHashes(preview), keepPaths: [], redo: false, expectedCurrentEpoch: preview.currentEpoch, commandId: newCommandId() });
      if (out.restored) restored(out, req);
      else setNote(refusalWords(out.refusal, out.detail));
    } catch (e) {
      setNote(`The Core did not restore: ${clean((e as Error).message)}`);
    } finally {
      setUndoBusy(false);
      setUndo(null);
    }
  };
  const head = (
    <span
      className="ckpt-actions"
      data-testid="undo-all-wrap"
      onBlur={(e) => {
        if (!e.currentTarget.contains(e.relatedTarget as Node | null)) setUndo(null);
      }}
    >
      {undo ? (
        <>
          <span className="meta" data-testid="undo-all-armed" role="status">
            {restoreSentence(changeCounts(undo.preview.entries))}
          </span>
          <Button size="sm" variant="danger" onClick={() => void confirmUndo()} disabled={undoBusy} data-testid="undo-all" data-armed="true">
            Click again to undo all
          </Button>
          <Button size="sm" variant="ghost" onClick={() => setUndo(null)} data-testid="undo-all-cancel">
            Cancel
          </Button>
        </>
      ) : (
        <Button size="sm" variant="ghost" onClick={() => void armUndo()} disabled={disabled !== "" || undoTarget === null || undoBusy} title={disabled || (undoTarget === null ? "No recorded state from before the agent started changing files." : undefined)} data-testid="undo-all" data-armed="false">
          Undo all
        </Button>
      )}
    </span>
  );

  const redo = disabled === "" ? redoCandidate(list, redone) : null;
  const overlay = (
    <>
      <RestoreDialog sessionId={sessionId} taskId={taskId} request={request} onClose={() => setRequest(null)} onRestored={restored} />
      <EditFilesDialog
        open={pendingEdit !== null}
        changedFiles={pendingEdit?.changed ?? 0}
        onCancel={() => setPendingEdit(null)}
        onKeep={() => {
          const e = pendingEdit;
          setPendingEdit(null);
          if (e) void send(e.text);
        }}
        onRevert={() => {
          const e = pendingEdit;
          setPendingEdit(null);
          if (e) setRequest({ checkpointId: e.before.checkpointId, heading: `before your message “${e.row.text.length > 48 ? `${e.row.text.slice(0, 48).trimEnd()}…` : e.row.text}”`, redo: false, thenSend: e.text });
        }}
      />
    </>
  );
  const bar = (
    <>
      {redo && <RedoBar checkpoint={redo} label="Files were restored. Redo puts them back exactly as they were before the restore." busy={false} onRedo={(c) => setRequest({ checkpointId: c.checkpointId, heading: "", redo: true })} />}
      <p className="mb-sr-only" role="status" aria-live="polite" data-testid="checkpoint-live">
        {note}
      </p>
      {note && (
        <p className="meta appr-last" data-testid="checkpoint-note" aria-hidden="true">
          {note}
        </p>
      )}
    </>
  );
  return { context, overlay, bar, head };
}
