import { useCallback, useEffect, useRef, useState } from "react";
import type { AccessLevel, ComputerAccessInfo } from "../../shared/computer-types.ts";
import { ACCESS_WORDS } from "./model.ts";

const plain = (e: unknown): string => (e as Error).message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "");

/** What the person is told after a change: the Core recorded it and it is in force from the agent's next action. */
export function changeWords(name: string, level: AccessLevel | "", changed: boolean): string {
  if (!changed) return level === "" ? `${name} already has no standing access set.` : `${name} is already set to ${ACCESS_WORDS[level].label.toLowerCase()}. Nothing changed.`;
  return level === ""
    ? `${name} has no standing access now: each task's own grant decides, as before. This applies from the agent's next action.`
    : `${name} is now ${ACCESS_WORDS[level].label.toLowerCase()}. This applies from the agent's next action.`;
}

/**
 * The person's standing level per application, as the Core holds it (PX-215).
 * Nothing is kept here but the last answer: a level is set by a typed Core
 * command and read back from the Core, so what the screen shows is what the
 * Core enforces. Read again when the window regains focus and after every
 * change of this window's own.
 */
export function useStandingAccess(connected: boolean): {
  info: ComputerAccessInfo | null;
  error: string;
  note: string;
  busy: boolean;
  set: (bundleId: string, name: string, level: AccessLevel | "") => Promise<boolean>;
} {
  const [info, setInfo] = useState<ComputerAccessInfo | null>(null);
  // The last change's refusal, and separately whether the last read failed: a read that succeeds never wipes the words of a refusal.
  const [changeError, setError] = useState("");
  const [readError, setReadError] = useState("");
  const [note, setNote] = useState("");
  const [busy, setBusy] = useState(false);
  const live = useRef(true);

  const refresh = useCallback(async () => {
    try {
      const v = await window.modbit.computerAccess();
      if (live.current) {
        setInfo(v);
        setReadError(v === null ? "The Core is not answering." : "");
      }
    } catch (e) {
      if (live.current) setReadError(plain(e));
    }
  }, []);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  useEffect(() => {
    if (!connected) {
      setInfo(null);
      return;
    }
    void refresh();
    // A level is a Core-wide record on the Core's own ledger, not on a session's stream: another window's change is read when this one regains focus.
    const onFocus = (): void => void refresh();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [connected, refresh]);

  const set = useCallback(
    async (bundleId: string, name: string, level: AccessLevel | ""): Promise<boolean> => {
      setBusy(true);
      setError("");
      setNote("");
      try {
        const r = await window.modbit.computerSetAccess(bundleId, name, level);
        if (live.current) setNote(changeWords(name || bundleId, r.level, r.changed));
        await refresh();
        return true;
      } catch (e) {
        if (live.current) setError(plain(e));
        await refresh();
        return false;
      } finally {
        if (live.current) setBusy(false);
      }
    },
    [refresh],
  );

  return { info, error: changeError || readError, note, busy, set };
}
