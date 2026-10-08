/**
 * The renderer's stylesheet text: the screens' styles (the former inline
 * <style> of index.html, now written against the design tokens, with the old
 * nine variables kept as aliases of token roles) and the shell's. The build
 * (build.mjs) concatenates the token stylesheet, the primitives' styles and
 * this text into dist/renderer/app.css. No literal colour appears here.
 */
export const legacyCss = `
:root { --bg: var(--mb-color-bg); --fg: var(--mb-color-text); --muted: var(--mb-color-text-muted); --card: var(--mb-color-panel); --line: var(--mb-color-border); --accent: var(--mb-color-accent-text); --warn: var(--mb-color-warn); --bad: var(--mb-color-danger); --ok: var(--mb-color-ok); }
html, body, #root { height: 100%; }
body { margin: 0; font: var(--mb-type-body-size)/1.4 var(--mb-font-sans); background: var(--bg); color: var(--fg); }
h1, h2, h3 { text-wrap: balance; }
.banner { padding: 10px 20px; border-bottom: 1px solid var(--line); }
.banner[data-kind="restarting"] { background: var(--mb-color-warn-tint); }
.banner[data-kind="recovered"] { background: var(--mb-color-ok-tint); }
.banner[data-kind="error"] { background: var(--mb-color-danger-tint); }
.banner[data-kind="info"] { background: var(--mb-color-info-tint); }
.fleet-main { display: grid; grid-template-columns: minmax(280px, 340px) minmax(0, 1fr); gap: 20px; padding: 20px; }
.fleet-main[hidden] { display: none; }
form.composer { display: flex; flex-direction: column; gap: 10px; background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 16px; }
section.welcome { margin: 12px 16px; background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 16px; }
section.welcome ol.steps { padding-left: 20px; display: flex; flex-direction: column; gap: 10px; }
section.welcome li[data-active="true"] { outline: 2px solid var(--accent); outline-offset: 4px; border-radius: 4px; }
section.welcome li[data-done="true"] { opacity: 0.85; }
section.welcome form { display: flex; flex-direction: column; gap: 6px; margin-top: 6px; }
section.welcome input, section.welcome select { max-width: 520px; }
textarea { min-height: 120px; font: inherit; padding: 8px; border: 1px solid var(--mb-color-border-control); border-radius: 6px; background: var(--mb-color-sunken); color: var(--fg); }
button { font: inherit; padding: 8px 12px; border-radius: 6px; border: 1px solid var(--mb-color-accent); background: var(--mb-color-accent); color: var(--mb-color-text-on-accent); cursor: pointer; }
button:hover:not(:disabled) { background: var(--mb-color-accent-hover); border-color: var(--mb-color-accent-hover); }
button:disabled { opacity: 0.5; cursor: default; }
.fleet { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 16px; }
section.column { background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 12px; min-height: 120px; }
section.column h2 { font-size: 13px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); margin: 0 0 8px; }
article.card { border: 1px solid var(--line); border-radius: 6px; padding: 10px; margin-bottom: 8px; }
article.card:focus { outline: 2px solid var(--accent); }
.meta { color: var(--muted); font-size: 12px; }
.empty { color: var(--muted); font-style: italic; }
.status { margin-left: auto; color: var(--muted); font-size: 12px; }
.sr-only { position: absolute; left: -9999px; }
input { font: inherit; padding: 8px; border: 1px solid var(--mb-color-border-control); border-radius: 6px; background: var(--mb-color-sunken); color: var(--fg); }
input[type="checkbox"], input[type="radio"] { padding: 0; }
.actions { display: flex; gap: 8px; margin-top: 8px; }
.review { padding: 16px 20px; }
.review-head { display: flex; align-items: center; gap: 16px; margin-bottom: 12px; }
.review-head button { margin-left: auto; }
.review-body { display: grid; grid-template-columns: 1fr 360px; gap: 20px; }
article.file { background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 12px; margin-bottom: 12px; }
article.file h3 { margin: 0 0 8px; font-size: 13px; display: flex; gap: 8px; align-items: baseline; }
article.file h3 button.small { margin-left: auto; font-size: 11px; padding: 1px 6px; }
.patch { display: grid; gap: 6px; margin: 0 0 8px; padding: 8px; border: 1px dashed var(--mb-color-border-control); font-size: 12px; }
.patch label { display: grid; gap: 2px; }
.patch textarea { min-height: 48px; font-family: var(--mb-font-mono); font-size: 12px; }
.hunk { border: 1px solid var(--line); border-radius: 6px; margin-bottom: 8px; overflow: hidden; }
.hunk[data-rejected="true"] { opacity: 0.6; border-style: dashed; }
.hunk-head { display: flex; justify-content: space-between; padding: 4px 8px; background: var(--bg); font-size: 12px; }
.hunk pre { margin: 0; padding: 8px; font: 12px/1.4 var(--mb-font-mono); overflow-x: auto; }
.hunk .add { color: var(--ok); }
.hunk .del { color: var(--bad); }
.hunk .ctx { color: var(--muted); }
aside.review-aside, .review-body aside { background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 12px; }
aside h3 { font-size: 12px; text-transform: uppercase; letter-spacing: 0.04em; color: var(--muted); margin: 12px 0 6px; }
aside h3:first-child { margin-top: 0; }
pre.small, ul.small { font-size: 11px; max-height: 160px; overflow: auto; }
.state { padding: 4px 8px; border-radius: 4px; margin-top: 4px; }
.state[data-kind="degraded"] { background: var(--mb-color-warn-tint); color: var(--fg); }
.state[data-kind="error"] { background: var(--mb-color-danger-tint); color: var(--fg); }
.state[data-kind="recovery"] { background: var(--mb-color-ok-tint); color: var(--fg); }
section.notifications { padding: 8px 20px; border-bottom: 1px solid var(--line); background: var(--card); }
section.notifications ul { margin: 0; padding-left: 18px; }
section.notifications li { margin: 2px 0; }
section.settings { margin-top: 16px; display: flex; flex-direction: column; gap: 8px; background: var(--card); border: 1px solid var(--line); border-radius: 8px; padding: 16px; }
section.settings fieldset { border: 1px solid var(--line); border-radius: 6px; display: flex; flex-wrap: wrap; gap: 10px; }
section.settings input[type="number"] { width: 56px; padding: 2px 4px; }
button.small { font-size: 11px; padding: 1px 6px; }
.browser-view { height: 60vh; min-height: 320px; border: 1px solid var(--line); border-radius: 8px; background: repeating-linear-gradient(45deg, var(--card), var(--card) 10px, var(--bg) 10px, var(--bg) 20px); }
.decision { margin-top: 6px; padding: 6px 8px; border: 1px dashed var(--mb-color-border-control); border-radius: 6px; display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
.decision input { padding: 2px 6px; font-size: 12px; }
.hunk:focus { outline: 2px solid var(--accent); outline-offset: -2px; }
table.split { width: 100%; border-collapse: collapse; font: 12px/1.4 var(--mb-font-mono); table-layout: fixed; }
table.split td { padding: 0 8px; white-space: pre; overflow-x: auto; vertical-align: top; width: 50%; }
table.split td.gap { background: var(--mb-color-hover); }
table.split td.add { color: var(--ok); }
table.split td.del { color: var(--bad); }
kbd { font: 11px var(--mb-font-mono); border: 1px solid var(--mb-color-border-control); border-radius: 3px; padding: 0 4px; }
[data-testid="review-check"]:focus, section.attention:focus, [data-testid="attention-item"]:focus, section.column:focus, [data-testid="review-verification"]:focus { outline: 2px solid var(--accent); }
`;

