/**
 * Argument validation for the computer-control handlers (PX-074): everything
 * the renderer sends is hostile until it is checked here. These checks narrow;
 * the Core decides.
 */
import { ACCESS_LEVELS, PERMISSIONS, type AccessLevel, type PermissionName } from "../shared/computer-types.ts";
import { bad } from "./control-args.ts";

/** The operating-system pane a deep link opens: one of the two permissions, never a free-form URL. */
export function validPane(v: unknown): PermissionName {
  if (typeof v !== "string" || !(PERMISSIONS as readonly string[]).includes(v)) return bad(`pane must be one of ${PERMISSIONS.join(", ")}`);
  return v as PermissionName;
}

/** A stored artifact: the object store's sha256 in hex. */
export function validArtifactRef(v: unknown): string {
  if (typeof v !== "string" || !/^[0-9a-f]{64}$/.test(v)) return bad("artifact must be a stored frame reference");
  return v;
}

/** A short note the person wrote when resolving a latch, or the reason of a Stop. */
export function validNote(v: unknown, what: string): string {
  if (v === undefined || v === null) return "";
  // eslint-disable-next-line no-control-regex
  if (typeof v !== "string" || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/.test(v)) return bad(`${what} must be plain text`);
  return v.slice(0, 200);
}

/** The System Settings deep links (macOS). A fixed table: the renderer names a pane, never an address. */
export const SETTINGS_LINKS: Record<PermissionName, string> = {
  accessibility: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
  screen_recording: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
};

/** A standing level, or "" to clear it. Anything else is refused here; the Core checks again. */
export function validAccessLevel(v: unknown): AccessLevel | "" {
  if (v === "") return "";
  if (typeof v !== "string" || !(ACCESS_LEVELS as readonly string[]).includes(v)) return bad(`level must be one of ${ACCESS_LEVELS.join(", ")}, or empty to clear it`);
  return v as AccessLevel;
}

/** One application's identity as a standing level names it: a bundle identifier or `exe:<file name>`, never a pattern. */
export function validApplicationKey(v: unknown): string {
  if (typeof v !== "string") return bad("application must be a bundle identifier");
  const key = v.trim();
  if (key === "" || key.length > 200) return bad("application must be a bundle identifier of 1 to 200 characters");
  if (/[\s*]/.test(key) || /[\u0000-\u001f\u007f]/.test(key)) return bad("application must be one bundle identifier: no spaces and no pattern");
  return key;
}

/** The label kept with a level for the record: plain, one line, bounded. */
export function validApplicationLabel(v: unknown): string {
  if (v === undefined || v === null) return "";
  if (typeof v !== "string" || /[\u0000-\u001f\u007f]/.test(v)) return bad("application name must be plain text");
  return v.trim().slice(0, 200);
}
