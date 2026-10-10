/**
 * The model-free states of the approval stack, the run-mode chooser, the context
 * ring and its trays, and the checkpoint controls (REQ-PX-057, 058, 060, 062)
 * for design review and for the accessibility pass. Fixtures only: no Core and
 * no model are behind anything here, and a gallery state is never evidence that
 * a behaviour works (the real-boundary specs prove those).
 */
import { useState } from "react";
import { Button } from "@modbit/ui";
import type { AccountingInfo, AllowRuleInfo, CheckpointInfo, DockApprovalView, RestoreOutcome, RewindPreviewInfo, RunModeInfo } from "../../shared/control-types.ts";
import { ApprovalDeck } from "../approvals/approval-deck.tsx";
import { RunModePanel } from "../approvals/run-mode.tsx";
import { BudgetTray } from "../context/tray-director.tsx";
import { ContextBreakdown, RingButton } from "../context/ring.tsx";
import { EditBox, EditFilesDialog, RedoBar, type CheckpointContext } from "../conversation/checkpoint-controls.tsx";
import { RestoreDialog, type RestoreCalls, type RestoreRequest } from "../conversation/restore-dialog.tsx";
import { RowView, type RowContext } from "../conversation/rows.tsx";
import type { TranscriptRowView } from "../../shared/conversation-types.ts";
import type { UsageSetting } from "../context/model.ts";
import { GALLERY_NOW } from "./workspace-fixtures.ts";

const H = (c: string) => c.repeat(64);

const approval = (over: Partial<DockApprovalView>): DockApprovalView => ({
  approvalId: "a1",
  taskId: "fixture-task-1",
  toolCallId: "c1",
  toolName: "shell.exec",
  effectClass: "ProtectedWrite",
  intentHash: H("a"),
  requestedAtMs: GALLERY_NOW,
  expiresAtMs: 0,
  reason: { code: "NOT_IN_ALLOWLIST", runMode: "ALLOWLIST", askClasses: [], contained: false, capabilities: ["shell.exec"], executionProfile: "local_trusted" },
  intent: { found: true, argv: ["cargo", "test", "-p", "parser"], command: "", cwd: "/work/example", paths: [], hosts: [], escalation: "", argsPreview: '{"argv":["cargo","test","-p","parser"],"cwd":"/work/example"}', truncated: false },
  ...over,
});

const DESTRUCTIVE = approval({
  approvalId: "a2",
  toolCallId: "c2",
  effectClass: "Destructive",
  intentHash: H("b"),
  reason: { code: "ALWAYS_ASK:DELETION", runMode: "RUN_EVERYTHING", askClasses: ["DELETION"], contained: false, capabilities: ["shell.exec"], executionProfile: "local_trusted" },
  intent: { found: true, argv: ["git", "clean", "-fd"], command: "", cwd: "", paths: [], hosts: [], escalation: "", argsPreview: '{"argv":["git","clean","-fd"]}', truncated: false },
});
const FOREIGN = approval({
  approvalId: "a3",
  taskId: "fixture-task-2",
  toolName: "net.fetch",
  effectClass: "ExternalSideEffect",
  intentHash: H("c"),
  reason: { code: "ALWAYS_ASK:NETWORK", runMode: "ASK", askClasses: ["NETWORK"], contained: false, capabilities: ["network.egress"], executionProfile: "local_trusted" },
  intent: { found: false, argv: null, command: "", cwd: "", paths: [], hosts: ["example.com"], escalation: "network", argsPreview: "", truncated: false },
});

