/** The seeded application of the live specs (PX-145 live leg): a tiny cart library with a type error, a wrong-argument call and a logic error only its test catches. */
import { chmodSync, mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { execFileSync } from "node:child_process";
import { appDir, git } from "./ui-harness.ts";

export const REPO_TSC = resolve(appDir, "..", "..", "node_modules", ".bin", "tsc");
export const LIVE = process.env.MODBIT_LIVE_DESKTOP === "1";

export const FILES: Record<string, string> = {
  "package.json": JSON.stringify({ name: "cart-app", version: "1.0.0", scripts: { build: "tsc -p .", test: "npm run build && node --test test/" } }, null, 2) + "\n",
  "package-lock.json": "{}\n",
  "tsconfig.json": JSON.stringify({ compilerOptions: { strict: true, target: "es2020", module: "commonjs", rootDir: "src", outDir: "dist", types: [] }, include: ["src"] }, null, 2) + "\n",
  ".gitignore": "node_modules/\ndist/\n",
  "README.md": "# cart-app\n\nA tiny shopping-cart library.\n\n- `npm run build` compiles `src/` to `dist/`.\n- `npm test` builds and runs the tests in `test/`.\n",
  "src/money.ts": 'export function cents(amount: number): number {\n  return Math.round(amount * 100);\n}\n\nexport function format(c: number): string {\n  return "$" + (c / 100).toFixed(2);\n}\n',
  "src/cart.ts": [
    'import { cents, format } from "./money";',
    "",
    "export interface Item {",
    "  name: string;",
    "  price: number;",
    "  qty: number;",
    "}",
    "",
    "export function subtotal(items: Item[]): number {",
    "  let sum: string = 0;",
    "  for (const it of items) sum += cents(it.price) * it.qty;",
    "  return sum;",
    "}",
    "",
    "export function discount(sub: number, code: string): number {",
    '  if (code === "SAVE10") return Math.floor(sub * 0.01);',
    "  return 0;",
    "}",
    "",
    "export function total(items: Item[], code: string): string {",
    "  const sub = subtotal(items);",
    '  return format(sub - discount(sub, code), "USD");',
    "}",
    "",
  ].join("\n"),
  "test/cart.test.js": [
    'const test = require("node:test");',
    'const assert = require("node:assert");',
    'const { total, subtotal } = require("../dist/cart.js");',
    "",
    "const items = [",
    '  { name: "pen", price: 20, qty: 1 },',
    '  { name: "ink", price: 10, qty: 1 },',
    "];",
    "",
    'test("the subtotal is in cents", () => {',
    "  assert.strictEqual(subtotal(items), 3000);",
    "});",
    "",
    'test("no code, no discount", () => {',
    '  assert.strictEqual(total(items, ""), "$30.00");',
    "});",
    "",
    'test("SAVE10 takes ten percent off", () => {',
    '  assert.strictEqual(total(items, "SAVE10"), "$27.00");',
    "});",
    "",
  ].join("\n"),
  ".modbit/verification.json": JSON.stringify({ commands: [] }),
};

export function seed(): string {
  const root = mkdtempSync(join(tmpdir(), "modbit-live-app-"));
  for (const [rel, text] of Object.entries(FILES)) {
    mkdirSync(dirname(join(root, rel)), { recursive: true });
    writeFileSync(join(root, rel), text);
  }
  git(root, "init", "-q", "-b", "main");
  git(root, "add", "-A");
  git(root, "-c", "user.name=t", "-c", "user.email=t@e", "commit", "-q", "-m", "base");
  mkdirSync(join(root, "node_modules/.bin"), { recursive: true });
  writeFileSync(join(root, "node_modules/.bin/tsc"), `#!/bin/sh\nexec "${REPO_TSC}" "$@"\n`);
  chmodSync(join(root, "node_modules/.bin/tsc"), 0o755);
  return root;
}

export const npm = (root: string, ...args: string[]): { ok: boolean; out: string } => {
  try {
    return { ok: true, out: execFileSync("npm", args, { cwd: root, encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }) };
  } catch (e) {
    const err = e as { stdout?: string; stderr?: string };
    return { ok: false, out: `${err.stdout ?? ""}${err.stderr ?? ""}` };
  }
};

