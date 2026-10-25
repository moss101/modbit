/**
 * REQ-PX-094 / QUAL-PX-094 (docs/78 ESH-B01..B04, docs/65): the window's chrome and its remembered state, as pure
 * functions plus one small file store, so the main process only wires them.
 *
 * - `chromeOptions` is the platform's window options for the inset title bar (macOS) and the opt-in translucency. It
 *   returns window-chrome keys and nothing else: never `webPreferences`, never a frame/security option. The hardening
 *   guard pins the key set it may return.
 * - `chromeQuery` is how the renderer learns which chrome it sits in (a query string on the page it loads, so the
 *   preload bridge and the IPC channel set stay byte-identical).
 * - `WindowStateStore` persists position, size, maximised and fullscreen per display configuration and
 *   `validateRestore` makes a saved window land on a screen that exists: a window never opens off every display.
 */
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface DisplayInfo {
  bounds: Rect;
  workArea: Rect;
  primary?: boolean | undefined;
}

export interface SavedWindow {
  bounds: Rect;
  maximized: boolean;
  fullscreen: boolean;
}

/** The pt of the spec: the 40 pt top bar of PX-045 is the drag region and the height the window controls are centred in. */
export const TOP_BAR_PT = 40;
/** Three controls of 12 pt with 8 pt gaps is 52 pt; the renderer reserves this plus the leading inset and a gap. */
export const CONTROLS_WIDTH_PT = 52;
export const CONTROLS_HEIGHT_PT = 14;

export interface ControlsPosition {
  x: number;
  y: number;
}

/**
 * Where the macOS window controls sit, in points from the window's top-left. The formula (a leading inset, the
 * top-bar height minus the controls' own height halved) is the reference's, inferred from code; the constants are
 * Modbit's own measurements per operating-system generation (docs/78 ESH-B01). `measured` names the Darwin majors
 * this table was read against on a machine; a later one uses the newest row until it has its own (UNVERIFIED).
 */
const CONTROLS_BY_DARWIN_MAJOR: readonly { fromDarwin: number; x: number; heightPt: number }[] = [
  { fromDarwin: 0, x: 14, heightPt: 14 },
  // macOS 26 and later draw taller, rounder controls; the inset grows with them.
  { fromDarwin: 25, x: 16, heightPt: 16 },
];

export const MEASURED_DARWIN_MAJORS: readonly number[] = [27];

export function darwinMajor(osRelease: string): number {
  const n = Number.parseInt(osRelease.split(".")[0] ?? "", 10);
  return Number.isFinite(n) ? n : 0;
}

export function controlsPosition(osRelease: string): ControlsPosition {
  const major = darwinMajor(osRelease);
  const row = [...CONTROLS_BY_DARWIN_MAJOR].reverse().find((r) => major >= r.fromDarwin) ?? CONTROLS_BY_DARWIN_MAJOR[0]!;
  return { x: row.x, y: Math.round((TOP_BAR_PT - row.heightPt) / 2) };
}

/** What the renderer must keep clear at the top left, in points: the controls' right edge and a gap. */
export function controlsReserve(osRelease: string): { width: number; height: number } {
  const p = controlsPosition(osRelease);
  return { width: p.x + CONTROLS_WIDTH_PT + 12, height: TOP_BAR_PT };
}

/** Which platforms have the inset title bar. Windows and Linux keep their platform frame behind this check (CI_COMPATIBLE, ESH-A02). */
export const insetSupported = (platform: string): boolean => platform === "darwin";
/** Which platforms have a translucent material the window can ask for. */
export const translucencySupported = (platform: string): boolean => platform === "darwin" || platform === "win32";

/** The opt-in (off by default) under an operating-system Reduce transparency that always wins. */
export function effectiveTranslucent(optIn: boolean, reduceTransparency: boolean, platform: string): boolean {
  return optIn && !reduceTransparency && translucencySupported(platform);
}

/** The only window options chrome may set; the hardening guard compares this list with what `chromeOptions` can return. */
export const CHROME_OPTION_KEYS = ["titleBarStyle", "trafficLightPosition", "vibrancy", "backgroundMaterial", "backgroundColor"] as const;

