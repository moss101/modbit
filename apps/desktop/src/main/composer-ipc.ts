/**
 * Main's side of the composer (REQ-PX-054, -055, -056): the typed,
 * argument-validated handlers over the Core's task-mode, execution-preference,
 * input-queue, interrupt, send-behaviour, slash-inventory, model-variant,
 * side-question and attachment commands. Every handler validates what the
 * renderer sent before it is forwarded (REQ-EV-0103), forwards it as the one
 * typed command it names, and converts the Core's answer to the plain shapes
 * of shared/composer-types.ts. Nothing is decided and nothing is kept here:
 * the queue's order, the posture a mode derives, whether a model may be pinned
 * and what a side question answers are the Core's.
 */
import { createHash } from "node:crypto";
import type { ExecutionPreferenceView, RoutingOutcomeView, SlashEntry } from "@modbit/surface-protocol";
import { ObjectiveProfile, TaskMode } from "@modbit/surface-protocol";
import { checkAttachment, labelOf, MAX_ATTACHMENT_BYTES } from "../shared/attachments.ts";
import { INPUT_MODES, OBJECTIVES, TASK_MODES, type AttachmentResult, type InputModeId, type InterruptView, type ModeChangedView, type ModelCatalogView, type ObjectiveId, type PostureView, type PreferencePatch, type PreferenceSetView, type QueuedChangeView, type QueueView, type SendBehaviorState, type SideAnswerView, type SlashInventoryView, type TaskModeId } from "../shared/composer-types.ts";
import type { Registrar } from "./conversation-ipc.ts";

export interface ComposerRegistrar extends Registrar {
  /** A small data: image of image bytes (null when the platform cannot make one). */
  thumbnail: (bytes: Uint8Array) => string | null;
  /** The size of a file in bytes and its content, refusing a directory or one over the cap before it is read. */
  readFile: (path: string, maxBytes: number) => { name: string; bytes: Uint8Array };
}

const bad = (what: string): never => {
  throw new Error(`BAD_ARGUMENT: ${what}`);
};

const MAX_TEXT = 20_000;
const INPUT_ID = /^[A-Za-z0-9_.:-]{1,64}$/;
const SKILL_NAME = /^[A-Za-z0-9._-]{1,64}$/;

export function requireText(v: unknown, what = "text"): string {
  if (typeof v !== "string" || v.trim().length === 0 || v.length > MAX_TEXT) return bad(`${what} must be 1..${MAX_TEXT} characters`);
  return v;
}
export function requireInputId(v: unknown): string {
  if (typeof v !== "string" || !INPUT_ID.test(v)) return bad("input id must be 1..64 letters, digits or _ . : -");
  return v;
}
export function requireMode(v: unknown): TaskModeId {
  if (typeof v !== "string" || !(TASK_MODES as readonly string[]).includes(v)) return bad(`mode must be one of ${TASK_MODES.join(", ")}`);
  return v as TaskModeId;
}
export function requireInputMode(v: unknown): InputModeId {
  if (typeof v !== "string" || !(INPUT_MODES as readonly string[]).includes(v)) return bad(`input mode must be one of ${INPUT_MODES.join(", ")}`);
  return v as InputModeId;
}
/** Explicitly named skills for a run: a short list of plain names. */
export function requireSkillNames(v: unknown): string[] {
  if (v === undefined || v === null) return [];
  if (!Array.isArray(v) || v.length > 8 || v.some((s) => typeof s !== "string" || !SKILL_NAME.test(s))) return bad("skills must be up to 8 plain skill names");
  return v as string[];
}

const isEffort = (s: string) => ["low", "medium", "high", "default"].includes(s);
const isTier = (s: string) => /^[A-Za-z0-9_]{1,32}$/.test(s);
const isModelPart = (s: string) => /^[A-Za-z0-9._:/@+-]{1,128}$/.test(s);

