/**
 * Main's side of native computer control's desktop surface (PX-074; docs/66
 * CUC-D05, CUC-E05, CUC-H01): typed, argument-validated handlers over the
 * Core's own commands. Nothing here touches the screen or the input devices:
 * there is no capture and no input API in this process or in the renderer, the
 * actuator is the Core's, and Stop is a Core command (`ComputerStop`) that
 * reaches the actuator at once. The Core says who holds the machine, which
 * permissions the actuator reports, whether a stop is sticky and whether a
 * latch stands; this file validates what the renderer sent, calls the one
 * client, joins the Core's records into the views the cards show, and converts
 * the answer to plain shapes. The "open System Settings" link is the one
 * thing it does on the operating system's side, and it opens a pane: it never
 * changes a setting.
 */
import type { CoreClient } from "@modbit/ide-adapter-core";
import { computerStop, getComputerRuntime, listComputerActivity, readObjectRange, resolveComputerLatch, getComputerPresence } from "@modbit/ide-adapter-core";
import { TranscriptDensity, type ComputerActivityEntry, type ComputerPresenceView, type TranscriptRow } from "@modbit/surface-protocol";
import type { ComputerCardView, ComputerCardsInfo, ComputerFrameInfo, ComputerPresenceInfo, ComputerStopInfo, OpenSettingsInfo, PermissionName, PermissionState, PresenceState } from "../shared/computer-types.ts";
import { PRESENCE_STATES } from "../shared/computer-types.ts";
import { intentOf } from "./approval-intent.ts";
import { computerIntentOf } from "./computer-intent.ts";
import { SETTINGS_LINKS, validArtifactRef, validNote, validPane } from "./computer-args.ts";
import { argumentsOf } from "./control-ipc.ts";
import { bad, validBool } from "./control-args.ts";
import type { Registrar } from "./conversation-ipc.ts";

const hexOf = (b: Uint8Array | undefined): string => Buffer.from(b ?? []).toString("hex");
const norm = (id: string): string => id.replace(/-/g, "").toLowerCase();
const permission = (s: string): PermissionState => (s === "GRANTED" || s === "DENIED" ? s : "UNKNOWN");
const clip = (s: string, n: number): string => (s.length > n ? `${s.slice(0, n)}…` : s);

export function presenceInfo(v: ComputerPresenceView): ComputerPresenceInfo {
  return {
    configured: v.configured,
    attached: v.attached,
    actuatorName: v.actuatorName,
    actuatorVersion: v.actuatorVersion,
    platform: v.platform,
    detail: clip(v.detail, 300),
    accessibility: permission(v.permissionAccessibility),
    screenRecording: permission(v.permissionScreenRecording),
    controls: v.control.map((p) => ({
      taskId: hexOf(p.taskId?.value),
      controlId: p.controlId,
      application: clip(p.application, 200),
      bundleId: p.bundleId,
      scope: p.scope === "APP" || p.scope === "SCREEN" ? p.scope : "",
      state: ((PRESENCE_STATES as readonly string[]).includes(p.state) ? p.state : "CONTROLLING") as PresenceState,
      grantExpiresMs: Number(p.grantExpiresMs),
      latch: clip(p.latch, 400),
      latchToolCallId: p.latchToolCallId,
      actions: p.actions,
      screenshots: p.screenshots,
      observations: p.observations,
    })),
  };
}

// ----------------------------------------------------------- the action cards

type ToolRowFacts = { toolCallId: string; toolName: string; state: string; failureCode: string; argumentsRef: string };

function computerRows(rows: readonly TranscriptRow[], out: ToolRowFacts[]): void {
  for (const r of rows) {
    if (r.facts.case === "tool" && r.facts.value.toolName.startsWith("computer.")) {
      const f = r.facts.value;
      out.push({ toolCallId: norm(f.toolCallId), toolName: f.toolName, state: r.hints?.status || f.state, failureCode: f.failureCode, argumentsRef: f.argumentsRef });
    }
    computerRows(r.children, out);
  }
}

const ROLE: Record<string, ComputerCardView["role"]> = {
  "computer.start": "start",
  "computer.start_screen": "start",
  "computer.apps": "observe",
  "computer.resolve": "observe",
  "computer.state": "observe",
  "computer.screenshot": "observe",
  "computer.wait": "observe",
  "computer.release": "release",
  "computer.press": "act",
  "computer.set_value": "act",
  "computer.click": "act",
  "computer.move": "act",
  "computer.drag": "act",
  "computer.type": "act",
  "computer.key": "act",
  "computer.scroll": "act",
};

/** The element a model named in an input's arguments (`<snapshot>/<id>`), for a call that never reached an approval. */
function elementOf(argsJson: string | null): string {
  if (!argsJson) return "";
  try {
    const a: unknown = JSON.parse(argsJson);
    if (a && typeof a === "object" && !Array.isArray(a)) {
      const e = (a as Record<string, unknown>).element;
      if (typeof e === "string") return clip(e, 120);
    }
  } catch {
    // Arguments that do not parse show no element.
  }
  return "";
}

