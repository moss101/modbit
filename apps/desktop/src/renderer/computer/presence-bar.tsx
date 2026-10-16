/**
 * Presence while native computer control is in use (PX-074; docs/66 CUC-H01,
 * CUC-D05): who holds the machine, which application, which task, and a Stop
 * that is always on screen and one click or one chord (mod+shift+S) away.
 * Everything shown is the Core's answer to the last read; the renderer holds no
 * state about control. State is said in words and drawn with a glyph, never
 * colour alone. Stop is a typed preload call to the Core's `ComputerStop`: the
 * Core stops the actuator at once and the stop is sticky for the run. A live
 * region announces control acquired, paused, stopped and released.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Button, Kbd, StatusDot } from "@modbit/ui";
import type { ComputerPresenceEntry, ComputerPresenceInfo } from "../../shared/computer-types.ts";
import { STATE_WORDS, announce, holdsMachine, scopeWords } from "./model.ts";

export const STOP_CHORD = "mod+shift+s";

/** The Stop calls, shared by the bar's buttons and the shell's chord. */
export function useComputerStop(sessionId: string | null, refresh: () => Promise<void>): { stop: (taskId: string) => Promise<void>; stopAll: (entries: readonly ComputerPresenceEntry[]) => Promise<void>; busy: boolean; note: string } {
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const stop = useCallback(
    async (taskId: string): Promise<void> => {
      if (!sessionId) return;
      setBusy(true);
      try {
        const r = await window.modbit.computerStop(sessionId, taskId, "on-screen Stop");
        setNote(r.actuatorStopped ? "Stopped. The helper stopped sending input at once, and control stays stopped until the next turn." : "Stopped. Control stays stopped until the next turn.");
      } catch (e) {
        setNote(`Stop was not accepted: ${(e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "")}`);
      } finally {
        setBusy(false);
        await refresh();
      }
    },
    [sessionId, refresh],
  );
  const stopAll = useCallback(
    async (entries: readonly ComputerPresenceEntry[]): Promise<void> => {
      // Every task that holds the machine: a chord that leaves one running would lie about what it stopped.
      await Promise.all(entries.filter(holdsMachine).map((e) => stop(e.taskId)));
    },
    [stop],
  );
  return { stop, stopAll, busy, note };
}

export interface PresenceBarProps {
  info: ComputerPresenceInfo | null;
  titleOf: (taskId: string) => string;
  onOpenTask: (taskId: string) => void;
  onStop: (taskId: string) => void;
  busy: boolean;
  note: string;
  /** The landmark's name; only the gallery, which shows several bars at once, names them apart. */
  label?: string;
}

/** The live region and the bar. The region is always in the document so a change is announced; the bar exists only while the Core says something is held, stopped or unresolved. */
export function PresenceBar({ info, titleOf, onOpenTask, onStop, busy, note, label = "Computer control" }: PresenceBarProps) {
  const entries = info?.controls ?? [];
  const prev = useRef<readonly ComputerPresenceEntry[]>([]);
  const [said, setSaid] = useState("");
  useEffect(() => {
    const text = announce(prev.current, entries, titleOf);
    prev.current = entries;
    if (text) setSaid(text);
    // `entries` is the Core's list; the announcement follows its changes only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [info]);
  return (
    <>
      <p className="mb-sr-only" role="status" aria-live="polite" data-testid="computer-live">
        {said}
      </p>
      {entries.length > 0 && (
        <section className="cc-bar" aria-label={label} data-testid="computer-presence" data-held={entries.some(holdsMachine)}>
          {entries.map((e, i) => {
            const w = STATE_WORDS[e.state];
            const holds = holdsMachine(e);
            const task = titleOf(e.taskId);
            return (
              <div key={e.taskId} className="cc-entry" data-testid="computer-presence-entry" data-task-id={e.taskId} data-state={e.state} data-scope={e.scope} data-application={e.application}>
                <StatusDot status={e.state === "CONTROLLING" ? "running" : e.state === "PARKED" ? "info" : e.state === "LATCHED" ? "warn" : "danger"} label={w.label} showLabel />
                <span className="cc-what" data-testid="computer-presence-text">
                  {holds ? (
                    <>
                      The agent <q>{task}</q> is operating {e.application ? <strong>{e.application}</strong> : "an application"}
                      {scopeWords(e.scope) ? `, ${scopeWords(e.scope)}` : ""}.
                    </>
                  ) : (
                    <>
                      <q>{task}</q>: {w.detail}
                    </>
                  )}
                </span>
                <span className="cc-spacer" />
                <button type="button" className="agents-link" onClick={() => onOpenTask(e.taskId)} data-testid="computer-presence-open">
                  Open task
                </button>
                {holds && (
                  <Button size="sm" variant="danger" disabled={busy} onClick={() => onStop(e.taskId)} data-testid="computer-stop" aria-label={`Stop computer control by ${task}`}>
                    Stop {i === 0 && <Kbd chord={STOP_CHORD} />}
                  </Button>
                )}
              </div>
            );
          })}
          {note && (
            <p className="meta cc-note" role="status" data-testid="computer-stop-note">
              {note}
            </p>
          )}
        </section>
      )}
    </>
  );
}
