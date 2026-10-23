/**
 * One terminal on screen (REQ-PX-048, AFW-I03): an xterm.js view attached to a
 * Core terminal session by cursor. The controller (terminal-controller.ts)
 * owns the attach, acknowledgement, reattach and lease logic; this component
 * only gives it a screen and a keyboard. xterm.js is MIT-licensed (docs/36).
 *
 * The keyboard: while the terminal has focus every key belongs to the process
 * behind it, so the shell's chords and the single-letter commands stand down.
 * Shift+Escape leaves the terminal for the toolbar (a terminal that kept Tab
 * and Escape would be a keyboard trap).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { useKeyScope } from "@modbit/ui";
import type { KeyScope } from "@modbit/ui/logic";
import type { TerminalViewJson } from "../../preload/preload.ts";
import { TerminalController, type TerminalViewState } from "./terminal-controller.ts";

const css = (name: string, fallback: string): string => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;

function themeFromTokens() {
  return { background: css("--mb-color-sunken", "#101010"), foreground: css("--mb-color-text", "#f0f0f0"), cursor: css("--mb-color-text", "#f0f0f0"), selectionBackground: css("--mb-color-selected", "#444444") };
}

export interface XtermViewProps {
  sessionId: string;
  terminal: TerminalViewJson;
  /** Where focus goes on Shift+Escape. */
  onLeave: () => void;
  onState: (s: TerminalViewState) => void;
  /** Lines the view shows apart from the output (refused input, a dropped replay window). */
  onNotice: (text: string) => void;
}

export function XtermView({ sessionId, terminal, onLeave, onState, onNotice }: XtermViewProps) {
  const host = useRef<HTMLDivElement>(null);
  // The callbacks change every render; the terminal is created once per session and reads the latest.
  const cb = useRef({ onLeave, onState, onNotice });
  cb.current = { onLeave, onState, onNotice };
  const [focused, setFocused] = useState(false);
  const [state, setState] = useState<TerminalViewState | null>(null);
  // The terminal owns the keyboard while it has focus: the shell's chords and single-letter commands stand down.
  const scope = useMemo<KeyScope>(() => ({ id: "terminal", blocking: true, resolve: () => null }), []);
  useKeyScope(focused, scope);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    const term = new Terminal({ cursorBlink: true, scrollback: 5000, fontFamily: css("--mb-font-mono", "monospace"), fontSize: 13, theme: themeFromTokens(), allowProposedApi: false, ariaLabel: "Terminal" } as ConstructorParameters<typeof Terminal>[0]);
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(el);
    const controller = new TerminalController(
      window.modbit,
      {
        write: (data, done) => term.write(data, done),
        reset: () => term.reset(),
        notice: (t) => cb.current.onNotice(t),
      },
      { sessionId, taskId: terminal.taskId, terminalId: terminal.terminalId },
      (s) => {
        setState(s);
        cb.current.onState(s);
      },
    );
    term.attachCustomKeyEventHandler((e) => {
      if (e.type === "keydown" && e.key === "Escape" && e.shiftKey) {
        cb.current.onLeave();
        return false;
      }
      return true;
    });
    const onData = term.onData((d) => void controller.input(d));
    const onBin = term.onBinary((d) => void controller.input(Uint8Array.from(d, (c) => c.charCodeAt(0))));
    const refit = () => {
      try {
        fit.fit();
      } catch {
        return; // not laid out yet
      }
      if (term.rows > 0 && term.cols > 0) void controller.resize(term.rows, term.cols);
    };
    const ro = new ResizeObserver(refit);
    ro.observe(el);
    refit();
    const themeObserver = new MutationObserver(() => {
      term.options.theme = themeFromTokens();
    });
    themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    const onFocusIn = () => setFocused(true);
    const onFocusOut = (e: FocusEvent) => {
      if (e.relatedTarget instanceof Node && el.contains(e.relatedTarget)) return;
      setFocused(false);
      void controller.release();
    };
    el.addEventListener("focusin", onFocusIn);
    el.addEventListener("focusout", onFocusOut);
    // Every key the terminal handles stays out of the app's own window handlers (Escape, Ctrl+N and the rest are the process's).
    const stop = (e: KeyboardEvent) => {
      if (!(e.shiftKey && e.key === "Escape")) e.stopPropagation();
    };
    el.addEventListener("keydown", stop);
    // The replay window starts at the oldest byte still held: a reattach or a reload shows the same output again.
    void controller.start(BigInt(terminal.oldestCursor));
    return () => {
      el.removeEventListener("focusin", onFocusIn);
      el.removeEventListener("focusout", onFocusOut);
      el.removeEventListener("keydown", stop);
      themeObserver.disconnect();
      ro.disconnect();
      onData.dispose();
      onBin.dispose();
      void controller.dispose();
      term.dispose();
    };
    // One view per terminal: a different terminal remounts this component (keyed by the caller).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [sessionId, terminal.terminalId, terminal.taskId]);

  return (
    <div className="xterm-wrap" data-testid="terminal-view" data-terminal-id={terminal.terminalId} data-phase={state?.phase ?? "idle"} data-cursor={state?.cursor.toString() ?? "0"} data-bytes={state?.bytes.toString() ?? "0"} data-lease={state?.leaseHeld ? "held" : "free"}>
      <div ref={host} className="xterm-host" data-testid="terminal-screen" />
    </div>
  );
}