/**
 * The task's computer tool calls as cards: each call of the transcript joined
 * with the Core's activity events (outcome, rung, masked frame, counts), the
 * approval that bound it (identity, window, element, arguments) and the
 * runtime's latch. Counts, kinds and references only (CUC-F03): the contents
 * of a frame are never read here.
 */
export async function computerCards(c: CoreClient, sessionId: string, taskId: string): Promise<ComputerCardsInfo> {
  const [activity, approvals, runtime] = await Promise.all([listComputerActivity(c, taskId), c.listApprovals(sessionId), getComputerRuntime(c, taskId)]);
  const head = await c.getTranscript(taskId, { density: TranscriptDensity.DETAILED, afterRow: 0, limit: 1 });
  const page = await c.getTranscript(taskId, { density: TranscriptDensity.DETAILED, afterRow: Math.max(0, head.totalRows - 500), limit: 500 });
  const calls: ToolRowFacts[] = [];
  computerRows(page.rows, calls);

  const byCall = new Map<string, ComputerActivityEntry[]>();
  for (const e of activity.entries) {
    const k = norm(e.toolCallId);
    if (!k) continue;
    byCall.set(k, [...(byCall.get(k) ?? []), e]);
  }
  const approvalByCall = new Map<string, (typeof approvals.approvals)[number]>();
  for (const a of approvals.approvals) if (hexOf(a.taskId?.value) === taskId && a.toolName.startsWith("computer.")) approvalByCall.set(hexOf(a.toolCallId?.value), a);

  const latchCall = norm(runtime.latchToolCallId);
  const cards: ComputerCardView[] = [];
  let app = { name: "", bundle: "", scope: "" };
  for (const call of calls) {
    const entries = byCall.get(call.toolCallId) ?? [];
    const started = entries.find((e) => e.kind === "SESSION_STARTED");
    const action = entries.find((e) => e.kind === "ACTION");
    const observed = entries.find((e) => e.kind === "OBSERVED");
    const approval = approvalByCall.get(call.toolCallId) ?? null;
    const intent = approval ? computerIntentOf(approval.scopeJson) : null;
    if (started) app = { name: started.application, bundle: started.bundleId, scope: started.scope };
    else if (intent && intent.kind === "observation_grant") app = { name: intent.applicationName, bundle: intent.application.bundleId, scope: intent.scope };
    const args = call.argumentsRef ? await argumentsOf(c, taskId, call.toolCallId) : null;
    const role = ROLE[call.toolName] ?? "other";
    const element = intent?.target?.kind === "element" ? { role: intent.target.role, name: intent.target.name, id: `${intent.target.snapshot}/${intent.target.id}` } : null;
    const named = !element && role === "act" ? elementOf(args) : "";
    const code = call.failureCode || action?.code || "";
    cards.push({
      toolCallId: call.toolCallId,
      tool: call.toolName,
      state: call.state,
      failureCode: call.failureCode,
      role,
      application: clip(intent?.applicationName || app.name, 200),
      bundleId: intent?.application.bundleId || app.bundle,
      scope: intent?.scope || app.scope,
      target: element ?? (named ? { role: "", name: "", id: named } : null),
      action: intent?.action || action?.detail || "",
      argumentsText: intent ? intent.argumentsText : args ? intentOf(args).argsPreview : "",
      modality: action?.modality || intent?.modality || "",
      outcome: action?.outcome ?? "",
      verified: action?.verifiedKnown ? action.verified : null,
      durationMs: Number(action?.durationMs ?? 0n),
      approval: approval ? { approvalId: hexOf(approval.approvalId?.value), intentHash: approval.intentHash, status: approval.status } : null,
      observation: observed ? { kind: observed.detail, elements: observed.elements, secureWithheld: observed.secureWithheld, maskedRegions: observed.maskedRegions, artifactRef: observed.artifactRef, artifactExpired: observed.artifactExpired } : null,
      latched: runtime.latch !== "" && latchCall === call.toolCallId,
      userAborted: code === "USER_ABORTED",
    });
  }
  return { taskId, cards };
}

// ---------------------------------------------------------------- the frames

const MAX_FRAME_BYTES = 8 * 1024 * 1024;

/** A stored frame, read through the Core's object store: the bytes the Core kept after masking, nothing else. */
async function readFrame(c: CoreClient, ref: string): Promise<Uint8Array> {
  const parts: Uint8Array[] = [];
  let offset = 0n;
  for (;;) {
    const chunk = await readObjectRange(c, ref, offset, 1024n * 1024n);
    if (chunk.totalBytes > BigInt(MAX_FRAME_BYTES)) bad("FRAME_TOO_LARGE: the stored frame is larger than the surface shows");
    parts.push(chunk.data);
    offset += BigInt(chunk.data.byteLength);
    if (chunk.data.byteLength === 0 || offset >= chunk.totalBytes) break;
  }
  return Buffer.concat(parts);
}

