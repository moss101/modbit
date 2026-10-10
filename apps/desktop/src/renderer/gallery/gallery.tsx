/**
 * The model-free state gallery (AFW-K02, REQ-PX-044): approvals, generating,
 * error, empty and offline states, the primitives and the shell geometry,
 * rendered from fixtures with no Core and no model, for design review and for
 * the accessibility pass over every state. A QA aid only, present in a build
 * made with MODBIT_GALLERY=1 and reached at `#gallery`; it never counts as
 * proof that a behaviour works.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Badge, Button, Dialog, IconButton, Kbd, List, Menu, Row, StatusDot, Tabs, Tooltip, TrayHost, TrayStore, useEscapeLayers, useKeyDispatch, type TabDef } from "@modbit/ui";
import { THEME_LABEL, THEME_PREFERENCES, type ThemePreference } from "@modbit/design-tokens";
import { AgentRegion } from "../shell/agent-region.tsx";
import { IconPlus } from "../shell/icons.tsx";
import { ShellFrame } from "../shell/shell-frame.tsx";
import { StatusRow } from "../shell/status-row.tsx";
import { TopBar } from "../shell/top-bar.tsx";
import { DEFAULT_UI_PREFS, type UiPrefs } from "../shell/prefs.ts";
import { AgentListView } from "../agents/agent-list-view.tsx";
import { DEFAULT_LIST_PREFS } from "../agents/prefs.ts";
import { RowView, type RowContext } from "../conversation/rows.tsx";
import { TailStatus } from "../conversation/conversation.tsx";
import { ComposerGallery } from "../composer/composer-gallery.tsx";
import { ControlsGallery } from "./controls-gallery.tsx";
import { AutomationsGallery } from "./automations-gallery.tsx";
import { ProjectsGallery } from "./projects-gallery.tsx";
import { AGENT_HEADER_FIXTURES, CONVERSATION_ROW_FIXTURES, GALLERY_NOW } from "./workspace-fixtures.ts";
import { APPROVAL_FIXTURE, EMPTY_FIXTURE, ERROR_FIXTURE, FIXTURE_ROWS, GENERATING_FIXTURE, OFFLINE_FIXTURE, type GalleryState } from "./fixtures.ts";

const WIDTHS = [1280, 900, 600] as const;

const FIXTURE_CTX: RowContext = { sessionId: null, card: undefined, live: new Map(), approvals: [], openGroups: new Set(), onToggleGroup: () => {}, codeOpen: false, onCodeOpen: () => {}, onResume: () => {}, onCopyTurn: () => {}, highlightRowId: null, latestUserRowId: null };

function applyTheme(theme: ThemePreference): void {
  const root = document.documentElement;
  if (theme === "system") delete root.dataset.theme;
  else root.dataset.theme = theme;
}

export function Gallery() {
  useEscapeLayers();
  useKeyDispatch();
  const [theme, setTheme] = useState<ThemePreference>("system");
  const [log, setLog] = useState<string[]>([]);
  const note = (text: string) => setLog((l) => [...l.slice(-9), text]);
  useEffect(() => applyTheme(theme), [theme]);

  // One tray store for the approvals/error dock, so the host's single-active-tray rule is what is on screen.
  const store = useMemo(() => new TrayStore<React.ReactNode>(), []);
  const seeded = useRef(false);
  useEffect(() => {
    if (seeded.current) return;
    seeded.current = true;
    store.present({ id: "queue", tone: "info", title: "2 messages queued", body: "They run in order when the current turn ends.", actions: [{ id: "clear", label: "Clear", run: () => { note("queue: clear"); store.dismiss("queue"); } }] });
  }, [store]);

  const [dialogOpen, setDialogOpen] = useState(false);
  const [tab, setTab] = useState("changes");
  const [selectedRow, setSelectedRow] = useState("r1");
  const [width, setWidth] = useState<(typeof WIDTHS)[number]>(1280);
  const [prefs, setPrefs] = useState<UiPrefs>({ ...DEFAULT_UI_PREFS });
  const [panelOpen, setPanelOpen] = useState(true);

  const tabs: TabDef[] = [
    { id: "changes", label: "Changes" },
    { id: "terminal", label: "Terminal" },
    { id: "files", label: "Files", disabled: true },
  ];

  return (
    <main className="gallery" data-testid="gallery">
      <h1>State gallery</h1>
      <p className="meta">Fixtures only: no Core and no model are behind anything on this page, and nothing here proves a feature.</p>
      <div role="group" aria-label="Theme" className="gallery-row" data-testid="gallery-theme">
        {THEME_PREFERENCES.map((t) => (
          <Button key={t} size="sm" aria-pressed={theme === t} data-testid={`theme-${t}`} onClick={() => setTheme(t)}>
            {THEME_LABEL[t]}
          </Button>
        ))}
      </div>

      <section aria-labelledby="g-approvals" data-testid={`gallery-state-${"approvals" satisfies GalleryState}`}>
        <h2 id="g-approvals">Approvals</h2>
        <p className="meta">A tray docks above the composer; the active tray owns its keys.</p>
        <div className="gallery-row">
          <Button
            data-testid="present-approval"
            onClick={() => {
              store.present({
                id: APPROVAL_FIXTURE.id,
                tone: "attention",
                title: "Approve this command?",
                body: <code>{APPROVAL_FIXTURE.intent}</code>,
                priority: 10,
                dismissible: false,
                actions: [
                  { id: "approve", label: "Approve", keys: "mod+enter", primary: true, run: () => note("approval-1: approve") },
                  { id: "deny", label: "Deny", keys: "mod+backspace", run: () => note("approval-1: deny") },
                ],
              });
            }}
          >
            Present an approval
          </Button>
          <Button
            data-testid="present-second"
            onClick={() =>
              store.present({
                id: APPROVAL_FIXTURE.secondId,
                tone: "attention",
                title: APPROVAL_FIXTURE.secondTitle,
                body: APPROVAL_FIXTURE.secondIntent,
                priority: 10,
                dismissible: false,
                actions: [{ id: "approve-2", label: "Approve write", keys: "mod+shift+enter", primary: true, run: () => note("approval-2: approve") }],
              })
            }
          >
            Present a second approval
          </Button>
          <Button data-testid="present-error" onClick={() => store.present({ id: ERROR_FIXTURE.id, tone: "error", title: ERROR_FIXTURE.title, body: ERROR_FIXTURE.detail, actions: [{ id: "retry", label: "Retry", keys: "mod+shift+r", primary: true, run: () => note("error: retry") }] })}>
            Present an error
          </Button>
        </div>
        <TrayHost store={store} />
        <ul className="meta" aria-label="Key log" data-testid="gallery-log">
          {log.map((l, i) => (
            <li key={i}>{l}</li>
          ))}
        </ul>
      </section>

      <section aria-labelledby="g-generating" data-testid={`gallery-state-${"generating" satisfies GalleryState}`}>
        <h2 id="g-generating">Generating</h2>
        <div className="gallery-card">
          <p>
            <StatusDot status="running" label="Running" showLabel /> <span className="mb-shimmer" data-testid="shimmer">{GENERATING_FIXTURE.action}</span>
          </p>
          <p className="meta">{GENERATING_FIXTURE.elapsed}. {GENERATING_FIXTURE.steps.join(", ")}.</p>
          <Button variant="danger" size="sm">
            Stop
          </Button>
        </div>
      </section>

      <section aria-labelledby="g-error" data-testid={`gallery-state-${"error" satisfies GalleryState}`}>
        <h2 id="g-error">Error</h2>
        <div className="gallery-card" role="alert" data-tone="error">
          <p>
            <StatusDot status="danger" label="Error" showLabel /> <strong>{ERROR_FIXTURE.title}</strong>
          </p>
          <p>{ERROR_FIXTURE.detail}</p>
          <p className="meta">Request id {ERROR_FIXTURE.requestId}</p>
          <Button size="sm" variant="primary">
            Retry
          </Button>
        </div>
      </section>

      <section aria-labelledby="g-empty" data-testid={`gallery-state-${"empty" satisfies GalleryState}`}>
        <h2 id="g-empty">Empty</h2>
        <div className="gallery-card">
          <p>
            <StatusDot status="idle" label="Empty" showLabel /> <strong>{EMPTY_FIXTURE.title}</strong>
          </p>
          <p className="meta">{EMPTY_FIXTURE.detail}</p>
          <Button size="sm" variant="primary">
            New task <Kbd chord="mod+n" />
          </Button>
        </div>
      </section>

      <section aria-labelledby="g-offline" data-testid={`gallery-state-${"offline" satisfies GalleryState}`}>
        <h2 id="g-offline">Offline</h2>
        <div className="gallery-card" role="status" data-tone="warn">
          <p>
            <StatusDot status="warn" label="Offline" showLabel /> <strong>{OFFLINE_FIXTURE.title}</strong>
          </p>
          <p>{OFFLINE_FIXTURE.detail}</p>
        </div>
      </section>

      <section aria-labelledby="g-agents" data-testid="gallery-agent-list">
        <h2 id="g-agents">Agent list</h2>
        <p className="meta">One row of every status class the Core serves: glyph and words, an unread dot, an attention dot, a pin. Fixtures only.</p>
        <div className="gallery-shell-box" style={{ width: 300, height: 520 }}>
          <AgentListView
            headers={AGENT_HEADER_FIXTURES}
            loaded
            error={null}
            selectedId="fixture-task-5"
            pins={["fixture-task-7"]}
            grouping="status"
            filters={{ ...DEFAULT_LIST_PREFS.filters, showArchived: true }}
            subtitleFields={DEFAULT_LIST_PREFS.subtitle}
            collapsed={new Set()}
            nowMs={GALLERY_NOW}
            query=""
            search={{ state: "idle", results: null, error: null }}
            note={null}
            onQuery={() => {}}
            onSelect={(id) => note(`agent list: open ${id}`)}
            onPin={(id) => note(`agent list: pin ${id}`)}
            onUnpin={(id) => note(`agent list: unpin ${id}`)}
            onArchive={(id) => note(`agent list: archive ${id}`)}
            onToggleSection={() => {}}
            onGrouping={() => {}}
            onFilters={() => {}}
            onSubtitleFields={() => {}}
          />
        </div>
      </section>

      <section aria-labelledby="g-conversation" data-testid="gallery-conversation">
        <h2 id="g-conversation">Conversation rows</h2>
        <p className="meta">A user message, a folded work group with a failed step, a finished answer with a collapsed code block, a streaming answer, a stopped one, an unread divider, an approval and a failed turn footer.</p>
        <div className="gallery-card" style={{ display: "flex", flexDirection: "column", gap: 8 }}>
          {CONVERSATION_ROW_FIXTURES.map((r) => (
            <RowView key={r.rowId} row={r} ctx={FIXTURE_CTX} />
          ))}
          <TailStatus phase="RUNNING" label="Running a command: cargo test" detail="" lastActivityMs={Date.now()} />
        </div>
      </section>

      <ComposerGallery note={note} />
      <ControlsGallery note={note} />
      <AutomationsGallery note={note} />
      <ProjectsGallery />

      <section aria-labelledby="g-primitives" data-testid="gallery-primitives">
        <h2 id="g-primitives">Primitives</h2>
        <div className="gallery-row">
          <Button>Secondary</Button>
          <Button variant="primary" data-testid="g-primary">Primary</Button>
          <Button variant="danger">Danger</Button>
          <Button variant="ghost">Ghost</Button>
          <Button disabled>Disabled</Button>
          <IconButton label="Add" icon={<IconPlus />} data-testid="g-icon-button" />
          <Tooltip text="Adds a thing">
            <Button data-testid="g-tooltip-trigger">Hover or focus me</Button>
          </Tooltip>
          <Menu
            label="Sort by"
            trigger={<>Sort by</>}
            triggerTestId="g-menu"
            items={[
              { id: "recent", label: "Most recent", onSelect: () => note("sort: recent"), checked: true, radio: true },
              { id: "name", label: "Name", onSelect: () => note("sort: name"), checked: false, radio: true },
              { id: "off", label: "Unavailable", disabled: true, onSelect: () => {} },
            ]}
          />
          <Button data-testid="g-open-dialog" onClick={() => setDialogOpen(true)}>
            Open dialog
          </Button>
          <Badge>3</Badge>
          <Badge tone="ok">Passed</Badge>
          <Badge tone="warn">Waiting</Badge>
          <Badge tone="danger">Failed</Badge>
          <Badge tone="info">New</Badge>
          <Kbd chord="mod+k" />
          <StatusDot status="ok" label="OK" showLabel />
          <StatusDot status="warn" label="Warning" showLabel />
          <StatusDot status="danger" label="Error" showLabel />
          <StatusDot status="info" label="Info" showLabel />
          <StatusDot status="running" label="Running" showLabel />
          <StatusDot status="idle" label="Idle" showLabel />
        </div>
        <Tabs label="Example tabs" tabs={tabs} selected={tab} onSelect={setTab} idPrefix="g-tabs" testId="g-tabs">
          <p>Panel for {tab}.</p>
        </Tabs>
        <List label="Example rows" testId="g-list">
          {FIXTURE_ROWS.map((r) => (
            <Row key={r.id} testId={`g-row-${r.id}`} title={r.title} subtitle={r.subtitle} leading={<StatusDot status={r.status} label={r.status === "running" ? "Running" : r.status === "warn" ? "Needs attention" : "Done"} />} trailing={r.age} selected={selectedRow === r.id} onActivate={() => setSelectedRow(r.id)} />
          ))}
        </List>
        <Dialog open={dialogOpen} title="Example dialog" onClose={() => setDialogOpen(false)} testId="g-dialog">
          <p>Focus is trapped here. Escape closes it and focus returns to the opener.</p>
          <div className="dialog-actions">
            <Button onClick={() => setDialogOpen(false)}>Cancel</Button>
            <Button variant="primary" onClick={() => setDialogOpen(false)}>
              Confirm
            </Button>
          </div>
        </Dialog>
      </section>

      <section aria-labelledby="g-shell" data-testid="gallery-shell">
        <h2 id="g-shell">Shell geometry</h2>
        <div role="group" aria-label="Container width" className="gallery-row">
          {WIDTHS.map((w) => (
            <Button key={w} size="sm" aria-pressed={width === w} data-testid={`shell-width-${w}`} onClick={() => setWidth(w)}>
              {w}
            </Button>
          ))}
          <Button size="sm" aria-pressed={panelOpen} data-testid="shell-panel-toggle" onClick={() => setPanelOpen((o) => !o)}>
            Apps panel
          </Button>
        </div>
        <div className="gallery-shell-box" style={{ width }}>
          <ShellFrame
            fixedWidth={width}
            contentElement="div"
            prefs={prefs}
            onPrefs={(u) => setPrefs(u)}
            panelOpen={panelOpen}
            panel={<p className="meta" style={{ padding: 12 }}>Apps panel fixture.</p>}
            agents={(rail) => <AgentRegion rail={rail} canExpand={false} taskCount={3} attentionCount={1} onNewTask={() => {}} onExpand={() => {}} />}
            topBar={<TopBar title="Fleet" location="Local, trusted workspace" agentListCollapsed={false} onToggleAgentList={() => {}} canGoBack={false} onBack={() => {}} menuItems={[]} dashboardOpen={false} dashboardDisabled onToggleDashboard={() => {}} panelOpen={panelOpen} panelUnavailable={null} onTogglePanel={() => setPanelOpen((o) => !o)} />}
            tray={null}
            statusRow={<StatusRow coreLabel="Core connected (fixture)" coreStatus="ok" platform={null} location="Local, trusted workspace" />}
          >
            <p className="meta" style={{ padding: 12 }}>Conversation region fixture.</p>
          </ShellFrame>
        </div>
      </section>
    </main>
  );
}
