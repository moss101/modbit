/**
 * The declared exceptions of DR-PX-2026-10-24-016 section 5.2 as data for the fidelity gate. The list is closed: a deviation
 * from ZCode that no entry covers fails the gate, and an entry that covers nothing in a full run fails it too (stale).
 */
import type { Exception } from "./fidelity.ts";

const EDGES = ["borderTopColor", "borderRightColor", "borderBottomColor", "borderLeftColor"] as const;
const key = (re: RegExp) => (k: string) => re.test(k);

export const EXCEPTIONS: readonly Exception[] = [
  { id: "ZDX-01", applies: key(/description\||label\||shortcut\||tabs-trigger|dropdown-menu-label/), props: ["color"], why: "secondary text is the 4.5:1 muted role, not ZCode's 60 or 40 percent foreground" },
  { id: "ZDX-02", applies: key(/badge\|destructive|dropdown-menu-item\|destructive/), props: ["color"], why: "status text uses the destructive surface foreground (5.4 to 6.6:1), not the 4.25:1 solid colour" },
  { id: "ZDX-02", applies: key(/badge\|destructive/), props: ["backgroundColor"], why: "a status chip composites its tint over the raised surface, so its text pair is fixed: axe measured 4.47:1 when the tint sat on a tinted card" },
  { id: "ZDX-03", applies: key(/\|outline\||fields\|input|fields\|textarea|fields\|checkbox|dialog\|input|select-trigger|alert-dialog-cancel/), props: EDGES, why: "an interactive control edge is at least 3:1; ZCode's border is 1.25:1 (light) and 1.36:1 (dark)" },
  { id: "ZDX-05", applies: (k, t) => t === "dark" && /\|destructive\|/.test(k), props: ["color"], why: "the white label on ZCode's dark destructive button is 3.03:1" },
];
