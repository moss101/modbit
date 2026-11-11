/**
 * The renderer's stylesheet text: the screens' styles (the former inline
 * <style> of index.html, now written against the design tokens, with the old
 * nine variables kept as aliases of token roles) and the shell's. The build
 * (build.mjs) concatenates the token stylesheet, the primitives' styles and
 * this text into dist/renderer/app.css. No literal colour appears here.
 */
export const legacyCss = `
:root { --bg: var(--mb-color-bg); --fg: var(--mb-color-text); --muted: var(--mb-color-text-muted); --card: var(--mb-color-raised); --line: var(--mb-color-border); --accent: var(--mb-color-accent-text); --warn: var(--mb-color-warn); --bad: var(--mb-color-danger); --ok: var(--mb-color-ok); }
html, body, #root { height: 100%; }
body { margin: 0; font-family: var(--font-sans); font-size: var(--text-ui-base); line-height: 1.5; background: var(--bg); color: var(--fg); }
h1, h2, h3 { text-wrap: balance; }
.banner { padding: 6px 20px; border-bottom: 1px solid var(--color-tab-border); font-size: var(--mb-type-caption-size); }
.banner[data-kind="restarting"] { background: var(--mb-color-warn-tint); }
.banner[data-kind="recovered"] { background: var(--mb-color-ok-tint); }
.banner[data-kind="error"] { background: var(--mb-color-danger-tint); }
section.welcome { font-size: var(--mb-type-chrome-size); }
section.welcome[data-needs-provider="true"] { margin: 0 0 var(--mb-space-3); background: var(--card); border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); padding: 12px; }
section.welcome .welcome-quiet { display: contents; }
.welcome-repo summary { cursor: pointer; width: fit-content; }
section.welcome ol.steps { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 10px; }
section.welcome form { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }
/* A bare <button> is the AWL default (AWL-05): the quiet 8 % fill, full text, no edge; a .mb-btn sets its own variant and wins by specificity. The main action of a form (a submit button) takes the primary recipe. */
:where(button:not(.mb-btn)) { box-sizing: border-box; min-height: var(--mb-control-md); padding: 0 10px; border: 1px solid transparent; border-radius: var(--radius-md); background: var(--color-selected); color: var(--color-foreground); font: inherit; font-size: var(--text-ui-base); line-height: 1.5; cursor: default; transition-property: var(--mb-transition-colors); transition-duration: var(--mb-motion-base); transition-timing-function: var(--mb-ease); }
:where(button:not(.mb-btn):hover:not(:disabled)) { background: var(--color-command-node-hover); }
:where(button[type="submit"]:not(.mb-btn)) { background: var(--color-primary); color: var(--color-primary-foreground); font-weight: 500; }
:where(button[type="submit"]:not(.mb-btn):hover:not(:disabled)) { background: var(--mb-color-accent-hover); }
:where(button:not(.mb-btn):active:not(:disabled)) { transform: scale(var(--mb-press-scale)); }
button:disabled { opacity: 0.5; }
.meta { color: var(--muted); font-size: var(--text-ui-sm); }
.empty { color: var(--muted); font-style: italic; }
.status { margin-left: auto; color: var(--muted); font-size: var(--text-ui-sm); }
.sr-only { position: absolute; left: -9999px; }
.actions { display: flex; gap: 8px; margin-top: 8px; }
.review { padding: 16px 20px; }
.review-head { display: flex; align-items: center; gap: 16px; margin-bottom: 12px; }
.review-head button { margin-left: auto; }
.review-body { display: grid; grid-template-columns: 1fr 360px; gap: 20px; }
article.file { background: var(--card); border: 1px solid var(--line); border-radius: var(--radius-xl); padding: 12px; margin-bottom: 12px; }
article.file h3 { margin: 0 0 8px; font-size: var(--text-ui-caption); display: flex; gap: 8px; align-items: baseline; }
article.file h3 button.small { margin-left: auto; font-size: var(--text-ui-xs); padding: 1px 6px; }
.patch { display: grid; gap: 6px; margin: 0 0 8px; padding: 8px; border: 1px dashed var(--mb-color-border-control); font-size: var(--text-ui-sm); }
.patch label { display: grid; gap: 2px; }
.patch textarea { min-height: 48px; font-family: var(--mb-font-mono); font-size: var(--text-ui-sm); }
.hunk { border: 1px solid var(--line); border-radius: var(--radius-lg); margin-bottom: 8px; overflow: hidden; }
.hunk[data-rejected="true"] { opacity: 0.6; border-style: dashed; }
.hunk-head { display: flex; justify-content: space-between; padding: 4px 8px; background: var(--bg); font-size: var(--text-ui-sm); }
.hunk pre { margin: 0; padding: 8px; font-family: var(--font-mono); font-size: var(--text-ui-sm); line-height: 1.5; overflow-x: auto; }
.hunk .add { color: var(--ok); }
.hunk .del { color: var(--bad); }
.hunk .ctx { color: var(--muted); }
aside.review-aside, .review-body aside { background: var(--card); border: 1px solid var(--line); border-radius: var(--radius-xl); padding: 12px; }
aside h3 { font-size: var(--text-ui-sm); font-weight: 600; color: var(--muted); margin: 12px 0 6px; }
aside h3:first-child { margin-top: 0; }
pre.small, ul.small { font-size: var(--text-ui-xs); max-height: 160px; overflow: auto; }
.state { padding: 4px 8px; border-radius: var(--radius-md); margin-top: 4px; }
/* A quiet state stays in the accessibility tree; only trouble takes room on screen. */
.state:not([data-kind="degraded"], [data-kind="error"], [data-kind="recovery"]) { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
.state[data-kind="degraded"] { background: var(--mb-color-warn-tint); color: var(--fg); }
.state[data-kind="error"] { background: var(--mb-color-danger-tint); color: var(--fg); }
.state[data-kind="recovery"] { background: var(--mb-color-ok-tint); color: var(--fg); }
.settings-legacy fieldset { border: 1px solid var(--line); border-radius: var(--radius-lg); display: flex; flex-wrap: wrap; gap: 10px; }
.settings input[type="number"] { width: 56px; padding: 2px 4px; }
button.small { font-size: var(--text-ui-xs); padding: 1px 6px; }
.browser-view { height: 60vh; min-height: 320px; border: 1px solid var(--line); border-radius: var(--radius-xl); background: repeating-linear-gradient(45deg, var(--card), var(--card) 10px, var(--bg) 10px, var(--bg) 20px); }
.decision { margin-top: 6px; padding: 6px 8px; border: 1px dashed var(--mb-color-border-control); border-radius: var(--radius-lg); display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
.decision input { padding: 2px 6px; font-size: var(--text-ui-sm); }
.hunk:focus { outline: 2px solid var(--accent); outline-offset: -2px; }
table.split { width: 100%; border-collapse: collapse; font-family: var(--font-mono); font-size: var(--text-ui-sm); line-height: 1.5; table-layout: fixed; }
table.split td { padding: 0 8px; white-space: pre; overflow-x: auto; vertical-align: top; width: 50%; }
table.split td.gap { background: var(--mb-color-hover); }
table.split td.add { color: var(--ok); }
table.split td.del { color: var(--bad); }
kbd { font-family: var(--font-mono); font-size: var(--text-ui-xs); }
[data-testid="review-check"]:focus, section.attention:focus, [data-testid="attention-item"]:focus, [data-testid="review-verification"]:focus { outline: 2px solid var(--accent); }
`;

