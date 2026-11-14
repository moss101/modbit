/**
 * The details rail (REQ-PX-201; DR-PX-2026-11-11-021 MWS-09): what the task's details popover held, beside the
 * conversation. Environment, approvals, context, tool servers, changes, evidence, background tasks and cost, each read
 * from the Core (the value's element names the preload method in `data-src`); state, phase, agents, generation,
 * attachments, risk, security and next action behind Details in its head and always in the accessibility tree.
 * The same body stands under the title bar's More where the task area has no room for the rail.
 * Nothing here decides: Stop asks the Core to cancel or kill, Review and restore open the surfaces that already ask.
 */
import { useEffect, useState, type ReactNode } from "react";
import { Button, Icon, type IconName } from "@modbit/ui";
import type { AgentHeaderView, PendingApprovalView } from "../../shared/conversation-types.ts";
import type { AccountingInfo, CheckpointListInfo } from "../../shared/control-types.ts";
import type { ExtensionRowInfo } from "../../shared/customize-types.ts";
import type { ContextInspectorSummary, TaskEconomicsSummary, TerminalViewJson } from "../../preload/preload.ts";
import { PHASE_LABEL, type TaskCard } from "../model.ts";
import { localMachine } from "../location.ts";
import { percentUsed, ringLabel } from "../context/model.ts";
import { useContextTray } from "../context/ring.tsx";
import { MODE_COPY, modeLabel } from "../approvals/run-mode-model.ts";
import type { RunModeState } from "../approvals/run-mode.tsx";
import { effectWords } from "../approvals/model.ts";
import type { CheckpointContext } from "./checkpoint-controls.tsx";
import { BackgroundList } from "./background-list.tsx";
import { budgetLines, economicsFacts, evidenceWords, changeWords, checkpointHeading, checkpointLabel, countWords, folderWords, locationWords, modelCallWords, restorable, spendWords, toolServers, verificationWords, type RailSource } from "./rail-model.ts";

export interface DetailsRailProps {
  taskId: string;
  sessionId: string | null;
  connected: boolean;
  /** The task's card with the facts the Core folded from its log (PX-196). */
  card: TaskCard | undefined;
  /** The task's state in the Core's word, from the card or the transcript page. */
  stateWords: string;
  /** The task's header as the task list read it; the rail reads it itself where the list is not there to. */
  header: AgentHeaderView | undefined;
  subtasks: readonly TaskCard[];
  terminals: readonly TerminalViewJson[];
  approvals: readonly PendingApprovalView[];
  runMode: RunModeState;
  accounting: AccountingInfo | null;
  economics: TaskEconomicsSummary | null;
  checkpoints: CheckpointListInfo | null;
  checkpointContext: CheckpointContext;
  /** Opens the run-mode panel, which also holds the saved rules. */
  onRunMode: () => void;
  onReview?: (() => void) | undefined;
  onOpenTask: (taskId: string) => void;
  onOpenTerminal?: ((terminalId: string) => void) | undefined;
  /** Asks to cancel a child task (the same confirmation Cancel task asks). */
  onStopChild: (taskId: string) => void;
  onTerminalsChanged?: (() => void) | undefined;
  /** Opens Customize, where the session's extensions and their tool servers are listed. */
  onCustomize?: (() => void) | undefined;
}

/** Marks an element as a value of the rail and names the preload read (or reads) it is drawn from. */
export const src = (...s: RailSource[]) => ({ "data-rail-value": "", "data-src": s.join(" ") });

function Section({ id, title, aside, children }: { id: string; title: string; aside?: ReactNode; children?: ReactNode }) {
  return (
    <section className="rail-sec" aria-label={title} data-testid={`rail-${id}`}>
      <h2 className="rail-h">
        <span>{title}</span>
        {aside !== undefined && <span className="rail-h-aside">{aside}</span>}
      </h2>
      {children}
    </section>
  );
}

function Row({ icon, children, aside, testid }: { icon: IconName; children: ReactNode; aside?: ReactNode; testid?: string }) {
  return (
    <div className="rail-row" data-testid={testid}>
      <span className="rail-ico">
        <Icon name={icon} size={16} />
      </span>
      <span className="rail-x">{children}</span>
      {aside !== undefined && aside !== null && aside !== "" && <span className="rail-aside">{aside}</span>}
    </div>
  );
}

