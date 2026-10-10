// Bundles the three Electron entry points with esbuild (docs/32 stack).
// main + preload: CommonJS (.cjs, because the package is type=module) for
// Electron's Node side; renderer: a classic script for the sandboxed window.
// No dev server; output is deterministic in dist/ (or $MODBIT_OUTDIR).
//
// The renderer's stylesheet is generated here from the design tokens, the UI
// primitives' styles and the screens' styles, so there is one source for every
// colour (packages/design-tokens). MODBIT_GALLERY=1 builds the model-free state
// gallery into the renderer (reachable at #gallery); a normal build defines the
// flag false and the gallery is not in the bundle.
import { build } from "esbuild";
import { copyFileSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { join } from "node:path";

const out = process.env.MODBIT_OUTDIR || "dist";
const gallery = process.env.MODBIT_GALLERY === "1";
mkdirSync(join(out, "main"), { recursive: true });
mkdirSync(join(out, "preload"), { recursive: true });
mkdirSync(join(out, "renderer"), { recursive: true });

const { tokensCss } = await import("../../packages/design-tokens/src/css.ts");
const { uiCss } = await import("../../packages/ui/src/styles.ts");
const { legacyCss, shellCss } = await import("./src/renderer/styles.ts");
const { workspaceCss } = await import("./src/renderer/workspace-css.ts");
const { composerCss } = await import("./src/renderer/composer/composer-css.ts");
const { controlsCss } = await import("./src/renderer/controls-css.ts");
const { automationsCss } = await import("./src/renderer/automations/automations-css.ts");
const { projectsCss } = await import("./src/renderer/projects/projects-css.ts");
// xterm.js ships its own stylesheet (the terminal app, REQ-PX-048); it is bundled with ours so the CSP needs nothing external.
const xtermCss = readFileSync(createRequire(import.meta.url).resolve("@xterm/xterm/css/xterm.css"), "utf8");
writeFileSync(join(out, "renderer", "app.css"), [tokensCss(), uiCss, xtermCss, legacyCss, shellCss, workspaceCss, composerCss, controlsCss, automationsCss, projectsCss].join("\n"));

const common = { bundle: true, sourcemap: true, logLevel: "warning", target: "es2024" };
await build({ ...common, entryPoints: ["src/main/main.ts"], outfile: join(out, "main/main.cjs"), platform: "node", format: "cjs", external: ["electron"] });
await build({ ...common, entryPoints: ["src/preload/preload.ts"], outfile: join(out, "preload/preload.cjs"), platform: "node", format: "cjs", external: ["electron"] });
await build({ ...common, entryPoints: ["src/renderer/index.tsx"], outfile: join(out, "renderer/index.js"), platform: "browser", format: "iife", jsx: "automatic", define: { __MODBIT_GALLERY__: gallery ? "true" : "false" } });
copyFileSync("src/renderer/index.html", join(out, "renderer/index.html"));
console.log(`desktop bundles written to ${out}/${gallery ? " (with the state gallery)" : ""}`);
