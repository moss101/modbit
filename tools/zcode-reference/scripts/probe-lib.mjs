// Shared in-page probe: computed style + geometry records, colours canonicalised to 8-bit sRGB through a canvas.
export const PROBE_SRC = `(sel, rootSel) => {
  const cv = document.createElement("canvas"); cv.width = cv.height = 1;
  const cx = cv.getContext("2d", { willReadFrequently: true });
  const rgba = (c) => {
    if (!c || c === "transparent") return "rgba(0,0,0,0)";
    cx.clearRect(0, 0, 1, 1); cx.fillStyle = "#000"; cx.fillStyle = c; cx.fillRect(0, 0, 1, 1);
    const d = cx.getImageData(0, 0, 1, 1).data; // premultiplied loss is acceptable at 8 bits for these checks
    // Re-derive alpha from a white and a black backdrop for exactness.
    cx.clearRect(0, 0, 1, 1); cx.fillStyle = "#fff"; cx.fillRect(0, 0, 1, 1); cx.fillStyle = c; cx.fillRect(0, 0, 1, 1);
    const w = cx.getImageData(0, 0, 1, 1).data;
    cx.clearRect(0, 0, 1, 1); cx.fillStyle = "#000"; cx.fillRect(0, 0, 1, 1); cx.fillStyle = c; cx.fillRect(0, 0, 1, 1);
    const b = cx.getImageData(0, 0, 1, 1).data;
    const a = Math.max(0, Math.min(1, 1 - (w[0] - b[0]) / 255));
    if (a < 0.004) return "rgba(0,0,0,0)";
    const r = (v) => Math.round(v);
    return "rgba(" + r(b[0] / a) + "," + r(b[1] / a) + "," + r(b[2] / a) + "," + (Math.round(a * 100) / 100) + ")";
  };
  const COLOR = ["color", "backgroundColor", "borderTopColor", "borderRightColor", "borderBottomColor", "borderLeftColor", "outlineColor", "textDecorationColor", "caretColor"];
  const PLAIN = ["fontFamily", "fontSize", "fontWeight", "lineHeight", "letterSpacing", "textTransform", "textDecorationLine", "textAlign", "whiteSpace",
    "display", "flexDirection", "alignItems", "justifyContent", "gap", "rowGap", "columnGap",
    "paddingTop", "paddingRight", "paddingBottom", "paddingLeft", "marginTop", "marginRight", "marginBottom", "marginLeft",
    "borderTopWidth", "borderRightWidth", "borderBottomWidth", "borderLeftWidth", "borderTopStyle",
    "borderTopLeftRadius", "borderTopRightRadius", "borderBottomRightRadius", "borderBottomLeftRadius",
    "boxShadow", "opacity", "overflow", "cursor", "outlineStyle", "outlineWidth", "outlineOffset", "backdropFilter", "backgroundImage", "backgroundClip",
    "transitionProperty", "transitionDuration", "transitionTimingFunction", "position", "zIndex"];
  const root = rootSel ? document.querySelector(rootSel) : document;
  const els = [...root.querySelectorAll(sel)];
  const out = [];
  const counts = {};
  for (const el of els) {
    const cs = getComputedStyle(el);
    const slot = el.getAttribute("data-slot") || el.tagName.toLowerCase();
    const spec = el.closest("[data-specimen]")?.getAttribute("data-specimen") || el.getAttribute("data-specimen") || "";
    const variant = el.getAttribute("data-variant") || "";
    const size = el.getAttribute("data-size") || "";
    const key = [spec, slot, variant, size].join("|");
    counts[key] = (counts[key] || 0) + 1;
    const r = el.getBoundingClientRect();
    const rec = { id: key + "#" + counts[key], spec, slot, variant, size, tag: el.tagName.toLowerCase(), text: (el.textContent || "").trim().slice(0, 40),
      rect: { w: Math.round(r.width * 100) / 100, h: Math.round(r.height * 100) / 100 }, style: {} };
    for (const p of COLOR) rec.style[p] = rgba(cs[p]);
    for (const p of PLAIN) rec.style[p] = cs[p];
    out.push(rec);
  }
  return out;
}`;

// Canonicalises custom-property colours: (names) => { "--color-x": [r, g, b, a] }. The same function reads ZCode's capture and Modbit's
// build (apps/desktop/e2e), so equal declarations always compare equal and a real difference never hides in the conversion.
export const TOKENS_SRC = `(names) => {
  const cv = document.createElement("canvas"); cv.width = cv.height = 1;
  const cx = cv.getContext("2d", { willReadFrequently: true });
  const px = (c, bg) => { cx.clearRect(0, 0, 1, 1); cx.fillStyle = bg; cx.fillRect(0, 0, 1, 1); cx.fillStyle = c; cx.fillRect(0, 0, 1, 1); return cx.getImageData(0, 0, 1, 1).data; };
  const probe = document.createElement("div"); document.body.appendChild(probe);
  const o = {};
  for (const n of names) {
    probe.style.color = "var(" + n + ")";
    const c = getComputedStyle(probe).color;
    if (!c) continue;
    const w = px(c, "#fff"), b = px(c, "#000");
    const a = Math.max(0, Math.min(1, 1 - (w[0] - b[0]) / 255));
    o[n] = a < 0.004 ? [0, 0, 0, 0] : [Math.round(b[0] / a), Math.round(b[1] / a), Math.round(b[2] / a), Math.round(a * 1000) / 1000];
  }
  probe.remove();
  return o;
}`;
