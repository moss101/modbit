/**
 * PX-030, docs/76 "Platform states": the app says what this platform has
 * earned and nothing more. The states are compiled in from
 * `tools/platforms.json`, the same record `tools/support_claims.py` checks
 * documentation and client text against; CI compatibility is a development
 * guarantee, never a support claim.
 */
import platforms from "../../../../tools/platforms.json" with { type: "json" };

export type PlatformState = { os: string; arch: string; state: string; statement: string };

const NAMES: Record<string, string> = { darwin: "macos", win32: "windows", linux: "linux" };
const LABELS: Record<string, string> = { macos: "macOS", windows: "Windows", linux: "Linux" };

export function platformState(nodePlatform: string = process.platform, arch: string = process.arch): PlatformState {
  const os = NAMES[nodePlatform] ?? nodePlatform;
  const recorded = (platforms.platforms as Record<string, string>)[os] ?? "UNSUPPORTED";
  const label = LABELS[os] ?? os;
  const statement =
    recorded === "RELEASE_GRADE"
      ? `${label} ${arch}: RELEASE_GRADE (packaged desktop E2E passed on this platform)`
      : recorded === "CI_COMPATIBLE"
        ? `${label} ${arch}: CI_COMPATIBLE — built and tested in CI; a development guarantee, not a support claim (docs/76)`
        : `${label} ${arch}: UNSUPPORTED — not built or tested on this platform (docs/76)`;
  return { os, arch, state: recorded, statement };
}