export const shellCss = `
.settings-page.settings-page { padding-top: var(--mb-space-3); }
.settings.settings { max-width: 680px; gap: var(--mb-space-4); outline: none; }
.settings-section { display: flex; flex-direction: column; gap: var(--mb-space-1_5); scroll-margin-top: var(--mb-space-3); }
.settings-heading { margin: 0; font-size: var(--mb-type-chrome-size); font-weight: 600; }
.settings-card { background: var(--color-panel); border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); font-size: var(--mb-type-chrome-size); overflow: hidden; }
.settings-card-body { padding: 12px; display: flex; flex-direction: column; gap: var(--mb-space-2); border-top: 1px solid var(--color-tab-border); }
.settings-card > .settings-card-body:first-child, .settings-card.settings-card-body { border-top: 0; }
.settings-card-pad { padding: 10px 12px 0; }
.settings-note { margin: 0; }
.settings-row { box-sizing: border-box; min-height: 44px; display: flex; align-items: center; gap: var(--mb-space-3); padding: 7px 12px; border-top: 1px solid var(--color-tab-border); }
.settings-card > .settings-row:first-child { border-top: 0; }
.settings-row-text { flex: 1 1 0; min-width: 0; overflow-wrap: anywhere; }
.settings-row-control { flex: none; display: inline-flex; align-items: center; gap: var(--mb-space-1); }
.settings-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.settings-list-row { display: flex; align-items: center; justify-content: space-between; gap: var(--mb-space-2); }
.settings-form { display: grid; grid-template-columns: 1fr 1fr; gap: var(--mb-space-2); }
.settings-form label { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.settings-form input, .provider-form input, .provider-form select { box-sizing: border-box; width: 100%; }
.settings-hours input { width: 56px; padding: 2px 4px; }
input.settings-toggle.settings-toggle { appearance: none; position: relative; flex: none; width: 32px; height: 18px; margin: 0; border: 1px solid var(--mb-color-border-control); border-radius: calc(infinity * 1px); background: var(--color-selected); cursor: pointer; transition: background-color var(--mb-motion-fast); }
input.settings-toggle.settings-toggle::before { content: ""; position: absolute; top: 1px; left: 1px; width: 14px; height: 14px; border-radius: calc(infinity * 1px); background: var(--color-foreground); transition: transform var(--mb-motion-fast); }
input.settings-toggle.settings-toggle:checked { background: var(--color-primary); border-color: var(--color-primary); }
input.settings-toggle.settings-toggle:checked::before { transform: translateX(14px); background: var(--color-primary-foreground); }
input.settings-toggle.settings-toggle:focus-visible { outline: 2px solid var(--mb-color-focus); outline-offset: 2px; }
input.settings-toggle.settings-toggle::after, input.settings-toggle.settings-toggle:checked::after { content: none; }
.settings-seg { display: inline-flex; gap: 2px; padding: 2px; border-radius: var(--radius-lg); background: var(--color-selected); }
.settings-seg-btn.settings-seg-btn { min-height: 24px; height: 24px; padding: 0 8px; border: 0; border-radius: var(--radius-md); background: transparent; color: var(--mb-color-text-muted); font-size: var(--mb-type-caption-size); }
.settings-seg-btn.settings-seg-btn[aria-checked="true"] { background: var(--color-panel); color: var(--color-foreground); }
.settings-nav-head.settings-nav-head { padding-right: 8px; }
.settings-back.settings-back { width: 100%; flex: 1 1 auto; justify-content: flex-start; }
.settings-back .agent-nav-label { flex: 0 1 auto; }
.settings-nav-search { flex: none; padding: 4px 8px; }
.settings-nav-search input { box-sizing: border-box; width: 100%; font-size: var(--mb-type-chrome-size); padding: 4px 8px; }
.settings-nav-list { flex: 1 1 0; min-height: 0; overflow: auto; }
.settings-nav .agent-nav-row[aria-current="true"] { background: var(--color-selected); color: var(--color-foreground); }
.agent-nav-heading { margin: var(--mb-space-2) 0 0; padding: 0 calc(var(--mb-row-inset) + 10px); height: var(--mb-control-sm); display: flex; align-items: center; font-size: var(--mb-type-caption-size); font-weight: 400; color: var(--mb-color-text-muted); }
.provider-form { display: flex; flex-direction: column; gap: 6px; }
.agent-person.agent-person { flex: 0 1 auto; min-width: 0; max-width: 100%; height: var(--mb-control-md); min-height: 0; display: inline-flex; align-items: center; gap: var(--mb-space-2); padding: 0 6px 0 0; border: 0; border-radius: var(--mb-row-radius); background: transparent; color: var(--mb-color-text); font-size: var(--mb-type-chrome-size); }
.agent-foot > .mb-menu-host { flex: 0 1 auto; min-width: 0; }
/* MWS-01: the top bar and every pane head are one height (45, 40 in Compact) with a 1 px rule at 8 % under them; MWS-02: the bar sits on the conversation's surface. */
.topbar { flex: none; box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 var(--mb-space-2) 0 var(--mb-space-5); background: transparent; border-bottom: 1px solid var(--color-tab-border); z-index: var(--mb-z-topbar); -webkit-app-region: drag; }
.topbar button, .topbar select { -webkit-app-region: no-drag; }
.topbar-title { margin: 0; flex: 0 1 auto; max-width: 60%; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); font-weight: 500; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.topbar-slot { flex: 1 1 0; min-width: 0; position: relative; display: flex; align-items: center; gap: var(--mb-space-2); }
.topbar-pill { flex: none; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); padding: 1px 6px; border-radius: calc(infinity * 1px); background: var(--color-selected); color: var(--mb-color-text-muted); }
.topbar-branch { flex: 0 1 auto; min-width: 0; display: inline-flex; align-items: center; gap: var(--mb-space-1); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.topbar-branch svg { flex: none; }
.topbar-branch span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.shell-toast { position: absolute; left: 50%; bottom: 16px; transform: translateX(-50%); z-index: var(--mb-z-tray); margin: 0; max-width: 80%; padding: var(--mb-space-1) var(--mb-space-3); border: 1px solid var(--color-tab-border); border-radius: var(--radius-lg); background: var(--color-panel); box-shadow: var(--mb-elevation-2); }
.shell-frame { position: relative; }
.task-details { position: absolute; top: calc(100% + (var(--mb-top-bar) - var(--mb-control-md)) / 2); right: 0; z-index: var(--mb-z-popover); width: min(440px, 80vw); max-height: 70vh; overflow: auto; box-sizing: border-box; display: flex; flex-direction: column; gap: var(--mb-space-2); padding: var(--mb-space-3); border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); background: var(--color-panel); box-shadow: var(--mb-elevation-2); font-size: var(--mb-type-chrome-size); -webkit-app-region: no-drag; }
.task-facts { margin: 0; display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: 2px var(--mb-space-3); }
.task-fact { display: contents; }
.task-fact dt { color: var(--mb-color-text-muted); }
.task-fact dd { margin: 0; overflow-wrap: anywhere; }
.task-subtasks { list-style: none; margin: 0; padding: 0; }
.task-section { margin: 0; }
.task-section summary { cursor: pointer; }
.task-controls { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); }
.home { height: 100%; min-height: 0; overflow: auto; display: flex; padding: var(--mb-space-5); box-sizing: border-box; }
.home-col { width: 100%; max-width: 640px; margin: auto; }
.home-form { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.home-goal.home-goal { padding: 2px 2px; font-size: var(--mb-type-body-size); line-height: var(--mb-type-body-line); }
.home-repo { flex: 1 1 0; min-width: 0; display: inline-flex; align-items: center; gap: var(--mb-space-1); color: var(--mb-color-text-muted); }
.home-repo svg { flex: none; }
.home-repo input, .home-issue { min-width: 0; flex: 1 1 0; box-sizing: border-box; border: 0; background: transparent; color: var(--mb-color-text); font: inherit; font-size: var(--mb-type-caption-size); padding: 2px 4px; outline: none; }
.home-issue { flex: 0 1 240px; text-align: right; }
.home-repo input::placeholder, .home-issue::placeholder { color: var(--mb-color-text-muted); opacity: 1; }
.home-repo input:focus-visible, .home-issue:focus-visible { box-shadow: inset 0 -1px 0 var(--mb-color-focus); }
.home-note { margin: 0; }
.page, .autos-scroll { height: 100%; min-height: 0; overflow: auto; box-sizing: border-box; }
.page { padding: var(--mb-space-3) var(--mb-space-5) var(--mb-space-5); }
.page-col { width: 100%; max-width: 720px; margin: 0 auto; display: flex; flex-direction: column; gap: var(--mb-space-2); }
.page-title { margin: 0; font-size: var(--mb-type-title-size); font-weight: 600; }
.page-heading { margin: var(--mb-space-3) 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); font-weight: 600; color: var(--mb-color-text-muted); }
.page-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; }
.page-row.page-row { width: 100%; height: auto; min-height: 44px; display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1_5) var(--mb-space-2); border: 0; border-radius: var(--radius-lg); background: transparent; color: var(--mb-color-text); text-align: left; }
.page-row.page-row:hover:not(:disabled) { background: var(--color-hover); }
.page-note { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1_5) var(--mb-space-2); }
.page-row-main { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; }
.page-row-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-chrome-size); }
.agent-region { padding: 0; display: flex; flex-direction: column; }
.agent-region-head { flex: none; box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; gap: 2px; padding: 0 8px; -webkit-app-region: drag; }
.agent-region-head button { -webkit-app-region: no-drag; }
:root[data-chrome="mac"] .agent-region-head { padding-left: 80px; }
/* MWS-01: navigation rows are the row height (32, 30 in Compact), 2 apart, radius 8, inset 8 from the list's edges. */
.agent-nav { list-style: none; margin: 0; padding: var(--mb-space-1) var(--mb-row-inset) 0; display: flex; flex-direction: column; gap: var(--mb-row-gap); }
.agent-nav-row.agent-nav-row { box-sizing: border-box; width: 100%; height: var(--mb-row); min-height: 0; display: flex; align-items: center; gap: 10px; padding: 0 10px; border: 0; border-radius: var(--mb-row-radius); background: transparent; color: var(--mb-color-text); font: inherit; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); text-align: left; transition: none; }
.agent-nav-row > svg { flex: none; color: var(--mb-color-text-muted); }
.agent-nav-count { flex: none; box-sizing: border-box; min-width: 18px; height: 18px; padding: 0 5px; display: inline-flex; align-items: center; justify-content: center; border-radius: calc(infinity * 1px); background: var(--mb-color-warn-tint); color: var(--mb-color-warn); font-size: var(--text-ui-xs); font-weight: 600; font-variant-numeric: tabular-nums; }
.agent-foot { flex: none; box-sizing: border-box; display: flex; align-items: center; gap: var(--mb-space-2); height: var(--mb-list-foot); padding: 0 var(--mb-row-inset) 0 14px; border-top: 1px solid var(--color-tab-border); color: var(--mb-color-text-muted); font-size: var(--mb-type-chrome-size); }
.agent-avatar { flex: none; width: 22px; height: 22px; display: grid; place-items: center; border-radius: calc(infinity * 1px); background: var(--color-selected); font-size: var(--text-ui-xs); font-weight: 600; }
.agent-core { flex: none; display: inline-flex; align-items: center; gap: 6px; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); white-space: nowrap; }
.agent-core-dot { width: 6px; height: 6px; border-radius: calc(infinity * 1px); background: var(--mb-color-warn); }
.agent-core[data-status="ok"] .agent-core-dot { background: var(--mb-color-ok); }
.agent-core[data-status="danger"] .agent-core-dot { background: var(--mb-color-danger); outline: 1px solid var(--mb-color-text); }
.apps-panel { position: relative; height: 100%; min-height: 0; display: flex; flex-direction: column; }
.apps-panel > .mb-tabs { flex: 1 1 0; min-height: 0; }
.apps-panel .mb-tablist { box-sizing: border-box; width: 100%; max-width: none; height: var(--mb-top-bar); padding: 0 64px 0 8px; border-radius: 0; border-bottom: 1px solid var(--color-tab-border); overflow-x: auto; }
.apps-panel .mb-tabpanel { min-height: 0; overflow: auto; display: flex; flex-direction: column; }
.apps-panel-tools { position: absolute; top: calc((var(--mb-top-bar) - var(--mb-control-sm)) / 2); right: 8px; z-index: 1; display: inline-flex; gap: 2px; }
.apps-panel .mb-tabpanel > * { padding-inline: var(--mb-space-3); }
.apps-panel .mb-tabpanel > .changes-app { padding: 0; flex: 1 1 0; min-height: 0; }
.changes-app { gap: 0; }
.changes-head { flex: none; box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 12px; border-bottom: 1px solid var(--color-tab-border); font-size: var(--mb-type-chrome-size); color: var(--mb-color-text-muted); }
.changes-summary { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.changes-scroll { flex: 1 1 0; min-height: 0; overflow: auto; }
.changes-files { padding: 6px; border-bottom: 1px solid var(--color-tab-border); }
.changes-row { position: relative; display: flex; align-items: center; gap: var(--mb-space-1); }
/* The status letter gives way to the stage box under the pointer and while the file is staged. */
.changes-row input[type="checkbox"] { position: absolute; left: 6px; top: 7px; z-index: 1; margin: 0; opacity: 0; }
.changes-row:hover input[type="checkbox"], .changes-row input[type="checkbox"]:focus-visible, .changes-row input[type="checkbox"]:checked, .changes-row input[type="checkbox"]:indeterminate { opacity: 1; }
.changes-row:hover .changes-status, .changes-row:has(input:focus-visible) .changes-status, .changes-row:has(input:checked) .changes-status, .changes-row:has(input:indeterminate) .changes-status { visibility: hidden; }
.changes-row .changes-file { flex: 1 1 0; min-width: 0; height: 28px; display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 8px; font-size: var(--mb-type-caption-size); }
.changes-status { flex: none; width: 12px; font-family: var(--mb-font-mono); font-size: var(--text-ui-xs); font-weight: 600; color: var(--mb-color-warn); }
.changes-status[data-status="A"] { color: var(--mb-color-ok); }
.changes-status[data-status="D"] { color: var(--mb-color-danger); }
.changes-path { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.changes-counts { flex: none; display: inline-flex; gap: 4px; font-family: var(--mb-font-mono); font-size: var(--text-ui-xs); }
.changes-row .changes-revert { position: absolute; right: 8px; top: 2px; opacity: 0; background: var(--color-panel); }
.changes-row:hover .changes-revert, .changes-row:focus-within .changes-revert { opacity: 1; }
.changes-row:hover .changes-counts, .changes-row:focus-within .changes-counts { visibility: hidden; }
.changes-diff-foot { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-2) 12px; }
.changes-diff-foot input { flex: 1 1 0; min-width: 0; box-sizing: border-box; border: 0; background: transparent; color: var(--mb-color-text); font: inherit; font-size: var(--mb-type-caption-size); outline: none; }
.changes-diff-foot input::placeholder { color: var(--mb-color-text-muted); opacity: 1; }
.changes-diff-foot input:focus-visible { box-shadow: inset 0 -1px 0 var(--mb-color-focus); }
.changes-commit { flex: none; display: flex; flex-direction: column; gap: var(--mb-space-1); padding: 10px 12px; border-top: 1px solid var(--color-tab-border); }
.changes-commit-actions { display: flex; align-items: center; gap: var(--mb-space-2); }
.changes-commit textarea { flex: 1 1 0; min-width: 0; height: var(--mb-control-md); min-height: var(--mb-control-md); max-height: 96px; box-sizing: border-box; resize: none; padding: 4px 10px; font: inherit; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-lg); background: transparent; color: var(--mb-color-text); }
.changes-commit textarea::placeholder { color: var(--mb-color-text-muted); opacity: 1; }
.changes-effects { padding: var(--mb-space-2) 12px; }
.apps-panel .hunk { border: 0; border-radius: 0; margin: 0; }
.apps-panel .hunk-head { position: relative; display: flex; align-items: center; min-height: 28px; gap: var(--mb-space-2); padding: 0 12px; background: var(--color-surface); border-bottom: 1px solid var(--color-tab-border); color: var(--mb-color-text-muted); font-family: var(--mb-font-mono); font-size: var(--text-ui-xs); }
.apps-panel .hunk-head > span:first-child { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.apps-panel .hunk-head .mb-btn, .apps-panel .hunk-head .changes-chip { font-family: var(--font-sans); opacity: 0; }
.apps-panel .hunk:hover .hunk-head .mb-btn, .apps-panel .hunk:hover .hunk-head .changes-chip, .apps-panel .hunk-head:focus-within .mb-btn, .apps-panel .hunk-head:focus-within .changes-chip { opacity: 1; }
.apps-panel .hunk pre { padding: 0; font-family: var(--mb-font-mono); font-size: var(--mb-type-mono-size); line-height: var(--mb-type-mono-line); }
.hunk .dl.dl { display: grid; grid-template-columns: 36px 36px minmax(0, 1fr); }
.dl-n { color: var(--mb-color-text-muted); text-align: right; padding-right: 10px; user-select: none; }
.dl-t { white-space: pre; }
.hunk .dl.add { background: var(--mb-color-ok-tint); color: var(--mb-color-text); }
.hunk .dl.del { background: var(--mb-color-danger-tint); color: var(--mb-color-text); }
.hunk .dl.ctx { color: var(--mb-color-text); }
/* AWL-06 with the surface mapping of DR-PX-2026-11-11-021 MWS-02: one window of unframed panes. The task list on the sidebar surface; the conversation, the top bar and the apps panel on the editor surface; 1 px dividers at 8 %, no radius and no gaps. */
.shell { display: flex; height: 100%; min-width: 0; overflow: hidden; background: var(--color-background-win-alt); }
.shell-list { box-sizing: border-box; position: relative; flex: none; background: var(--mb-surface-list); border-right: 1px solid var(--color-tab-border); }
.shell-center { flex: 1 1 0; min-width: 0; box-sizing: border-box; display: flex; flex-direction: column; }
.shell-frame { flex: 1 1 0; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--mb-surface-conversation); }
.shell-content { flex: 1 1 0; min-height: 0; overflow: auto; }
.shell-panel { box-sizing: border-box; position: relative; flex: none; background: var(--mb-surface-conversation); border-left: 1px solid var(--color-tab-border); }
.shell-segments { display: flex; gap: var(--mb-space-1); padding: var(--mb-space-1) var(--mb-space-3); border-bottom: 1px solid var(--mb-color-border); }
.shell-scroll { height: 100%; overflow: auto; }
.splitter { position: absolute; top: 0; bottom: 0; width: 4px; z-index: var(--mb-z-panel); cursor: col-resize; background: transparent; }
.splitter[data-edge="right"] { right: -2px; }
.splitter[data-edge="left"] { left: -2px; }
.splitter::after { content: ""; position: absolute; top: 0; bottom: 0; left: 50%; width: 2px; margin-left: -1px; background: var(--color-resize-handle); opacity: 0; transition: opacity var(--mb-motion-fast); }
.splitter:hover::after, .splitter:focus-visible::after, .splitter:active::after { opacity: 1; }
.topbar-spacer { flex: 1 1 0; }
.agent-nav-row.agent-nav-row:hover:not(:disabled) { background: var(--color-hover); color: var(--color-foreground); }
.agent-nav-row.agent-nav-row[aria-pressed="true"] { background: var(--color-selected); color: var(--color-foreground); }
.agent-nav-row.agent-nav-row:active:not(:disabled) { transform: none; }
.agent-nav-label { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agent-nav-key { flex: none; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agent-core-word { overflow: hidden; text-overflow: ellipsis; }
.agent-rail { padding-top: var(--mb-top-bar); display: flex; flex-direction: column; align-items: center; gap: var(--mb-space-1); }
.brand { font-weight: 600; font-size: var(--mb-type-title-size); }
.terminal-app, .changes-app, .files-app, .evidence-app, .browser-app { display: flex; flex-direction: column; gap: var(--mb-space-2); min-width: 0; }
.terminal-list, .changes-files, .files-tree, .evidence-list { list-style: none; margin: 0; padding: 0; }
.terminal-row { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) 0; }
.terminal-pick, .changes-file, .files-entry { background: transparent; color: var(--mb-color-text); border: 1px solid transparent; text-align: left; padding: var(--mb-space-1) var(--mb-space-2); border-radius: var(--radius-md); }
.terminal-pick { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.terminal-pick:hover:not(:disabled), .changes-file:hover:not(:disabled), .files-entry:hover:not(:disabled) { background: var(--mb-color-hover); border-color: transparent; }
.terminal-pick[aria-pressed="true"], .changes-file[aria-pressed="true"], .files-entry[aria-pressed="true"] { background: var(--mb-color-selected); }
.changes-refusal { border: 1px solid var(--mb-color-danger); background: var(--mb-color-danger-tint); border-radius: var(--radius-md); padding: var(--mb-space-2); }
.changes-refusal p, .changes-result p, .changes-approval p { margin: 0 0 var(--mb-space-1); }
.changes-undo, .changes-result { border: 1px solid var(--mb-color-border); border-radius: var(--radius-md); padding: var(--mb-space-2); }
.changes-approval { border: 1px solid var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--radius-md); padding: var(--mb-space-2); }
.changes-effects ul { list-style: none; margin: 0; padding: 0; }
.changes-chip { font: inherit; font-size: var(--mb-type-caption-size); background: transparent; color: var(--accent); border: 1px solid var(--line); border-radius: 999px; padding: 1px 8px; cursor: pointer; }
.changes-chip:disabled { color: var(--muted); cursor: default; }
.changes-chip:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.changes-decision { padding: 4px 8px; border: 1px solid var(--line); border-radius: 6px; margin-bottom: 8px; }
.customize { padding: var(--mb-space-3); max-width: 960px; }
.customize-head { display: flex; align-items: center; gap: var(--mb-space-2); }
.customize-head h2 { margin-right: auto; }
.customize-list { list-style: none; padding: 0; margin: 0; }
.customize-row { border: 1px solid var(--line); border-radius: 6px; padding: 8px; margin-bottom: 8px; }
.customize-row p { margin: 2px 0; }
.customize-actions { display: flex; gap: var(--mb-space-2); margin-top: 4px; }
.customize-import { display: flex; align-items: center; gap: var(--mb-space-2); margin-top: var(--mb-space-2); }
.xterm-wrap { background: var(--mb-surface-fill); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); padding: var(--mb-space-1); height: 320px; min-height: 160px; resize: vertical; overflow: hidden; }
.xterm-host { height: 100%; }
.terminal-history-pane { max-height: 200px; overflow: auto; background: var(--mb-surface-fill); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); }
.terminal-history-pane:focus-visible { outline: 2px solid var(--mb-color-accent); }
.terminal-history-pane pre { margin: 0; padding: var(--mb-space-2); font-family: var(--font-mono); font-size: var(--text-ui-sm); line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
.xterm-wrap:focus-within { outline: 2px solid var(--mb-color-accent); }
pre.codeview { margin: 0; padding: var(--mb-space-2); max-height: 50vh; overflow: auto; font-family: var(--font-mono); font-size: var(--text-ui-sm); line-height: 1.5; background: var(--mb-surface-fill); border-radius: var(--radius-md); }
pre.codeview .add { background: var(--mb-color-ok-tint); color: var(--mb-color-text); }
.evidence-table { border-collapse: collapse; width: 100%; font-size: var(--mb-type-chrome-size); }
.evidence-table th, .evidence-table td { text-align: left; padding: var(--mb-space-0_5) var(--mb-space-2); border-bottom: 1px solid var(--mb-color-border); }
.mb-tab:hover:not(:disabled) { background: var(--color-hover); color: var(--mb-color-text); border-color: transparent; }
.mb-tab[aria-selected="true"]:hover:not(:disabled) { background: var(--color-selected); }
/* AWL-09: apps-panel headings are sentence-case captions; explanatory text is caption size and muted. */
.apps-panel h2, .apps-panel h3 { font-size: var(--mb-type-caption-size); font-weight: 600; color: var(--mb-color-text-muted); margin: var(--mb-space-3) 0 var(--mb-space-1); }
.apps-panel p.meta { margin: var(--mb-space-1) 0; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); }
.palette { padding: var(--mb-space-2); }
.palette-input { width: 100%; box-sizing: border-box; font-size: var(--mb-type-body-size); }
.palette-list { list-style: none; margin: var(--mb-space-2) 0 0; padding: 0; max-height: 50vh; overflow: auto; }
.palette-option { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1_5) var(--mb-space-2); border-radius: var(--radius-md); cursor: pointer; }
.palette-option[data-active="true"] { background: var(--mb-color-selected); }
.palette-option[aria-disabled="true"] .palette-title { color: var(--mb-color-text-muted); }
.palette-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.help-group { font-size: var(--mb-type-caption-size); font-weight: 600; margin: var(--mb-space-3) 0 var(--mb-space-1); color: var(--mb-color-text-muted); }
.help-list { list-style: none; margin: 0; padding: 0; }
.help-list li { display: flex; justify-content: space-between; gap: var(--mb-space-3); padding: var(--mb-space-0_5) 0; }
.gallery { padding: var(--mb-space-4); max-width: 1100px; margin: 0 auto; height: auto; }
.gallery section { margin: var(--mb-space-5) 0; }
.gallery-row { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); margin: var(--mb-space-2) 0; }
.gallery-card { border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-xl); padding: var(--mb-space-3); background: var(--mb-color-panel); }
.gallery-card[data-tone="error"] { background: var(--mb-color-danger-tint); }
.gallery-card[data-tone="warn"] { background: var(--mb-color-warn-tint); }
.gallery-shell-box { height: 360px; border: 1px solid var(--mb-color-border-control); overflow: hidden; }
.gallery-list-box { display: flex; flex-direction: column; background: var(--mb-surface-list); }
html:has(.gallery), html:has(.gallery) body, html:has(.gallery) #root { height: auto; }
`;