export interface ChromeOptions {
  titleBarStyle?: "hidden";
  trafficLightPosition?: ControlsPosition;
  vibrancy?: "sidebar";
  backgroundMaterial?: "mica";
  backgroundColor?: string;
}

export function chromeOptions(i: { platform: string; osRelease: string; translucent: boolean }): ChromeOptions {
  const out: ChromeOptions = {};
  if (insetSupported(i.platform)) {
    out.titleBarStyle = "hidden";
    out.trafficLightPosition = controlsPosition(i.osRelease);
  }
  if (i.translucent && translucencySupported(i.platform)) {
    if (i.platform === "darwin") out.vibrancy = "sidebar";
    else out.backgroundMaterial = "mica";
    // A material shows through a window with no fill of its own.
    out.backgroundColor = "#00000000";
  }
  return out;
}

/** The query the renderer's page is loaded with: its chrome, nothing else. */
export function chromeQuery(i: { platform: string; osRelease: string; translucent: boolean }): Record<string, string> {
  const q: Record<string, string> = {};
  if (insetSupported(i.platform)) {
    q.chrome = "inset";
    const r = controlsReserve(i.osRelease);
    q.reserve = `${r.width}x${r.height}`;
  }
  if (i.translucent && translucencySupported(i.platform)) q.translucent = "1";
  return q;
}

// ---- window state ---------------------------------------------------------

const MAX_CONFIGS = 8;
/** A window must show at least this much of itself on a display (width x height of the overlap), the bar included, to count as visible. */
const MIN_VISIBLE = { width: 160, height: 80 } as const;

const isRect = (v: unknown): v is Rect => {
  if (typeof v !== "object" || v === null) return false;
  const r = v as Record<string, unknown>;
  return ["x", "y", "width", "height"].every((k) => typeof r[k] === "number" && Number.isFinite(r[k] as number));
};

/** One display configuration, as a key: the displays' bounds, order-independent. */
export function displaySignature(displays: readonly DisplayInfo[]): string {
  return displays
    .map((d) => `${d.bounds.width}x${d.bounds.height}@${d.bounds.x},${d.bounds.y}`)
    .sort()
    .join("|");
}

const overlap = (a: Rect, b: Rect): { width: number; height: number } => ({
  width: Math.max(0, Math.min(a.x + a.width, b.x + b.width) - Math.max(a.x, b.x)),
  height: Math.max(0, Math.min(a.y + a.height, b.y + b.height) - Math.max(a.y, b.y)),
});

/** A window is visible when a display's usable area shows enough of it, and its top (the bar) is not above that area. */
export function isVisible(r: Rect, displays: readonly DisplayInfo[]): boolean {
  return displays.some((d) => {
    const o = overlap(r, d.workArea);
    return o.width >= Math.min(MIN_VISIBLE.width, r.width) && o.height >= Math.min(MIN_VISIBLE.height, r.height) && r.y >= d.workArea.y - 1 && r.y < d.workArea.y + d.workArea.height - MIN_VISIBLE.height / 2;
  });
}

export interface Restore {
  bounds: Rect;
  maximized: boolean;
  fullscreen: boolean;
  /** Why the saved window was not used as it was: for the log and the proof, never shown. */
  moved: "none" | "no-record" | "off-screen" | "invalid";
}

