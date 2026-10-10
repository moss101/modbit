/**
 * Styles of the agent list (PX-046) and the conversation surface (PX-047),
 * written against the design tokens only: no literal colour appears here.
 * Status is never colour alone: every class has a glyph and words in the
 * markup; the colours below only reinforce them.
 */
export const workspaceCss = `
.agent-region { height: 100%; min-height: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); padding: var(--mb-space-2) 0 0; box-sizing: border-box; }
.agent-region-head { padding: 0 var(--mb-space-3); gap: var(--mb-space-1); }
.agent-region-head .brand { margin-right: auto; }
.agent-region-meta { padding: 0 var(--mb-space-3); margin: 0; }
.agent-fleet[aria-pressed="true"] { background: var(--mb-color-selected); }
.agents { flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }
.agents-controls { padding: var(--mb-space-1) var(--mb-space-3) var(--mb-space-2); display: flex; flex-direction: column; gap: var(--mb-space-1); border-bottom: 1px solid var(--mb-color-border); }
.agents-search { width: 100%; box-sizing: border-box; font-size: var(--mb-type-chrome-size); padding: var(--mb-space-1) var(--mb-space-2); }
.agents-tools { display: flex; align-items: center; gap: var(--mb-space-1); }
.agents-group { flex: 1 1 0; min-width: 0; }
.agents-group select { width: 100%; font: inherit; font-size: var(--mb-type-chrome-size); padding: var(--mb-space-0_5) var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.agents-filters { display: flex; flex-direction: column; gap: var(--mb-space-1); }
.agents-fieldset { border: 0; margin: 0; padding: 0; min-width: 0; }
.agents-fieldset legend { padding: 0; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agents-chips { display: flex; flex-wrap: wrap; gap: var(--mb-space-1); margin-top: var(--mb-space-0_5); }
.agents-chip { font: inherit; font-size: var(--mb-type-caption-size); padding: 1px var(--mb-space-2); border-radius: var(--mb-radius-full); border: 1px solid var(--mb-color-border-control); background: transparent; color: var(--mb-color-text); }
.agents-chip:hover:not(:disabled) { background: var(--mb-color-hover); border-color: var(--mb-color-border-control); }
.agents-chip[aria-pressed="true"] { background: var(--mb-color-selected); border-color: var(--mb-color-accent); }
.agents-link { align-self: flex-start; font: inherit; font-size: var(--mb-type-caption-size); background: transparent; border: 0; color: var(--mb-color-accent-text); text-decoration: underline; padding: 0; }
.agents-link:hover:not(:disabled) { background: transparent; }
.agents-note { margin: 0; padding: var(--mb-space-1) var(--mb-space-3); }
.agents-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; overflow-x: hidden; position: relative; }
.agents-canvas { position: relative; width: 100%; }
.agents-empty { margin: var(--mb-space-3); font-style: normal; }
.agents-section { position: absolute; left: 0; right: 0; height: 28px; display: flex; align-items: center; }
.agents-section-btn { width: 100%; display: flex; align-items: center; gap: var(--mb-space-1); font: inherit; font-size: var(--mb-type-caption-size); font-weight: 600; text-transform: none; padding: 0 var(--mb-space-3); border: 0; border-radius: 0; background: transparent; color: var(--mb-color-text-muted); text-align: left; }
.agents-section-btn:hover:not(:disabled) { background: var(--mb-color-hover); }
.agents-section-label { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agents-row { position: absolute; left: 0; right: 0; box-sizing: border-box; display: flex; align-items: stretch; padding: 0 var(--mb-space-1); }
.agents-open { flex: 1 1 0; min-width: 0; display: flex; align-items: center; gap: var(--mb-space-2); font: inherit; text-align: left; padding: var(--mb-space-1) var(--mb-space-2); border: 0; border-radius: var(--mb-radius-4); background: transparent; color: var(--mb-color-text); }
.agents-open:hover:not(:disabled) { background: var(--mb-color-hover); }
.agents-row[data-selected="true"] .agents-open { background: var(--mb-color-selected); }
.agents-glyph { flex: none; display: inline-flex; }
.agents-text { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; }
.agents-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-chrome-size); }
.agents-origin { display: inline-block; margin-right: var(--mb-space-1); padding: 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); font-weight: 600; border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-full); color: var(--mb-color-text); background: var(--mb-color-sunken); vertical-align: baseline; }
.agents-row[data-unread="true"] .agents-title { font-weight: 700; }
.agents-sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agents-status { font-weight: 600; }
.agents-row[data-class="NEEDS_ATTENTION"] .agents-status, .agents-row[data-class="FAILED"] .agents-status { color: var(--mb-color-text); }
.agents-end { flex: none; display: inline-flex; align-items: center; gap: var(--mb-space-1); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agents-dot { width: 8px; height: 8px; border-radius: var(--mb-radius-full); display: inline-block; }
.agents-dot[data-kind="unread"] { background: var(--mb-color-accent); }
.agents-dot[data-kind="attention"] { background: var(--mb-color-warn); outline: 1px solid var(--mb-color-text); }
.agents-actions { position: absolute; right: var(--mb-space-2); top: 50%; transform: translateY(-50%); display: none; gap: 0; background: var(--mb-color-panel); border-radius: var(--mb-radius-4); }
.agents-row:hover .agents-actions, .agents-row:focus-within .agents-actions { display: inline-flex; }
.agents-row:hover .agents-end, .agents-row:focus-within .agents-end { visibility: hidden; }
.agents-results { padding-bottom: var(--mb-space-3); }
.agents-hit { padding: var(--mb-space-1) var(--mb-space-2); border-bottom: 1px solid var(--mb-color-border); }
.agents-hit-title, .agents-snippet { width: 100%; display: flex; align-items: baseline; gap: var(--mb-space-2); font: inherit; text-align: left; border: 0; background: transparent; color: var(--mb-color-text); border-radius: var(--mb-radius-3); padding: var(--mb-space-1); }
.agents-hit-title:hover:not(:disabled), .agents-snippet:hover:not(:disabled) { background: var(--mb-color-hover); }
.agents-snippet { font-size: var(--mb-type-caption-size); align-items: flex-start; }
.agents-snippet-source { flex: none; min-width: 3.5em; }
.agents-snippet-text { overflow-wrap: anywhere; min-width: 0; }
.agents-snippet mark { background: var(--mb-color-warn-tint); color: var(--mb-color-text); border-radius: var(--mb-radius-1); outline: 1px solid var(--mb-color-warn); }

.shell-content:has(> .conv-wrap) { overflow: hidden; }
/* The recovery, context and economics banners scroll within a capped band: stacked unbounded they squeeze the message list to a few pixels after a Core restart. */
.conv-banners { flex: none; max-height: 22%; overflow-y: auto; }
.conv-wrap { height: 100%; min-height: 0; display: flex; flex-direction: column; }
/* The status region (banners, the welcome card) never takes the surface from the content below it: on a short window it scrolls inside its own share instead of overlapping the controls under it. */
.conv-wrap > [role="region"][aria-label="Status and notifications"] { flex: 0 0 auto; max-height: 45%; overflow-y: auto; }
.conv { flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }
.conv-head { flex: none; display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) var(--mb-space-4); border-bottom: 1px solid var(--mb-color-border); font-size: var(--mb-type-chrome-size); }
.conv-spacer { flex: 1 1 0; }
.conv-density { display: inline-flex; align-items: center; gap: var(--mb-space-1); }
.conv-density select { font: inherit; font-size: var(--mb-type-chrome-size); padding: 1px var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.conv-body { position: relative; flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }
.conv-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; padding: var(--mb-space-3) var(--mb-space-4); }
.conv-inner { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.conv-scroll:focus-visible { outline: 2px solid var(--mb-color-focus); outline-offset: -2px; }
.conv-empty { margin: var(--mb-space-4) 0; font-style: normal; }
.conv-row { flex: none; content-visibility: auto; contain-intrinsic-size: auto 48px; max-width: 78ch; }
.conv-row[data-highlight="true"] { outline: 2px solid var(--mb-color-focus); outline-offset: 2px; border-radius: var(--mb-radius-3); }
.conv-user { align-self: flex-end; background: var(--mb-color-raised); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-5); padding: var(--mb-space-2) var(--mb-space-3); max-width: 70ch; }
.conv-user[data-sticky="true"] { position: sticky; top: calc(-1 * var(--mb-space-3)); z-index: var(--mb-z-panel); box-shadow: var(--mb-elevation-1); content-visibility: visible; }
.conv-user-text { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.conv-user .meta { margin: var(--mb-space-0_5) 0 0; }
.conv-assistant { content-visibility: visible; }
.conv-text { line-height: 1.55; }
.conv-para { margin: 0 0 var(--mb-space-2); white-space: pre-wrap; overflow-wrap: anywhere; }
.conv-fresh { text-decoration: underline 1px transparent; text-underline-offset: 2px; animation: conv-fade var(--mb-motion-slow, 300ms) ease-out both; }
/* New text is marked by a fading underline, never by fading the glyphs: text at partial opacity is below AA contrast for as long as it fades (WCAG 1.4.3). */
@keyframes conv-fade { from { text-decoration-color: var(--mb-color-accent-text); } to { text-decoration-color: transparent; } }
.conv-state { margin: 0 0 var(--mb-space-0_5); }
.conv-code { margin: 0 0 var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); }
.conv-code summary { cursor: pointer; padding: var(--mb-space-1) var(--mb-space-2); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.conv-code pre { margin: 0; padding: var(--mb-space-2); overflow-x: auto; font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.conv-reasoning { font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); margin-bottom: var(--mb-space-1); }
.conv-aborted { border: 1px dashed var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--mb-radius-3); padding: var(--mb-space-2); }
.conv-aborted p { margin: 0 0 var(--mb-space-1); }
.conv-partial { opacity: 0.85; border-left: 2px solid var(--mb-color-warn); padding-left: var(--mb-space-2); }
.conv-tool { display: flex; flex-wrap: wrap; align-items: baseline; gap: var(--mb-space-2); padding: var(--mb-space-0_5) var(--mb-space-2); border-left: 2px solid var(--mb-color-border-control); font-size: var(--mb-type-chrome-size); }
.conv-tool[data-status="FAILED"], .conv-tool[data-status="DENIED"] { border-left-color: var(--mb-color-danger); }
.conv-tool-class { font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); min-width: 4.5em; }
.conv-tool-text { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 46ch; }
.conv-tool-failure { color: var(--mb-color-text); font-weight: 600; }
.conv-group { border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-4); background: var(--mb-color-panel); }
.conv-group > summary { cursor: pointer; padding: var(--mb-space-1) var(--mb-space-2); font-size: var(--mb-type-chrome-size); }
.conv-group-label { font-weight: 600; }
.conv-group-body { padding: var(--mb-space-1) var(--mb-space-2) var(--mb-space-2); display: flex; flex-direction: column; gap: var(--mb-space-1); }
.conv-approval { border: 1px solid var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--mb-radius-4); padding: var(--mb-space-2) var(--mb-space-3); }
.conv-approval[data-state="APPROVED"], .conv-approval[data-state="ANSWERED"] { border-color: var(--mb-color-border); background: var(--mb-color-panel); }
.conv-approval p { margin: 0 0 var(--mb-space-1); }
.conv-footer { display: flex; align-items: center; gap: var(--mb-space-2); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.conv-boundary { align-self: center; text-align: center; padding: var(--mb-space-1) 0; }
.conv-divider { display: flex; align-items: center; gap: var(--mb-space-2); color: var(--mb-color-accent-text); font-size: var(--mb-type-caption-size); font-weight: 600; max-width: none; }
.conv-divider::before, .conv-divider::after { content: ""; flex: 1 1 0; border-top: 1px solid var(--mb-color-accent); }
.conv-tail { flex: none; display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) 0; font-size: var(--mb-type-chrome-size); }
.conv-tail[data-stall="2"], .conv-tail[data-stall="3"] { font-weight: 600; }
.conv-failure { border: 1px solid var(--mb-color-danger); background: var(--mb-color-danger-tint); border-radius: var(--mb-radius-4); padding: var(--mb-space-2) var(--mb-space-3); max-width: 78ch; flex: none; }
.conv-failure p { margin: 0 0 var(--mb-space-1); }
.conv-float { position: absolute; right: var(--mb-space-4); bottom: var(--mb-space-3); display: flex; flex-direction: column; align-items: flex-end; gap: var(--mb-space-1); z-index: var(--mb-z-panel); }
.conv-pill { display: inline-flex; align-items: center; background: var(--mb-color-inverse); color: var(--mb-color-text-on-inverse); border-radius: var(--mb-radius-full); box-shadow: var(--mb-elevation-2); }
.conv-pill button { font: inherit; background: transparent; border: 0; color: inherit; padding: var(--mb-space-1) var(--mb-space-3); }
.conv-pill button:hover:not(:disabled) { background: transparent; text-decoration: underline; }
.conv-pill .conv-pill-x { padding: var(--mb-space-1) var(--mb-space-2); border-left: 1px solid var(--mb-color-text-muted); }
.conv-composer { flex: none; border-top: 1px solid var(--mb-color-border); padding: var(--mb-space-2) var(--mb-space-4); background: var(--mb-color-panel); }
.conv-composer p { margin: 0 0 var(--mb-space-1); }
.conv-composer input { flex: 1 1 14em; min-width: 0; }
@media (prefers-reduced-motion: reduce) { .conv-fresh { animation: none; } }
`;
