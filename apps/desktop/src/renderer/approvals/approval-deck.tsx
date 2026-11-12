/**
 * The approval stack's card and deck (REQ-PX-058, REQ-PX-200; docs/65 AFW-F01..F13; DR-PX-2026-11-11-021 MWS-08), docked
 * in the tray host. The card says what the effect will do in plain words, shows the exact command and why the Core asked
 * (its typed reason), then Skip, Always allow and Allow once. Always allow opens one menu holding the prefix and the place a
 * rule would apply. The tool's own name, the exact arguments and the intent hash a decision must name sit behind one
 * disclosure; closed, they stay in the accessibility tree. Decisions are typed preload calls made by the container; this
 * component draws and reads the keys. Nothing here approves anything by itself, and a decision is bound to the hash of the
 * card on screen.
 */
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Badge, Button, Icon, Kbd, useKeyScope, useLayer, useOutsidePress } from "@modbit/ui";
import type { KeyScope } from "@modbit/ui/logic";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { ComputerApprovalBody } from "../computer/approval-body.tsx";
import { approvalTitle } from "../computer/model.ts";
import { alwaysAvailable, askClassLabel, cardKey, commandLine, effectWords, humanTitle, isIrreversible, prefixLadder, prefixWords, SCOPE_WORDS, stepIndex, whyAsked, type CardKey, type KeyFacts } from "./model.ts";

export interface DeckProps {
  approvals: readonly DockApprovalView[];
  index: number;
  onIndex: (i: number) => void;
  busy: boolean;
  /** The second step of an irreversible effect is showing. */
  confirming: boolean;
  /** The prefix chosen for "always allow" (a ladder of the command's own words). */
  prefix: string[];
  onPrefix: (p: string[]) => void;
  /** Where a rule made from the card applies. */
  scope: "TASK" | "REPO";
  onScope: (s: "TASK" | "REPO") => void;
  /** What the last decision attempt said (a refusal in the Core's words, or a receipt). */
  note: { kind: "ok" | "refused"; text: string } | null;
  onRun: () => void;
  onConfirm: () => void;
  onBack: () => void;
  onSkip: () => void;
  onAlways: () => void;
  onChangeMode: () => void;
  /** The conversation that is open: approvals of other agents are marked and are decided with the pointer. */
  openTaskId: string;
  titleOf: (taskId: string) => string;
  onOpenTask: (taskId: string) => void;
}

const isEditable = (el: Element | null): boolean => !!el && (el.tagName === "TEXTAREA" || (el.tagName === "INPUT" && !["button", "checkbox", "radio", "submit"].includes((el as HTMLInputElement).type)) || (el as HTMLElement).isContentEditable);
const isControl = (el: Element | null): boolean => !!el && (el.tagName === "BUTTON" || el.tagName === "SELECT" || el.tagName === "A" || el.tagName === "SUMMARY" || (el.tagName === "INPUT" && ["button", "checkbox", "radio", "submit"].includes((el as HTMLInputElement).type)));

