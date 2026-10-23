/**
 * The restore and redo dialog (REQ-PX-062; docs/65 AFW-G02, AFW-G03): states
 * exactly what the Core's preview says will change, asks per file about files
 * the person edited themselves, and only then restores. Restore records the
 * state it replaces first, so Redo restores exactly that. Nothing is decided
 * here: the preview, the refusals and the restore are the Core's, and a restore
 * names the hashes this dialog showed and the epoch it saw, so a dialog that
 * went stale is refused rather than applied. There is no "do not ask again":
 * a restore deletes files, so it always asks.
 */
import { useEffect, useRef, useState } from "react";
import { Button, Dialog } from "@modbit/ui";
import type { RestoreOutcome, RewindPreviewInfo } from "../../shared/control-types.ts";
import { ACTION_WORDS, allChosen, changeCounts, changedEntries, editedAndAffected, expectedHashes, keepPathsOf, newCommandId, refusalWords, restoreSentence, type KeepChoice } from "./checkpoints-model.ts";

export interface RestoreRequest {
  checkpointId: string;
  /** Where the checkpoint is, in the person's words: "before your message “…”". */
  heading: string;
  redo: boolean;
  /** The message this restore was asked from: it becomes editable in place afterwards. */
  messageRowId?: string | undefined;
  /** An edited message to send once the files are restored. */
  thenSend?: string | undefined;
}

const SHOWN_FILES = 60;

/** The Core calls the dialog makes; the gallery replaces them with fixtures, the app uses the preload bridge. */
export interface RestoreCalls {
  preview: (taskId: string, checkpointId: string) => Promise<RewindPreviewInfo>;
  restore: (sessionId: string, taskId: string, checkpointId: string, o: { expected: { path: string; contentHash: string }[]; keepPaths: string[]; redo: boolean; expectedCurrentEpoch: number; commandId: string }) => Promise<RestoreOutcome>;
}

const bridgeCalls: RestoreCalls = {
  preview: (taskId, checkpointId) => window.modbit.previewRestore(taskId, { checkpointId }),
  restore: (sessionId, taskId, checkpointId, o) => window.modbit.restoreCheckpoint(sessionId, taskId, { checkpointId }, o),
};

