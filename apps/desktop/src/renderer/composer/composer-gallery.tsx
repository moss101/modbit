/**
 * The composer's states for the model-free gallery (AFW-K02; REQ-PX-054..056):
 * every tray, menu, chip and the picker, rendered from fixtures with no Core,
 * for design review and the accessibility pass. A QA aid only: the behaviour
 * of each is proven against a real Core in e2e/composer.spec.ts.
 */
import { useState } from "react";
import { Badge, Button } from "@modbit/ui";
import type { ModelCatalogView, PostureView, QueuedInputItem, SendBehaviorState, SlashInventoryView } from "../../shared/composer-types.ts";
import { MentionMenu, SlashMenuView, useSlashItems, type MentionOption } from "./menus.tsx";
import { MODE_CYCLE, MODE_DEFS, queueRows } from "./model.ts";
import { ModelPicker } from "./model-picker.tsx";
import { EducationTray, PolicyTray, QueueTray, SendBehaviorPanel, SideQuestionPanel, StoppedTray, TerminalsChip, TerminalsTray } from "./trays.tsx";

const ONE_PIXEL = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";

const item = (id: string, position: number, text: string, extra: Partial<QueuedInputItem> = {}): QueuedInputItem => ({ inputId: id, position, mode: "FOLLOW_UP", text, state: "QUEUED", model: "", edited: false, sentNow: false, provenance: "", untrusted: false, ...extra });
const QUEUE = queueRows([item("a", 1, "Also update the changelog"), item("b", 2, "Use tabs, not spaces, in the new file", { edited: true }), item("c", 3, "Then run the whole test suite and tell me what failed", { mode: "STEER" })]);

const BEHAVIOR: SendBehaviorState = {
  whileRunning: "QUEUE",
  sendNow: "INTERRUPT",
  whileRunningValues: ["QUEUE", "COLLECT", "STEER", "STOP_AND_SEND"],
  sendNowValues: ["INTERRUPT", "STEER"],
  whileRunningConsequence: "Enter queues the message as its own turn after the current one ends.",
  sendNowConsequence: "Cuts the current model call at a safe point and starts a new turn from the message.",
};

const INVENTORY: SlashInventoryView = {
  dividerAt: 2,
  entries: [
    { kind: "SKILL", id: "core-guide", displayName: "core-guide", description: "How the product itself works.", scope: "SYSTEM", trust: "SYSTEM", trustDetail: "", enabled: true, invocation: "BOTH", builtIn: true },
    { kind: "SKILL", id: "review", displayName: "review", description: "Review the diff on this branch.", scope: "SYSTEM", trust: "SYSTEM", trustDetail: "", enabled: true, invocation: "BOTH", builtIn: true },
    { kind: "SKILL", id: "alpha", displayName: "alpha", description: "A user skill the owner trusts.", scope: "USER", trust: "TRUSTED_BY_OWNER", trustDetail: "", enabled: true, invocation: "BOTH", builtIn: false },
    { kind: "SKILL", id: "zeta", displayName: "zeta", description: "<b>Looks like markup</b> but is only text.", scope: "PROJECT", trust: "UNTRUSTED", trustDetail: "Not trusted: nobody has reviewed this skill's content yet.", enabled: false, invocation: "USER_ONLY", builtIn: false },
  ],
};

const MENTIONS: MentionOption[] = [
  { id: "file:src/lib.rs", group: "Files and folders", label: "lib.rs", detail: "src/lib.rs", mention: { kind: "file", value: "src/lib.rs", label: "src/lib.rs" }, disabledReason: null },
  { id: "dir:src/", group: "Files and folders", label: "src/", detail: "folder, press Enter to open it", mention: null, descend: "src/", disabledReason: null },
  { id: "file:.env", group: "Files and folders", label: ".env", detail: "", mention: null, disabledReason: "Protected by the workspace path policy: it cannot be attached." },
  { id: "term:1", group: "Terminals", label: "cargo watch", detail: "RUNNING", mention: { kind: "terminal", value: "terminal:1", label: "cargo watch" }, disabledReason: null },
  { id: "conv:1", group: "Past conversations", label: "Fix the flaky test", detail: "Done", mention: { kind: "conversation", value: "task:1", label: "Fix the flaky test" }, disabledReason: null },
  { id: "changes", group: "Branch", label: "Changes on this branch", detail: "the task's diff against its base", mention: { kind: "changes", value: "changes", label: "branch changes" }, disabledReason: null },
];