export function ApprovalDeck(props: DeckProps) {
  const { approvals, index, busy, confirming } = props;
  const a = approvals[index];
  const root = useRef<HTMLDivElement>(null);
  const latest = useRef(props);
  latest.current = props;
  const alwaysOk = a ? alwaysAvailable(a).ok : false;

  // The active tray owns Enter, Shift+Enter and Escape for the card; a person typing elsewhere decides nothing (AFW-F04).
  const scope = useMemo<KeyScope>(
    () => ({
      id: "approval-card",
      blocking: false,
      resolve: (e) => {
        const p = latest.current;
        const cur = p.approvals[p.index];
        if (!cur || p.busy) return null;
        const active = document.activeElement;
        const facts: KeyFacts = {
          key: e.key,
          shiftKey: e.shiftKey,
          ctrlKey: e.ctrlKey,
          metaKey: e.metaKey,
          altKey: e.altKey,
          focus: isEditable(active) ? "editable" : isControl(active) ? "control" : "neutral",
          inCard: !!root.current && !!active && root.current.contains(active),
          confirming: p.confirming,
          alwaysOffered: alwaysAvailable(cur).ok,
          foreign: cur.taskId !== p.openTaskId,
        };
        const k: CardKey | null = cardKey(facts);
        if (!k) return null;
        return () => {
          if (k === "run") p.onRun();
          else if (k === "confirm") p.onConfirm();
          else if (k === "back") p.onBack();
          else if (k === "skip") p.onSkip();
          else p.onAlways();
        };
      },
    }),
    [],
  );
  useKeyScope(approvals.length > 0, scope);

  // The one disclosure and the one menu belong to the card on screen: another card starts with both closed.
  const exactId = useId();
  const alwaysId = useId();
  const [exactOpen, setExactOpen] = useState(false);
  const [alwaysOpen, setAlwaysOpen] = useState(false);
  const alwaysTrigger = useRef<HTMLButtonElement>(null);
  const alwaysPop = useRef<HTMLDivElement>(null);
  const chosenPrefix = useRef<HTMLInputElement>(null);
  const closeAlways = (restoreFocus: boolean) => {
    setAlwaysOpen(false);
    if (restoreFocus) alwaysTrigger.current?.focus({ preventScroll: true });
  };
  const shownId = a?.approvalId ?? null;
  useEffect(() => {
    setExactOpen(false);
    setAlwaysOpen(false);
  }, [shownId]);
  useEffect(() => {
    if (confirming || busy) setAlwaysOpen(false);
  }, [confirming, busy]);
  // A layer: Escape closes the menu and nothing else, before the card's key scope can read it as Skip.
  useLayer(alwaysOpen, "approval-always", false, () => closeAlways(true));
  useOutsidePress(alwaysPop, alwaysOpen, () => closeAlways(false), alwaysTrigger);
  useEffect(() => {
    if (alwaysOpen) chosenPrefix.current?.focus({ preventScroll: true });
  }, [alwaysOpen]);

  if (!a) return null;
  const why = whyAsked(a.reason);
  const foreign = a.taskId !== props.openTaskId;
  const irreversible = isIrreversible(a.effectClass);
  // PX-074: a native-control card has its own body and offers no rule of any kind.
  const cc = a.computer ?? null;
  const ladder = !cc && a.intent.argv ? prefixLadder(a.intent.argv) : [];
  const prefixKey = props.prefix.join("\u0000");
  const n = approvals.length;
  const command = a.intent.argv ? commandLine(a.intent.argv) : a.intent.command;
  return (
    <div ref={root} className="appr" tabIndex={-1} role="group" aria-label={`Approval request ${index + 1} of ${n}`} data-testid="approval-card" data-approval-id={a.approvalId} data-intent-hash={a.intentHash} data-effect-class={a.effectClass} data-ask-reason={a.reason.code}>
      <div className={n > 1 ? "appr-deckbar" : "appr-deckbar mb-sr-only"} data-testid="approval-deck">
        <span data-testid="approval-count" aria-live="polite">
          {n} pending{n > 1 ? `, showing ${index + 1} of ${n}` : ""}
        </span>
        <span className="appr-spacer" />
        {n > 1 && (
          <>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => props.onIndex(stepIndex(index, n, -1))} data-testid="approval-prev">
              Previous
            </Button>
            <Button size="sm" variant="ghost" disabled={busy} onClick={() => props.onIndex(stepIndex(index, n, 1))} data-testid="approval-next">
              Next
            </Button>
          </>
        )}
      </div>
      {foreign && (
        <p className="appr-fine" data-testid="approval-foreign" data-task-id={a.taskId}>
          From another agent: “{props.titleOf(a.taskId)}”. Decide it here with the buttons, or{" "}
          <button type="button" className="agents-link" onClick={() => props.onOpenTask(a.taskId)} data-testid="approval-open-task">
            open that task
          </button>
          .
        </p>
      )}
      {/* The headline is what will happen, in words; the tool's own name is behind the disclosure. */}
      <h2 className="appr-title" data-testid="approval-title">
        <span className="appr-title-words">{cc ? approvalTitle(cc.tool, cc.kind) : `${humanTitle(a.toolName)}?`}</span>
        <Badge tone={irreversible ? "danger" : "warn"}>{effectWords(a.effectClass)}</Badge>
      </h2>
      {cc && <ComputerApprovalBody taskId={a.taskId} intent={cc} expiresAtMs={a.expiresAtMs} />}
      {!cc && (
        <div className="appr-body">
          {command && (
            <p className="appr-command-line">
              <span className="mb-sr-only">Command: </span>
              <code className="appr-command" data-testid="approval-command">
                {command}
              </code>
            </p>
          )}
          <dl className="appr-intent" data-testid="approval-intent">
            {a.intent.paths.length > 0 && (
              <>
                <dt>Paths</dt>
                <dd data-testid="approval-paths">{a.intent.paths.join(", ")}</dd>
              </>
            )}
            {a.intent.hosts.length > 0 && (
              <>
                <dt>Contacts</dt>
                <dd data-testid="approval-hosts">{a.intent.hosts.join(", ")}</dd>
              </>
            )}
            {a.intent.escalation && a.intent.escalation !== "none" && (
              <>
                <dt>Asks for</dt>
                <dd data-testid="approval-escalation">more access: {a.intent.escalation}</dd>
              </>
            )}
            {a.reason.askClasses.length > 0 && (
              <>
                <dt>Always asks</dt>
                <dd data-testid="approval-classes">{a.reason.askClasses.map(askClassLabel).join(", ")}</dd>
              </>
            )}
          </dl>
          <p className="appr-why" data-testid="approval-why">
            {why.headline} <span className="mb-sr-only">{why.detail}</span>
            {a.reason.runMode && (
              <>
                {" "}
                <button type="button" className="agents-link" onClick={props.onChangeMode} data-testid="approval-change-mode">
                  Change run mode
                </button>
              </>
            )}
          </p>
        </div>
      )}
      {/* One disclosure for the exact intent. Closed, its content is clipped from sight and stays in the accessibility tree: the decision names this hash. */}
      <div className="appr-exact-wrap">
        <button type="button" className="appr-disc" aria-expanded={exactOpen} aria-controls={exactId} onClick={() => setExactOpen((o) => !o)} data-testid="approval-args-toggle">
          <Icon name={exactOpen ? "chevron-down" : "chevron-right"} size={14} />
          {cc ? "Exact intent" : `Exact arguments and intent${a.intent.truncated ? " (long values clipped)" : ""}`}
        </button>
        <div id={exactId} className={exactOpen ? "appr-exact" : "appr-exact mb-sr-only"} data-testid="approval-exact" data-open={exactOpen}>
          <dl className="appr-exact-list">
            <dt>Tool</dt>
            <dd>
              <code data-testid="approval-tool">{a.toolName}</code>
            </dd>
            <dt>Intent</dt>
            <dd>
              <code data-testid="approval-intent-hash" title={a.intentHash}>
                {a.intentHash.slice(0, 16)}
              </code>{" "}
              <span className="mb-sr-only">{cc ? "Approving it once approves exactly this; the same action asked again asks again." : "Allowing it once runs exactly this; the same command asked again asks again."}</span>
            </dd>
            {!cc && a.intent.cwd && (
              <>
                <dt>Working directory</dt>
                <dd data-testid="approval-cwd">{a.intent.cwd}</dd>
              </>
            )}
            <dt>Effect</dt>
            <dd data-testid="approval-effect">
              {effectWords(a.effectClass)}
              {cc ? "" : a.reason.contained ? ", contained in a sandbox" : ", not sandboxed"}
            </dd>
            {!cc && a.reason.runMode && (
              <>
                <dt>Run mode</dt>
                <dd data-testid="approval-mode">{a.reason.runMode.replace(/_/g, " ").toLowerCase()}</dd>
              </>
            )}
          </dl>
          {!cc && !a.intent.found && <p data-testid="approval-no-args">The call's arguments could not be read from the Core's record. The decision is still bound to the intent hash above.</p>}
          {!cc && a.intent.found && (
            <pre data-testid="approval-args" tabIndex={exactOpen ? 0 : undefined}>
              {a.intent.argsPreview}
            </pre>
          )}
        </div>
      </div>
      {props.note && (
        <p className="appr-note" role="status" data-testid="approval-note" data-kind={props.note.kind}>
          {props.note.text}
        </p>
      )}
      {confirming ? (
        <div className="appr-foot" data-testid="approval-confirm">
          <p className="appr-warn">{cc ? (cc.kind === "observation_grant" ? `Let the agent observe ${cc.applicationName || "this application"}?` : `This input cannot be taken back. Let the agent do it in ${cc.applicationName || "this application"}?`) : "This effect cannot be undone. Run it?"}</p>
          <span className="appr-spacer" />
          <Button size="sm" onClick={props.onBack} disabled={busy} data-testid="approval-back">
            Back <Kbd chord="esc" />
          </Button>
          <Button size="sm" variant="danger" onClick={props.onConfirm} disabled={busy} data-testid="approval-confirm-run">
            Confirm run <Kbd chord="enter" />
          </Button>
        </div>
      ) : (
        <div className="appr-foot">
          <Button size="sm" variant="ghost" onClick={props.onSkip} disabled={busy} data-testid="approval-skip" aria-label="Skip this effect (it does not run)">
            Skip <Kbd chord="esc" />
          </Button>
          <span className="appr-spacer" />
          {alwaysOk && ladder.length > 0 && (
            <span className="appr-always-host">
              <Button ref={alwaysTrigger} size="sm" aria-haspopup="dialog" aria-expanded={alwaysOpen} aria-controls={alwaysOpen ? alwaysId : undefined} onClick={() => setAlwaysOpen((o) => !o)} disabled={busy} data-testid="approval-always">
                Always allow <Icon name="chevron-down" size={12} />
              </Button>
              {alwaysOpen && (
                <div ref={alwaysPop} id={alwaysId} className="appr-always" role="dialog" aria-label="Always allow: which commands, and where" data-testid="approval-always-menu">
                  <fieldset className="appr-always-group" data-testid="approval-prefix" data-value={commandLine(props.prefix)}>
                    <legend>Allow without asking</legend>
                    {ladder.map((p, i) => {
                      const key = p.join("\u0000");
                      const words = prefixWords(p, a.intent.argv ?? []);
                      return (
                        <label key={key} className="appr-always-item" data-checked={key === prefixKey}>
                          <input ref={key === prefixKey ? chosenPrefix : undefined} type="radio" name={`${alwaysId}-prefix`} value={key} checked={key === prefixKey} onChange={() => props.onPrefix(p)} disabled={busy} data-testid="approval-prefix-option" data-prefix={commandLine(p)} data-index={i} />
                          <span className="appr-always-a">{commandLine(p)}</span>
                          <span className="appr-always-b">{words}</span>
                        </label>
                      );
                    })}
                  </fieldset>
                  <fieldset className="appr-always-group" data-testid="approval-scope" data-value={props.scope}>
                    <legend>Where</legend>
                    {(["REPO", "TASK"] as const).map((sc) => (
                      <label key={sc} className="appr-always-item" data-checked={props.scope === sc}>
                        <input type="radio" name={`${alwaysId}-scope`} value={sc} checked={props.scope === sc} onChange={() => props.onScope(sc)} disabled={busy} data-testid="approval-scope-option" data-scope={sc} />
                        <span className="appr-always-a appr-always-plain">{SCOPE_WORDS[sc].label}</span>
                        <span className="appr-always-b">{SCOPE_WORDS[sc].detail}</span>
                      </label>
                    ))}
                  </fieldset>
                  <div className="appr-always-foot">
                    <Button size="sm" variant="ghost" onClick={() => closeAlways(true)} data-testid="approval-always-cancel">
                      Cancel
                    </Button>
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => {
                        closeAlways(true);
                        props.onAlways();
                      }}
                      disabled={busy || props.prefix.length === 0}
                      data-testid="approval-always-confirm"
                    >
                      Allow and run
                    </Button>
                  </div>
                </div>
              )}
            </span>
          )}
          <Button size="sm" variant="primary" onClick={props.onRun} disabled={busy} data-testid="approval-run">
            {cc ? "Approve" : "Allow once"} <Kbd chord="enter" />
          </Button>
        </div>
      )}
      {!cc && !alwaysOk && !confirming && (
        <p className="appr-fine" data-testid="approval-always-why">
          {alwaysAvailable(a).ok ? "" : (alwaysAvailable(a) as { why: string }).why}
        </p>
      )}
    </div>
  );
}
