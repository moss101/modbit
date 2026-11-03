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
/* AWL-09: an information banner (languages, support claim) is one caption line on its surface; its full text stays in the DOM and in its title. */
.banner[data-kind="info"] { background: transparent; color: var(--mb-color-text-muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.fleet-main { display: grid; grid-template-columns: minmax(280px, 340px) minmax(0, 1fr); gap: 20px; padding: 20px; }
.fleet-main[hidden] { display: none; }
form.composer { display: flex; flex-direction: column; gap: 10px; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius-xl); padding: 16px; }
section.welcome { margin: 12px 16px; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius-xl); padding: 16px; }
section.welcome ol.steps { padding-left: 20px; display: flex; flex-direction: column; gap: 10px; }
section.welcome li[data-active="true"] { outline: 2px solid var(--accent); outline-offset: 4px; border-radius: var(--radius-md); }
section.welcome li[data-done="true"] { opacity: 0.85; }
section.welcome form { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }
section.welcome input, section.welcome select { max-width: 520px; }
/* A bare <button> is the AWL default (AWL-05): the quiet 8 % fill, full text, no edge; a .mb-btn sets its own variant and wins by specificity. The main action of a form (a submit button) takes the primary recipe. */
:where(button:not(.mb-btn)) { box-sizing: border-box; min-height: var(--mb-control-md); padding: 0 10px; border: 1px solid transparent; border-radius: var(--radius-md); background: var(--color-selected); color: var(--color-foreground); font: inherit; font-size: var(--text-ui-base); line-height: 1.5; cursor: default; transition-property: var(--mb-transition-colors); transition-duration: var(--mb-motion-base); transition-timing-function: var(--mb-ease); }
:where(button:not(.mb-btn):hover:not(:disabled)) { background: var(--color-command-node-hover); }
:where(button[type="submit"]:not(.mb-btn)) { background: var(--color-primary); color: var(--color-primary-foreground); font-weight: 500; }
:where(button[type="submit"]:not(.mb-btn):hover:not(:disabled)) { background: var(--mb-color-accent-hover); }
:where(button:not(.mb-btn):active:not(:disabled)) { transform: scale(var(--mb-press-scale)); }
button:disabled { opacity: 0.5; }
/* Each column is never narrower than a card's controls (the Browser button is ~80 px): the board scrolls sideways rather than letting one column's controls spill under its neighbour (E2E: a sibling column intercepted the button). */
.fleet { display: grid; grid-template-columns: repeat(3, minmax(200px, 1fr)); gap: 16px; overflow-x: auto; min-width: 0; }
section.column { background: var(--color-panel); border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); padding: 12px; min-height: 120px; min-width: 0; position: relative; }
section.column h2 { font-size: var(--text-ui-caption); font-weight: 600; color: var(--muted); margin: 0 0 8px; }
article.card { border: 1px solid var(--line); border-radius: var(--radius-lg); padding: 10px; margin-bottom: 8px; }
article.card:focus { outline: 2px solid var(--accent); }
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
.state[data-kind="degraded"] { background: var(--mb-color-warn-tint); color: var(--fg); }
.state[data-kind="error"] { background: var(--mb-color-danger-tint); color: var(--fg); }
.state[data-kind="recovery"] { background: var(--mb-color-ok-tint); color: var(--fg); }
section.notifications { padding: 8px 20px; border-bottom: 1px solid var(--line); background: var(--card); }
section.notifications ul { margin: 0; padding-left: 18px; }
section.notifications li { margin: 2px 0; }
section.settings { margin-top: 16px; display: flex; flex-direction: column; gap: 8px; background: var(--card); border: 1px solid var(--line); border-radius: var(--radius-xl); padding: 16px; }
section.settings fieldset { border: 1px solid var(--line); border-radius: var(--radius-lg); display: flex; flex-wrap: wrap; gap: 10px; }
section.settings input[type="number"] { width: 56px; padding: 2px 4px; }
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
[data-testid="review-check"]:focus, section.attention:focus, [data-testid="attention-item"]:focus, section.column:focus, [data-testid="review-verification"]:focus { outline: 2px solid var(--accent); }
`;

export const shellCss = `
/* AWL-06: one window of unframed panes. The agent list on the sidebar surface, the conversation on chrome, the apps panel on the editor surface, 1 px dividers at 8 %, no radius and no gaps. */
.shell { display: flex; height: 100%; min-width: 0; overflow: hidden; background: var(--color-background-win-alt); }
.shell-list { box-sizing: border-box; position: relative; flex: none; background: var(--color-sidebar); border-right: 1px solid var(--color-tab-border); }
.shell-center { flex: 1 1 0; min-width: 0; box-sizing: border-box; display: flex; flex-direction: column; }
.shell-frame { flex: 1 1 0; min-width: 0; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--color-background); }
.shell-content { flex: 1 1 0; min-height: 0; overflow: auto; }
.shell-panel { box-sizing: border-box; position: relative; flex: none; background: var(--color-panel); border-left: 1px solid var(--color-tab-border); }
.shell-segments { display: flex; gap: var(--mb-space-1); padding: var(--mb-space-1) var(--mb-space-3); border-bottom: 1px solid var(--mb-color-border); }
.shell-scroll { height: 100%; overflow: auto; }
.splitter { position: absolute; top: 0; bottom: 0; width: 4px; z-index: var(--mb-z-panel); cursor: col-resize; background: transparent; }
.splitter[data-edge="right"] { right: -2px; }
.splitter[data-edge="left"] { left: -2px; }
.splitter::after { content: ""; position: absolute; top: 0; bottom: 0; left: 50%; width: 2px; margin-left: -1px; background: var(--color-resize-handle); opacity: 0; transition: opacity var(--mb-motion-fast); }
.splitter:hover::after, .splitter:focus-visible::after, .splitter:active::after { opacity: 1; }
.topbar { flex: none; box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; gap: var(--mb-space-1); padding: 0 8px 0 12px; background: transparent; z-index: var(--mb-z-topbar); }
.topbar-title { margin: 0 var(--mb-space-1); font-size: var(--text-ui-base); line-height: 1.5; font-weight: 500; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.topbar-spacer { flex: 1 1 0; }
/* AWL-08: the status row is one line at any width; each item ellipses and keeps its full text in its title and accessible name. */
.statusrow { flex: none; box-sizing: border-box; display: flex; align-items: center; gap: var(--mb-space-3); min-width: 0; height: 28px; padding: 0 var(--mb-space-4); overflow: hidden; white-space: nowrap; background: transparent; color: var(--mb-color-text-muted); font-size: var(--mb-type-caption-size); }
.statusrow > * { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.statusrow > .meta { flex: 1 1 0; }
.statusrow .status { margin-left: 0; }
.agent-region { padding: 0 var(--mb-space-2) var(--mb-space-2); display: flex; flex-direction: column; gap: var(--mb-space-1); }
.agent-region-head { box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; justify-content: space-between; padding: 0 var(--mb-space-4); }
.agent-nav { list-style: none; margin: 0 var(--mb-space-2) var(--mb-space-2); padding: 0; display: flex; flex-direction: column; gap: 1px; }
.agent-nav-row.agent-nav-row { box-sizing: border-box; width: 100%; height: 30px; min-height: 0; display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 var(--mb-space-2); border: 0; border-radius: var(--radius-lg); background: transparent; color: var(--mb-color-text-muted); font: inherit; font-size: var(--mb-type-chrome-size); text-align: left; transition: none; }
.agent-nav-row.agent-nav-row:hover:not(:disabled) { background: var(--color-hover); color: var(--color-foreground); }
.agent-nav-row.agent-nav-row[aria-pressed="true"] { background: var(--color-selected); color: var(--color-foreground); }
.agent-nav-row.agent-nav-row:active:not(:disabled) { transform: none; }
.agent-nav-label { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agent-nav-key { flex: none; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agent-rail { display: flex; flex-direction: column; align-items: center; gap: var(--mb-space-1); padding: var(--mb-space-1) 0; }
.brand { font-weight: 600; font-size: var(--mb-type-title-size); }
.apps-panel { padding: var(--mb-space-2) var(--mb-space-3) var(--mb-space-3); }
.apps-panel-task { margin: 0 0 var(--mb-space-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.terminal-app, .changes-app, .files-app, .evidence-app, .browser-app { display: flex; flex-direction: column; gap: var(--mb-space-2); min-width: 0; }
.terminal-list, .changes-files, .files-tree, .evidence-list { list-style: none; margin: 0; padding: 0; }
.terminal-row { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) 0; }
.terminal-pick, .changes-file, .files-entry { background: transparent; color: var(--mb-color-text); border: 1px solid transparent; text-align: left; padding: var(--mb-space-1) var(--mb-space-2); border-radius: var(--radius-md); }
.terminal-pick { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.terminal-pick:hover:not(:disabled), .changes-file:hover:not(:disabled), .files-entry:hover:not(:disabled) { background: var(--mb-color-hover); border-color: transparent; }
.terminal-pick[aria-pressed="true"], .changes-file[aria-pressed="true"], .files-entry[aria-pressed="true"] { background: var(--mb-color-selected); }
.changes-row { display: flex; align-items: center; gap: var(--mb-space-1); }
.changes-row .changes-file { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.changes-refusal { border: 1px solid var(--mb-color-danger); background: var(--mb-color-danger-tint); border-radius: var(--radius-md); padding: var(--mb-space-2); }
.changes-refusal p, .changes-result p, .changes-approval p { margin: 0 0 var(--mb-space-1); }
.changes-undo, .changes-result { border: 1px solid var(--mb-color-border); border-radius: var(--radius-md); padding: var(--mb-space-2); }
.changes-commit { display: flex; flex-direction: column; gap: var(--mb-space-1); }
.changes-commit textarea { width: 100%; box-sizing: border-box; font: inherit; }
.changes-commit-actions { display: flex; align-items: center; flex-wrap: wrap; gap: var(--mb-space-2); }
.changes-approval { border: 1px solid var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--radius-md); padding: var(--mb-space-2); }
.changes-effects ul { list-style: none; margin: 0; padding: 0; }
.hunk-head .mb-btn { margin-left: var(--mb-space-2); }
.hunk-head { flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); }
.changes-chip { font: inherit; font-size: 12px; background: transparent; color: var(--accent); border: 1px solid var(--line); border-radius: 999px; padding: 1px 8px; cursor: pointer; }
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
.terminal-toolbar, .changes-head { display: flex; align-items: center; flex-wrap: wrap; gap: var(--mb-space-2); }
.xterm-wrap { background: var(--mb-color-sunken); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); padding: var(--mb-space-1); height: 320px; min-height: 160px; resize: vertical; overflow: hidden; }
.xterm-host { height: 100%; }
.terminal-history-pane { max-height: 200px; overflow: auto; background: var(--mb-color-sunken); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); }
.terminal-history-pane:focus-visible { outline: 2px solid var(--mb-color-accent); }
.terminal-history-pane pre { margin: 0; padding: var(--mb-space-2); font-family: var(--font-mono); font-size: var(--text-ui-sm); line-height: 1.5; white-space: pre-wrap; overflow-wrap: anywhere; }
.xterm-wrap:focus-within { outline: 2px solid var(--mb-color-accent); }
pre.codeview { margin: 0; padding: var(--mb-space-2); max-height: 50vh; overflow: auto; font-family: var(--font-mono); font-size: var(--text-ui-sm); line-height: 1.5; background: var(--mb-color-sunken); border-radius: var(--radius-md); }
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
html:has(.gallery), html:has(.gallery) body, html:has(.gallery) #root { height: auto; }
`;