export const shellCss = `
.shell { display: flex; height: 100%; min-width: 0; overflow: hidden; background: var(--mb-color-bg); }
.shell-list { box-sizing: border-box; position: relative; flex: none; background: var(--mb-color-panel); border-right: 1px solid var(--mb-color-border); }
.shell-center { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; }
.shell-content { flex: 1 1 0; min-height: 0; overflow: auto; }
.shell-panel { box-sizing: border-box; position: relative; flex: none; background: var(--mb-color-panel); border-left: 1px solid var(--mb-color-border); }
.shell-segments { display: flex; gap: var(--mb-space-1); padding: var(--mb-space-1) var(--mb-space-3); border-bottom: 1px solid var(--mb-color-border); }
.shell-scroll { height: 100%; overflow: auto; }
.splitter { position: absolute; top: 0; bottom: 0; width: 6px; z-index: var(--mb-z-panel); cursor: col-resize; background: transparent; }
.splitter[data-edge="right"] { right: -3px; }
.splitter[data-edge="left"] { left: -3px; }
.splitter:hover, .splitter:focus-visible { background: var(--mb-color-border-control); }
.topbar { flex: none; box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 var(--mb-space-3); border-bottom: 1px solid var(--mb-color-border); background: var(--mb-color-panel); z-index: var(--mb-z-topbar); }
.topbar-title { margin: 0; font-size: var(--mb-type-title-size); line-height: var(--mb-type-title-line); font-weight: 600; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.topbar-spacer { flex: 1 1 0; }
.statusrow { flex: none; display: flex; align-items: center; gap: var(--mb-space-3); padding: var(--mb-space-1) var(--mb-space-3); border-top: 1px solid var(--mb-color-border); background: var(--mb-color-panel); font-size: var(--mb-type-caption-size); }
.statusrow .status { margin-left: 0; }
.agent-region { padding: var(--mb-space-3); display: flex; flex-direction: column; gap: var(--mb-space-2); }
.agent-region-head { display: flex; align-items: center; justify-content: space-between; }
.agent-rail { display: flex; flex-direction: column; align-items: center; gap: var(--mb-space-1); padding: var(--mb-space-1) 0; }
.brand { font-weight: 700; font-size: var(--mb-type-title-size); }
.apps-panel { padding: var(--mb-space-3); }
.apps-panel-task { margin: 0 0 var(--mb-space-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.terminal-app, .changes-app, .files-app, .evidence-app, .browser-app { display: flex; flex-direction: column; gap: var(--mb-space-2); min-width: 0; }
.terminal-list, .changes-files, .files-tree, .evidence-list { list-style: none; margin: 0; padding: 0; }
.terminal-row { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) 0; }
.terminal-pick, .changes-file, .files-entry { background: transparent; color: var(--mb-color-text); border: 1px solid transparent; text-align: left; padding: var(--mb-space-1) var(--mb-space-2); border-radius: var(--mb-radius-3); }
.terminal-pick { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.terminal-pick:hover:not(:disabled), .changes-file:hover:not(:disabled), .files-entry:hover:not(:disabled) { background: var(--mb-color-hover); border-color: transparent; }
.terminal-pick[aria-pressed="true"], .changes-file[aria-pressed="true"], .files-entry[aria-pressed="true"] { background: var(--mb-color-selected); }
.terminal-toolbar, .changes-head { display: flex; align-items: center; flex-wrap: wrap; gap: var(--mb-space-2); }
.xterm-wrap { background: var(--mb-color-sunken); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-3); padding: var(--mb-space-1); height: 320px; min-height: 160px; resize: vertical; overflow: hidden; }
.xterm-host { height: 100%; }
.xterm-wrap:focus-within { outline: 2px solid var(--mb-color-accent); }
pre.codeview { margin: 0; padding: var(--mb-space-2); max-height: 50vh; overflow: auto; font: 12px/1.4 var(--mb-font-mono); background: var(--mb-color-sunken); border-radius: var(--mb-radius-3); }
pre.codeview .add { background: var(--mb-color-ok-tint); color: var(--mb-color-text); }
.evidence-table { border-collapse: collapse; width: 100%; font-size: var(--mb-type-chrome-size); }
.evidence-table th, .evidence-table td { text-align: left; padding: var(--mb-space-0_5) var(--mb-space-2); border-bottom: 1px solid var(--mb-color-border); }
.mb-tab:hover:not(:disabled) { background: var(--mb-color-hover); color: var(--mb-color-text); border-color: transparent; }
.mb-tab[aria-selected="true"]:hover:not(:disabled) { border-bottom-color: var(--mb-color-accent); }
.apps-panel h3 { font-size: var(--mb-type-chrome-size); text-transform: uppercase; letter-spacing: 0.04em; color: var(--mb-color-text-muted); margin: var(--mb-space-3) 0 var(--mb-space-1); }
.palette { padding: var(--mb-space-2); }
.palette-input { width: 100%; box-sizing: border-box; font-size: var(--mb-type-body-size); }
.palette-list { list-style: none; margin: var(--mb-space-2) 0 0; padding: 0; max-height: 50vh; overflow: auto; }
.palette-option { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1_5) var(--mb-space-2); border-radius: var(--mb-radius-3); cursor: pointer; }
.palette-option[data-active="true"] { background: var(--mb-color-selected); }
.palette-option[aria-disabled="true"] .palette-title { color: var(--mb-color-text-muted); }
.palette-title { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.help-group { font-size: var(--mb-type-chrome-size); margin: var(--mb-space-3) 0 var(--mb-space-1); color: var(--mb-color-text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
.help-list { list-style: none; margin: 0; padding: 0; }
.help-list li { display: flex; justify-content: space-between; gap: var(--mb-space-3); padding: var(--mb-space-0_5) 0; }
.dialog-actions { display: flex; justify-content: flex-end; gap: var(--mb-space-2); margin-top: var(--mb-space-4); }
.gallery { padding: var(--mb-space-4); max-width: 1100px; margin: 0 auto; height: auto; }
.gallery section { margin: var(--mb-space-5) 0; }
.gallery-row { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); margin: var(--mb-space-2) 0; }
.gallery-card { border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-5); padding: var(--mb-space-3); background: var(--mb-color-panel); }
.gallery-card[data-tone="error"] { background: var(--mb-color-danger-tint); }
.gallery-card[data-tone="warn"] { background: var(--mb-color-warn-tint); }
.gallery-shell-box { height: 360px; border: 1px solid var(--mb-color-border-control); overflow: hidden; }
html:has(.gallery), html:has(.gallery) body, html:has(.gallery) #root { height: auto; }
`;