/** What the rail reads for itself: the branch (the change set's), the session's extensions, and the header where no task list read it. */
function useRailReads(taskId: string, sessionId: string | null, connected: boolean, started: boolean, listed: AgentHeaderView | undefined, tick: string, offset: string): { branch: string; extensions: ExtensionRowInfo[] | null; header: AgentHeaderView | undefined } {
  const [branch, setBranch] = useState("");
  const [extensions, setExtensions] = useState<ExtensionRowInfo[] | null>(null);
  const [own, setOwn] = useState<AgentHeaderView | undefined>(undefined);
  useEffect(() => {
    if (!connected || !started) return;
    let live = true;
    window.modbit
      .changesState(taskId)
      .then((s) => live && setBranch(s.branch))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [taskId, connected, started, tick]);
  useEffect(() => {
    if (!connected || !sessionId) return;
    let live = true;
    window.modbit
      .customizeHooks(sessionId)
      .then((h) => live && setExtensions(h.extensions))
      .catch(() => {});
    return () => {
      live = false;
    };
  }, [sessionId, connected]);
  const missing = listed === undefined;
  // Where no task list is on screen to read the headers (the list is a rail), the rail reads them, once per burst of the task's events.
  useEffect(() => {
    if (!missing || !connected || !sessionId) return;
    let live = true;
    const t = setTimeout(() => {
      window.modbit
        .agentHeaders(sessionId, true)
        .then((l) => live && setOwn(l.headers.find((h) => h.taskId === taskId)))
        .catch(() => {});
    }, 400);
    return () => {
      live = false;
      clearTimeout(t);
    };
  }, [missing, taskId, sessionId, connected, offset]);
  return { branch, extensions, header: listed ?? own };
}

/** The context inspector and the economics in full, as the popover wrote them: read and shown only while Details is open. */
function DetailsMore({ taskId, offset, economics }: { taskId: string; offset: string | undefined; economics: TaskEconomicsSummary | null }) {
  const [inspector, setInspector] = useState<ContextInspectorSummary | null>(null);
  useEffect(() => {
    let live = true;
    window.modbit
      .contextInspector(taskId)
      .then((i) => live && setInspector(i))
      .catch(() => live && setInspector(null));
    return () => {
      live = false;
    };
  }, [taskId, offset]);
  return (
    <>
      {inspector && inspector.packId !== "" && (
        <details className="task-section" data-testid="context-inspector">
          <summary>
            Context: {inspector.entries.filter((e) => e.injected).length} of {inspector.entries.length} fragment(s) injected, {inspector.tokenUsed}/{inspector.tokenBudget} tokens, {inspector.omittedCount} omitted
            {inspector.rejectedRefs.length > 0 ? `, ${inspector.rejectedRefs.length} refused for missing provenance` : ""}
          </summary>
          {inspector.compactionEpochs > 0 && (
            <p className="meta" data-testid="context-compaction">
              Compaction: epoch {inspector.compactionEpoch} of {inspector.compactionEpochs}, {String(inspector.compactedEntries)} earlier entr{Number(inspector.compactedEntries) === 1 ? "y" : "ies"} summarised and still in the log; prompt prefix reused on {inspector.prefixCacheHits} turn(s), rebuilt on {inspector.prefixCacheMisses}.
            </p>
          )}
          <ul className="meta">
            {inspector.entries.map((e) => (
              <li key={e.entryId} data-testid="context-entry">
                {e.sourceRef}
                {e.lineEnd > 0 ? ` L${e.lineStart}-${e.lineEnd}` : ""} — {e.reason}; {e.freshness}; {e.tokenCost} tokens
                {e.injected ? "; injected" : "; not injected"}
                {e.used ? "; used" : ""}
                {e.stub ? "; stub" : ""}
              </li>
            ))}
            {inspector.omittedPaths.map((p) => (
              <li key={p} data-testid="context-omitted">
                {p} — omitted for the token budget
              </li>
            ))}
          </ul>
        </details>
      )}
      {economics && economics.modelCalls > 0 && (
        <dl className="task-facts task-section" aria-label="What the task has used" data-testid="task-economics">
          {economicsFacts(economics).map((f) => (
            <Fact key={f.id} label={f.label}>
              <span data-testid={`task-economics-${f.id}`} {...src("taskEconomics")}>
                {f.value}
              </span>
            </Fact>
          ))}
        </dl>
      )}
    </>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="task-fact">
      <dt>{label}</dt>
      <dd>{children}</dd>
    </div>
  );
}

/** The rail's content. `layout` says where it stands: beside the conversation, or under More. */
export function DetailsRail(p: DetailsRailProps) {
  const { taskId, sessionId, connected, card, stateWords, subtasks, terminals, runMode, accounting, economics, checkpointContext } = p;
  const contextTray = useContextTray();
  const [detailsOpen, setDetailsOpen] = useState(false);
  const [checkpointsOpen, setCheckpointsOpen] = useState(false);
  const started = !!card && card.state !== "Created" && card.state !== "Queued";
  const reads = useRailReads(taskId, sessionId, connected, started, p.header, `${card?.state ?? ""}:${p.header?.filesChanged ?? ""}`, card?.lastOffset ?? "");
  const header = reads.header;
  const approvals = p.approvals.filter((a) => a.taskId === taskId);
  const location = locationWords(header?.executionLocation, localMachine());
  const folder = header ? folderWords(header.checkoutRoot || header.workspaceRoot) : null;
  const worktree = Boolean(header?.checkoutRoot);
  const mode = runMode.info;
  const percent = percentUsed(accounting);
  const servers = toolServers(reads.extensions);
  const change = header ? changeWords(header) : null;
  const verification = verificationWords(economics);
  const points = restorable(p.checkpoints);
  const spend = spendWords(economics);
  const budgets = budgetLines(accounting);
  const active = card ? card.agents.running + card.agents.background : 0;
  const detailsId = `rail-details-${taskId}`;
  const restoreWhy = checkpointContext.disabled;

  return (
    <div className="rail-body" data-testid="rail-body" data-task-id={taskId}>
      <div className="rail-head">
        <span className="rail-pill" data-testid="top-bar-state" {...src(card ? "sessionSnapshot" : "transcript")}>
          {stateWords}
        </span>
        <span className="rail-grow" />
        <Button size="sm" variant="ghost" aria-expanded={detailsOpen} aria-controls={detailsId} onClick={() => setDetailsOpen((o) => !o)} data-testid="rail-details-toggle">
          Details
          <Icon name={detailsOpen ? "chevron-down" : "chevron-right"} size={14} />
        </Button>
      </div>
      {/* Closed, the facts are clipped from sight and stay in the accessibility tree (MWS-09). */}
      <div id={detailsId} className={detailsOpen ? "rail-details" : "rail-details mb-sr-only"} role="group" aria-label="Task details" data-testid="task-details" data-open={detailsOpen} data-task-id={taskId} data-state={card?.state} data-phase={card?.phase}>
        {card && (
          <dl className="task-facts">
            <Fact label="State">
              <span {...src("sessionSnapshot")}>
                <span data-testid="task-state">{card.state}</span>
                {card.waitReason ? ` · waiting on ${card.waitReason}` : ""}
              </span>
            </Fact>
            <Fact label="Phase">
              <span data-testid="task-phase" {...src("transcript")}>
                {PHASE_LABEL[card.phase]}
              </span>
            </Fact>
            <Fact label="Agents">
              <span data-testid="task-agents" aria-label={`${active} active of ${card.agents.total} agents`} {...src("transcript")}>
                {active}/{card.agents.total}
              </span>
            </Fact>
            <Fact label="Updates recorded">
              <span data-testid="task-generation" title="How many times the Core has recorded a change to this task" {...src("sessionSnapshot")}>
                {card.generation}
              </span>
            </Fact>
            <Fact label="Attachments">
              <span data-testid="task-attachments" {...src("transcript")}>
                {card.attachments ?? 0}
              </span>
            </Fact>
            {card.risk && (
              <Fact label="Risk">
                <span data-testid="task-risk" {...src("transcript")}>
                  {card.risk}
                </span>
              </Fact>
            )}
            {card.security?.length ? (
              <Fact label="Security">
                <span data-testid="task-security" data-count={card.security.length} {...src("sessionSnapshot")} title={card.security.map((s) => `${s.kind} in ${s.toolName}: ${s.patterns.join(", ")} (${s.action.toLowerCase()})`).join("\n")}>
                  {card.security.filter((s) => s.action === "BLOCKED").length} blocked, {card.security.filter((s) => s.action === "MARKED").length} marked
                </span>
              </Fact>
            ) : null}
            {card.nextAction && (
              <Fact label="Next">
                <span data-testid="task-next" {...src("sessionSnapshot", "transcript")}>
                  {card.nextAction}
                </span>
              </Fact>
            )}
          </dl>
        )}
        {detailsOpen && <DetailsMore taskId={taskId} offset={card?.lastOffset} economics={economics} />}
      </div>

      <Section id="environment" title="Environment">
        {location !== null && (
          <Row icon={header?.executionLocation === "cloud" ? "globe" : "laptop"}>
            <span data-testid="conv-location" data-location={header?.executionLocation} {...src("agentHeaders")}>
              {location}
            </span>
          </Row>
        )}
        {folder && (
          <Row icon="folder">
            <span title={header?.checkoutRoot || header?.workspaceRoot} data-testid="rail-folder" {...src("agentHeaders")}>
              {folder.name}
              {folder.parent && <span className="rail-dim"> · {folder.parent}</span>}
            </span>
          </Row>
        )}
        {(reads.branch || worktree) && (
          <Row icon="git-branch" aside={worktree ? <span data-testid="rail-worktree" title={header?.workspaceRoot} {...src("agentHeaders")}>own worktree</span> : undefined}>
            {reads.branch ? (
              <span data-testid="top-bar-branch" title={reads.branch} {...src("changesState")}>
                {reads.branch}
              </span>
            ) : (
              <span className="rail-dim">branch not read yet</span>
            )}
          </Row>
        )}
        {mode && (
          <button type="button" className="rail-row rail-rowbtn" onClick={p.onRunMode} aria-haspopup="dialog" title={MODE_COPY[mode.mode]?.summary} data-testid="rail-runmode" data-mode={mode.mode}>
            <span className="rail-ico">
              <Icon name="shield-check" size={16} />
            </span>
            <span className="rail-x">
              <span className="mb-sr-only">Run mode: </span>
              <span data-testid="rail-runmode-current" {...src("runMode")}>
                {modeLabel(mode.mode)}
              </span>
            </span>
            <span className="rail-aside">change</span>
          </button>
        )}
      </Section>

      <Section id="approvals" title="Approvals">
        <Row icon={approvals.length > 0 ? "circle-question-mark" : "check"} aside={approvals.length > 0 ? <span {...src("pendingApprovals")}>{effectWords(approvals[0]!.effectClass)}</span> : undefined} testid="rail-approvals-waiting-row">
          <span className={approvals.length > 0 ? "rail-warn" : "rail-dim"} data-testid="rail-approvals-waiting" data-count={approvals.length} {...src("pendingApprovals")}>
            {approvals.length > 0 ? `${approvals.length} waiting` : "None waiting"}
          </span>
        </Row>
        {mode && (
          <button type="button" className="rail-row rail-rowbtn" onClick={p.onRunMode} aria-haspopup="dialog" data-testid="rail-rules" data-count={mode.rulesInForce}>
            <span className="rail-ico">
              <Icon name="list-tree" size={16} />
            </span>
            <span className="rail-x" data-testid="rail-rules-count" {...src("runMode")}>
              {countWords(mode.rulesInForce, "saved rule", "saved rules")}
            </span>
            <span className="rail-aside">manage</span>
          </button>
        )}
      </Section>

      <section className="rail-sec" aria-label="Context" data-testid="rail-context">
        <h2 className="rail-h">
          <button type="button" className="rail-hbtn" aria-expanded={contextTray.open} aria-label={`Context: ${ringLabel(accounting)}. Show the breakdown`} onClick={contextTray.toggle} data-testid="rail-context-toggle" data-percent={percent ?? ""}>
            <span>Context</span>
            <Icon name="chevron-right" size={14} />
            <span className="rail-h-aside">
              <span data-testid="rail-context-use" {...src("contextAccounting")}>
                {percent === null ? ringLabel(accounting) : `${percent}%`}
              </span>
              {percent !== null && (
                <span className="rail-bar" aria-hidden="true">
                  <i style={{ width: `${percent}%` }} />
                </span>
              )}
            </span>
          </button>
        </h2>
      </section>

      {servers && (
        <section className="rail-sec" aria-label="Tool servers" data-testid="rail-tool-servers">
          <h2 className="rail-h">
            {p.onCustomize ? (
              <button type="button" className="rail-hbtn" onClick={p.onCustomize} title="Open Customize, which lists them" data-testid="rail-tool-servers-open">
                <span>Tool servers</span>
                <Icon name="chevron-right" size={14} />
                <span className="rail-h-aside" data-testid="rail-tool-servers-count" {...src("customizeHooks")}>
                  {servers.words}
                </span>
              </button>
            ) : (
              <>
                <span>Tool servers</span>
                <span className="rail-h-aside" data-testid="rail-tool-servers-count" {...src("customizeHooks")}>
                  {servers.words}
                </span>
              </>
            )}
          </h2>
        </section>
      )}

      {header && started && (
        <Section id="changes" title="Changes">
          <div className="rail-row">
            <span className="rail-ico">
              <Icon name="file-diff" size={16} />
            </span>
            {change ? (
              <span className="rail-x rail-mono" data-testid="rail-changes-size" data-files={header.filesChanged} {...src("agentHeaders")}>
                <span className="conv-add">{change.added}</span> <span className="conv-del">{change.removed}</span> <span className="rail-dim">· {change.files}</span>
              </span>
            ) : (
              <span className="rail-x rail-dim" data-testid="rail-changes-size" data-files={0} {...src("agentHeaders")}>
                No changes yet
              </span>
            )}
            {p.onReview && change && (
              <Button size="sm" variant="secondary" onClick={p.onReview} data-testid="rail-review">
                Review
              </Button>
            )}
          </div>
        </Section>
      )}

      {(verification !== null || card?.latestEvidence || points.length > 0) && (
        <Section id="evidence" title="Evidence">
          {verification !== null && (
            <Row icon="shield-check" aside={<span data-testid="rail-verification" {...src("taskEconomics")}>{verification}</span>}>
              Verification
            </Row>
          )}
          {card?.latestEvidence && (
            <Row icon="eye">
              <span title={evidenceWords(card.latestEvidence)} data-testid="task-evidence" data-core={card.latestEvidence} {...src("transcript")}>
                {evidenceWords(card.latestEvidence)}
              </span>
            </Row>
          )}
          {points.length > 0 && (
            <>
              <button type="button" className="rail-row rail-rowbtn" aria-expanded={checkpointsOpen} onClick={() => setCheckpointsOpen((o) => !o)} data-testid="rail-checkpoints" data-count={points.length}>
                <span className="rail-ico">
                  <Icon name="clock" size={16} />
                </span>
                <span className="rail-x" data-testid="rail-checkpoints-count" {...src("checkpoints")}>
                  {countWords(points.length, "checkpoint", "checkpoints")}
                </span>
                <span className="rail-aside">restore</span>
              </button>
              {checkpointsOpen && (
                <ul className="rail-list" aria-label="Checkpoints" data-testid="rail-checkpoint-list">
                  {points.map((c) => (
                    <li key={c.checkpointId} className="rail-row rail-sub" data-testid="rail-checkpoint" data-checkpoint-id={c.checkpointId} data-turn-ordinal={c.turnOrdinal}>
                      <span className="rail-x" {...src("checkpoints")}>
                        {checkpointLabel(c)}
                      </span>
                      <span className="rail-aside" {...src("checkpoints")}>
                        {countWords(c.files + c.removed, "file", "files")}
                      </span>
                      <Button size="sm" variant="ghost" disabled={restoreWhy !== ""} title={restoreWhy || undefined} onClick={() => checkpointContext.onRestore(c, checkpointHeading(c))} aria-label={`Restore files to the end of ${c.turnOrdinal > 0 ? `turn ${c.turnOrdinal}` : "this turn"}`} data-testid="rail-restore">
                        Restore
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
        </Section>
      )}

      {(subtasks.length > 0 || terminals.length > 0) && (
        <Section id="background" title="Background tasks">
          <BackgroundList taskId={taskId} sessionId={sessionId} subtasks={subtasks} terminals={terminals} onOpenTask={p.onOpenTask} onOpenTerminal={p.onOpenTerminal} onStopChild={p.onStopChild} onTerminalsChanged={p.onTerminalsChanged} />
        </Section>
      )}

      {economics && (
        <Section id="cost" title="Cost" aside={<span data-testid="rail-spend" title={economics.pricingKnown ? "At catalog list prices" : "The model has no price in the registry"} {...src("taskEconomics")}>{spend}</span>}>
          <Row icon="chart-column" aside={economics.model ? <span data-testid="rail-model" title={economics.model} {...src("taskEconomics")}>{economics.model}</span> : undefined}>
            <span data-testid="rail-model-calls" data-count={economics.modelCalls} {...src("taskEconomics")}>
              {modelCallWords(economics.modelCalls)}
            </span>
          </Row>
          {budgets.map((b) => (
            <Row key={b.kind} icon={b.kind === "cost" ? "layers" : "clock"} aside={<span {...src("contextAccounting")}>{b.percent}%</span>}>
              <span className="mb-sr-only">{b.kind === "cost" ? "Cost cap: " : "Time cap: "}</span>
              <span data-testid={`rail-budget-${b.kind}`} data-percent={b.percent} {...src("contextAccounting")}>
                {b.text}
              </span>
            </Row>
          ))}
        </Section>
      )}
    </div>
  );
}