export function RestoreDialog({ sessionId, taskId, request, onClose, onRestored, calls = bridgeCalls }: { sessionId: string | null; taskId: string; request: RestoreRequest | null; onClose: () => void; onRestored: (outcome: RestoreOutcome, request: RestoreRequest) => void; calls?: RestoreCalls }) {
  const [preview, setPreview] = useState<RewindPreviewInfo | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [choices, setChoices] = useState<Record<string, KeepChoice | undefined>>({});
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const commandId = useRef("");

  useEffect(() => {
    setPreview(null);
    setLoadError(null);
    setChoices({});
    setError(null);
    setBusy(false);
    if (!request) return;
    commandId.current = newCommandId();
    let live = true;
    calls
      .preview(taskId, request.checkpointId)
      .then((p) => live && setPreview(p))
      .catch((e: Error) => live && setLoadError(e.message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "")));
    return () => {
      live = false;
    };
    // `calls` is fixed for the life of a dialog.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [request, taskId]);

  const open = request !== null;
  const edited = preview && !preview.refusal ? editedAndAffected(preview) : [];
  const counts = preview ? changeCounts(preview.entries) : null;
  const changed = preview ? changedEntries(preview.entries) : [];
  const nothing = preview !== null && !preview.refusal && changed.length === 0;
  const ready = preview !== null && !preview.refusal && !nothing && allChosen(edited, choices) && !busy;

  const go = async () => {
    if (!request || !preview || !sessionId) return;
    setBusy(true);
    setError(null);
    try {
      const out = await calls.restore(sessionId, taskId, request.checkpointId, { expected: expectedHashes(preview), keepPaths: keepPathsOf(choices), redo: request.redo, expectedCurrentEpoch: preview.currentEpoch, commandId: commandId.current });
      if (out.restored) onRestored(out, request);
      else setError(refusalWords(out.refusal, out.detail));
    } catch (e) {
      setError((e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, ""));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open={open} onClose={onClose} title={request?.redo ? "Redo the restore?" : "Restore files to this checkpoint?"} role="alertdialog" size="lg" testId="restore-dialog">
      <div className="ckpt-dialog" data-testid="restore-body" data-redo={request?.redo ?? false}>
        {request && (
          <p data-testid="restore-intro">
            {request.redo
              ? "This puts the workspace back to exactly the state it was in just before you restored, which undoes that restore."
              : `This puts the workspace back to how it was ${request.heading}. Files the agent changed after that point go back, and files it created are removed. The conversation itself is kept, and you can undo this with Redo.`}
          </p>
        )}
        {!preview && !loadError && (
          <p className="meta" role="status" data-testid="restore-loading">
            Asking the Core what this would change…
          </p>
        )}
        {loadError && (
          <p className="appr-note" role="alert" data-testid="restore-load-error">
            The Core could not preview this: {loadError}
          </p>
        )}
        {preview?.refusal && (
          <p className="appr-note" role="alert" data-testid="restore-refusal" data-code={preview.refusal}>
            {refusalWords(preview.refusal, preview.detail)}
          </p>
        )}
        {preview && !preview.refusal && counts && (
          <>
            <p data-testid="restore-summary" data-written={counts.written} data-deleted={counts.deleted} data-reverted={counts.reverted}>
              {restoreSentence(counts)}
            </p>
            {changed.length > 0 && (
              <ul className="ckpt-files" data-testid="restore-files" aria-label="Files this changes">
                {changed.slice(0, SHOWN_FILES).map((e) => (
                  <li key={e.path} data-testid="restore-file" data-path={e.path} data-action={e.action}>
                    <code>{e.path}</code>
                    <span className="meta">{ACTION_WORDS[e.action] ?? e.action.toLowerCase()}</span>
                  </li>
                ))}
                {changed.length > SHOWN_FILES && <li className="meta">and {changed.length - SHOWN_FILES} more</li>}
              </ul>
            )}
            {edited.length > 0 && (
              <fieldset data-testid="restore-edited">
                <legend>
                  <strong>You changed these files yourself since the agent&apos;s last turn.</strong> Choose what happens to each; nothing is overwritten until you do.
                </legend>
                {edited.map((p) => (
                  <div key={p} className="rm-rule" data-testid="restore-edited-file" data-path={p}>
                    <code>{p}</code>
                    <span className="rm-spacer" />
                    <label>
                      <input type="radio" name={`keep-${p}`} checked={choices[p] === "keep"} onChange={() => setChoices((c) => ({ ...c, [p]: "keep" }))} data-testid="restore-keep" /> Keep my version
                    </label>
                    <label>
                      <input type="radio" name={`keep-${p}`} checked={choices[p] === "overwrite"} onChange={() => setChoices((c) => ({ ...c, [p]: "overwrite" }))} data-testid="restore-overwrite" /> Replace with the checkpoint
                    </label>
                  </div>
                ))}
              </fieldset>
            )}
            {nothing && (
              <p className="meta" data-testid="restore-nothing">
                Nothing to restore.
              </p>
            )}
          </>
        )}
        {error && (
          <p className="appr-note" role="alert" data-testid="restore-error">
            {error}
          </p>
        )}
        <div className="rm-actions">
          <Button onClick={onClose} disabled={busy} data-testid="restore-cancel">
            Cancel
          </Button>
          <Button variant="danger" disabled={!ready} onClick={() => void go()} data-testid="restore-continue">
            {request?.redo ? "Redo" : "Restore files"}
          </Button>
        </div>
      </div>
    </Dialog>
  );
}
