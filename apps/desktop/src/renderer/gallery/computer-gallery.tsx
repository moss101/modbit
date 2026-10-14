/**
 * The model-free states of native computer control's desktop surface (PX-074):
 * the permission window in every state, the presence bar while the agent holds
 * the machine, when the person has stopped it and when an outcome is unknown,
 * the transcript's action cards (done, refused, skipped, unknown, expired
 * picture) and the approval card of an action, an observation grant and a
 * whole-screen grant. Fixtures only: no Core, actuator or model is behind
 * anything here, and a gallery state is never evidence that a behaviour works
 * (computer-surface.spec.ts proves those against a real Core and actuator).
 */
import { useState } from "react";
import type { ComputerCardView, ComputerIntentView, ComputerPresenceEntry, ComputerPresenceInfo, PermissionName } from "../../shared/computer-types.ts";
import type { DockApprovalView } from "../../shared/control-types.ts";
import { ApprovalDeck } from "../approvals/approval-deck.tsx";
import { ComputerActionCard, type ComputerCardContext } from "../computer/action-card.tsx";
import { PermissionsBody } from "../computer/permissions-dialog.tsx";
import { PresenceBar } from "../computer/presence-bar.tsx";
import { GALLERY_NOW } from "./workspace-fixtures.ts";

const H = (c: string) => c.repeat(64);

const info = (over: Partial<ComputerPresenceInfo>): ComputerPresenceInfo => ({ configured: true, attached: true, actuatorName: "modbit-actuator-macos", actuatorVersion: "1.0.0", platform: "macos", detail: "", accessibility: "GRANTED", screenRecording: "GRANTED", controls: [], ...over });
const entry = (over: Partial<ComputerPresenceEntry>): ComputerPresenceEntry => ({ taskId: "fixture-task-1", controlId: "c1", application: "TextEdit", bundleId: "com.apple.TextEdit", scope: "APP", state: "CONTROLLING", grantExpiresMs: GALLERY_NOW + 600_000, latch: "", latchToolCallId: "", actions: 3, screenshots: 1, observations: 5, ...over });

const intent = (over: Partial<ComputerIntentView>): ComputerIntentView => ({
  tool: "computer.type",
  kind: "action",
  scope: "APP",
  application: { bundleId: "com.apple.TextEdit", executablePath: "/System/Applications/TextEdit.app/Contents/MacOS/TextEdit", signingIdentity: "Software Signing", pid: 4312 },
  applicationName: "TextEdit",
  window: { id: "w-17", title: "Untitled" },
  target: { kind: "element", snapshot: "s9d2d", id: "n7", role: "textarea", name: "Document", frame: "", windowVersion: 0 },
  action: "type",
  modality: "ax",
  argumentsText: '{"text":"Dear Ada, the invoice is attached.","then":"enter"}',
  argumentsTruncated: false,
  reason: "",
  takesDisplayAndCursor: false,
  grant: null,
  reversible: false,
  beforeArtifact: "",
  ...over,
});

const approval = (over: Partial<DockApprovalView>, c: ComputerIntentView): DockApprovalView => ({
  approvalId: "a-cc-1",
  taskId: "fixture-task-1",
  toolCallId: "c-cc-1",
  toolName: c.tool,
  effectClass: "ExternalSideEffect",
  intentHash: H("c"),
  requestedAtMs: GALLERY_NOW,
  expiresAtMs: GALLERY_NOW + 300_000,
  reason: { code: "ALWAYS_ASK:COMPUTER_CONTROL", runMode: "RUN_EVERYTHING", askClasses: [], contained: false, capabilities: ["computer.act"], executionProfile: "local_trusted" },
  intent: { found: true, argv: null, command: "", cwd: "", paths: [], hosts: [], escalation: "", argsPreview: "", truncated: false },
  computer: c,
  ...over,
});

