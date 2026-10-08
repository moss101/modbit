/**
 * The page's structure for the semantic compiler (PX-122, PX-123): the DOM
 * document of each frame walked once for the facts the accessibility tree
 * does not carry (the stable attribute that identifies an element, a link's
 * destination, the form a control belongs to and where it submits, the
 * `type` and `autocomplete` of a field), and the accessibility nodes of each
 * frame merged with them into the compiler's raw nodes. Pure functions over
 * the DevTools shapes, so the merge is tested without a browser.
 */

export interface DomNode {
  nodeType: number;
  nodeName: string;
  backendNodeId: number;
  localName?: string;
  attributes?: string[];
  children?: DomNode[];
  contentDocument?: DomNode;
  shadowRoots?: DomNode[];
  templateContent?: DomNode;
  frameId?: string;
  documentURL?: string;
  baseURL?: string;
}

export interface DomInfo {
  tag: string;
  /** `testid:<v>` | `id:<v>` | `name:<v>`: what identifies the element on the page. */
  ident?: string;
  type?: string;
  href?: string;
  autocomplete?: string;
  placeholder?: string;
  required?: boolean;
  formKey?: string;
  formAction?: string;
  formMethod?: string;
  /** For a control that submits its form. */
  submit?: boolean;
}

export interface FrameOwner {
  /** The iframe element's backend node id in the document that contains it. */
  ownerBackendId: number;
  /** The frame the iframe element is in. */
  inFrameId: string;
}

export interface DomIndex {
  byBackend: Map<number, DomInfo>;
  /** `<iframe>` elements by the frame they host. */
  owners: Map<string, FrameOwner>;
  /** Nodes walked (bounded). */
  walked: number;
}

const MAX_WALK = 30_000;

function attrMap(n: DomNode): Map<string, string> {
  const m = new Map<string, string>();
  const a = n.attributes ?? [];
  for (let i = 0; i + 1 < a.length; i += 2) m.set(a[i]!.toLowerCase(), a[i + 1]!);
  return m;
}

function absolute(raw: string | undefined, base: string | undefined): string | undefined {
  if (raw === undefined) return undefined;
  try {
    return new URL(raw, base || undefined).href;
  } catch {
    return undefined;
  }
}