const MODE: RunModeInfo = { taskId: "fixture-task-1", mode: "ALLOWLIST", acknowledged: true, alwaysAsk: ["OUTSIDE_WORKSPACE_WRITE", "NETWORK", "SECRET", "PROTECTED_PATH", "DELETION", "PUSH", "ESCALATION"], modes: ["ASK", "ALLOWLIST", "ALLOWLIST_SANDBOX", "RUN_EVERYTHING"], warning: "A mode that approves effects without asking lets anything the agent is made to attempt run inside its envelope.", sessionOnly: false, offset: "9", rulesInForce: 1 };
const RULES: AllowRuleInfo[] = [
  { ruleId: "r1", pattern: ["cargo", "test"], scope: "REPO", scopeKey: "/work/example", createdBy: "user:u", createdAtMs: GALLERY_NOW - 60_000, expiresAtMs: 0, coversAlwaysAsk: false, state: "ACTIVE", revokedBy: "", originTask: "t", offset: "5" },
  { ruleId: "r2", pattern: ["git", "status"], scope: "TASK", scopeKey: "t", createdBy: "user:u", createdAtMs: GALLERY_NOW - 120_000, expiresAtMs: GALLERY_NOW + 3_600_000, coversAlwaysAsk: false, state: "REVOKED", revokedBy: "user:u", originTask: "t", offset: "4" },
];

const CATS = ["SYSTEM", "TOOLS", "RULES", "SKILLS", "MCP", "MEMORY", "SUMMARY", "SUBAGENTS", "CONVERSATION"] as const;
const TOKENS = [210, 120, 80, 40, 30, 20, 50, 10, 440];
const ACCOUNTING: AccountingInfo = {
  available: true,
  totalTokens: "1000",
  totalSource: "PROVIDER_REPORTED",
  windowTokens: "1200",
  windowSource: "REGISTRY",
  usedBp: 8333,
  estimatorErrorBp: 120,
  declaredErrorBp: 1500,
  model: "fixture-model",
  turnOrdinal: 4,
  categories: CATS.map((c, i) => ({ category: c, tokens: String(TOKENS[i]), estimatedTokens: String(TOKENS[i]), shareBp: TOKENS[i]! * 10, sources: [] })),
  compactionEpoch: 0,
  compactionSummaries: 1,
  budgets: { maxCostMinor: "500", spentMinor: "430", heldByChildrenMinor: "0", maxWallMs: "600000", wallMsUsed: "120000", maxChildren: 0, liveChildren: 0, forbidSpawn: false },
};
const ECONOMICS = { state: "Running", verified: false, checksPassed: 0, checksFailed: 0, model: "fixture-model", modelCalls: 4, inputTokens: "52000", cachedInputTokens: "40000", outputTokens: "2100", costUsd: 0.4312, pricingKnown: true, toolCalls: 9, wallMs: "120000", modelMs: "80000", toolMs: "30000", prefixCacheHits: 3, prefixCacheMisses: 1, compactionEpochs: 1, contextTokensInjected: "900" };

const cp = (over: Partial<CheckpointInfo>): CheckpointInfo => ({ checkpointId: "00000000-0000-4000-8000-000000000001", epoch: 1, kind: "DELTA", status: "SUPERSEDED", reason: "turn_boundary", files: 2, removed: 0, eventOffset: "40", createdAtMs: GALLERY_NOW, name: "", turnId: "turnone", turnOrdinal: 1, retention: [], ...over });
const CP_TURN1 = cp({});
const CP_CTX: CheckpointContext = {
  byTurn: new Map([["turnone", CP_TURN1]]),
  before: () => CP_TURN1,
  disabled: "",
  onRestore: () => {},
  onFork: () => {},
  forking: false,
  editingRowId: null,
  onEdit: () => {},
  onSendEdit: () => {},
};
const baseHints = { renderable: true, groupable: false, hasReasoning: false, durationMs: 0, shortText: "", linesAdded: 0, linesRemoved: 0, status: "" };
const baseRow = { ordinal: 0, offset: "50", lastOffset: "50", atMs: GALLERY_NOW - 60_000, turnId: "turnone", text: "", textTruncated: false, textRef: "", children: [] as TranscriptRowView[], hints: baseHints, facts: { type: "none" } as TranscriptRowView["facts"] };
const USER_ROW: TranscriptRowView = { ...baseRow, rowId: "user:in:fixture", kind: "USER_MESSAGE", text: "Now also write the changelog entry.", facts: { type: "user", inputId: "in1", source: "queued_input", mode: "STEER", provenance: "", untrusted: false } };
const FOOTER_ROW: TranscriptRowView = { ...baseRow, rowId: "footer:fixture", kind: "TURN_FOOTER", facts: { type: "footer", turnId: "turnone", runId: "r1", outcome: "COMPLETED", failureCode: "", durationMs: 4200, inputTokens: "1200", outputTokens: "340" } };
const ROW_CTX = (checkpoints: CheckpointContext): RowContext => ({ sessionId: null, card: undefined, live: new Map(), approvals: [], openGroups: new Set(), onToggleGroup: () => {}, codeOpen: false, onCodeOpen: () => {}, onResume: () => {}, onCopyTurn: () => {}, highlightRowId: null, latestUserRowId: null, checkpoints });

