import { useRef } from "react";
import type { ScreenState as DerivedScreenState } from "../screens.ts";

/** PX-023: one screen's state, with cause, next action and evidence when it
 *  is not the plain populated state. Every screen renders one. */
const KIND_MARK: Record<DerivedScreenState["kind"], string> = { empty: "○", loading: "…", populated: "●", error: "✖", degraded: "⚠", recovery: "↻" };
export function StateLine({ state, testid }: { state: DerivedScreenState; testid: string }) {
  // The kind is in the text (a mark and the word), never in colour alone.
  // `data-seen` keeps the labels this line has shown, in order: a state
  // that lasts a moment (a Core back in a second) is still on record.
  const seen = useRef<string[]>([]);
  if (seen.current[seen.current.length - 1] !== `${state.kind}:${state.label}`) seen.current = [...seen.current.slice(-19), `${state.kind}:${state.label}`];
  return (
    <div className="meta state" data-testid={testid} data-screen={state.screen} data-kind={state.kind} data-seen={seen.current.join("|")} role={state.kind === "error" ? "alert" : "status"} aria-label={`${state.kind}: ${state.label}`}>
      <span aria-hidden="true">{KIND_MARK[state.kind]} </span>
      <span className="sr-only">{state.kind}: </span>
      <span data-testid={`${testid}-label`}>{state.label}</span>
      {state.cause && (
        <>
          {" · cause: "}
          <span data-testid={`${testid}-cause`}>{state.cause}</span>
        </>
      )}
      {state.nextAction && (
        <>
          {" · next: "}
          <strong data-testid={`${testid}-next`}>{state.nextAction}</strong>
        </>
      )}
      {state.evidence && (
        <>
          {" · evidence: "}
          <span data-testid={`${testid}-evidence`}>{state.evidence}</span>
        </>
      )}
    </div>
  );
}
