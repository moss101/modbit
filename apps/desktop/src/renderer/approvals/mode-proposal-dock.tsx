/**
 * The agent's mode-switch suggestion (REQ-PX-055; docs/65 AFW-D06), docked in
 * the tray host above the composer. It is a typed question card with Switch and
 * Skip. The suggestion is data the Core recorded: nothing changes unless the
 * person presses Switch, the Core refuses a late press, and a suggestion nobody
 * answers is skipped by the Core after 15 seconds (never accepted). The agent's
 * words are shown as quoted text, not as an instruction. No key accepts it: both
 * choices are buttons, reachable by keyboard like every control.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Badge, Button, trays } from "@modbit/ui";
import type { ModeProposalInfo } from "../../shared/control-types.ts";
import { latestSettled, modeWords, pendingOf, PROPOSAL_WINDOW_S, proposalRefusalWords, proposalTitle, secondsLeft, settledWords } from "./proposal-model.ts";
import { useModeProposals } from "./use-mode-proposals.ts";

const TRAY_ID = "mode-proposal";

interface CardProps {
  proposal: ModeProposalInfo;
  /** The Core's clock minus this machine's, read when the card re-renders (the tray is not re-presented for it). */
  skew: () => number;
  busy: boolean;
  note: string | null;
  onSwitch: () => void;
  onSkip: () => void;
}

function ProposalCard({ proposal, skew, busy, note, onSwitch, onSkip }: CardProps) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(() => setNow(Date.now()), 500);
    return () => clearInterval(t);
  }, []);
  const left = secondsLeft(proposal, now, skew());
  return (
    <div className="appr" role="group" aria-label="The agent suggests a mode switch" data-testid="mode-proposal-card" data-proposal-id={proposal.proposalId} data-from-mode={proposal.fromMode} data-to-mode={proposal.toMode}>
      <h2 className="appr-title" data-testid="mode-proposal-title">
        {proposalTitle(proposal)}
        <Badge tone="warn">
          {modeWords(proposal.fromMode)} to {modeWords(proposal.toMode)}
        </Badge>
      </h2>
      {proposal.reason && (
        <p className="appr-why" data-testid="mode-proposal-reason">
          The agent says: <q>{proposal.reason}</q>
        </p>
      )}
      <p className="appr-fine" data-testid="mode-proposal-clock" data-seconds-left={left}>
        This is only a suggestion; the mode stays {modeWords(proposal.fromMode)} unless you switch. If you do not answer in {PROPOSAL_WINDOW_S} seconds it is skipped{left > 0 ? ` (${left} s left)` : ""}.
      </p>
      <div className="appr-foot">
        <Button size="sm" variant="primary" disabled={busy} onClick={onSwitch} data-testid="mode-proposal-switch">
          Switch to {modeWords(proposal.toMode)}
        </Button>
        <Button size="sm" disabled={busy} onClick={onSkip} data-testid="mode-proposal-skip">
          Skip
        </Button>
      </div>
      {note && (
        <p className="appr-note" role="alert" data-testid="mode-proposal-refused">
          {note}
        </p>
      )}
    </div>
  );
}

export interface ModeProposalDockProps {
  sessionId: string | null;
  taskId: string;
  connected: boolean;
}

export function ModeProposalDock({ sessionId, taskId, connected }: ModeProposalDockProps) {
  const { list, skewMs, refresh } = useModeProposals(taskId, connected);
  const [busy, setBusy] = useState(false);
  const [note, setNote] = useState<string | null>(null);
  const [dismissed, setDismissed] = useState<string | null>(null);
  const pending = list ? pendingOf(list.proposals) : null;
  const settled = latestSettled(list);
  const pendingRef = useRef(pending);
  pendingRef.current = pending;
  const skewRef = useRef(skewMs);
  skewRef.current = skewMs;
  const skew = useCallback(() => skewRef.current, []);

  const decide = useCallback(
    async (accept: boolean) => {
      const p = pendingRef.current;
      if (!sessionId || !p) return;
      setBusy(true);
      setNote(null);
      try {
        await window.modbit.decideModeProposal(sessionId, taskId, p.proposalId, accept);
      } catch (e) {
        setNote(proposalRefusalWords((e as Error).message));
      } finally {
        setBusy(false);
        refresh();
      }
    },
    [sessionId, taskId, refresh],
  );

  const pendingId = pending?.proposalId ?? null;
  useEffect(() => {
    if (!pending) {
      trays.dismiss(TRAY_ID);
      return;
    }
    trays.present({
      id: TRAY_ID,
      tone: "attention",
      title: proposalTitle(pending),
      priority: 90,
      dismissible: false,
      body: <ProposalCard proposal={pending} skew={skew} busy={busy} note={note} onSwitch={() => void decide(true)} onSkip={() => void decide(false)} />,
    });
    // The card is re-presented when the proposal, the busy flag or the note changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pendingId, busy, note, skew, decide]);
  useEffect(() => () => trays.dismiss(TRAY_ID), []);
  useEffect(() => {
    setNote(null);
    setDismissed(null);
  }, [taskId]);

  if (!settled || dismissed === settled.proposalId) return null;
  const words = settledWords(settled);
  return (
    <div className="meta appr-last" role="status" data-testid="mode-proposal-result" data-outcome={settled.status} data-reason={settled.outcomeReason} data-proposal-id={settled.proposalId}>
      <strong>{words.headline}</strong> {words.detail}{" "}
      <Button size="sm" variant="ghost" onClick={() => setDismissed(settled.proposalId)} data-testid="mode-proposal-result-dismiss">
        Dismiss
      </Button>
    </div>
  );
}
