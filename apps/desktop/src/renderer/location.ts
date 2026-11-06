/**
 * What a local task's location is called on this machine. A task made here runs
 * here: the desktop has no cloud client yet (PX-128), so this is the only place
 * it can name.
 */
export function thisMachine(platform: string): string {
  const p = platform.toUpperCase();
  if (p.includes("MAC")) return "This Mac";
  if (p.includes("WIN")) return "This PC";
  return "This computer";
}

export const localMachine = (): string => thisMachine(typeof navigator === "undefined" ? "" : navigator.platform);

/** Why the other location cannot be chosen; the words the home menu and Settings share. */
export const CLOUD_UNAVAILABLE = "not available from this app yet";