/** The window to open: the saved one when it is on a screen that exists, otherwise a centred default on the primary display. */
export function validateRestore(saved: SavedWindow | undefined, displays: readonly DisplayInfo[], fallback: { width: number; height: number }, min: { width: number; height: number }): Restore {
  const screens = displays.length > 0 ? displays : [{ bounds: { x: 0, y: 0, width: 1920, height: 1080 }, workArea: { x: 0, y: 0, width: 1920, height: 1080 }, primary: true }];
  const primary = screens.find((d) => d.primary) ?? screens[0]!;
  const centred = (size: { width: number; height: number }): Rect => {
    const width = Math.max(min.width, Math.min(size.width, primary.workArea.width));
    const height = Math.max(min.height, Math.min(size.height, primary.workArea.height));
    return { x: Math.round(primary.workArea.x + (primary.workArea.width - width) / 2), y: Math.round(primary.workArea.y + (primary.workArea.height - height) / 2), width, height };
  };
  if (!saved) return { bounds: centred(fallback), maximized: false, fullscreen: false, moved: "no-record" };
  if (!isRect(saved.bounds) || saved.bounds.width < 1 || saved.bounds.height < 1) return { bounds: centred(fallback), maximized: false, fullscreen: false, moved: "invalid" };
  const b: Rect = { x: Math.round(saved.bounds.x), y: Math.round(saved.bounds.y), width: Math.max(min.width, Math.round(saved.bounds.width)), height: Math.max(min.height, Math.round(saved.bounds.height)) };
  if (!isVisible(b, screens)) return { bounds: centred({ width: b.width, height: b.height }), maximized: saved.maximized === true, fullscreen: false, moved: "off-screen" };
  return { bounds: b, maximized: saved.maximized === true, fullscreen: saved.fullscreen === true, moved: "none" };
}

interface StateFile {
  version: 1;
  /** The configuration the window was last saved under: the fallback when the displays have changed. */
  last: string;
  byDisplays: Record<string, SavedWindow>;
}

const emptyState = (): StateFile => ({ version: 1, last: "", byDisplays: {} });

function sanitize(v: unknown): StateFile {
  if (typeof v !== "object" || v === null) return emptyState();
  const f = v as Partial<StateFile>;
  if (f.version !== 1 || typeof f.byDisplays !== "object" || f.byDisplays === null) return emptyState();
  const out = emptyState();
  for (const [k, s] of Object.entries(f.byDisplays)) {
    const w = s as Partial<SavedWindow> | null;
    if (w && isRect(w.bounds)) out.byDisplays[k] = { bounds: { x: w.bounds.x, y: w.bounds.y, width: w.bounds.width, height: w.bounds.height }, maximized: w.maximized === true, fullscreen: w.fullscreen === true };
  }
  out.last = typeof f.last === "string" && f.last in out.byDisplays ? f.last : "";
  return out;
}

/** Position, size, maximised and fullscreen per display configuration, in the profile (a file, written whole and renamed into place). */
export class WindowStateStore {
  private state: StateFile;

  private readonly file: string;

  constructor(file: string) {
    this.file = file;
    try {
      this.state = sanitize(JSON.parse(readFileSync(file, "utf8")));
    } catch {
      this.state = emptyState();
    }
  }

  /** What is saved for these displays, else what was saved last under another configuration (it is validated on restore). */
  get(displays: readonly DisplayInfo[]): SavedWindow | undefined {
    const sig = displaySignature(displays);
    return this.state.byDisplays[sig] ?? (this.state.last ? this.state.byDisplays[this.state.last] : undefined);
  }

  /** The configuration key the saved window belongs to, for the proof. */
  keys(): string[] {
    return Object.keys(this.state.byDisplays);
  }

  set(displays: readonly DisplayInfo[], w: SavedWindow): void {
    const sig = displaySignature(displays);
    delete this.state.byDisplays[sig];
    this.state.byDisplays[sig] = w;
    this.state.last = sig;
    const keys = Object.keys(this.state.byDisplays);
    for (const k of keys.slice(0, Math.max(0, keys.length - MAX_CONFIGS))) delete this.state.byDisplays[k];
    this.flush();
  }

  private flush(): void {
    try {
      mkdirSync(dirname(this.file), { recursive: true });
      const tmp = `${this.file}.tmp`;
      writeFileSync(tmp, JSON.stringify(this.state));
      renameSync(tmp, this.file);
    } catch {
      // a window position is a convenience: failing to remember it never fails the window
    }
  }
}

/** The opt-in lives in the profile's preference file (`<profile>/window-chrome.json`); absent or unreadable means off. */
export function readTranslucencyOptIn(file: string): boolean {
  try {
    return (JSON.parse(readFileSync(file, "utf8")) as { translucency?: unknown }).translucency === true;
  } catch {
    return false;
  }
}
