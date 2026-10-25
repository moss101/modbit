/**
 * The host's side of the WebMCP rung (PX-124, docs/22 "Action hierarchy" rung 1). A page declares tools through
 * `navigator.modelContext` (`registerTool`, `unregisterTool`, `provideContext`, `clearContext`); the host offers that
 * interface to every document it loads, in the page's own world (the page must be able to call it), keeps what was
 * declared in a closure the page cannot replace, and reads it through the DevTools protocol. Identity is never taken
 * from the page: the frame, its origin and the page load come from the browser (`Page.getFrameTree`), and a tool is
 * bound to the document that registered it, so a navigation takes its tools with it.
 *
 * Nothing here needs a browser feature flag: the interface is offered when the browser has none of its own.
 */

/** The global the shim keeps its control object under (non-enumerable, non-writable, non-configurable). */
export const WEBMCP_GLOBAL = "__modbitWebMcp";

/** The bounds the host applies to whatever a page declares or returns (the shim applies them too; the host never trusts that). */
export const BOUNDS = { tools: 64, description: 2000, schemaBytes: 8 * 1024, resultBytes: 64 * 1024, callMs: 30_000 } as const;

/** The source injected into every document of the page (and run in the current one). */
export const WEBMCP_SOURCE = `(() => {
  if (globalThis.${WEBMCP_GLOBAL}) return;
  const tools = new Map();
  const jsonSafe = (v, max) => { try { const s = JSON.stringify(v); return s !== undefined && s.length <= max ? JSON.parse(s) : {}; } catch (e) { return {}; } };
  const register = (t) => {
    if (!t || typeof t.name !== 'string' || t.name === '' || typeof t.execute !== 'function') throw new TypeError('a tool needs a name and an execute function');
    if (tools.has(t.name)) throw new Error('a tool named ' + t.name + ' is already registered');
    tools.set(t.name, { name: t.name, description: String(t.description === undefined ? '' : t.description).slice(0, ${BOUNDS.description}), inputSchema: t.inputSchema, annotations: t.annotations, execute: t.execute });
  };
  const context = {
    registerTool(t) { register(t); },
    unregisterTool(name) { tools.delete(String(name)); },
    provideContext(c) { tools.clear(); for (const t of (c && c.tools) || []) register(t); },
    clearContext() { tools.clear(); },
  };
  const control = Object.freeze({
    list() {
      return [...tools.values()].slice(0, ${BOUNDS.tools + 1}).map((t) => ({ name: t.name, description: t.description, input_schema: jsonSafe(t.inputSchema, ${BOUNDS.schemaBytes}), annotations: jsonSafe(t.annotations, 1024) }));
    },
    async call(name, args) {
      const t = tools.get(String(name));
      if (!t) return { gone: true };
      try {
        const r = await t.execute(args, { requestUserInteraction: async (cb) => cb() });
        let s = JSON.stringify(r === undefined ? null : r);
        if (s === undefined) s = 'null';
        const truncated = s.length > ${BOUNDS.resultBytes};
        return { ok: true, json: truncated ? JSON.stringify(s.slice(0, ${BOUNDS.resultBytes})) : s, truncated };
      } catch (e) {
        return { ok: false, error: String((e && e.message) || e).slice(0, 500) };
      }
    },
  });
  Object.defineProperty(globalThis, '${WEBMCP_GLOBAL}', { value: control, enumerable: false, writable: false, configurable: false });
  if (typeof navigator !== 'undefined' && !('modelContext' in navigator)) {
    Object.defineProperty(Navigator.prototype, 'modelContext', { get() { return context; }, enumerable: true, configurable: true });
  }
})();`;

export interface FrameFacts {
  /** DevTools frame id. */
  id: string;
  /** The document loader of the frame's current document. */
  loaderId: string;
  url: string;
  /** From the browser (`securityOrigin`), lower-cased; empty when the origin is opaque or not http(s). */
  origin: string;
  top: boolean;
  /** The DevTools session the frame is reached through (undefined = the page's own). */
  sessionId?: string;
}

/** The origin a frame is offered under: its security origin when it is an http(s) one, else none (an opaque origin has no tools offered). */
export function frameOrigin(securityOrigin: string | undefined, url: string): string {
  const of = (u: string): string => {
    try {
      const x = new URL(u);
      return x.protocol === "http:" || x.protocol === "https:" ? x.origin.toLowerCase() : "";
    } catch {
      return "";
    }
  };
  if (securityOrigin && securityOrigin !== "://" && securityOrigin !== "null") return of(securityOrigin);
  return of(url);
}

export interface RawDecl {
  name?: unknown;
  description?: unknown;
  input_schema?: unknown;
  annotations?: unknown;
}

export interface Decl {
  frame: string;
  origin: string;
  url: string;
  top: boolean;
  load: string;
  name: string;
  description: string;
  input_schema: unknown;
  annotations: unknown;
}

export interface Refusal {
  frame: string;
  origin: string;
  name: string;
  code: string;
  reason: string;
}

/** What a frame's raw declarations become: bounded, with the frame's identity from the browser. Declarations over the page's limit are refused, not silently dropped. */
export function bindDecls(frame: FrameFacts, raw: readonly RawDecl[], room: number): { decls: Decl[]; refused: Refusal[] } {
  const decls: Decl[] = [];
  const refused: Refusal[] = [];
  const seen = new Set<string>();
  for (const r of raw) {
    const name = typeof r.name === "string" ? r.name : "";
    if (seen.has(name)) continue;
    seen.add(name);
    if (decls.length >= room) {
      refused.push({ frame: frame.id, origin: frame.origin, name: name.slice(0, 80), code: "TOO_MANY_TOOLS", reason: `the page offers more than ${BOUNDS.tools} tools` });
      continue;
    }
    const schema = r.input_schema ?? {};
    if (JSON.stringify(schema).length > BOUNDS.schemaBytes) {
      refused.push({ frame: frame.id, origin: frame.origin, name: name.slice(0, 80), code: "SCHEMA_TOO_LARGE", reason: `the input schema is larger than ${BOUNDS.schemaBytes} bytes` });
      continue;
    }
    decls.push({
      frame: frame.id,
      origin: frame.origin,
      url: frame.url,
      top: frame.top,
      load: frame.loaderId,
      name,
      description: typeof r.description === "string" ? r.description.slice(0, BOUNDS.description) : "",
      input_schema: schema,
      annotations: typeof r.annotations === "object" && r.annotations !== null ? r.annotations : {},
    });
  }
  return { decls, refused };
}

export type CallVerdict = { ok: true } | { ok: false; code: "PAGE_TOOL_STALE"; message: string };

/**
 * Whether a call still names the document that declared the tool: the frame is still there, in the same page load,
 * at the same origin. A tool of an earlier page, of another frame or of another origin is never reached by a name.
 */
export function admitsCall(req: { frame: string; load: string; origin: string }, frames: readonly FrameFacts[]): CallVerdict {
  const f = frames.find((x) => x.id === req.frame);
  if (!f) return { ok: false, code: "PAGE_TOOL_STALE", message: "the frame that declared this tool is gone" };
  if (f.loaderId !== req.load) return { ok: false, code: "PAGE_TOOL_STALE", message: "the page that declared this tool has navigated or reloaded" };
  if (f.origin === "" || f.origin !== req.origin) return { ok: false, code: "PAGE_TOOL_STALE", message: "the frame is no longer at the origin that declared this tool" };
  return { ok: true };
}
