/**
 * The transcript card of one native-control tool call (PX-074; docs/66
 * CUC-H01, CUC-F01, CUC-D03): a number, what was done, to which application and
 * element, the rung used, whether it was verified, the approval that bound it
 * and - for a screenshot - the masked frame by reference. A refusal is shown in
 * its typed code and plain words; an unknown outcome is shown as a latch with
 * the person's resolution action. Everything an application or the model wrote
 * (names, titles, arguments, reasons) is shown quoted as text: data, never an
 * instruction. The pixels are read through main from the Core's object store
 * after masking; nothing here can capture or input.
 */
import { useEffect, useState } from "react";
import { Button, StatusDot } from "@modbit/ui";
import type { ComputerCardView, ComputerFrameInfo } from "../../shared/computer-types.ts";
import { cardStatus, modalityWords, targetWords, toolTitle } from "./model.ts";

export interface ComputerCardContext {
  taskId: string;
  cards: ReadonlyMap<string, ComputerCardView>;
  /** tool call key -> its number among the task's computer calls. */
  ordinals: ReadonlyMap<string, number>;
  /** The person resolves a latch (a typed preload call made by the container). */
  onResolveLatch: (taskId: string) => Promise<string>;
}

const frames = new Map<string, ComputerFrameInfo | null>();

/** A masked frame: a thumbnail first, the stored frame on request. */
export function ComputerFrame({ taskId, artifactRef, expired }: { taskId: string; artifactRef: string; expired: boolean }) {
  const [thumb, setThumb] = useState<ComputerFrameInfo | null | undefined>(() => frames.get(`${artifactRef}:t`));
  const [full, setFull] = useState<ComputerFrameInfo | null | undefined>(undefined);
  const [open, setOpen] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (expired || !artifactRef || thumb !== undefined) return;
    let live = true;
    window.modbit
      .computerFrame(taskId, artifactRef, false)
      .then((f) => {
        frames.set(`${artifactRef}:t`, f);
        if (live) setThumb(f);
      })
      .catch((e: Error) => live && setError(e.message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "")));
    return () => {
      live = false;
    };
  }, [taskId, artifactRef, expired, thumb]);
  const showFull = (): void => {
    setOpen((o) => !o);
    if (full === undefined) {
      window.modbit
        .computerFrame(taskId, artifactRef, true)
        .then(setFull)
        .catch((e: Error) => setError(e.message));
    }
  };
  if (expired)
    return (
      <p className="cc-fine" data-testid="computer-frame-expired">
        The picture was removed when the task's retention window passed. The record of the observation stays.
      </p>
    );
  return (
    <figure className="cc-frame" data-testid="computer-frame" data-artifact={artifactRef.slice(0, 12)}>
      {thumb ? (
        <img src={thumb.dataUrl} alt="A screenshot of the application window the agent looked at. Secure fields are blacked out." width={thumb.width} height={thumb.height} data-testid="computer-frame-img" />
      ) : (
        <span className="cc-fine" role="status">
          {error ? `The picture could not be read: ${error}` : "Loading the picture…"}
        </span>
      )}
      <figcaption className="cc-fine">
        Masked frame <code>{artifactRef.slice(0, 12)}</code>
        {thumb && (
          <>
            {" "}
            <button type="button" className="agents-link" onClick={showFull} aria-expanded={open} data-testid="computer-frame-full">
              {open ? "Hide full size" : "Show full size"}
            </button>
          </>
        )}
      </figcaption>
      {open && full && <img className="cc-frame-full" src={full.dataUrl} alt="The stored screenshot at full size." data-testid="computer-frame-full-img" />}
    </figure>
  );
}