const POSTURE: PostureView = {
  mode: "AGENT",
  modeInForce: "AGENT",
  modeOffset: "0",
  writes: true,
  effectCeiling: "none",
  subagents: false,
  reproductionFirst: false,
  preference: { objective: "BALANCE", effort: "", serviceTier: "", pinEndpoint: "", pinModel: "", offset: "0", appliedOffset: "0", effortApplied: "", serviceTierApplied: "" },
  routing: { outcome: "DIRECT", reasonCode: "NO_ACTIVE_REGISTRY", detail: "", floorMode: "", registryGeneration: "" },
};

const CATALOG: ModelCatalogView = {
  objectives: ["COST", "BALANCE", "INTELLIGENCE"],
  defaultObjective: "BALANCE",
  models: [
    { endpoint: "openai", model: "gpt-5", provider: "openai", reasoning: true, vision: true, contextTokens: 400000, defaultEffort: "", defaultServiceTier: "", credentialAvailable: true, blockedByPolicy: "", pinRefusalCode: "", pinAllowed: true, variants: [
      { effort: "low", serviceTier: "", label: "Low effort", isDefault: false, raisesCost: false },
      { effort: "medium", serviceTier: "", label: "Medium effort", isDefault: true, raisesCost: false },
      { effort: "high", serviceTier: "", label: "High effort", isDefault: false, raisesCost: true },
    ] },
    { endpoint: "openai", model: "gpt-5-mini", provider: "openai", reasoning: true, vision: true, contextTokens: 400000, defaultEffort: "", defaultServiceTier: "", credentialAvailable: true, blockedByPolicy: "block=openai/gpt-5-mini", pinRefusalCode: "POLICY_BLOCKED", pinAllowed: false, variants: [{ effort: "medium", serviceTier: "", label: "Medium effort", isDefault: true, raisesCost: false }] },
    { endpoint: "openai", model: "gpt-4.1", provider: "openai", reasoning: false, vision: true, contextTokens: 1000000, defaultEffort: "", defaultServiceTier: "", credentialAvailable: true, blockedByPolicy: "", pinRefusalCode: "", pinAllowed: true, variants: [{ effort: "", serviceTier: "", label: "Standard", isDefault: true, raisesCost: false }] },
  ],
};

const TERMINALS = [
  { terminalId: "t1", taskId: "x", owner: "agent", title: "cargo watch", state: "RUNNING", exitCode: null, startedAtMs: Date.now() - 125_000, elapsedMs: "125000", bytesSoFar: "0", oldestCursor: "0", replayWindowBytes: "0", pty: true, rows: 24, cols: 80, inputLeaseHolder: "", cwd: "", timedOut: false },
];

