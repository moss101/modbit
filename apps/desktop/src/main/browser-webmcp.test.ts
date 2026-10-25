import assert from "node:assert/strict";
import { test } from "node:test";
import vm from "node:vm";
import { admitsCall, BOUNDS, bindDecls, frameOrigin, WEBMCP_GLOBAL, WEBMCP_SOURCE, type FrameFacts } from "./browser-webmcp.ts";

const frame = (over: Partial<FrameFacts> = {}): FrameFacts => ({ id: "F1", loaderId: "L1", url: "https://app.test/x", origin: "https://app.test", top: true, ...over });

/** A page world with a Navigator, as far as the shim needs one. */
function world(): { ctx: vm.Context; run: (src: string) => unknown } {
  const ctx = vm.createContext({});
  vm.runInContext("function Navigator() {} globalThis.navigator = new Navigator();", ctx);
  vm.runInContext(WEBMCP_SOURCE, ctx);
  return { ctx, run: (s) => vm.runInContext(s, ctx) };
}

test("PX-124: a frame's identity is the browser's: an opaque or non-http origin is offered no tools", () => {
  assert.equal(frameOrigin("https://App.Test", "https://other.test/"), "https://app.test");
  assert.equal(frameOrigin("://", "https://app.test/p"), "https://app.test");
  assert.equal(frameOrigin(undefined, "about:blank"), "");
  assert.equal(frameOrigin("null", "data:text/html,x"), "");
  assert.equal(frameOrigin("file://", "file:///etc/passwd"), "");
});

test("PX-124: declarations are bound to the frame, origin and page load the browser reports, whatever the page says about itself", () => {
  const { decls } = bindDecls(frame(), [{ name: "search", description: "x", input_schema: {}, annotations: { readOnlyHint: true, origin: "https://evil.test" } }], 10);
  assert.equal(decls.length, 1);
  assert.equal(decls[0]!.origin, "https://app.test");
  assert.equal(decls[0]!.frame, "F1");
  assert.equal(decls[0]!.load, "L1");
});

test("PX-124: bounds are refused, not silently dropped; a repeated name is one tool", () => {
  const many = Array.from({ length: 5 }, (_, i) => ({ name: `t${i}`, input_schema: {} }));
  const r = bindDecls(frame(), many, 3);
  assert.equal(r.decls.length, 3);
  assert.deepEqual(r.refused.map((x) => x.code), ["TOO_MANY_TOOLS", "TOO_MANY_TOOLS"]);
  const big = bindDecls(frame(), [{ name: "big", input_schema: { x: "y".repeat(BOUNDS.schemaBytes) } }], 3);
  assert.equal(big.refused[0]?.code, "SCHEMA_TOO_LARGE");
  assert.equal(bindDecls(frame(), [{ name: "a" }, { name: "a" }], 3).decls.length, 1);
});

test("PX-124: a call is admitted only for the same frame, the same page load and the same origin", () => {
  const req = { frame: "F1", load: "L1", origin: "https://app.test" };
  assert.deepEqual(admitsCall(req, [frame()]), { ok: true });
  assert.equal(admitsCall(req, [frame({ loaderId: "L2" })]).ok, false, "the page navigated");
  assert.equal(admitsCall(req, [frame({ id: "F2" })]).ok, false, "another frame");
  assert.equal(admitsCall(req, [frame({ origin: "https://evil.test" })]).ok, false, "another origin");
  assert.equal(admitsCall(req, []).ok, false, "gone");
  assert.equal(admitsCall({ ...req, origin: "" }, [frame({ origin: "" })]).ok, false, "an opaque origin is never admitted");
});

test("PX-124: the shim offers navigator.modelContext, keeps declarations out of the page's reach and removes them when withdrawn", async () => {
  const { run } = world();
  run(`navigator.modelContext.registerTool({ name: 'search', description: 'd', inputSchema: {type:'object'}, annotations: {readOnlyHint: true}, execute: async (a) => ({ found: a.q }) });
       navigator.modelContext.registerTool({ name: 'cancel', execute: () => 'ok' });`);
  const listed = run(`JSON.stringify(globalThis.${WEBMCP_GLOBAL}.list())`) as string;
  const tools = JSON.parse(listed) as { name: string; annotations: Record<string, unknown> }[];
  assert.deepEqual(tools.map((t) => t.name), ["search", "cancel"]);
  assert.deepEqual(tools[0]!.annotations, { readOnlyHint: true });
  assert.deepEqual(tools[1]!.annotations, {}, "no annotation is no annotation");
  assert.throws(() => run(`navigator.modelContext.registerTool({ name: 'search', execute() {} })`), /already registered/);
  assert.throws(() => run(`navigator.modelContext.registerTool({ name: 'x' })`), /execute/);
  const out = (await run(`globalThis.${WEBMCP_GLOBAL}.call('search', {q: 'open'})`)) as { ok: boolean; json: string };
  assert.deepEqual(JSON.parse(out.json), { found: "open" });
  // The page cannot replace the control object the host reads.
  assert.throws(() => run(`'use strict'; globalThis.${WEBMCP_GLOBAL} = {}`), /read only|Cannot assign|Cannot redefine/i);
  assert.throws(() => run(`'use strict'; delete globalThis.${WEBMCP_GLOBAL}`), /Cannot delete|not configurable/i);
  run(`navigator.modelContext.unregisterTool('search')`);
  assert.equal(JSON.stringify(await run(`globalThis.${WEBMCP_GLOBAL}.call('search', {})`)), '{"gone":true}');
  run(`navigator.modelContext.provideContext({ tools: [{ name: 'only', execute() { return 1; } }] })`);
  assert.deepEqual((JSON.parse(run(`JSON.stringify(globalThis.${WEBMCP_GLOBAL}.list())`) as string) as { name: string }[]).map((t) => t.name), ["only"]);
  run(`navigator.modelContext.clearContext()`);
  assert.equal(run(`globalThis.${WEBMCP_GLOBAL}.list().length`), 0);
});

test("PX-124: a tool that throws is reported as thrown, not as a result; an oversized schema is not carried", async () => {
  const { run } = world();
  run(`navigator.modelContext.registerTool({ name: 'boom', execute() { throw new Error('nope'); } });
       navigator.modelContext.registerTool({ name: 'wide', inputSchema: { x: 'y'.repeat(${BOUNDS.schemaBytes}) }, execute() { return 1; } });`);
  const out = (await run(`globalThis.${WEBMCP_GLOBAL}.call('boom', {})`)) as { ok: boolean; error: string };
  assert.equal(out.ok, false);
  assert.equal(out.error, "nope");
  const listed = JSON.parse(run(`JSON.stringify(globalThis.${WEBMCP_GLOBAL}.list())`) as string) as { name: string; input_schema: unknown }[];
  assert.deepEqual(listed.find((t) => t.name === "wide")!.input_schema, {});
});
