/**
 * Inbox (REQ-PX-206; DR-PX-2026-11-11-021 MWS-13, MWS-14's page shape, section 9). Two parts:
 *
 * - **Needs you**: every pending approval of the session as a card (what it will do in words, the exact command, why it asks),
 *   decided in place through the one decision path the task's card and home use (`decide.ts`), bound to the intent hash shown;
 *   what only its task may decide (an effect that cannot be undone, native control, a budget raise) says why and opens its task,
 *   where the card asks twice. Then the agent's mode proposals, answered in place through the one answer the task's card uses;
 *   then every other item the Core lists (a question, a stall, a failure …) in plain words, opening its task.
 * - **Updates**: main's persisted record (ready to review, a finished child task, an automation's run, a failure), each with its
 *   time and the Core's read state; opening one marks it read through the Core and opens its task. The person can mark all read
 *   and clear the record.
 *
 * Every count is the Core's: Needs you counts what the Core lists, Updates counts main's record; nothing is counted from the
 * renderer's task store. Nothing is listed that the Core did not list, and nothing is decided here that the Core would not take
 * from the task's own card.
 */
import { useCallback, useEffect, useState } from "react";
import { Badge, Button, Icon, IconButton } from "@modbit/ui";
import type {
  DockApprovalView,
  ModeProposalInfo,
} from "../../shared/control-types.ts";
import type { UpdateView } from "../../shared/inbox-types.ts";
import { attentionWords } from "../home/model.ts";
import type { AppState } from "../state/use-app.ts";
import { decideFromList, decideProposal } from "./decide.ts";
import {
  askWords,
  effectTag,
  exactLine,
  fullTime,
  needsYouCount,
  proposalQuestion,
  UPDATE_WORDS,
  whenWords,
  type InboxView,
} from "./inbox-model.ts";
import {
  decidableFromList,
  isIrreversible,
  refusalWords,
  whyAsked,
} from "./model.ts";
import {
  modeWords,
  proposalRefusalWords,
  secondsLeft,
} from "./proposal-model.ts";
import { useDockApprovals } from "./use-dock-approvals.ts";
import { useInboxUpdates, useSessionProposals } from "./use-inbox.ts";

export interface InboxProps {
  app: AppState;
  onOpenTask: (taskId: string) => void;
  /** Opens Settings at the notification preferences. */
  onNotificationSettings: () => void;
  /** A count that rises each time the first item should take the focus (the `a` shortcut); honoured once an item is drawn. */
  focusRequest?: number;
}

type Note = { kind: "ok" | "refused"; text: string };

