/**
 * Where electron-builder's `--dir` output puts the packaged executable on this
 * platform, and the resources directory beside it (M10.2, docs/76 "packaging").
 */
import { existsSync, readdirSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");

export function packagedExecutable(releaseDir = join(appDir, "release")): { exe: string; resources: string } {
  if (process.platform === "darwin") {
    for (const d of readdirSync(releaseDir).filter((n) => n === "mac" || n.startsWith("mac-"))) {
      const exe = join(releaseDir, d, "Modbit.app", "Contents", "MacOS", "Modbit");
      if (existsSync(exe)) return { exe, resources: join(releaseDir, d, "Modbit.app", "Contents", "Resources") };
    }
  } else if (process.platform === "win32") {
    const exe = join(releaseDir, "win-unpacked", "Modbit.exe");
    if (existsSync(exe)) return { exe, resources: join(releaseDir, "win-unpacked", "resources") };
  } else {
    const dir = join(releaseDir, "linux-unpacked");
    const exe = join(dir, "modbit-desktop");
    if (existsSync(exe)) return { exe, resources: join(dir, "resources") };
  }
  throw new Error(`no packaged Modbit under ${releaseDir}; run \`pnpm --filter @modbit/desktop package:dir\` first`);
}
