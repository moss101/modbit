// Bundles the extension (CommonJS for the extension host, `vscode` external)
// and the extension-host test entry points with esbuild. Deterministic output in dist/.
import { build } from "esbuild";
import { mkdirSync } from "node:fs";

mkdirSync("dist/test", { recursive: true });
const common = { bundle: true, sourcemap: true, logLevel: "warning", target: "es2024", platform: "node", format: "cjs" };
await build({ ...common, entryPoints: ["src/extension.ts"], outfile: "dist/extension.cjs", external: ["vscode"] });
await build({ ...common, entryPoints: ["test/suite.ts"], outfile: "dist/test/suite.cjs", external: ["vscode", "mocha"] });
await build({ ...common, entryPoints: ["test/run.ts"], outfile: "dist/test/run.cjs", external: ["@vscode/test-electron"] });
console.log("vscode-adapter bundles written to dist/");