const GRANT = approval(
  { approvalId: "a-cc-0", toolCallId: "c-cc-0", intentHash: H("d") },
  intent({ tool: "computer.start", kind: "observation_grant", target: null, action: "", modality: "", argumentsText: "", reason: "I need to draft the letter in TextEdit.", grant: { endsWith: "this turn, a mode switch, the time limit or the call cap", maxSeconds: 900, callCap: 100 } }),
);
const ACTION = approval({}, intent({}));
const SCREEN = approval({ approvalId: "a-cc-2", toolCallId: "c-cc-2", toolName: "computer.start_screen", intentHash: H("e") }, intent({ tool: "computer.start_screen", kind: "observation_grant", scope: "SCREEN", target: null, action: "", modality: "", argumentsText: "", takesDisplayAndCursor: true, reason: "The window has no interface I can read.", grant: { endsWith: "this turn", maxSeconds: 900, callCap: 100 } }));

const card = (id: string, over: Partial<ComputerCardView>): ComputerCardView => ({
  toolCallId: id,
  tool: "computer.press",
  state: "SUCCEEDED",
  failureCode: "",
  role: "act",
  application: "TextEdit",
  bundleId: "com.apple.TextEdit",
  scope: "APP",
  target: { role: "button", name: "Bold", id: "s9d2d/n3" },
  action: "press",
  argumentsText: '{"action":"press","element":"s9d2d/n3"}',
  modality: "ax",
  outcome: "SUCCEEDED",
  verified: true,
  durationMs: 41,
  approval: { approvalId: "01a13804c3787f71", intentHash: H("a"), status: "APPROVED" },
  observation: null,
  latched: false,
  userAborted: false,
  ...over,
});

const CARDS: ComputerCardView[] = [
  card("c1", { tool: "computer.start", role: "start", target: null, action: "", argumentsText: "", modality: "", outcome: "" }),
  card("c2", { tool: "computer.state", role: "observe", target: null, action: "", argumentsText: "", modality: "", outcome: "", approval: null, observation: { kind: "state", elements: 41, secureWithheld: 1, maskedRegions: 0, artifactRef: "", artifactExpired: false } }),
  card("c3", {}),
  card("c4", { state: "FAILED", failureCode: "TARGET_STALE", outcome: "", modality: "", verified: null, approval: null, durationMs: 0 }),
  card("c5", { state: "FAILED", failureCode: "APPROVAL_DENIED", outcome: "", modality: "", verified: null, durationMs: 0, approval: { approvalId: "01a13804c3787f72", intentHash: H("b"), status: "DENIED" } }),
  card("c6", { tool: "computer.screenshot", role: "observe", target: null, action: "", argumentsText: "", modality: "", outcome: "", approval: null, observation: { kind: "screenshot", elements: 0, secureWithheld: 0, maskedRegions: 2, artifactRef: H("f"), artifactExpired: true } }),
  card("c7", { state: "UNKNOWN_OUTCOME", outcome: "UNKNOWN", latched: true, verified: null }),
  card("c8", { state: "FAILED", failureCode: "USER_ABORTED", userAborted: true, outcome: "", modality: "", verified: null, approval: null, durationMs: 0 }),
];