export function Inbox({ app, onOpenTask, onNotificationSettings, focusRequest = 0 }: InboxProps) {
  const { attentionItems, model, core, refreshAttention } = app;
  const connected = core.state === "connected";
  const sessionId = model.sessionId;
  const titleOf = useCallback(
    (id: string) => model.tasks.get(id)?.goalText || id.slice(0, 8),
    [model],
  );
  const { approvals, refresh: refreshApprovals } = useDockApprovals(
    sessionId,
    null,
    connected,
  );
  const {
    proposals,
    skewMs,
    refresh: refreshProposals,
  } = useSessionProposals(sessionId, connected);
  const updates = useInboxUpdates(connected);
  const [view, setView] = useState<InboxView>("all");
  const [busy, setBusy] = useState<string | null>(null);
  const [note, setNote] = useState<Note | null>(null);
  const [clearing, setClearing] = useState(false);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const t = setInterval(
      () => setNow(Date.now()),
      proposals.length > 0 ? 500 : 30_000,
    );
    return () => clearInterval(t);
  }, [proposals.length]);

  // Approvals come from the Core's approval list (with what each will do); its attention items for them are not repeated.
  const others = attentionItems.filter((i) => i.kind !== "APPROVAL");
  const needs = needsYouCount(attentionItems, approvals, proposals);
  const list = updates.view?.updates ?? [];
  const unread = updates.view?.unread ?? 0;

  const afterDecision = useCallback(() => {
    refreshApprovals();
    refreshProposals();
    if (sessionId) refreshAttention(sessionId);
  }, [refreshApprovals, refreshProposals, refreshAttention, sessionId]);

  const decideApproval = useCallback(
    async (a: DockApprovalView, approve: boolean) => {
      if (!sessionId) return;
      setBusy(a.approvalId);
      setNote(null);
      try {
        await decideFromList(sessionId, a, approve, "inbox");
        setNote({
          kind: "ok",
          text: approve
            ? `Allowed once: “${titleOf(a.taskId)}” runs it now.`
            : `Skipped: “${titleOf(a.taskId)}” is told you skipped it.`,
        });
      } catch (e) {
        const m = (e as Error).message;
        setNote({
          kind: "refused",
          text: m.startsWith("NOT_FROM_A_LIST: ")
            ? `Not decided here. ${m.slice("NOT_FROM_A_LIST: ".length)}`
            : refusalWords(m),
        });
      } finally {
        setBusy(null);
        afterDecision();
      }
    },
    [sessionId, titleOf, afterDecision],
  );

  const answerProposal = useCallback(
    async (p: ModeProposalInfo, accept: boolean) => {
      if (!sessionId) return;
      setBusy(p.proposalId);
      setNote(null);
      try {
        await decideProposal(sessionId, p.taskId, p.proposalId, accept);
        setNote({
          kind: "ok",
          text: accept
            ? `Switched “${titleOf(p.taskId)}” to ${modeWords(p.toMode)}.`
            : `Kept “${titleOf(p.taskId)}” in ${modeWords(p.fromMode)}.`,
        });
      } catch (e) {
        setNote({
          kind: "refused",
          text: proposalRefusalWords((e as Error).message),
        });
      } finally {
        setBusy(null);
        afterDecision();
      }
    },
    [sessionId, titleOf, afterDecision],
  );

  const openUpdate = useCallback(
    async (u: UpdateView) => {
      // Marked read through the Core first, so the mark holds whatever the task's view does; the task opens either way.
      if (!u.read) await updates.markRead(u.id);
      onOpenTask(u.taskId);
    },
    [updates, onOpenTask],
  );

  // The `a` shortcut asks for the first item; the lists arrive from the Core after the page opens, so the request waits for one (up to 5 s).
  const [focusWanted, setFocusWanted] = useState<{ n: number; at: number } | null>(null);
  useEffect(() => {
    if (focusRequest > 0) setFocusWanted({ n: focusRequest, at: Date.now() });
  }, [focusRequest]);
  useEffect(() => {
    if (!focusWanted) return;
    const first = document.querySelector<HTMLElement>('[data-testid="inbox"] [data-testid="attention-item"]');
    if (first) {
      first.focus();
      setFocusWanted(null);
    } else if (Date.now() - focusWanted.at > 5000) setFocusWanted(null);
  });

  const showNeeds = view !== "updates";
  const showUpdates = view !== "needs";
  const segs: { id: InboxView; label: string }[] = [
    { id: "needs", label: `Needs you · ${needs}` },
    { id: "updates", label: `Updates · ${list.length}` },
    { id: "all", label: "All" },
  ];

  return (
    <div className="page inbox" data-testid="inbox">
      <header className="inbox-head">
        <h2 className="mb-sr-only">Inbox</h2>
        <span className="inbox-seg" role="radiogroup" aria-label="Show">
          {segs.map((s) => (
            <button
              key={s.id}
              type="button"
              role="radio"
              aria-checked={view === s.id}
              className="inbox-seg-btn"
              data-testid={`inbox-view-${s.id}`}
              onClick={() => setView(s.id)}
            >
              {s.label}
            </button>
          ))}
        </span>
        <span className="inbox-spacer" />
        <Button
          size="sm"
          variant="ghost"
          disabled={unread === 0}
          title={unread === 0 ? "Every update is read" : undefined}
          onClick={() => void updates.markAllRead()}
          data-testid="inbox-mark-all-read"
        >
          Mark all read
        </Button>
        <IconButton
          label="Notification settings"
          icon={<Icon name="settings" />}
          onClick={onNotificationSettings}
          data-testid="inbox-notification-settings"
        />
      </header>
      <div className="inbox-body">
        <div className="inbox-col">
          {note && (
            <p
              className="meta inbox-note"
              role="status"
              data-testid="inbox-note"
              data-kind={note.kind}
            >
              {note.text}
            </p>
          )}
          {showNeeds && (
            <>
              <h3 className="inbox-label" id="inbox-needs">
                Needs you
              </h3>
              {needs === 0 ? (
                <p className="meta inbox-empty" data-testid="inbox-empty">
                  Nothing needs you right now.
                </p>
              ) : (
                <section
                  className="attention inbox-needs"
                  data-testid="attention"
                  aria-labelledby="inbox-needs"
                  tabIndex={-1}
                >
                  <ul className="inbox-cards">
                    {approvals.map((a) => (
                      <ApprovalCard
                        key={a.approvalId}
                        a={a}
                        title={titleOf(a.taskId)}
                        busy={busy !== null}
                        onOpenTask={onOpenTask}
                        onDecide={(approve) => void decideApproval(a, approve)}
                      />
                    ))}
                  </ul>
                  {(proposals.length > 0 || others.length > 0) && (
                    <ul className="inbox-list">
                      {proposals.map((p) => (
                        <li
                          key={p.proposalId}
                          className="inbox-row"
                          data-testid="inbox-proposal"
                          data-proposal-id={p.proposalId}
                          data-task-id={p.taskId}
                          data-to-mode={p.toMode}
                        >
                          <Icon
                            name="layers"
                            size={16}
                            className="inbox-row-icon"
                          />
                          <div className="inbox-row-text">
                            <span className="inbox-row-title">
                              {proposalQuestion(p)}
                            </span>
                            <span className="inbox-row-line">
                              <button
                                type="button"
                                className="inbox-task-link"
                                data-testid="attention-item"
                                data-kind="MODE_PROPOSAL"
                                data-task-id={p.taskId}
                                onClick={() => onOpenTask(p.taskId)}
                              >
                                {titleOf(p.taskId)}
                              </button>
                              {p.reason && (
                                <>
                                  {" · the agent says "}
                                  <q data-testid="inbox-proposal-reason">
                                    {p.reason}
                                  </q>
                                </>
                              )}
                            </span>
                          </div>
                          <span
                            className="inbox-when"
                            data-testid="inbox-proposal-left"
                          >
                            {secondsLeft(p, now, skewMs)} s
                          </span>
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={busy !== null}
                            onClick={() => void answerProposal(p, false)}
                            data-testid="inbox-proposal-keep"
                            aria-label={`Keep ${modeWords(p.fromMode)}: ${titleOf(p.taskId)}`}
                          >
                            Keep {modeWords(p.fromMode)}
                          </Button>
                          <Button
                            size="sm"
                            variant="primary"
                            disabled={busy !== null}
                            onClick={() => void answerProposal(p, true)}
                            data-testid="inbox-proposal-switch"
                            aria-label={`Switch to ${modeWords(p.toMode)}: ${titleOf(p.taskId)}`}
                          >
                            Switch
                          </Button>
                        </li>
                      ))}
                      {others.map((i) => (
                        <li key={`${i.kind}:${i.taskId}:${i.reference}`}>
                          <button
                            type="button"
                            className="inbox-row inbox-row-button"
                            data-testid="attention-item"
                            data-kind={i.kind}
                            data-task-id={i.taskId}
                            onClick={() => onOpenTask(i.taskId)}
                          >
                            <Icon
                              name={
                                i.kind === "QUESTION"
                                  ? "circle-question-mark"
                                  : "clock"
                              }
                              size={16}
                              className="inbox-row-icon"
                            />
                            <span className="inbox-row-text">
                              <span
                                className="inbox-row-title"
                                data-testid="inbox-other-words"
                              >
                                {attentionWords(i)}
                              </span>
                              <span className="inbox-row-line">
                                {titleOf(i.taskId)}
                              </span>
                            </span>
                            <Icon
                              name="chevron-right"
                              size={16}
                              className="inbox-row-icon"
                            />
                          </button>
                        </li>
                      ))}
                    </ul>
                  )}
                </section>
              )}
            </>
          )}
          {showUpdates && (
            <>
              <div className="inbox-label-row">
                <h3 className="inbox-label" id="inbox-updates">
                  Updates
                </h3>
                <span className="inbox-spacer" />
                {list.length > 0 &&
                  (clearing ? (
                    <>
                      <span className="meta">
                        Clear{" "}
                        {list.length === 1
                          ? "this update"
                          : `these ${list.length} updates`}
                        ?
                      </span>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => setClearing(false)}
                        data-testid="inbox-clear-cancel"
                      >
                        Cancel
                      </Button>
                      <Button
                        size="sm"
                        variant="danger"
                        onClick={() => {
                          setClearing(false);
                          void updates.clear();
                        }}
                        data-testid="inbox-clear-confirm"
                      >
                        Clear
                      </Button>
                    </>
                  ) : (
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => setClearing(true)}
                      data-testid="inbox-clear"
                    >
                      Clear
                    </Button>
                  ))}
              </div>
              {updates.error && (
                <p
                  className="meta inbox-note"
                  role="alert"
                  data-testid="inbox-updates-error"
                >
                  Updates could not be read: {updates.error}
                </p>
              )}
              {list.length === 0 ? (
                <p
                  className="meta inbox-empty"
                  data-testid="inbox-updates-empty"
                >
                  No updates yet. When a task is ready to review, finishes or
                  fails, it is listed here.
                </p>
              ) : (
                <ul
                  className="inbox-list"
                  aria-labelledby="inbox-updates"
                  data-testid="inbox-updates"
                >
                  {list.map((u) => {
                    const w = UPDATE_WORDS[u.kind];
                    return (
                      <li key={u.id}>
                        <button
                          type="button"
                          className="inbox-row inbox-row-button"
                          data-testid="inbox-update"
                          data-update-id={u.id}
                          data-kind={u.kind}
                          data-task-id={u.taskId}
                          data-read={u.read ? "true" : "false"}
                          aria-label={`${w.title}: ${u.title}, ${fullTime(u.atMs)}${u.read ? "" : ", unread"}`}
                          onClick={() => void openUpdate(u)}
                        >
                          <Icon
                            name={w.icon}
                            size={16}
                            className="inbox-row-icon"
                          />
                          <span className="inbox-row-text">
                            <span className="inbox-row-title">{w.title}</span>
                            <span className="inbox-row-line">{u.title}</span>
                          </span>
                          <span
                            className="inbox-when"
                            title={fullTime(u.atMs)}
                            data-testid="inbox-update-when"
                          >
                            {whenWords(u.atMs, now)}
                          </span>
                          <span
                            className="inbox-dot"
                            data-read={u.read ? "true" : "false"}
                            aria-hidden="true"
                          />
                        </button>
                      </li>
                    );
                  })}
                </ul>
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function ApprovalCard({
  a,
  title,
  busy,
  onOpenTask,
  onDecide,
}: {
  a: DockApprovalView;
  title: string;
  busy: boolean;
  onOpenTask: (taskId: string) => void;
  onDecide: (approve: boolean) => void;
}) {
  const rule = decidableFromList(a);
  const irreversible = isIrreversible(a.effectClass);
  const command = exactLine(a);
  const why = whyAsked(a.reason);
  return (
    <li
      className="inbox-card"
      data-testid="inbox-approval"
      data-task-id={a.taskId}
      data-approval-id={a.approvalId}
      data-intent-hash={a.intentHash}
      data-effect-class={a.effectClass}
      data-decidable={rule.ok ? "true" : "false"}
    >
      <div className="inbox-card-head">
        <Icon name="shield-check" size={16} className="inbox-card-icon" />
        <span className="inbox-card-ask">{askWords(a)}</span>
        <button
          type="button"
          className="inbox-task-link"
          data-testid="attention-item"
          data-kind="APPROVAL"
          data-task-id={a.taskId}
          onClick={() => onOpenTask(a.taskId)}
        >
          {title}
        </button>
        <span className="inbox-tag" data-testid="inbox-approval-tag">
          <Badge tone={irreversible ? "danger" : "warn"}>
            {effectTag(a.effectClass)}
          </Badge>
        </span>
      </div>
      <div className="inbox-card-body">
        {command && (
          <code className="appr-command" data-testid="inbox-approval-command">
            {command}
          </code>
        )}
        <p className="appr-why" data-testid="inbox-approval-why">
          {rule.ok ? why.headline : rule.why}
        </p>
      </div>
      <div className="inbox-card-foot">
        {rule.ok ? (
          <>
            <Button
              size="sm"
              variant="ghost"
              disabled={busy}
              onClick={() => onDecide(false)}
              data-testid="inbox-approval-skip"
              aria-label={`Skip: ${title} (it does not run)`}
            >
              Skip
            </Button>
            <span className="inbox-spacer" />
            <Button
              size="sm"
              variant="primary"
              disabled={busy}
              onClick={() => onDecide(true)}
              data-testid="inbox-approval-allow"
              aria-label={`Allow once: ${title}`}
            >
              Allow once
            </Button>
          </>
        ) : (
          <>
            <span className="inbox-spacer" />
            <Button
              size="sm"
              variant="primary"
              onClick={() => onOpenTask(a.taskId)}
              data-testid="inbox-approval-open"
              title={rule.why}
              aria-label={`Open to decide: ${title}. ${rule.why}`}
            >
              Open to decide
            </Button>
          </>
        )}
      </div>
    </li>
  );
}