/** Walk a document (shadow roots and same-process frames included) and index every element worth knowing about. */
export function indexDom(root: DomNode, rootFrameId: string): DomIndex {
  const byBackend = new Map<number, DomInfo>();
  const owners = new Map<string, FrameOwner>();
  let walked = 0;
  // Per document: a counter of forms in order, so a form without an id or name has a stable key.
  type Ctx = { frameId: string; base: string; formCount: { n: number }; form: { key: string; action: string; method: string } | null };
  const stack: { node: DomNode; ctx: Ctx }[] = [{ node: root, ctx: { frameId: rootFrameId, base: root.baseURL ?? root.documentURL ?? "", formCount: { n: 0 }, form: null } }];
  while (stack.length > 0 && walked < MAX_WALK) {
    const { node, ctx } = stack.pop()!;
    walked += 1;
    let next = ctx;
    if (node.nodeType === 1) {
      const a = attrMap(node);
      const tag = (node.localName ?? node.nodeName).toLowerCase();
      const info: DomInfo = { tag };
      const testid = a.get("data-testid") ?? a.get("data-test-id") ?? a.get("data-test");
      const id = a.get("id");
      const name = a.get("name");
      if (testid) info.ident = `testid:${testid}`;
      else if (id) info.ident = `id:${id}`;
      else if (name && ["input", "select", "textarea", "button"].includes(tag)) info.ident = `name:${name}`;
      const type = a.get("type");
      if (type !== undefined) info.type = type.toLowerCase();
      if (tag === "a" && a.has("href")) {
        const href = absolute(a.get("href"), ctx.base);
        if (href !== undefined) info.href = href;
      }
      const ac = a.get("autocomplete");
      if (ac) info.autocomplete = ac.toLowerCase();
      const ph = a.get("placeholder");
      if (ph) info.placeholder = ph;
      if (a.has("required")) info.required = true;
      if (tag === "form") {
        const key = id ? `id:${id}` : name ? `name:${name}` : `idx:${ctx.formCount.n}`;
        ctx.formCount.n += 1;
        const action = absolute(a.get("action") ?? "", ctx.base) ?? ctx.base;
        const method = (a.get("method") ?? "get").toLowerCase();
        next = { ...ctx, form: { key, action, method } };
        info.formKey = key;
        info.formAction = action;
        info.formMethod = method;
      } else if (a.has("form")) {
        // A control associated with a form elsewhere by its id.
        info.formKey = `id:${a.get("form")}`;
      } else if (ctx.form && ["input", "select", "textarea", "button", "fieldset", "output", "object"].includes(tag)) {
        info.formKey = ctx.form.key;
        info.formAction = ctx.form.action;
        info.formMethod = ctx.form.method;
      } else if (ctx.form) {
        // Any other element inside a form (a label, a div) still belongs to it.
        info.formKey = ctx.form.key;
        info.formAction = ctx.form.action;
        info.formMethod = ctx.form.method;
      }
      if (tag === "button") info.submit = (info.type ?? "submit") === "submit";
      else if (tag === "input" && (info.type === "submit" || info.type === "image")) info.submit = true;
      else if (tag === "input" && info.type === "button") info.submit = false;
      if (tag === "iframe" || tag === "frame") {
        if (node.frameId) owners.set(node.frameId, { ownerBackendId: node.backendNodeId, inFrameId: ctx.frameId });
      }
      byBackend.set(node.backendNodeId, info);
    }
    // Children in document order: push in reverse so pop() yields them first-to-last.
    const kids: { node: DomNode; ctx: Ctx }[] = [];
    for (const c of node.children ?? []) kids.push({ node: c, ctx: next });
    for (const s of node.shadowRoots ?? []) kids.push({ node: s, ctx: next });
    if (node.templateContent) kids.push({ node: node.templateContent, ctx: next });
    if (node.contentDocument) {
      const doc = node.contentDocument;
      kids.push({ node: doc, ctx: { frameId: node.frameId ?? doc.frameId ?? next.frameId, base: doc.baseURL ?? doc.documentURL ?? "", formCount: { n: 0 }, form: null } });
    }
    for (let i = kids.length - 1; i >= 0; i--) stack.push(kids[i]!);
  }
  return { byBackend, owners, walked };
}

// ---- merging the accessibility tree ----

export interface AxProp {
  name: string;
  value?: { value?: unknown };
}

export interface AxNode {
  nodeId: string;
  ignored?: boolean;
  role?: { value?: unknown };
  name?: { value?: unknown };
  value?: { value?: unknown };
  childIds?: string[];
  parentId?: string;
  backendDOMNodeId?: number;
  properties?: AxProp[];
}

export interface RawNode {
  id: string;
  parent: string | null;
  role: string;
  name: string;
  value: string;
  depth: number;
  ignored: boolean;
  backend_dom_node_id: number | null;
  bounds: { x: number; y: number; width: number; height: number } | null;
  disabled: boolean;
  frame?: string;
  checked?: string;
  expanded?: boolean;
  required?: boolean;
  invalid?: boolean;
  selected?: boolean;
  modal?: boolean;
  href?: string;
  input_type?: string;
  dom_ident?: string;
  autocomplete?: string;
  form_key?: string;
  form_action?: string;
  form_method?: string;
  submit?: boolean;
  placeholder?: string;
}

function prop(n: AxNode, name: string): unknown {
  return (n.properties ?? []).find((p) => p.name === name)?.value?.value;
}

/** Truthiness of an AX boolean property, which Chromium sends as a boolean or as a string. */
function flag(v: unknown): boolean | undefined {
  if (v === undefined) return undefined;
  if (typeof v === "boolean") return v;
  if (typeof v === "string") return v === "true";
  return undefined;
}