export function requirePreferencePatch(v: unknown): PreferencePatch {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return bad("preference must be an object");
  const o = v as Record<string, unknown>;
  const out: PreferencePatch = {};
  if (o.objective !== undefined) {
    if (typeof o.objective !== "string" || !(OBJECTIVES as readonly string[]).includes(o.objective)) return bad(`objective must be one of ${OBJECTIVES.join(", ")}`);
    out.objective = o.objective as ObjectiveId;
  }
  if (o.effort !== undefined) {
    if (typeof o.effort !== "string" || !isEffort(o.effort)) return bad("effort must be low, medium, high or default");
    out.effort = o.effort;
  }
  if (o.serviceTier !== undefined) {
    if (typeof o.serviceTier !== "string" || !(o.serviceTier === "default" || isTier(o.serviceTier))) return bad("service tier must be a tier name or default");
    out.serviceTier = o.serviceTier;
  }
  if (o.pin !== undefined) {
    const p = o.pin as Record<string, unknown> | null;
    if (typeof p !== "object" || p === null || typeof p.endpoint !== "string" || typeof p.model !== "string" || !isModelPart(p.endpoint) || !isModelPart(p.model)) return bad("a pin names an endpoint and a model");
    out.pin = { endpoint: p.endpoint, model: p.model };
  }
  if (o.clearPin !== undefined) {
    if (typeof o.clearPin !== "boolean") return bad("clearPin must be a boolean");
    out.clearPin = o.clearPin;
  }
  if (out.pin && out.clearPin) return bad("a pin is either set or cleared in one command");
  if (Object.keys(out).length === 0) return bad("a preference names an objective, an effort, a tier or a pin");
  return out;
}

const routingOf = (r: RoutingOutcomeView | undefined) => ({ outcome: r?.outcome ?? "", reasonCode: r?.reasonCode ?? "", detail: r?.detail ?? "", floorMode: r?.floorMode ?? "", registryGeneration: r?.registryGeneration ?? "" });
const preferenceOf = (p: ExecutionPreferenceView | undefined): PostureView["preference"] => ({
  objective: p && p.objective !== ObjectiveProfile.UNSPECIFIED ? (ObjectiveProfile[p.objective] ?? "") : "",
  effort: p?.effort ?? "",
  serviceTier: p?.serviceTier ?? "",
  pinEndpoint: p?.pinEndpoint ?? "",
  pinModel: p?.pinModel ?? "",
  offset: (p?.offset ?? 0n).toString(),
  appliedOffset: (p?.appliedOffset ?? 0n).toString(),
  effortApplied: p?.effortApplied ?? "",
  serviceTierApplied: p?.serviceTierApplied ?? "",
});
const modeName = (m: TaskMode): TaskModeId => ((TASK_MODES as readonly string[]).includes(TaskMode[m] ?? "") ? (TaskMode[m] as TaskModeId) : "AGENT");

export function slashView(entries: readonly SlashEntry[], dividerAt: number): SlashInventoryView {
  return {
    dividerAt,
    entries: entries.map((e) => ({ kind: e.kind, id: e.id, displayName: e.displayName, description: e.description, scope: e.scope, trust: e.trust, trustDetail: e.trustDetail, enabled: e.enabled, invocation: e.invocation, builtIn: e.builtIn })),
  };
}

const MAX_BYTES_FROM_RENDERER = MAX_ATTACHMENT_BYTES + 1;

