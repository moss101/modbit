/**
 * REQ-PX-094 (docs/78 ESH-B01..B03): which window chrome the renderer sits in. The main process loads the page with a
 * query naming it (`?chrome=inset&reserve=WxH&translucent=1`), so no preload surface or IPC channel carries it. The
 * result is a few attributes and custom properties on the document element; the stylesheet does the rest:
 *
 * - `data-chrome="inset"`: the native window controls float over the top left of the agent-list strip. The strip
 *   reserves their box (`--mb-chrome-reserve-w` / `-h`, in native points) and the 40 pt bar is the window's drag
 *   region; controls inside the bar opt out of dragging.
 * - `data-translucent="1"`: the window has a material; the surfaces the stylesheet lists let it show through. Absent
 *   (the default, and always under Reduce transparency) every surface is opaque.
 */
export interface ChromeInfo {
  inset: boolean;
  translucent: boolean;
  reserve: { width: number; height: number };
}

const NONE: ChromeInfo = { inset: false, translucent: false, reserve: { width: 0, height: 0 } };

export function parseChrome(search: string): ChromeInfo {
  const q = new URLSearchParams(search);
  const inset = q.get("chrome") === "inset";
  const m = /^(\d{1,3})x(\d{1,3})$/.exec(q.get("reserve") ?? "");
  return {
    inset,
    translucent: q.get("translucent") === "1",
    reserve: inset && m ? { width: Number(m[1]), height: Number(m[2]) } : NONE.reserve,
  };
}

/** Applies the chrome to the document element; called once before the first render. */
export function applyChrome(doc: Document, search: string): ChromeInfo {
  const info = parseChrome(search);
  const root = doc.documentElement;
  if (info.inset) {
    root.dataset.chrome = "inset";
    root.style.setProperty("--mb-chrome-reserve-w", `${info.reserve.width}px`);
    root.style.setProperty("--mb-chrome-reserve-h", `${info.reserve.height}px`);
  }
  if (info.translucent) root.dataset.translucent = "1";
  return info;
}

export const insetChrome = (): boolean => typeof document !== "undefined" && document.documentElement.dataset.chrome === "inset";