/** One computer tool call as a card. `card` is main's join; absent, the row's own facts are all there is. */
export function ComputerActionCard({ toolCallId, tool, state, failureCode, atMs, highlight, rowId, ctx }: { toolCallId: string; tool: string; state: string; failureCode: string; atMs: number; highlight: boolean; rowId: string; ctx: ComputerCardContext }) {
  const key = toolCallId.replace(/-/g, "").toLowerCase();
  const card: ComputerCardView = ctx.cards.get(key) ?? {
    toolCallId: key,
    tool,
    state,
    failureCode,
    role: "other",
    application: "",
    bundleId: "",
    scope: "",
    target: null,
    action: "",
    argumentsText: "",
    modality: "",
    outcome: "",
    verified: null,
    durationMs: 0,
    approval: null,
    observation: null,
    latched: false,
    userAborted: failureCode === "USER_ABORTED",
  };
  const n = ctx.ordinals.get(key) ?? 0;
  const st = cardStatus({ ...card, state: card.state || state, failureCode: card.failureCode || failureCode });
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState("");
  const target = targetWords(card);
  const resolve = (): void => {
    setBusy(true);
    ctx
      .onResolveLatch(ctx.taskId)
      .then(setNote)
      .catch((e: Error) => setNote(`Not resolved: ${e.message.replace(/^Error invoking remote method '[^']*': (Error: )?/, "")}`))
      .finally(() => setBusy(false));
  };
  return (
    <div className="conv-row conv-tool cc-card" data-testid="computer-card" data-row-id={rowId} data-kind="TOOL_CARD" data-tool-call-id={key} data-tool={tool} data-n={n} data-status={st.tone} data-state={card.state || state} data-code={card.failureCode || failureCode} data-outcome={card.outcome} data-latched={card.latched} data-highlight={highlight} data-bundle={card.bundleId}>
      <div className="cc-card-head">
        <span className="cc-num" data-testid="computer-card-number" aria-label={`Computer action ${n}`}>
          {n || "·"}
        </span>
        <strong data-testid="computer-card-title">{toolTitle(tool)}</strong>
        {card.application && (
          <span className="cc-card-app" data-testid="computer-card-app">
            in <q>{card.application}</q>
          </span>
        )}
        {target && (
          <span className="cc-card-target" data-testid="computer-card-target">
            on {target}
          </span>
        )}
        <span className="cc-spacer" />
        <span data-testid="computer-card-status" data-tone={st.tone}>
          <StatusDot status={st.tone} label={st.label} showLabel />
        </span>
      </div>
      {st.detail && (
        <p className="cc-fine" data-testid="computer-card-detail">
          {st.detail}
        </p>
      )}
      <dl className="cc-card-facts">
        {card.bundleId && (
          <>
            <dt>Application</dt>
            <dd data-testid="computer-card-bundle">
              <code>{card.bundleId}</code>
              {card.scope ? ` · ${card.scope === "SCREEN" ? "whole screen" : "this application only"}` : ""}
            </dd>
          </>
        )}
        {card.argumentsText && card.role === "act" && (
          <>
            <dt>Arguments</dt>
            <dd data-testid="computer-card-args">
              <code>{card.argumentsText}</code>
            </dd>
          </>
        )}
        {card.modality && (
          <>
            <dt>How</dt>
            <dd data-testid="computer-card-how">
              {modalityWords(card.modality)}
              {card.verified === true ? ", and the change was seen afterwards" : card.verified === false ? ", but the change was not seen afterwards" : ""}
              {card.durationMs > 0 ? ` · ${card.durationMs} ms` : ""}
            </dd>
          </>
        )}
        {card.observation && card.role === "observe" && (
          <>
            <dt>Looked at</dt>
            <dd data-testid="computer-card-observed">
              {card.observation.kind === "state" ? `${card.observation.elements} elements` : card.observation.kind}
              {card.observation.secureWithheld > 0 ? ` · ${card.observation.secureWithheld} secure field${card.observation.secureWithheld === 1 ? "" : "s"} withheld` : ""}
              {card.observation.maskedRegions > 0 ? ` · ${card.observation.maskedRegions} region${card.observation.maskedRegions === 1 ? "" : "s"} blacked out` : ""}
            </dd>
          </>
        )}
        {card.approval && (
          <>
            <dt>Approved by</dt>
            <dd data-testid="computer-card-approval" data-approval-id={card.approval.approvalId} data-intent-hash={card.approval.intentHash}>
              your approval <code>{card.approval.approvalId.slice(0, 8)}</code> of intent <code>{card.approval.intentHash.slice(0, 12)}</code> ({card.approval.status.toLowerCase()})
            </dd>
          </>
        )}
      </dl>
      {card.observation?.artifactRef && <ComputerFrame taskId={ctx.taskId} artifactRef={card.observation.artifactRef} expired={card.observation.artifactExpired} />}
      {card.latched && (
        <div className="decision cc-latch" data-testid="computer-latch">
          <span>
            Look at the application. If the input did happen, or does not matter, say so; the agent must still open a new session, look again and ask your approval before it acts.
          </span>
          <Button size="sm" variant="primary" disabled={busy} onClick={resolve} data-testid="computer-latch-resolve">
            I looked: resolve
          </Button>
        </div>
      )}
      {note && (
        <p className="cc-fine" role="status" data-testid="computer-latch-note">
          {note}
        </p>
      )}
      {atMs > 0 && <span className="mb-sr-only">{new Date(atMs).toLocaleString()}</span>}
    </div>
  );
}
