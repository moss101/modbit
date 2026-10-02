// Bundles the three Electron entry points with esbuild (docs/32 stack).
// main + preload: CommonJS (.cjs, because the package is type=module) for
// Electron's Node side; renderer: a classic script for the sandboxed window. No dev server; output is deterministic in dist/.
import { build } from "esbuild";
import { copyFileSync, mkdirSync } from "node:fs";

mkdirSync("dist/main", { recursive: true });
mkdirSync("dist/preload", { recursive: true });
mkdirSync("dist/renderer", { recursive: true });

const common = { bundle: true, sourcemap: true, logLevel: "warning", target: "es2024" };
await build({ ...common, entryPoints: ["src/main/main.ts"], outfile: "dist/main/main.cjs", platform: "node", format: "cjs", external: ["electron"] });
await build({ ...common, entryPoints: ["src/preload/preload.ts"], outfile: "dist/preload/preload.cjs", platform: "node", format: "cjs", external: ["electron"] });
await build({ ...common, entryPoints: ["src/renderer/index.tsx"], outfile: "dist/renderer/index.js", platform: "browser", format: "iife", jsx: "automatic" });
// The install helper runs as plain Node under ELECTRON_RUN_AS_NODE, outside the app bundle.
await build({ ...common, entryPoints: ["src/main/update/apply-helper.ts"], outfile: "dist/main/apply-helper.cjs", platform: "node", format: "cjs" });
copyFileSync("src/renderer/index.html", "dist/renderer/index.html");
console.log("desktop bundles written to dist/");
