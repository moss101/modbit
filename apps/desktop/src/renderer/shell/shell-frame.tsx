/**
 * The three-region frame (AFW-A01..A09): agent list | conversation | apps
 * panel, with resizable splits inside the limits of layout.ts. Presentational:
 * the real shell and the state gallery both render it, from the same
 * preferences type, so what the gallery shows is the geometry the app has.
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { GEOMETRY } from "@modbit/design-tokens";
import { computeLayout, ratioForPanel, clampListWidth, clampPanelRatio } from "./layout.ts";
import { Splitter } from "./splitter.tsx";
import type { UiPrefs } from "./prefs.ts";

export interface ShellFrameProps {
  prefs: UiPrefs;
  /** Applies a preference change (the owner persists it). */
  onPrefs: (update: (p: UiPrefs) => UiPrefs) => void;
  /** The apps panel exists (a task has an artifact) and the person has it open. */
  panelOpen: boolean;
  /** The panel's content; null when no task has an artifact (the region is then absent). */
  panel: ReactNode | null;
  agents: (rail: boolean) => ReactNode;
  topBar: ReactNode;
  statusRow: ReactNode;
  tray: ReactNode;
  children: ReactNode;
  /** A fixed container width (the gallery); otherwise the frame follows its own box. */
  fixedWidth?: number | undefined;
  /** The content wrapper: the app's single <main>, or a plain div where the host page already has one (the gallery). */
  contentElement?: "main" | "div" | undefined;
}

export function ShellFrame({ prefs, onPrefs, panelOpen, panel, agents, topBar, statusRow, tray, children, fixedWidth, contentElement = "main" }: ShellFrameProps) {
  const root = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(fixedWidth ?? (typeof window === "undefined" ? GEOMETRY.windowDefault.width : window.innerWidth));
  const [segment, setSegment] = useState<"conversation" | "apps">("conversation");
  useEffect(() => {
    const el = root.current;
    if (!el || fixedWidth !== undefined) return;
    const measure = () => setWidth(Math.round(el.getBoundingClientRect().width));
    measure();
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    return () => ro.disconnect();
  }, [fixedWidth]);
  const hasPanel = panel !== null;
  const layout = computeLayout({ width: fixedWidth ?? width, listWidth: prefs.listWidth, listCollapsed: prefs.listCollapsed, panelRatio: prefs.panelRatio, panelOpen: hasPanel && panelOpen });
  const showApps = layout.stacked && segment === "apps";
  useEffect(() => {
    if (!layout.stacked && segment !== "conversation") setSegment("conversation");
  }, [layout.stacked, segment]);

  const rootLeft = () => root.current?.getBoundingClientRect().left ?? 0;
  const setList = useCallback((w: number) => onPrefs((p) => ({ ...p, listWidth: clampListWidth(w) })), [onPrefs]);
  const setPanel = useCallback((px: number) => onPrefs((p) => ({ ...p, panelRatio: clampPanelRatio(ratioForPanel(px, fixedWidth ?? width)) })), [onPrefs, width, fixedWidth]);

  const Content = contentElement;
  return (
    <div ref={root} className="shell" data-testid="shell" data-list-px={layout.listPx} data-center-px={layout.centerPx} data-panel-px={layout.panelPx} data-rail={layout.rail} data-stacked={layout.stacked} data-single-pane={layout.singlePane} style={fixedWidth === undefined ? undefined : { width: fixedWidth }}>
      <nav className="shell-list" aria-label="Agents" data-testid="region-agents" data-rail={layout.rail} style={{ width: layout.listPx }}>
        <div className="shell-scroll">{agents(layout.rail)}</div>
        {!layout.rail && (
          <Splitter
            label="Resize agent list"
            testId="splitter-list"
            edge="right"
            value={layout.listPx}
            min={GEOMETRY.listMin}
            max={GEOMETRY.listMax}
            onDragTo={(x) => setList(x - rootLeft())}
            onNudge={(d) => setList(layout.listPx + d)}
            onJump={(to) => setList(to === "min" ? GEOMETRY.listMin : GEOMETRY.listMax)}
            onReset={() => setList(GEOMETRY.listDefault)}
          />
        )}
      </nav>
      <div className="shell-center" data-testid="region-conversation">
        {topBar}
        {layout.stacked && (
          <nav aria-label="Conversation or apps" className="shell-segments" data-testid="stack-segments">
            <button type="button" className="mb-btn" data-variant="secondary" data-size="sm" aria-pressed={!showApps} data-testid="segment-conversation" onClick={() => setSegment("conversation")}>
              Conversation
            </button>
            <button type="button" className="mb-btn" data-variant="secondary" data-size="sm" aria-pressed={showApps} data-testid="segment-apps" onClick={() => setSegment("apps")}>
              Apps
            </button>
          </nav>
        )}
        <Content className="shell-content" data-testid="shell-content">
          {showApps ? panel : children}
        </Content>
        {tray}
        {statusRow}
      </div>
      {hasPanel && panelOpen && !layout.stacked && (
        <PanelRegion nested={contentElement === "div"} style={{ width: layout.panelPx }}>
          <Splitter
            label="Resize apps panel"
            testId="splitter-panel"
            edge="left"
            value={layout.panelPx}
            min={GEOMETRY.panelMin}
            max={layout.panelMax}
            growsLeft
            onDragTo={(x) => setPanel(rootLeft() + (fixedWidth ?? width) - x)}
            onNudge={(d) => setPanel(layout.panelPx + d)}
            onJump={(to) => setPanel(to === "min" ? GEOMETRY.panelMin : layout.panelMax)}
            onReset={() => onPrefs((p) => ({ ...p, panelRatio: GEOMETRY.panelRatioDefault }))}
          />
          <div className="shell-scroll">{panel}</div>
        </PanelRegion>
      )}
    </div>
  );
}

/** The apps panel's landmark: a complementary region in the app, a plain labelled region where the page already has landmarks around it (the gallery). */
function PanelRegion({ nested, style, children }: { nested: boolean; style: { width: number }; children: ReactNode }) {
  const props = { className: "shell-panel", "aria-label": "Apps", "data-testid": "region-apps", style };
  return nested ? <section {...props}>{children}</section> : <aside {...props}>{children}</aside>;
}