export function ComputerGallery({ note }: { note: (text: string) => void }) {
  const [notes, setNotes] = useState<Partial<Record<PermissionName, string>>>({});
  const noop = () => {};
  const open = (pane: PermissionName) => {
    note(`open System Settings: ${pane}`);
    setNotes((n) => ({ ...n, [pane]: "System Settings is open. Turn the switch on there; Modbit does not change it for you." }));
  };
  const ctx: ComputerCardContext = { taskId: "fixture-task-1", cards: new Map(CARDS.map((c) => [c.toolCallId, c])), ordinals: new Map(CARDS.map((c, i) => [c.toolCallId, i + 1])), onResolveLatch: async () => "Resolved." };
  const deck = (list: DockApprovalView[], extra: Partial<Parameters<typeof ApprovalDeck>[0]> = {}) => (
    <ApprovalDeck approvals={list} index={0} onIndex={noop} busy={false} confirming={false} prefix={[]} onPrefix={noop} scope="REPO" onScope={noop} note={null} onRun={() => note("computer approval: approve")} onConfirm={noop} onBack={noop} onSkip={() => note("computer approval: skip")} onAlways={() => note("computer approval: always (must never be offered)")} onChangeMode={noop} openTaskId="fixture-task-1" titleOf={() => "Draft the letter"} onOpenTask={noop} {...extra} />
  );
  const bar = (label: string, i: ComputerPresenceInfo, n = "") => <PresenceBar label={label} info={i} titleOf={() => "Draft the letter"} onOpenTask={noop} onStop={() => note("computer: stop")} busy={false} note={n} />;
  return (
    <section aria-labelledby="g-computer" data-testid="gallery-computer">
      <h2 id="g-computer">Native computer control</h2>
      <p className="meta">Fixtures only: the permission window, presence and Stop, the transcript's action cards and the approval card of a native action.</p>

      <h3>Permission window</h3>
      <div className="gallery-card" data-testid="gallery-computer-perm-allow">
        <PermissionsBody info={info({ accessibility: "DENIED", screenRecording: "UNKNOWN" })} onCheck={() => note("permissions: check again")} pollingEnded={false} onOpenPane={open} notes={notes} />
      </div>
      <div className="gallery-card" data-testid="gallery-computer-perm-ready">
        <PermissionsBody info={info({})} onCheck={noop} pollingEnded={false} onOpenPane={open} notes={{}} />
      </div>
      <div className="gallery-card" data-testid="gallery-computer-perm-ended">
        <PermissionsBody info={info({ accessibility: "DENIED", screenRecording: "DENIED" })} onCheck={noop} pollingEnded onOpenPane={open} notes={{}} />
      </div>
      <div className="gallery-card" data-testid="gallery-computer-perm-nohelper">
        <PermissionsBody info={info({ attached: false, configured: true, detail: "the helper exited" })} onCheck={noop} pollingEnded={false} onOpenPane={open} notes={{}} />
      </div>

      <h3>Presence and Stop</h3>
      <div data-testid="gallery-computer-presence-held">{bar("Computer control: agent in control", info({ controls: [entry({})] }))}</div>
      <div data-testid="gallery-computer-presence-screen">{bar("Computer control: whole screen", info({ controls: [entry({ scope: "SCREEN" }), entry({ taskId: "fixture-task-2", application: "Numbers", state: "PARKED" })] }))}</div>
      <div data-testid="gallery-computer-presence-stopped">{bar("Computer control: stopped", info({ controls: [entry({ state: "USER_ABORTED", controlId: "" })] }), "Stopped. The helper stopped sending input at once, and control stays stopped until the next turn.")}</div>
      <div data-testid="gallery-computer-presence-latched">{bar("Computer control: outcome unknown", info({ controls: [entry({ state: "LATCHED", controlId: "", latch: "computer.press press: ACTUATOR_LOST" })] }))}</div>

      <h3>Action cards</h3>
      <div className="gallery-card" style={{ display: "flex", flexDirection: "column", gap: 8 }} data-testid="gallery-computer-cards">
        {CARDS.map((c) => (
          <ComputerActionCard key={c.toolCallId} toolCallId={c.toolCallId} tool={c.tool} state={c.state} failureCode={c.failureCode} atMs={GALLERY_NOW} highlight={false} rowId={`row-${c.toolCallId}`} ctx={ctx} />
        ))}
      </div>

      <h3>Approval card</h3>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-computer-approval-action">
        {deck([ACTION, GRANT])}
      </div>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-computer-approval-grant">
        {deck([GRANT])}
      </div>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-computer-approval-screen">
        {deck([SCREEN])}
      </div>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-computer-approval-confirm">
        {deck([ACTION], { confirming: true })}
      </div>
      <div className="mb-tray" data-tone="attention" data-testid="gallery-computer-approval-stale">
        {deck([ACTION], { note: { kind: "refused", text: "The Core refused: the effect is no longer the one this card showed. Nothing was decided; the card now shows what is asked." } })}
      </div>
    </section>
  );
}
