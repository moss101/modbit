/**
 * Styles of the agent list (PX-046) and the conversation surface (PX-047),
 * written against the design tokens only: no literal colour appears here.
 * Status is never colour alone: every class has a glyph and words in the
 * markup; the colours below only reinforce them.
 */
export const workspaceCss = `
.agent-region-meta { padding: 0 var(--mb-space-3); margin: 0; }
.agent-region { height: 100%; min-height: 0; box-sizing: border-box; }
.agents { flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }
.agents-controls { padding: var(--mb-space-1) var(--mb-space-3) var(--mb-space-2); display: flex; flex-direction: column; gap: var(--mb-space-1); flex: 0 1 auto; min-height: 0; max-height: 60%; overflow-y: auto; border-bottom: 1px solid var(--mb-color-border); }
.agents-search { width: 100%; box-sizing: border-box; font-size: var(--mb-type-chrome-size); padding: var(--mb-space-1) var(--mb-space-2); }
.agents-tools { display: flex; align-items: center; gap: var(--mb-space-1); }
.agents-group { flex: 1 1 0; min-width: 0; }
.agents-group select { width: 100%; font: inherit; font-size: var(--mb-type-chrome-size); padding: var(--mb-space-0_5) var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.agents-filters { display: flex; flex-direction: column; gap: var(--mb-space-1); }
.agents-fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
.agents-fieldset legend { padding: 0; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agents-chips { display: flex; flex-wrap: wrap; gap: var(--mb-space-1); margin-top: var(--mb-space-0_5); }
.agents-chip { font: inherit; font-size: var(--mb-type-caption-size); padding: 1px var(--mb-space-2); border-radius: calc(infinity * 1px); border: 1px solid var(--mb-color-border-control); background: transparent; color: var(--mb-color-text); }
.agents-chip:hover:not(:disabled) { background: var(--mb-color-hover); border-color: var(--mb-color-border-control); }
.agents-chip[aria-pressed="true"] { background: var(--mb-color-selected); border-color: var(--mb-color-accent); }
.agents-link { align-self: flex-start; font: inherit; font-size: var(--mb-type-caption-size); background: transparent; border: 0; color: var(--mb-color-accent-text); text-decoration: underline; padding: 0; }
.agents-link:hover:not(:disabled) { background: transparent; }
.agents-note { margin: 0; padding: var(--mb-space-1) var(--mb-space-3); }
.agents-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; overflow-x: hidden; position: relative; }
.agents-canvas { position: relative; width: 100%; }
.agents-empty { margin: var(--mb-space-3); font-style: normal; }
.agents-section-btn:hover:not(:disabled) { background: var(--mb-color-hover); }
.agents-section { position: absolute; left: 0; right: 0; height: 24px; margin-top: 4px; display: flex; align-items: center; }
.agents-section-btn { flex: 1 1 0; min-width: 0; display: flex; align-items: center; gap: var(--mb-space-1); font: inherit; font-size: var(--mb-type-caption-size); font-weight: 400; min-height: 0; height: 24px; padding: 0 16px; border: 0; border-radius: 0; background: transparent; color: var(--mb-color-text-muted); text-align: left; }
.agents-section .agents-caret { display: none; }
.agents-section-tools { position: absolute; right: 8px; display: inline-flex; gap: 2px; }
.agents-section-label { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agents-open { flex: 1 1 0; min-width: 0; display: grid; grid-template-columns: 14px minmax(0, 1fr) auto; column-gap: 8px; row-gap: 2px; align-content: center; align-items: center; height: auto; font: inherit; text-align: left; padding: 6px 8px; border: 0; border-radius: var(--radius-lg); background: transparent; color: var(--mb-color-text); }
.agents-glyph { display: inline-flex; justify-content: center; }
.agents-text { display: contents; }
.agents-title { grid-column: 2; grid-row: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-chrome-size); line-height: 18px; }
.agents-sub { grid-column: 2 / 4; grid-row: 2; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-caption-size); line-height: 16px; color: var(--mb-color-text-muted); }
.agents-status { font-weight: 400; }
.agents-end { grid-column: 3; grid-row: 1; display: inline-flex; align-items: center; gap: var(--mb-space-1); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agents-row { position: absolute; left: 0; right: 0; box-sizing: border-box; display: flex; align-items: stretch; padding: 0 8px; }
.agents-open:hover:not(:disabled) { background: var(--color-surface-hover); }
.agents-row[data-selected="true"] .agents-open { background: var(--color-selected); }
.agents-origin { display: inline-block; margin-right: var(--mb-space-1); padding: 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); font-weight: 600; border: 1px solid var(--mb-color-border-control); border-radius: calc(infinity * 1px); color: var(--mb-color-text); background: var(--mb-color-sunken); vertical-align: baseline; }
.agents-row[data-unread="true"] .agents-title { font-weight: 700; }
.agents-row[data-class="NEEDS_ATTENTION"] .agents-status, .agents-row[data-class="FAILED"] .agents-status { color: var(--mb-color-text); }
.agents-dot { width: 6px; height: 6px; border-radius: calc(infinity * 1px); display: inline-block; }
.agents-dot[data-kind="unread"] { background: var(--mb-color-accent); }
.agents-dot[data-kind="attention"] { background: var(--mb-color-warn); outline: 1px solid var(--mb-color-text); }
.agents-project { position: absolute; left: 0; right: 0; box-sizing: border-box; display: flex; align-items: center; padding: 0 8px; }
.agents-project-toggle.agents-project-toggle { position: absolute; left: 0; width: 8px; min-height: 0; height: 28px; padding: 0; border: 0; background: transparent; color: var(--mb-color-text-muted); font-size: 8px; opacity: 0; }
.agents-project:hover .agents-project-toggle, .agents-project-toggle:focus-visible { opacity: 1; }
.agents-project-open.agents-project-open { flex: 1 1 0; min-width: 0; height: 28px; min-height: 0; display: flex; align-items: center; gap: 8px; padding: 0 8px; border: 0; border-radius: var(--radius-lg); background: transparent; color: var(--mb-color-text-muted); font: inherit; font-size: var(--mb-type-chrome-size); text-align: left; }
.agents-project-open.agents-project-open:hover:not(:disabled) { background: var(--color-hover); color: var(--color-foreground); }
.agents-project[data-selected="true"] .agents-project-open { background: var(--color-selected); color: var(--color-foreground); }
.agents-project:hover .agents-age, .agents-project:focus-within .agents-age { visibility: hidden; }
.agents-project .agents-text { display: flex; flex: 1 1 0; min-width: 0; }
.agents-project .agents-title { flex: 1 1 0; }
.agents-project .agents-age { flex: none; font-size: var(--mb-type-caption-size); }
.agents-project:hover .agents-actions, .agents-project:focus-within .agents-actions { display: inline-flex; }
.agents-actions { position: absolute; right: var(--mb-space-2); top: 50%; transform: translateY(-50%); display: none; gap: 0; background: var(--color-background-win-alt); border-radius: var(--radius-lg); }
.agents-row:hover .agents-actions, .agents-row:focus-within .agents-actions { display: inline-flex; }
.agents-row:hover .agents-end, .agents-row:focus-within .agents-end { visibility: hidden; }
.agents-results { padding-bottom: var(--mb-space-3); }
.agents-hit { padding: var(--mb-space-1) var(--mb-space-2); border-bottom: 1px solid var(--mb-color-border); }
.agents-hit-title, .agents-snippet { width: 100%; display: flex; align-items: baseline; gap: var(--mb-space-2); font: inherit; text-align: left; border: 0; background: transparent; color: var(--mb-color-text); border-radius: var(--radius-md); padding: var(--mb-space-1); }
.agents-hit-title:hover:not(:disabled), .agents-snippet:hover:not(:disabled) { background: var(--mb-color-hover); }
.agents-snippet { font-size: var(--mb-type-caption-size); align-items: flex-start; }
.agents-snippet-source { flex: none; min-width: 3.5em; }
.agents-snippet-text { overflow-wrap: anywhere; min-width: 0; }
.agents-snippet mark { background: var(--mb-color-warn-tint); color: var(--mb-color-text); border-radius: var(--radius-sm); outline: 1px solid var(--mb-color-warn); }

.shell-content:has(> .conv-wrap) { overflow: hidden; }
/* Banners appear only for trouble and scroll within a capped band. */
.conv-banners { flex: none; max-height: 22%; overflow-y: auto; }
.conv-banners:not(:has(.banner, .state[data-kind="degraded"], .state[data-kind="error"], .state[data-kind="recovery"])) { height: 0; overflow: hidden; }
.conv-wrap { height: 100%; min-height: 0; display: flex; flex-direction: column; }
.conv-wrap > :not(.conv-banners) { flex: 1 1 0; min-height: 0; }
.conv { flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }
.conv-spacer { flex: 1 1 0; }
.conv-density { display: inline-flex; align-items: center; gap: var(--mb-space-1); }
.conv-density select { font: inherit; font-size: var(--mb-type-chrome-size); padding: 1px var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.conv-body { position: relative; flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }
.conv-scroll:focus-visible { outline: 2px solid var(--mb-color-focus); outline-offset: -2px; }
.conv-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; padding: var(--mb-space-2) var(--mb-space-5) var(--mb-space-4); }
.conv-inner { display: flex; flex-direction: column; gap: 14px; width: 100%; max-width: 640px; margin: 0 auto; }
.conv-empty { margin: var(--mb-space-4) 0; font-style: normal; }
.conv-row { flex: none; content-visibility: auto; contain-intrinsic-size: auto 48px; max-width: none; }
.conv-row[data-highlight="true"] { outline: 2px solid var(--mb-color-focus); outline-offset: 2px; border-radius: var(--radius-md); }
.conv-user { align-self: stretch; background: var(--color-panel); border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); padding: 8px 12px; max-width: none; font-size: var(--mb-type-body-size); line-height: var(--mb-type-body-line); }
.conv-user[data-sticky="true"] { position: sticky; top: calc(-1 * var(--mb-space-3)); z-index: var(--mb-z-panel); box-shadow: var(--mb-elevation-1); content-visibility: visible; }
.conv-user-text { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.conv-user { position: relative; }
.conv-user .meta { margin: var(--mb-space-0_5) 0 0; font-size: var(--mb-type-caption-size); }
.conv-assistant { content-visibility: visible; }
.conv-text { font-size: var(--mb-type-body-size); line-height: var(--mb-type-body-line); }
.conv-para { margin: 0 0 var(--mb-space-2); white-space: pre-wrap; overflow-wrap: anywhere; }
.conv-fresh { text-decoration: underline 1px transparent; text-underline-offset: 2px; animation: conv-fade var(--mb-motion-slow, 300ms) ease-out both; }
/* New text is marked by a fading underline, never by fading the glyphs: text at partial opacity is below AA contrast for as long as it fades (WCAG 1.4.3). */
@keyframes conv-fade { from { text-decoration-color: var(--mb-color-accent-text); } to { text-decoration-color: transparent; } }
.conv-state { margin: 0 0 var(--mb-space-0_5); }
.conv-code { margin: 0 0 var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--radius-md); background: var(--mb-color-sunken); }
.conv-code summary { cursor: pointer; padding: var(--mb-space-1) var(--mb-space-2); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.conv-code pre { margin: 0; padding: var(--mb-space-2); overflow-x: auto; font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.conv-reasoning { font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); margin-bottom: var(--mb-space-1); }
.conv-aborted { border: 1px dashed var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--radius-md); padding: var(--mb-space-2); }
.conv-aborted p { margin: 0 0 var(--mb-space-1); }
.conv-partial { opacity: 0.85; border-left: 2px solid var(--mb-color-warn); padding-left: var(--mb-space-2); }
.conv-tool, .conv-group > summary { display: flex; align-items: center; gap: 6px; min-height: 18px; color: var(--mb-color-text-muted); font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); }
.conv-tool { flex-wrap: wrap; padding: 0; content-visibility: visible; }
.conv-tool > svg, .conv-group > summary > svg { flex: none; }
.conv-tool-class { color: var(--mb-color-text); }
.conv-tool-text { min-width: 0; max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.conv-tool-text[data-mono="true"] { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.conv-add { color: var(--mb-color-ok); }
.conv-del { color: var(--mb-color-danger); }
.conv-user { box-shadow: 0 1px 2px rgb(0 0 0 / 0.05); }
.conv-tool-failure { color: var(--mb-color-text); font-weight: 600; }
/* AWL-07: a step group is a one-line disclosure row with no box. */
.conv-group { border: 0; border-radius: 0; background: transparent; }
.conv-group > summary { cursor: pointer; list-style: none; }
.conv-group > summary::-webkit-details-marker { display: none; }
.conv-group-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--mb-color-text); }
.conv-group-label[data-open-work="true"] { color: var(--mb-color-text-muted); }
.conv-group > summary:hover { color: var(--mb-color-text); }
.conv-group-body { padding: var(--mb-space-1) 0 var(--mb-space-1) 20px; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.conv-approval { border: 1px solid var(--mb-color-warn); background: var(--color-panel); border-radius: var(--radius-xl); padding: var(--mb-space-2) var(--mb-space-3); }
.conv-approval[data-state="APPROVED"], .conv-approval[data-state="ANSWERED"] { border-color: var(--mb-color-border); background: var(--mb-color-panel); }
.conv-approval p { margin: 0 0 var(--mb-space-1); }
/* AWL-07: a completed turn's footer is transparent until the turn is hovered or focused; it stays in the tab order and the accessibility tree. A failed or interrupted turn always shows it. */
.conv-footer { display: flex; flex-wrap: nowrap; align-items: center; gap: var(--mb-space-2); min-height: 24px; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); white-space: nowrap; }
.conv-footer > .meta { flex: 0 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; }
.conv-footer .ckpt { flex: 0 1 auto; min-width: 0; flex-wrap: nowrap; overflow: hidden; }
.conv-footer .ckpt > span { overflow: hidden; text-overflow: ellipsis; }
.conv-footer[data-outcome="COMPLETED"] { opacity: 0; min-height: 0; height: 12px; margin-block: -14px; transition: opacity var(--mb-motion-fast); }
.conv-footer[data-outcome="COMPLETED"]:hover, .conv-footer[data-outcome="COMPLETED"]:focus-within, .conv-assistant:hover + .conv-footer[data-outcome="COMPLETED"] { opacity: 1; }
.conv-boundary { align-self: center; text-align: center; padding: var(--mb-space-1) 0; }
.conv-divider { display: flex; align-items: center; gap: var(--mb-space-2); color: var(--mb-color-accent-text); font-size: var(--mb-type-caption-size); font-weight: 600; max-width: none; }
.conv-divider::before, .conv-divider::after { content: ""; flex: 1 1 0; border-top: 1px solid var(--mb-color-accent); }
.conv-tail { flex: none; display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) 0; font-size: var(--mb-type-chrome-size); }
.conv-tail[data-stall="2"], .conv-tail[data-stall="3"] { font-weight: 600; }
.conv-failure { border: 1px solid var(--mb-color-danger); background: var(--mb-color-danger-tint); border-radius: var(--radius-lg); padding: var(--mb-space-2) var(--mb-space-3); max-width: 78ch; flex: none; }
.conv-failure p { margin: 0 0 var(--mb-space-1); }
.conv-float { position: absolute; right: var(--mb-space-4); bottom: var(--mb-space-3); display: flex; flex-direction: column; align-items: flex-end; gap: var(--mb-space-1); z-index: var(--mb-z-panel); }
.conv-pill { display: inline-flex; align-items: center; background: var(--mb-color-inverse); color: var(--mb-color-text-on-inverse); border-radius: calc(infinity * 1px); box-shadow: var(--mb-elevation-2); }
.conv-pill button { font: inherit; background: transparent; border: 0; color: inherit; padding: var(--mb-space-1) var(--mb-space-3); }
.conv-pill button:hover:not(:disabled) { background: transparent; text-decoration: underline; }
.conv-pill .conv-pill-x { padding: var(--mb-space-1) var(--mb-space-2); border-left: 1px solid var(--mb-color-text-muted); }
.conv-composer { flex: none; padding: var(--mb-space-2) var(--mb-space-4); background: transparent; }
.conv-composer p { margin: 0 0 var(--mb-space-1); }
.conv-composer input { flex: 1 1 14em; min-width: 0; }
@media (prefers-reduced-motion: reduce) { .conv-fresh { animation: none; } }
`;