const PREVIEW: RewindPreviewInfo = {
  refusal: "",
  detail: "",
  checkpointId: CP_TURN1.checkpointId,
  epoch: 1,
  filesWritten: 1,
  filesReverted: 2,
  entries: [
    { path: "src/lib.rs", action: "WRITE", currentHash: H("1"), targetHash: H("2") },
    { path: "src/new_module.rs", action: "REVERT_TO_HEAD", currentHash: H("3"), targetHash: "" },
    { path: "docs/CHANGELOG.md", action: "REVERT_TO_HEAD", currentHash: H("4"), targetHash: "" },
    { path: "README.md", action: "UNCHANGED", currentHash: H("5"), targetHash: H("5") },
  ],
  userEdited: ["docs/CHANGELOG.md"],
  currentEpoch: 4,
};
const FIXTURE_CALLS: RestoreCalls = {
  preview: () => Promise.resolve(PREVIEW),
  restore: () => Promise.resolve({ restored: true, refusal: "", detail: "", checkpointId: CP_TURN1.checkpointId, epoch: 1, filesWritten: 1, filesReverted: 1, preRestoreCheckpointId: "", keptPaths: ["docs/CHANGELOG.md"], redo: false, replayed: false } satisfies RestoreOutcome),
};

/** One section of the gallery: every state of this change, by name. */
export function ControlsGallery({ note }: { note: (text: string) => void }) {
  const [modeOpen, setModeOpen] = useState(false);
  const [restore, setRestore] = useState<RestoreRequest | null>(null);
  const [editFiles, setEditFiles] = useState(false);
  const [ringOpen, setRingOpen] = useState(false);
  const [usage, setUsage] = useState<UsageSetting>("auto");
  const [prefix, setPrefix] = useState<string[]>(["cargo", "test", "-p", "parser"]);
  const [scope, setScope] = useState<"TASK" | "REPO">("REPO");
  const noop = () => {};
  const deck = (list: DockApprovalView[], index: number, extra: Partial<Parameters<typeof ApprovalDeck>[0]> = {}) => (
    <ApprovalDeck approvals={list} index={index} onIndex={noop} busy={false} confirming={false} prefix={prefix} onPrefix={setPrefix} scope={scope} onScope={setScope} note={null} onRun={() => note("approval: run")} onConfirm={noop} onBack={noop} onSkip={() => note("approval: skip")} onAlways={() => note("approval: always")} onChangeMode={() => setModeOpen(true)} openTaskId="fixture-task-1" titleOf={() => "Refactor the loader"} onOpenTask={noop} {...extra} />
  );
  return (
    <section aria-labelledby="g-controls" data-testid="gallery-controls">
      <h2 id="g-controls">Approvals, run modes, context and checkpoints</h2>
      <p className="meta">Fixtures only: the docked approval card in each of its states, the run-mode dialog, the context ring at every level and its trays, and the checkpoint controls.</p>

      <h3>Approval card</h3>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-approval-plain">
        {deck([approval({}), DESTRUCTIVE, FOREIGN], 0)}
      </div>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-approval-confirm">
        {deck([DESTRUCTIVE], 0, { confirming: true })}
      </div>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-approval-foreign">
        {deck([approval({}), FOREIGN], 1, { note: { kind: "refused", text: "The Core refused: the effect is no longer the one this card showed. Nothing was decided; the card now shows what is asked." } })}
      </div>

      <h3>Run mode and allowlist</h3>
      <div className="gallery-row">
        <Button data-testid="gallery-open-runmode" onClick={() => setModeOpen(true)}>
          Open the run-mode dialog
        </Button>
      </div>
      <RunModePanel open={modeOpen} onClose={() => setModeOpen(false)} info={MODE} rules={RULES} onSetMode={() => Promise.resolve()} onAddRule={() => Promise.reject(new Error("ALWAYS_ASK_PATTERN: fixture refusal"))} onRevoke={() => Promise.resolve()} now={GALLERY_NOW} />

      <h3>Context ring and trays</h3>
      <div className="gallery-row" data-testid="gallery-rings">
        <RingButton accounting={null} open={false} onToggle={noop} />
        <RingButton accounting={{ ...ACCOUNTING, usedBp: 2500 }} open={false} onToggle={noop} />
        <RingButton accounting={ACCOUNTING} open={ringOpen} onToggle={() => setRingOpen((o) => !o)} />
        <RingButton accounting={{ ...ACCOUNTING, usedBp: 9700 }} open={false} onToggle={noop} />
      </div>
      <div className="mb-tray" data-tone="info" data-testid="gallery-context-tray">
        <ContextBreakdown accounting={ACCOUNTING} economics={ECONOMICS} setting={usage} onSetting={setUsage} />
      </div>
      <div className="mb-tray" data-tone="info" data-testid="gallery-context-none">
        <ContextBreakdown accounting={{ ...ACCOUNTING, available: false }} economics={null} setting="auto" onSetting={noop} />
      </div>
      <div className="mb-tray" data-tone="warn" data-testid="gallery-budget-tray">
        <BudgetTray spec={{ kind: "budget", id: "budget-exhausted", title: "This task reached a limit", text: "max_wall_ms: 612345/600000", taskId: "fixture-task-1" }} sessionId={null} onResume={noop} onDone={noop} />
      </div>
      <div className="mb-tray" data-tone="warn" data-testid="gallery-offline-tray" role="status">
        <p>The Core stopped and has not come back. (exit 9) Open conversations stay readable. Anything that needs the Core fails until it is back, and nothing shown is new progress.</p>
      </div>
      <div className="mb-tray" data-tone="error" data-testid="gallery-policy-tray" role="alert">
        <p>endpoint openai model gpt-5 is blocked by organization policy (block=*)</p>
        <p>
          <strong>What you can do:</strong> pick a model your organisation allows, or ask its administrator.
        </p>
        <p className="meta">The reason comes from the Core. Your prompt is kept; nothing was sent to a model.</p>
      </div>

      <h3>Checkpoints</h3>
      <div className="gallery-card" style={{ display: "flex", flexDirection: "column", gap: 8 }} data-testid="gallery-checkpoint-rows">
        <RowView row={USER_ROW} ctx={ROW_CTX(CP_CTX)} />
        <RowView row={FOOTER_ROW} ctx={ROW_CTX(CP_CTX)} />
        <RowView row={{ ...USER_ROW, rowId: "user:in:editing" }} ctx={ROW_CTX({ ...CP_CTX, editingRowId: "user:in:editing" })} />
        <EditBox row={USER_ROW} cp={CP_CTX} />
        <RedoBar checkpoint={cp({ checkpointId: "00000000-0000-4000-8000-000000000002", retention: ["PRE_RESTORE"] })} label="Files were restored. Redo puts them back exactly as they were before the restore." busy={false} onRedo={() => note("redo")} />
      </div>
      <div className="gallery-row">
        <Button data-testid="gallery-open-restore" onClick={() => setRestore({ checkpointId: CP_TURN1.checkpointId, heading: "before your message “Now also write the changelog entry.”", redo: false })}>
          Open the restore dialog
        </Button>
        <Button data-testid="gallery-open-redo" onClick={() => setRestore({ checkpointId: CP_TURN1.checkpointId, heading: "", redo: true })}>
          Open the redo dialog
        </Button>
        <Button data-testid="gallery-open-edit-files" onClick={() => setEditFiles(true)}>
          Open Revert or Keep
        </Button>
      </div>
      <RestoreDialog sessionId="fixture" taskId="fixture-task-1" request={restore} onClose={() => setRestore(null)} onRestored={() => setRestore(null)} calls={FIXTURE_CALLS} />
      <EditFilesDialog open={editFiles} changedFiles={3} onRevert={() => setEditFiles(false)} onKeep={() => setEditFiles(false)} onCancel={() => setEditFiles(false)} />
    </section>
  );
}
