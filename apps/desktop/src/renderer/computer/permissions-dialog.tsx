/**
 * The permission window (PX-074; docs/66 CUC-E05): the minimum labelled
 * capabilities native control needs, in plain language - access to application
 * interfaces and use of screenshots as context - each shown as checking, allow
 * or ready from what the Core's actuator reports. The one-time steps are
 * explained, and a button opens the right pane of System Settings; it only
 * opens it. Modbit never turns a permission on, and a grant made in System
 * Settings is noticed on the way back (the window re-reads when it regains
 * focus). It opens once, on a blocked episode, in the root workspace window
 * only (the shell owns that routing); a person can also open it by name.
 */
import { useEffect, useRef, useState } from "react";
import { Button, Dialog, StatusDot } from "@modbit/ui";
import type { ComputerPresenceInfo, PermissionName } from "../../shared/computer-types.ts";
import { PERMISSION_DEFS, ROW_WORDS, allReady, noHelperWords, permissionStateOf, rowStatus, type RowStatus } from "./model.ts";

/** The window re-reads on its own for this long after it opens; then it says so and offers a refresh (the polling has ended). */
export const POLL_WINDOW_MS = 2 * 60 * 1000;

const TONE: Record<RowStatus, "running" | "warn" | "ok"> = { checking: "running", allow: "warn", ready: "ok" };

export interface PermissionsBodyProps {
  info: ComputerPresenceInfo | null;
  /** Re-read the Core now. */
  onCheck: () => void;
  /** Polling has ended without every permission being ready: the refresh is the person's. */
  pollingEnded: boolean;
  onOpenPane: (pane: PermissionName) => void;
  /** What the last "Open System Settings" said, per pane. */
  notes: Partial<Record<PermissionName, string>>;
}

export function PermissionsBody({ info, onCheck, pollingEnded, onOpenPane, notes }: PermissionsBodyProps) {
  const attached = !!info?.attached;
  const ready = allReady(info);
  return (
    <div className="cc-perm" data-testid="computer-permissions-body" data-ready={ready}>
      <p>Modbit can operate other applications on this computer when you ask it to. It needs two things, and only the Modbit helper gets them, not the app you are looking at. You turn each on once, in System Settings.</p>
      {!attached && (
        <p className="cc-fine" role="status" data-testid="perm-nohelper">
          {noHelperWords(info)}
        </p>
      )}
      <ul className="cc-perm-list">
        {PERMISSION_DEFS.map((d) => {
          const status = rowStatus(info ? permissionStateOf(info, d.name) : "UNKNOWN", attached);
          const w = ROW_WORDS[status];
          return (
            <li key={d.name} className="cc-perm-row" data-testid={`perm-${d.name}`} data-status={status}>
              <div className="cc-perm-head">
                <strong>{d.label}</strong>
                <span data-testid={`perm-${d.name}-status`}>
                  <StatusDot status={TONE[status]} label={w.label} showLabel />
                </span>
              </div>
              <p className="cc-fine">{d.why}</p>
              {status === "allow" && (
                <>
                  <p className="cc-fine" data-testid={`perm-${d.name}-steps`}>
                    {d.steps}
                  </p>
                  <div className="cc-perm-actions">
                    <Button size="sm" onClick={() => onOpenPane(d.name)} data-testid={`perm-open-${d.name}`}>
                      Open System Settings: {d.pane}
                    </Button>
                    {notes[d.name] && (
                      <span className="cc-fine" role="status" data-testid={`perm-${d.name}-note`}>
                        {notes[d.name]}
                      </span>
                    )}
                  </div>
                </>
              )}
              {status !== "allow" && <p className="cc-fine">{w.detail}</p>}
            </li>
          );
        })}
      </ul>
      {ready && (
        <p role="status" data-testid="perm-allready">
          Both are on. The agent can now ask to operate an application, and you approve each action.
        </p>
      )}
      <div className="cc-perm-actions">
        <Button size="sm" variant={pollingEnded && !ready ? "primary" : "secondary"} onClick={onCheck} data-testid="perm-check">
          Check again
        </Button>
        {pollingEnded && !ready && (
          <span className="cc-fine" role="status" data-testid="perm-polling-ended">
            This window stopped checking on its own. Press Check again after you turn a permission on.
          </span>
        )}
      </div>
      <p className="cc-fine">Opening System Settings changes nothing by itself. A permission stays off until you switch it on there.</p>
    </div>
  );
}

export interface PermissionsDialogProps {
  open: boolean;
  onClose: () => void;
  info: ComputerPresenceInfo | null;
  refresh: () => Promise<void>;
}

export function PermissionsDialog({ open, onClose, info, refresh }: PermissionsDialogProps) {
  const [notes, setNotes] = useState<Partial<Record<PermissionName, string>>>({});
  const [pollingEnded, setPollingEnded] = useState(false);
  const done = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    if (!open) return;
    setPollingEnded(false);
    setNotes({});
    void refresh();
    const t = setTimeout(() => setPollingEnded(true), POLL_WINDOW_MS);
    return () => clearTimeout(t);
    // The window's own clock starts when it opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);
  // Re-read on the beat while it is open (the presence hook's own beat is the clock); a manual check also restarts the window.
  const check = (): void => {
    setPollingEnded(false);
    void refresh();
  };
  const openPane = (pane: PermissionName): void => {
    window.modbit
      .computerOpenSettings(pane)
      .then((r) => setNotes((n) => ({ ...n, [pane]: r.note })))
      .catch((e: Error) => setNotes((n) => ({ ...n, [pane]: `Could not open System Settings: ${e.message}` })));
  };
  return (
    <Dialog open={open} title="Let Modbit operate other applications" onClose={onClose} testId="computer-permissions" size="md" initialFocus={done}>
      <PermissionsBody info={info} onCheck={check} pollingEnded={pollingEnded} onOpenPane={openPane} notes={notes} />
      <div className="cc-dialog-actions">
        <Button ref={done} variant="primary" onClick={onClose} data-testid="perm-done">
          Done
        </Button>
      </div>
    </Dialog>
  );
}