/** The commands of the composer. */
export function registerComposerHandlers(r: ComposerRegistrar): void {
  r.handle("composer:posture", async (...a): Promise<PostureView> => {
    const v = await r.client().taskPosture(r.taskId(a[0]));
    return {
      mode: modeName(v.mode),
      modeInForce: modeName(v.modeInForce),
      modeOffset: v.modeOffset.toString(),
      writes: v.posture?.writes ?? true,
      effectCeiling: v.posture?.effectCeiling ?? "",
      subagents: v.posture?.subagents ?? false,
      reproductionFirst: v.posture?.reproductionFirst ?? false,
      preference: preferenceOf(v.preference),
      routing: routingOf(v.routing),
    };
  });
  r.handle("composer:setMode", async (...a): Promise<ModeChangedView> => {
    const [sessionId, taskId, mode] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const m = requireMode(mode);
    const c = r.client();
    await r.lease(c, sid);
    const v = await c.setTaskMode(sid, tid, TaskMode[m], "set from the composer");
    return { mode: modeName(v.mode), previousMode: modeName(v.previousMode), effective: v.effective, offset: v.offset.toString() };
  });
  r.handle("composer:setPreference", async (...a): Promise<PreferenceSetView> => {
    const [sessionId, taskId, patch] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const p = requirePreferencePatch(patch);
    const c = r.client();
    await r.lease(c, sid);
    const v = await c.setExecutionPreference(sid, tid, { ...(p.objective ? { objective: ObjectiveProfile[p.objective] } : {}), ...(p.effort !== undefined ? { effort: p.effort } : {}), ...(p.serviceTier !== undefined ? { serviceTier: p.serviceTier } : {}), ...(p.pin ? { pin: p.pin } : {}), ...(p.clearPin ? { clearPin: true } : {}) });
    return { effective: v.effective, offset: v.offset.toString(), routing: routingOf(v.routing), preference: preferenceOf(v.preference) };
  });
  r.handle("composer:variants", async (...a): Promise<ModelCatalogView> => {
    const task = a[0] === undefined || a[0] === null || a[0] === "" ? undefined : r.taskId(a[0]);
    const v = await r.client().listModelVariants(task);
    return {
      objectives: v.objectives,
      defaultObjective: v.defaultObjective,
      models: v.models.map((m) => ({
        endpoint: m.endpoint,
        model: m.model,
        provider: m.provider,
        reasoning: m.reasoning,
        vision: m.vision,
        contextTokens: m.contextTokens,
        defaultEffort: m.defaultEffort,
        defaultServiceTier: m.defaultServiceTier,
        variants: m.variants.map((x) => ({ effort: x.effort, serviceTier: x.serviceTier, label: x.label, isDefault: x.isDefault, raisesCost: x.raisesCost })),
        credentialAvailable: m.credentialAvailable,
        blockedByPolicy: m.blockedByPolicy,
        pinRefusalCode: m.pinRefusalCode,
        pinAllowed: m.pinAllowed,
      })),
    };
  });
  r.handle("composer:slash", async (...a): Promise<SlashInventoryView> => {
    const task = a[0] === undefined || a[0] === null || a[0] === "" ? undefined : r.taskId(a[0]);
    const v = await r.client().listSkills(task);
    return slashView(v.slash, v.slashDividerAt);
  });

  // ---- the input queue and the typed interrupt (PX-050) ----
  r.handle("composer:queue", async (...a): Promise<QueueView> => {
    const v = await r.client().listQueuedInputs(r.taskId(a[0]), false);
    return {
      queued: v.queued,
      runAlive: v.runAlive,
      offset: v.offset.toString(),
      items: v.items.map((i) => ({ inputId: i.inputId, position: i.position, mode: i.mode, text: i.text, state: i.state, model: i.model, edited: i.edited, sentNow: i.sentNow, provenance: i.provenance, untrusted: i.untrusted })),
    };
  });
  r.handle("composer:queueInput", async (...a) => {
    const [sessionId, taskId, text, mode, inputId] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const t = requireText(text);
    const m = requireInputMode(mode);
    const id = requireInputId(inputId);
    const c = r.client();
    await r.lease(c, sid);
    // The command id is a function of the task and the input id: the same send retried (after a crash, a reload) replays and is one record.
    const commandId = new Uint8Array(createHash("sha256").update(`composer.queue:${tid}:${id}`).digest().subarray(0, 16));
    const v = await c.queueInput(sid, tid, t, m, id, commandId);
    return { inputId: id, sequence: v.sequence.toString(), offset: v.offset.toString() };
  });
  const changed = (v: { inputId: string; state: string; position: number; offset: bigint }): QueuedChangeView => ({ inputId: v.inputId, state: v.state, position: v.position, offset: v.offset.toString() });
  r.handle("composer:editQueued", async (...a) => {
    const [sessionId, taskId, inputId, change] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const id = requireInputId(inputId);
    const o = (change ?? {}) as Record<string, unknown>;
    if (typeof o !== "object" || Array.isArray(o)) bad("change must be an object");
    const text = o.text === undefined ? undefined : requireText(o.text);
    const mode = o.mode === undefined ? undefined : (["STEER", "COLLECT", "FOLLOW_UP"].includes(o.mode as string) ? (o.mode as string) : bad("mode must be STEER, COLLECT or FOLLOW_UP"));
    if (text === undefined && mode === undefined) bad("an edit changes the text or the mode");
    const c = r.client();
    await r.lease(c, sid);
    return changed(await c.editQueuedInput(sid, tid, id, { ...(text !== undefined ? { text } : {}), ...(mode !== undefined ? { mode } : {}) }));
  });
  r.handle("composer:removeQueued", async (...a) => {
    const [sessionId, taskId, inputId] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const id = requireInputId(inputId);
    const c = r.client();
    await r.lease(c, sid);
    return changed(await c.removeQueuedInput(sid, tid, id, "removed from the queue tray"));
  });
  r.handle("composer:reorderQueued", async (...a) => {
    const [sessionId, taskId, inputId, before] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const id = requireInputId(inputId);
    const b = before === undefined || before === null || before === "" ? "" : requireInputId(before);
    const c = r.client();
    await r.lease(c, sid);
    return changed(await c.reorderQueuedInput(sid, tid, id, b));
  });
  const interrupt = (v: { kind: string; runAlive: boolean; interruptId: string; offset: bigint }): InterruptView => ({ kind: v.kind, runAlive: v.runAlive, interruptId: v.interruptId, offset: v.offset.toString() });
  r.handle("composer:sendNow", async (...a) => {
    const [sessionId, taskId, inputId, interruptId] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const id = requireInputId(inputId);
    const ii = interruptId === undefined || interruptId === null || interruptId === "" ? "" : requireInputId(interruptId);
    const c = r.client();
    await r.lease(c, sid);
    return interrupt(await c.sendQueuedInputNow(sid, tid, id, ii, "send now from the queue tray"));
  });
  r.handle("composer:interrupt", async (...a) => {
    const [sessionId, taskId, interruptId] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const ii = interruptId === undefined || interruptId === null || interruptId === "" ? "" : requireInputId(interruptId);
    const c = r.client();
    await r.lease(c, sid);
    return interrupt(await c.interruptTask(sid, tid, ii, "stopped by the person"));
  });
  const behavior = (v: { whileRunning: string; sendNow: string; whileRunningValues: string[]; sendNowValues: string[]; whileRunningConsequence: string; sendNowConsequence: string }): SendBehaviorState => ({ whileRunning: v.whileRunning, sendNow: v.sendNow, whileRunningValues: v.whileRunningValues, sendNowValues: v.sendNowValues, whileRunningConsequence: v.whileRunningConsequence, sendNowConsequence: v.sendNowConsequence });
  r.handle("composer:sendBehavior", async (...a) => behavior(await r.client().sendBehavior(r.taskId(a[0]))));
  r.handle("composer:setSendBehavior", async (...a) => {
    const [sessionId, taskId, whileRunning, sendNow] = a;
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const w = whileRunning === undefined || whileRunning === null ? "" : whileRunning;
    const n = sendNow === undefined || sendNow === null ? "" : sendNow;
    if (typeof w !== "string" || !["", "QUEUE", "COLLECT", "STEER", "STOP_AND_SEND"].includes(w)) bad("while-running must be QUEUE, COLLECT, STEER or STOP_AND_SEND");
    if (typeof n !== "string" || !["", "INTERRUPT", "STEER"].includes(n)) bad("send-now must be INTERRUPT or STEER");
    const c = r.client();
    await r.lease(c, sid);
    return behavior(await c.setSendBehavior(sid, tid, w as string, n as string));
  });
  r.handle("composer:sideQuestion", async (...a): Promise<SideAnswerView> => {
    const v = await r.client().askSideQuestion(r.taskId(a[0]), requireText(a[1], "question"));
    return { text: v.text, inputTokens: v.inputTokens.toString(), outputTokens: v.outputTokens.toString(), snapshotMessages: v.snapshotMessages, lastOffset: v.lastOffset.toString() };
  });

  // ---- attachments (AFW-D09): every byte goes through the Core's IngestAttachment ----
  const ingest = async (sessionId: unknown, taskId: unknown, name: string, bytes: Uint8Array, declaredMime: string): Promise<AttachmentResult> => {
    const sid = r.sessionId(sessionId);
    const tid = r.taskId(taskId);
    const label = labelOf(name);
    const verdict = checkAttachment(label, bytes, declaredMime);
    if (!verdict.ok) throw new Error(`ATTACHMENT_REFUSED: ${verdict.reason ?? "this file cannot be attached"}`);
    const c = r.client();
    await r.lease(c, sid);
    const v = await c.ingestAttachment(sid, tid, label, bytes);
    return { attachmentId: v.attachmentId, kind: v.kind, mime: v.mime, contentRef: v.contentRef, replayed: v.replayed, name: label, bytes: bytes.byteLength, thumbnail: verdict.kind === "image" ? r.thumbnail(bytes) : null };
  };
  r.handle("composer:attachPath", async (...a) => {
    const [sessionId, taskId, path, declaredMime] = a;
    if (typeof path !== "string" || path.length === 0 || path.length > 4096) bad("a file path is required");
    const mime = typeof declaredMime === "string" && declaredMime.length <= 128 ? declaredMime : "";
    const f = r.readFile(path as string, MAX_ATTACHMENT_BYTES);
    return ingest(sessionId, taskId, f.name, f.bytes, mime);
  });
  r.handle("composer:attachBytes", async (...a) => {
    const [sessionId, taskId, name, data, declaredMime] = a;
    if (typeof name !== "string" || name.length === 0 || name.length > 512) bad("a name is required");
    const bytes = data instanceof Uint8Array ? data : data instanceof ArrayBuffer ? new Uint8Array(data) : null;
    if (bytes === null) return bad("the file's bytes are required");
    if ((bytes as Uint8Array).byteLength > MAX_BYTES_FROM_RENDERER) bad("the file is too large to attach");
    const mime = typeof declaredMime === "string" && declaredMime.length <= 128 ? declaredMime : "";
    return ingest(sessionId, taskId, name as string, bytes as Uint8Array, mime);
  });
}
