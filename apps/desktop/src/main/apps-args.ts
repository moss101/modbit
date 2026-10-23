/**
 * Argument validation for the apps panel's IPC calls (REQ-PX-048, docs/81):
 * every value the renderer sends is hostile until these accept it. Pure, so
 * the node tests cover each refusal without Electron.
 */
const HEX32 = /^[0-9a-f]{32}$/;
/** Core-minted terminal ids and attach ids are short opaque tokens (hex today); nothing else gets through. */
const HANDLE = /^[0-9A-Za-z_-]{1,64}$/;
const DIGITS = /^\d{1,20}$/;

export const MAX_WRITE_BYTES = 64 * 1024;
export const MAX_TERMINAL_DIMENSION = 1000;
export const MAX_WINDOW_BYTES = 4 * 1024 * 1024;

const bad = (what: string): never => {
  throw new Error(`BAD_ARGUMENT: ${what}`);
};

export function requireHex32(v: unknown, what: string): string {
  return typeof v === "string" && HEX32.test(v) ? v : bad(`${what} must be 32 hex chars`);
}

export function requireHandle(v: unknown, what: string): string {
  return typeof v === "string" && HANDLE.test(v) ? v : bad(`${what} must be a terminal handle`);
}

/** A byte cursor as a decimal string (it is 64-bit on the wire). */
export function requireCursor(v: unknown, what = "cursor"): bigint {
  if (typeof v === "number" && Number.isSafeInteger(v) && v >= 0) return BigInt(v);
  return typeof v === "string" && DIGITS.test(v) ? BigInt(v) : bad(`${what} must be a non-negative integer`);
}

export function requireDimension(v: unknown, what: string): number {
  return typeof v === "number" && Number.isInteger(v) && v >= 1 && v <= MAX_TERMINAL_DIMENSION ? v : bad(`${what} must be 1..${MAX_TERMINAL_DIMENSION}`);
}

export function requireBool(v: unknown, what: string): boolean {
  return typeof v === "boolean" ? v : bad(`${what} must be true or false`);
}

/** Keystrokes: text or bytes, at most 64 KiB. */
export function requireKeystrokes(v: unknown): Uint8Array {
  const bytes = typeof v === "string" ? new TextEncoder().encode(v) : v instanceof Uint8Array ? v : bad("input must be text or bytes");
  if (bytes.length === 0 || bytes.length > MAX_WRITE_BYTES) bad(`input must be 1..${MAX_WRITE_BYTES} bytes`);
  return bytes;
}

export function optionalWindow(v: unknown): bigint {
  if (v === undefined || v === null) return 0n;
  const n = requireCursor(v, "windowBytes");
  return n > BigInt(MAX_WINDOW_BYTES) ? bad("windowBytes is at most 4 MiB") : n;
}

/**
 * A workspace-relative path for the Files app: no absolute form, no `..`
 * component, no NUL, no drive or UNC prefix. The Core's path policy is the
 * authority (symlinks, protected names); this only refuses what is plainly not
 * a relative path before the request leaves this process. "" is the root.
 */
export function requireWorkspacePath(v: unknown, allowRoot: boolean): string {
  if (typeof v !== "string" || v.length > 4096 || v.includes("\0")) return bad("path must be a workspace-relative path");
  if (v === "") return allowRoot ? v : bad("path must name a file");
  const forward = v.replace(/\\/g, "/");
  if (forward.startsWith("/") || /^[a-zA-Z]:/.test(forward)) return bad("path must be workspace-relative");
  if (forward.split("/").some((part) => part === "..")) return bad("path must not climb out of the workspace");
  return v;
}