const PNG_MAGIC = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];

export interface ComputerDeps {
  /** Hands an address to the operating system (Electron's `shell.openExternal`). */
  openExternal: (url: string) => Promise<void>;
  /** The automation switch: record the address instead of opening it, so a test run never opens System Settings. */
  suppressOpen: boolean;
  platform: string;
  /** A reduced copy of a PNG as a data URL (Electron's nativeImage); null when it is not an image. */
  thumbnail: (bytes: Uint8Array, maxWidth: number) => { dataUrl: string; width: number; height: number } | null;
  /** The size of a PNG without decoding it into the renderer. */
  size: (bytes: Uint8Array) => { width: number; height: number } | null;
}

export const openedSettings: { pane: PermissionName; url: string; atMs: number }[] = [];

export function registerComputerHandlers(r: Registrar, deps: ComputerDeps): void {
  // A Core that is restarting holds the machine for nobody (the actuator stops with the Core's pipe): the answer is "no answer", never a stale one.
  r.handle("computer:presence", async (): Promise<ComputerPresenceInfo | null> => {
    let c: CoreClient;
    try {
      c = r.client();
    } catch (e) {
      if ((e as Error).message.startsWith("CORE_UNAVAILABLE")) return null;
      throw e;
    }
    return presenceInfo(await getComputerPresence(c));
  });

  // The on-screen Stop (CUC-D05): the Core stops the actuator at once, closes the session and makes the stop sticky for the run.
  r.handle("computer:stop", async (...a): Promise<ComputerStopInfo> => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const reason = validNote(a[2], "reason") || "on-screen Stop";
    const c = r.client();
    await r.lease(c, sid);
    const s = await computerStop(c, sid, tid, reason);
    return { sessionsClosed: s.sessionsClosed, actuatorStopped: s.actuatorStopped };
  });

  r.handle("computer:resolveLatch", async (...a) => {
    const sid = r.sessionId(a[0]);
    const tid = r.taskId(a[1]);
    const note = validNote(a[2], "note");
    const c = r.client();
    await r.lease(c, sid);
    const res = await resolveComputerLatch(c, sid, tid, note);
    return { wasLatched: res.wasLatched };
  });

  r.handle("computer:cards", async (...a): Promise<ComputerCardsInfo> => computerCards(r.client(), r.sessionId(a[0]), r.taskId(a[1])));

  // A masked frame by reference, only one the Core's activity for this task names (never an arbitrary object).
  r.handle("computer:frame", async (...a): Promise<ComputerFrameInfo | null> => {
    const tid = r.taskId(a[0]);
    const ref = validArtifactRef(a[1]);
    const full = a[2] === undefined ? false : validBool(a[2], "full");
    const c = r.client();
    const named = (await listComputerActivity(c, tid)).entries.find((e) => e.artifactRef === ref);
    if (!named) bad("UNKNOWN_ARTIFACT: that frame is not one of this task's observations");
    if (named!.artifactExpired) return null;
    const bytes = await readFrame(c, ref);
    if (!PNG_MAGIC.every((m, i) => bytes[i] === m)) bad("NOT_A_FRAME: the stored object is not an image");
    if (full) {
      const sz = deps.size(bytes);
      return { dataUrl: `data:image/png;base64,${Buffer.from(bytes).toString("base64")}`, width: sz?.width ?? 0, height: sz?.height ?? 0, full: true };
    }
    const t = deps.thumbnail(bytes, 320);
    return t ? { ...t, full: false } : null;
  });

  // CUC-E05: the deep link opens a pane of System Settings and nothing else. It changes no setting; the person does.
  r.handle("computer:openSettings", async (...a): Promise<OpenSettingsInfo> => {
    const pane = validPane(a[0]);
    const url = SETTINGS_LINKS[pane];
    if (deps.suppressOpen) {
      openedSettings.push({ pane, url, atMs: Date.now() });
      if (openedSettings.length > 50) openedSettings.splice(0, openedSettings.length - 50);
      return { pane, opened: true, note: "Recorded, not opened (automation switch)." };
    }
    if (deps.platform !== "darwin") return { pane, opened: false, note: "This computer has no macOS System Settings. Grant the permission where your system keeps it." };
    try {
      await deps.openExternal(url);
      return { pane, opened: true, note: "System Settings is open. Turn the switch on there; Modbit does not change it for you." };
    } catch {
      return { pane, opened: false, note: "System Settings could not be opened. Open it yourself: Privacy & Security." };
    }
  });
}
