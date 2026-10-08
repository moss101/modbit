/**
 * The approval stack's card and deck (REQ-PX-058; docs/65 AFW-F01..F13), docked
 * in the tray host. The card shows the exact intent: the tool, the command or
 * paths, the effect class, why the Core asked (its typed reason) and the intent
 * hash a decision must name. Decisions are typed preload calls made by the
 * container; this component draws and reads the keys. Nothing here approves
 * anything by itself, and a decision is bound to the hash of the card on screen.
 */
import { useMemo, useRef } from "react";
import { Badge, Button, Kbd, useKeyScope } from "@modbit/ui";
import type { KeyScope } from "@modbit/ui/logic";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { alwaysAvailable, askClassLabel, cardKey, commandLine, effectWords, humanTitle, isIrreversible, prefixLadder, stepIndex, whyAsked, type CardKey, type KeyFacts } from "./model.ts";

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

  if (!a) return null;
  const why = whyAsked(a.reason);
  const foreign = a.taskId !== props.openTaskId;
  const irreversible = isIrreversible(a.effectClass);
  const ladder = a.intent.argv ? prefixLadder(a.intent.argv) : [];
  const prefixKey = props.prefix.join("\u0000");
  const n = approvals.length;
  return (
    <div ref={root} className="appr" tabIndex={-1} role="group" aria-label={`Approval request ${index + 1} of ${n}`} data-testid="approval-card" data-approval-id={a.approvalId} data-intent-hash={a.intentHash} data-effect-class={a.effectClass} data-ask-reason={a.reason.code}>
      <div className="appr-deckbar" data-testid="approval-deck">
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
      <h2 className="appr-title" data-testid="approval-title">
        {humanTitle(a.toolName)} <code data-testid="approval-tool">{a.toolName}</code>
        <Badge tone={irreversible ? "danger" : "warn"}>{effectWords(a.effectClass)}</Badge>
      </h2>
      <dl className="appr-intent" data-testid="approval-intent">
        {a.intent.argv ? (
          <>
            <dt>Command</dt>
            <dd>
              <code className="appr-command" data-testid="approval-command">
                {commandLine(a.intent.argv)}
              </code>
            </dd>
          </>
        ) : a.intent.command ? (
          <>
            <dt>Command</dt>
            <dd>
              <code className="appr-command" data-testid="approval-command">
                {a.intent.command}
              </code>
            </dd>
          </>
        ) : null}
        {a.intent.cwd && (
          <>
            <dt>Working directory</dt>
            <dd data-testid="approval-cwd">{a.intent.cwd}</dd>
          </>
        )}
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
        <dt>Effect</dt>
        <dd data-testid="approval-effect">
          {effectWords(a.effectClass)}
          {a.reason.contained ? ", contained in a sandbox" : ", not sandboxed"}
        </dd>
        {a.reason.askClasses.length > 0 && (
          <>
            <dt>Always asks</dt>
            <dd data-testid="approval-classes">{a.reason.askClasses.map(askClassLabel).join(", ")}</dd>
          </>
        )}
      </dl>
      {!a.intent.found && (
        <p className="appr-fine" data-testid="approval-no-args">
          The call's arguments could not be read from the Core's record. The decision is still bound to the intent hash below.
        </p>
      )}
      {a.intent.found && (
        <details className="appr-args">
          <summary data-testid="approval-args-toggle">Exact arguments{a.intent.truncated ? " (long values clipped)" : ""}</summary>
          <pre data-testid="approval-args">{a.intent.argsPreview}</pre>
        </details>
      )}
      <p className="appr-why" data-testid="approval-why">
        <strong>{why.headline}</strong> {why.detail}
      </p>
      <p className="appr-fine">
        Intent <code data-testid="approval-intent-hash" title={a.intentHash}>{a.intentHash.slice(0, 16)}</code>. Running it once runs exactly this; the same command asked again asks again.
        {a.reason.runMode && (
          <>
            {" "}Run mode: <span data-testid="approval-mode">{a.reason.runMode.replace(/_/g, " ").toLowerCase()}</span>.{" "}
            <button type="button" className="agents-link" onClick={props.onChangeMode} data-testid="approval-change-mode">
              Change run mode
            </button>
          </>
        )}
      </p>
      {props.note && (
        <p className="appr-note" role="status" data-testid="approval-note" data-kind={props.note.kind}>
          {props.note.text}
        </p>
      )}
      {confirming ? (
        <div className="appr-foot" data-testid="approval-confirm">
          <p className="appr-warn">This effect cannot be undone. Run it?</p>
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
          <Button size="sm" onClick={props.onSkip} disabled={busy} data-testid="approval-skip" aria-label="Skip this effect (it does not run)">
            Skip <Kbd chord="esc" />
          </Button>
          <span className="appr-spacer" />
          {alwaysOk && ladder.length > 0 && (
            <>
              <label className="meta" htmlFor={`prefix-${a.approvalId}`}>
                Allow prefix
              </label>
              <select id={`prefix-${a.approvalId}`} className="appr-prefix" value={prefixKey} onChange={(e) => props.onPrefix(e.target.value.split("\u0000"))} disabled={busy} data-testid="approval-prefix">
                {ladder.map((p) => (
                  <option key={p.join("\u0000")} value={p.join("\u0000")}>
                    {commandLine(p)}
                  </option>
                ))}
              </select>
              <label className="meta" htmlFor={`scope-${a.approvalId}`}>
                in
              </label>
              <select id={`scope-${a.approvalId}`} className="appr-prefix" value={props.scope} onChange={(e) => props.onScope(e.target.value === "TASK" ? "TASK" : "REPO")} disabled={busy} data-testid="approval-scope">
                <option value="REPO">this repository</option>
                <option value="TASK">this task</option>
              </select>
              <Button size="sm" onClick={props.onAlways} disabled={busy} data-testid="approval-always">
                Always <Kbd chord="shift+enter" />
              </Button>
            </>
          )}
          <Button size="sm" variant="primary" onClick={props.onRun} disabled={busy} data-testid="approval-run">
            Run <Kbd chord="enter" />
          </Button>
        </div>
      )}
      {!alwaysOk && !confirming && (
        <p className="appr-fine" data-testid="approval-always-why">
          {alwaysAvailable(a).ok ? "" : (alwaysAvailable(a) as { why: string }).why}
        </p>
      )}
    </div>
  );
}