export function ComposerGallery({ note }: { note: (t: string) => void }) {
  const slash = useSlashItems(INVENTORY, "", false);
  const [mentionActive, setMentionActive] = useState(0);
  return (
    <section aria-labelledby="g-composer" data-testid="gallery-composer" className="cmp-gallery">
      <h2 id="g-composer">Composer</h2>
      <p className="meta">Mode chips, the typed menus, the trays that dock above the composer and the model picker, from fixtures.</p>

      <h3>Mode chips</h3>
      <div className="gallery-row" data-testid="gallery-mode-chips">
        {MODE_CYCLE.filter((m) => m !== "AGENT").map((m) => (
          <span key={m} className="cmp-mode" data-mode={m}>
            <span className="cmp-chip cmp-mode-btn">
              <span aria-hidden="true">{MODE_DEFS[m].icon}</span> {MODE_DEFS[m].label}
            </span>
          </span>
        ))}
        <span className="cmp-chip">
          Plan <span className="cmp-dim">unconfirmed</span>
        </span>
        <TerminalsChip count={1} open={false} onToggle={() => note("terminals chip")} />
      </div>

      <h3>Attachments and tokens</h3>
      <div className="cmp-attach-row" data-testid="gallery-attachments">
        <span className="cmp-token cmp-attachment">
          <img src={ONE_PIXEL} alt="Thumbnail of label.png" width={20} height={20} />
          label.png <span className="meta"> · stays with the task</span>
        </span>
        <span className="cmp-token">/core-guide</span>
        <span className="cmp-token">file: src/lib.rs</span>
      </div>
      <div role="alert">
        <ul className="cmp-notes">
          <li>setup.exe: A .exe file is not a type that can be attached. Images, PDFs and plain-text files can.</li>
        </ul>
      </div>

      <h3>Queue, education and stopped trays</h3>
      <div style={{ display: "flex", flexDirection: "column", gap: 8, maxWidth: 840 }}>
        <QueueTray rows={QUEUE} behavior={BEHAVIOR} runAlive busy={false} onSendNow={(id) => note(`send now ${id}`)} onEdit={async (id) => (note(`edit ${id}`), true)} onDelete={(id) => note(`delete ${id}`)} onMove={(id, d) => note(`move ${id} ${d}`)} onClear={() => note("clear")} onLeave={() => note("leave")} />
        <EducationTray onKeep={() => note("keep queuing")} onSettings={() => note("settings")} />
        <StoppedTray text="Refactor the parser to use the new token type" canContinue onEdit={() => note("edit message")} onContinue={() => note("continue")} onDismiss={() => note("dismiss")} />
        <PolicyTray model="gpt-5-mini" code="POLICY_BLOCKED" reason="gpt-5-mini is blocked by the organisation model policy (block=openai/gpt-5-mini)" note="" onAuto={() => note("auto")} onCopy={() => note("copy")} onDismiss={() => note("dismiss policy")} />
        <TerminalsTray terminals={TERMINALS} killing={null} onOpen={(id) => note(`open ${id}`)} onKill={async (id) => note(`kill ${id}`)} />
        <SideQuestionPanel onAsk={async () => ({ text: "The parser lives in src/parse.rs.", tokens: "12" })} onAdd={(t) => note(`add ${t}`)} onClose={() => note("close side")} />
        <SendBehaviorPanel behavior={BEHAVIOR} onSet={async (w, n) => note(`behavior ${w} ${n}`)} onClose={() => note("close behavior")} />
      </div>

      <h3>The slash menu and the @ menu</h3>
      <div className="gallery-row" style={{ alignItems: "flex-start" }}>
        <div style={{ position: "relative", width: 520 }} data-testid="gallery-slash">
          <SlashMenuView id="g-slash" items={slash.items} dividerAt={slash.dividerAt} active={3} onActive={() => {}} onChoose={(it) => note(`slash ${it.entry.id}`)} loaded />
        </div>
        <div style={{ position: "relative", width: 420 }} data-testid="gallery-mention">
          <MentionMenu id="g-mention" options={MENTIONS} loading={false} active={mentionActive} onActive={setMentionActive} onChoose={(o) => note(`mention ${o.id}`)} />
        </div>
      </div>

      <h3>The model chip and picker</h3>
      <div className="gallery-row" style={{ minHeight: 40 }} data-testid="gallery-picker">
        <ModelPicker posture={POSTURE} catalog={CATALOG} catalogError={null} disabled={false} onOpen={() => {}} onChoose={(c) => note(`choose ${c.kind}`)} />
        <Badge tone="neutral">routing: direct</Badge>
        <Button size="sm" onClick={() => note("noop")}>
          Noop
        </Button>
      </div>
    </section>
  );
}