export interface MergeOptions {
  /** Prefix for node ids (`f1:`), so frames do not collide. */
  prefix: string;
  /** The frame key nodes carry; undefined = the top frame. */
  frame?: string;
  /** At most this many live nodes. */
  limit: number;
  dom: DomIndex;
  /** The parent id the frame's root attaches to (the iframe element's AX node, or the parent frame's root). */
  rootParent?: string | null;
  /** The role to give the frame's root (sub-frames are `iframe`). */
  rootRole?: string;
  /** The name to give the frame's root. */
  rootName?: string;
}

/** The compiler's raw nodes of one frame's accessibility tree, with the DOM facts merged in. */
export function mergeAx(nodes: AxNode[], o: MergeOptions): { nodes: RawNode[]; truncated: boolean; rootId: string | null } {
  const byId = new Map(nodes.map((n) => [n.nodeId, n]));
  const depth = new Map<string, number>();
  for (const n of nodes) {
    let d = 0;
    let p = n.parentId;
    while (p) {
      d += 1;
      p = byId.get(p)?.parentId;
      if (d > 64) break;
    }
    depth.set(n.nodeId, d);
  }
  const live = nodes.filter((n) => !n.ignored);
  const kept = live.slice(0, o.limit);
  const keptIds = new Set(kept.map((n) => n.nodeId));
  // A node's parent is its nearest ancestor the compiler is given: an
  // ignored wrapper between a landmark and its controls must not cut the
  // landmark path.
  const parentOf = (n: AxNode): string | null => {
    let p = n.parentId;
    let guard = 0;
    while (p !== undefined && !keptIds.has(p) && guard++ < 128) p = byId.get(p)?.parentId;
    return p !== undefined && keptIds.has(p) ? p : null;
  };
  const out: RawNode[] = kept.map((n) => {
    const di = n.backendDOMNodeId !== undefined ? o.dom.byBackend.get(n.backendDOMNodeId) : undefined;
    const parent = parentOf(n);
    const isRoot = n === kept[0];
    const role = isRoot && o.rootRole ? o.rootRole : String(n.role?.value ?? "");
    const checked = prop(n, "checked");
    const r: RawNode = {
      id: `${o.prefix}${n.nodeId}`,
      parent: isRoot ? (o.rootParent ?? null) : parent === null ? null : `${o.prefix}${parent}`,
      role,
      name: String(isRoot && o.rootName ? o.rootName : (n.name?.value ?? "")).slice(0, 200),
      value: String(n.value?.value ?? "").slice(0, 200),
      depth: depth.get(n.nodeId) ?? 0,
      ignored: false,
      backend_dom_node_id: typeof n.backendDOMNodeId === "number" ? n.backendDOMNodeId : null,
      bounds: null,
      disabled: (n.properties ?? []).some((p) => (p.name === "disabled" || p.name === "readonly") && p.value?.value === true),
    };
    if (o.frame) r.frame = o.frame;
    if (checked !== undefined) r.checked = typeof checked === "boolean" ? String(checked) : String(checked);
    const expanded = flag(prop(n, "expanded"));
    if (expanded !== undefined) r.expanded = expanded;
    if (flag(prop(n, "required")) === true) r.required = true;
    const inv = prop(n, "invalid");
    if (inv !== undefined && inv !== false && inv !== "false") r.invalid = true;
    const selected = flag(prop(n, "selected"));
    if (selected !== undefined) r.selected = selected;
    if (flag(prop(n, "modal")) === true) r.modal = true;
    const url = prop(n, "url");
    if (di) {
      if (di.ident) r.dom_ident = di.ident;
      if (di.type) r.input_type = di.type;
      if (di.autocomplete) r.autocomplete = di.autocomplete;
      if (di.placeholder) r.placeholder = di.placeholder;
      if (di.required && r.required === undefined) r.required = true;
      if (di.formKey) r.form_key = di.formKey;
      if (di.formAction) r.form_action = di.formAction;
      if (di.formMethod) r.form_method = di.formMethod;
      if (di.submit !== undefined) r.submit = di.submit;
      if (di.href) r.href = di.href;
    }
    if (r.href === undefined && typeof url === "string" && url !== "") r.href = url;
    return r;
  });
  return { nodes: out, truncated: live.length > o.limit, rootId: kept.length > 0 ? `${o.prefix}${kept[0]!.nodeId}` : null };
}
