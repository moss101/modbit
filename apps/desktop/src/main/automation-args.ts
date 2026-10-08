/**
 * Argument validation for the automation handlers (REQ-PX-086; REQ-EV-0103):
 * everything the renderer sends is hostile until it is checked here, and what
 * passes is the narrowest shape the Core's command takes. These checks narrow;
 * they never decide. Whether a definition is valid, what an approval must list
 * and whether a run may start are the Core's answers, not this file's.
 */
import { isAbsolute } from "node:path";
import type { AutoEnableInput, AutoRunInput } from "../shared/automation-types.ts";
import { bad } from "./control-args.ts";

const HEX32 = /^[0-9a-f]{32}$/;
const HEX64 = /^[0-9a-f]{64}$/;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;
const EFFECTS = ["read_only", "reversible_write", "protected_write", "external_side_effect"] as const;

/** The most a definition document is passed on at; the Core's own bound (64 KiB) yields the typed issue. */
export const MAX_DEFINITION_CHARS = 256 * 1024;
const MAX_PAYLOAD_CHARS = 256 * 1024;

export function validAutomationId(v: unknown): string {
  if (typeof v !== "string" || !HEX32.test(v)) return bad("automationId must be 32 lowercase hex characters");
  return v;
}

/** An empty id names every definition (the global switch); anything else must be an id. */
export function validOptionalAutomationId(v: unknown): string {
  if (v === undefined || v === null || v === "") return "";
  return validAutomationId(v);
}

export function validDefinitionJson(v: unknown): string {
  if (typeof v !== "string") return bad("the definition must be text");
  if (v.length > MAX_DEFINITION_CHARS) return bad(`the definition is at most ${MAX_DEFINITION_CHARS} characters`);
  return v;
}

export function validWorkspaceRoot(v: unknown): string {
  if (typeof v !== "string" || v.trim().length === 0 || v.length > 4096 || CONTROL.test(v)) return bad("workspaceRoot must be an absolute folder path");
  if (!isAbsolute(v.trim())) return bad("workspaceRoot must be an absolute folder path");
  return v.trim();
}

export function validNote(v: unknown): string {
  if (v === undefined || v === null) return "";
  if (typeof v !== "string") return bad("note must be text");
  return v.slice(0, 512);
}

export function validDispatchKey(v: unknown): string {
  if (typeof v !== "string" || !/^[0-9a-f]{16,128}$/.test(v)) return bad("dispatchKey must be a run's key");
  return v;
}

function list(v: unknown, what: string): string[] {
  if (!Array.isArray(v) || v.length > 256) return bad(`${what} must be a list of at most 256 entries`);
  return v.map((x) => {
    if (typeof x !== "string" || x.length === 0 || x.length > 512 || CONTROL.test(x)) return bad(`each entry of ${what} must be 1..512 printable characters`);
    return x;
  });
}

/**
 * The approval as the dialog showed it: the version, the full hash and the
 * exact lists. Nothing is defaulted and nothing is looked up here, so an
 * approval that names less than the dialog showed cannot be forwarded.
 */
export function validEnableInput(v: unknown): AutoEnableInput {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return bad("the approval must be an object");
  const o = v as Record<string, unknown>;
  const automationId = validAutomationId(o.automationId);
  if (typeof o.version !== "number" || !Number.isInteger(o.version) || o.version < 1 || o.version > 1_000_000) return bad("version must be a whole number from 1");
  if (typeof o.definitionHash !== "string" || !HEX64.test(o.definitionHash)) return bad("definitionHash must be the 64-hex-digit hash the dialog showed");
  if (typeof o.effects !== "string" || !(EFFECTS as readonly string[]).includes(o.effects)) return bad(`effects must be one of ${EFFECTS.join(", ")}`);
  return { automationId, version: o.version, definitionHash: o.definitionHash, effects: o.effects, capabilities: list(o.capabilities, "capabilities"), paths: list(o.paths, "paths"), hosts: list(o.hosts, "hosts") };
}

function jsonObjectText(v: unknown, what: string, max: number, needObject: boolean): string {
  if (v === undefined || v === null || v === "") return "";
  if (typeof v !== "string" || v.length > max) return bad(`${what} must be JSON text of at most ${max} characters`);
  let parsed: unknown;
  try {
    parsed = JSON.parse(v);
  } catch {
    return bad(`${what} must be valid JSON`);
  }
  if (needObject && (typeof parsed !== "object" || parsed === null || Array.isArray(parsed))) return bad(`${what} must be a JSON object`);
  return v;
}

export function validRunInput(v: unknown): AutoRunInput {
  if (typeof v !== "object" || v === null || Array.isArray(v)) return bad("the run request must be an object");
  const o = v as Record<string, unknown>;
  const automationId = validAutomationId(o.automationId);
  const text = (x: unknown, what: string, max: number): string => {
    if (x === undefined || x === null || x === "") return "";
    if (typeof x !== "string" || x.length > max || CONTROL.test(x)) return bad(`${what} must be at most ${max} printable characters`);
    return x;
  };
  if (o.test !== undefined && typeof o.test !== "boolean") return bad("test must be a boolean");
  const source = text(o.source, "source", 32);
  if (source !== "" && source !== "forge" && source !== "webhook") return bad("source must be forge or webhook");
  return {
    automationId,
    triggerId: text(o.triggerId, "triggerId", 64),
    inputsJson: jsonObjectText(o.inputsJson, "inputsJson", 64 * 1024, true),
    test: o.test === true,
    payloadJson: jsonObjectText(o.payloadJson, "payloadJson", MAX_PAYLOAD_CHARS, false),
    source,
    event: text(o.event, "event", 128),
    eventId: text(o.eventId, "eventId", 200),
  };
}

/** A command id the renderer keeps so a retried create acts once: 32 hex characters (it becomes the new definition's id). */
export function validCommandHex(v: unknown): Uint8Array | undefined {
  if (v === undefined || v === null || v === "") return undefined;
  if (typeof v !== "string" || !HEX32.test(v)) return bad("commandId must be 32 hex characters");
  return new Uint8Array(Buffer.from(v, "hex"));
}
