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
/* MWS-04: the Tasks head (search, list options, more), the state views, then the groups. Every height and text size is a token, so Compact is the token block alone. */
.agents-head { flex: none; display: flex; align-items: center; gap: var(--mb-space-0_5); padding: var(--mb-space-4) var(--mb-row-inset) var(--mb-space-1) calc(var(--mb-row-inset) + 10px); }
.agents-head-title { flex: 1 1 0; min-width: 0; margin: 0; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); font-weight: 500; color: var(--mb-color-text); }
.agents-head-tools { flex: none; display: inline-flex; align-items: center; gap: var(--mb-space-0_5); }
.agents-states { flex: none; display: flex; flex-wrap: wrap; gap: var(--mb-space-0_5); padding: 0 var(--mb-row-inset) var(--mb-space-1) calc(var(--mb-row-inset) + 4px); }
.agents-state.agents-state { font: inherit; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); height: var(--mb-control-sm); min-height: 0; padding: 0 6px; border: 0; border-radius: var(--radius-md); background: transparent; color: var(--mb-color-text-muted); white-space: nowrap; }
.agents-state:hover:not(:disabled) { background: var(--mb-color-hover); color: var(--mb-color-text); }
.agents-state[aria-pressed="true"] { background: var(--mb-color-selected); color: var(--mb-color-text); }
.agents-state-count { font-variant-numeric: tabular-nums; font-weight: 600; }
.agents-controls { padding: var(--mb-space-1) var(--mb-space-3) var(--mb-space-2); display: flex; flex-direction: column; gap: var(--mb-space-1); flex: 0 1 auto; min-height: 0; max-height: 60%; overflow-y: auto; border-bottom: 1px solid var(--mb-color-border); }
.agents-search { width: 100%; box-sizing: border-box; font-size: var(--mb-type-chrome-size); padding: var(--mb-space-1) var(--mb-space-2); }
.agents-tools { display: flex; align-items: center; gap: var(--mb-space-1); }
.agents-grouping { flex: 1 1 0; min-width: 0; }
.agents-grouping select { width: 100%; font: inherit; font-size: var(--mb-type-chrome-size); padding: var(--mb-space-0_5) var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); background: var(--mb-color-sunken); color: var(--mb-color-text); }
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
.agents-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; overflow-x: hidden; position: relative; padding-bottom: var(--mb-space-2); }
.agents-canvas { position: relative; width: 100%; }
.agents-empty { margin: var(--mb-space-3) calc(var(--mb-row-inset) + 10px); font-style: normal; }
/* A group's heading: its words, then the chevron; the heading sits at the foot of its slot, so the space above it is the slot's. */
.agents-group { position: absolute; left: 0; right: 0; box-sizing: border-box; display: flex; align-items: flex-end; padding: 0 var(--mb-row-inset); }
.agents-section { flex: 1 1 0; min-width: 0; display: flex; }
.agents-section-btn, .agents-project-open.agents-project-open { box-sizing: border-box; flex: 0 1 auto; min-width: 0; max-width: 100%; display: inline-flex; align-items: center; gap: var(--mb-space-1); height: var(--mb-control-md); min-height: 0; padding: 0 var(--mb-space-1_5) 0 10px; border: 0; border-radius: var(--mb-row-radius); background: transparent; color: var(--mb-color-text-muted); font: inherit; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); font-weight: 400; text-align: left; transition: none; }
.agents-section-btn:hover:not(:disabled), .agents-project-open.agents-project-open:hover:not(:disabled), .agents-project-toggle.agents-project-toggle:hover:not(:disabled) { background: var(--color-hover); color: var(--color-foreground); }
.agents-section-btn:active:not(:disabled), .agents-project-open.agents-project-open:active:not(:disabled) { transform: none; }
.agents-section-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.agents-section-count { flex: none; font-variant-numeric: tabular-nums; }
.agents-section-count::before { content: "· "; }
.agents-caret { flex: none; color: var(--mb-color-icon-muted); }
/* A project parent is a group's heading whose words open the project's page; its chevron is its own control after them. */
.agents-project-open.agents-project-open { order: 1; padding-right: var(--mb-space-1); }
.agents-project-toggle.agents-project-toggle { order: 2; box-sizing: border-box; flex: none; width: var(--mb-control-xs); height: var(--mb-control-md); min-height: 0; display: inline-grid; place-items: center; padding: 0; border: 0; border-radius: var(--radius-md); background: transparent; color: var(--mb-color-icon-muted); transition: none; }
.agents-project[data-selected="true"] .agents-project-open { background: var(--color-selected); color: var(--color-foreground); }
.agents-project[data-drop="true"] { outline: 2px dashed var(--mb-color-accent); outline-offset: -2px; border-radius: var(--mb-row-radius); }
.agents-project[data-archived="true"] .agents-section-label { font-style: italic; }
.agents-group > .agents-actions { order: 3; top: auto; bottom: 0; transform: none; height: var(--mb-control-md); align-items: center; }
.agents-group:hover > .agents-actions, .agents-group:focus-within > .agents-actions { display: inline-flex; }
.agents-project:hover .agents-dot, .agents-project:focus-within .agents-dot { visibility: hidden; }
/* A task row (MWS-04): one line at the row height, the title fading at its end, then the state's glyph, the changed files and the unread dot. */
.agents-row { position: absolute; left: 0; right: 0; box-sizing: border-box; display: flex; align-items: stretch; padding: 0 var(--mb-row-inset); }
.agents-row[draggable="true"] { cursor: grab; }
.agents-open { box-sizing: border-box; flex: 1 1 0; min-width: 0; height: 100%; min-height: 0; display: flex; align-items: center; gap: var(--mb-space-2); font: inherit; text-align: left; padding: 0 10px 0 calc(10px + var(--agents-depth, 0) * 14px); border: 0; border-radius: var(--mb-row-radius); background: transparent; color: var(--mb-color-text); transition: none; }
.agents-open:active:not(:disabled) { transform: none; }
.agents-open:hover:not(:disabled), .agents-row[data-preview="true"] .agents-open { background: var(--color-surface-hover); }
.agents-row[data-selected="true"] .agents-open { background: var(--color-selected); }
/* A child task's mark: an elbow from its parent's row, drawn in the muted icon colour. */
.agents-tree { flex: none; width: 8px; height: 8px; margin: 0 0 6px 2px; border-left: 1.5px solid var(--mb-color-icon-muted); border-bottom: 1.5px solid var(--mb-color-icon-muted); border-bottom-left-radius: 3px; }
.agents-text { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; justify-content: center; }
.agents-title { min-width: 0; overflow: hidden; white-space: nowrap; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); -webkit-mask-image: linear-gradient(90deg, #000 calc(100% - 28px), transparent); mask-image: linear-gradient(90deg, #000 calc(100% - 28px), transparent); }
.agents-sub { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); color: var(--mb-color-text-muted); }
.agents-status { font-weight: 400; }
.agents-row[data-class="NEEDS_ATTENTION"] .agents-status, .agents-row[data-class="FAILED"] .agents-status { color: var(--mb-color-text); }
.agents-end { flex: none; display: inline-flex; align-items: center; gap: var(--mb-space-1_5); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agents-glyph { display: inline-flex; justify-content: center; }
.agents-count { font-variant-numeric: tabular-nums; }
.agents-origin { display: inline-block; margin-right: var(--mb-space-1); padding: 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); font-weight: 600; border: 1px solid var(--mb-color-border-control); border-radius: calc(infinity * 1px); color: var(--mb-color-text); background: var(--mb-color-sunken); vertical-align: baseline; }
.agents-dot { flex: none; width: 6px; height: 6px; border-radius: calc(infinity * 1px); display: inline-block; }
.agents-dot[data-kind="unread"] { background: var(--mb-color-accent); }
.agents-dot[data-kind="attention"] { background: var(--mb-color-warn); outline: 1px solid var(--mb-color-text); }
/* Pin and archive take the place of the row's marks under the pointer and on keyboard focus. */
.agents-actions { position: absolute; right: calc(var(--mb-row-inset) + 4px); top: 50%; transform: translateY(-50%); display: none; gap: 0; border-radius: var(--mb-row-radius); }
.agents-row:hover .agents-actions, .agents-row:focus-within .agents-actions { display: inline-flex; }
.agents-row:hover .agents-end, .agents-row:focus-within .agents-end { visibility: hidden; }
/* The row or heading whose menu is open paints over the rows and headings that follow it (its actions form their own stacking context). */
.agents-row:hover, .agents-row:focus-within, .agents-group:hover, .agents-group:focus-within { z-index: 3; }
/* The preview card: beside the list, level with its row. The row names itself as the anchor, so the card follows it under any zoom. */
.agents-row[data-preview="true"] { anchor-name: --agents-preview; }
.agents-preview { position: fixed; position-anchor: --agents-preview; left: calc(anchor(right) - var(--mb-row-inset)); top: anchor(top); position-try-fallbacks: flip-block; z-index: var(--mb-z-popover); box-sizing: border-box; width: 320px; max-width: calc(100vw - 2 * var(--mb-space-4)); display: flex; flex-direction: column; gap: var(--mb-space-2); padding: var(--mb-space-3) var(--mb-space-4); border: 1px solid var(--color-popover-border); border-radius: var(--radius-xl); background: var(--color-popover); color: var(--color-foreground); box-shadow: var(--mb-elevation-2); font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); }
.agents-preview-head { display: flex; align-items: center; gap: var(--mb-space-2); }
.agents-preview-state { flex: 1 1 0; min-width: 0; font-size: var(--mb-type-caption-size); font-weight: 500; }
.agents-preview-state .mb-dot { color: var(--mb-color-text); }
.agents-preview-age { flex: none; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.agents-preview-title { margin: 0; font-weight: 500; overflow-wrap: anywhere; }
.agents-preview-reason { margin: 0; color: var(--mb-color-text-muted); overflow-wrap: anywhere; }
.agents-preview-facts { margin: 0; display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-0_5) var(--mb-space-3); font-size: var(--mb-type-caption-size); }
.agents-preview-fact { display: contents; }
.agents-preview-fact dt { color: var(--mb-color-text-muted); }
.agents-preview-fact dd { margin: 0; font-variant-numeric: tabular-nums; }
.agents-preview-actions { display: flex; gap: var(--mb-space-1_5); }
.agents-results { padding-bottom: var(--mb-space-3); }
.agents-hit { padding: var(--mb-space-1) var(--mb-space-2); border-bottom: 1px solid var(--mb-color-border); }
.agents-hit-title, .agents-snippet { width: 100%; display: flex; align-items: baseline; gap: var(--mb-space-2); font: inherit; text-align: left; border: 0; background: transparent; color: var(--mb-color-text); border-radius: var(--radius-md); padding: var(--mb-space-1); }
.agents-hit-title .agents-title { -webkit-mask-image: none; mask-image: none; text-overflow: ellipsis; }
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
.conv-density select { font: inherit; font-size: var(--mb-type-chrome-size); padding: 1px var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); background: var(--mb-surface-fill); color: var(--mb-color-text); }
.conv-body { position: relative; flex: 1 1 0; min-height: 0; display: flex; flex-direction: column; }
.conv-scroll:focus-visible { outline: 2px solid var(--mb-color-focus); outline-offset: -2px; }
.conv-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; padding: var(--mb-space-2) var(--mb-space-5) var(--mb-space-4); }
/* MWS-01: the conversation's column is 768 with its text inset to 736; the dock under it (notice, approval, composer) takes the whole column. */
.conv-inner { display: flex; flex-direction: column; gap: 14px; width: 100%; max-width: var(--mb-column-text); margin: 0 auto; }
.conv-empty { margin: var(--mb-space-4) 0; font-style: normal; }
.conv-row { flex: none; content-visibility: auto; contain-intrinsic-size: auto 48px; max-width: none; }
.conv-row[data-highlight="true"] { outline: 2px solid var(--mb-color-focus); outline-offset: 2px; border-radius: var(--radius-md); }
/* MWS-02: the person's message is a chrome fill on the conversation's editor surface. */
.conv-user { align-self: stretch; background: var(--mb-surface-fill); border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); padding: 8px 12px; max-width: none; font-size: var(--mb-type-body-size); line-height: var(--mb-type-body-line); }
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
.conv-code { margin: 0 0 var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--radius-md); background: var(--mb-surface-fill); }
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
.conv-tool-failure { color: var(--mb-color-text); font-weight: 600; }
/* AWL-07: a step group is a one-line disclosure row with no box. */
.conv-group { border: 0; border-radius: 0; background: transparent; }
.conv-group > summary { cursor: pointer; list-style: none; }
.conv-group > summary::-webkit-details-marker { display: none; }
.conv-group-label { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--mb-color-text); }
.conv-group-label[data-open-work="true"] { color: var(--mb-color-text-muted); }
.conv-group > summary:hover { color: var(--mb-color-text); }
.conv-group-body { padding: var(--mb-space-1) 0 var(--mb-space-1) 20px; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.conv-approval { border: 1px solid var(--mb-color-warn); background: var(--mb-surface-conversation); border-radius: var(--radius-xl); padding: var(--mb-space-2) var(--mb-space-3); }
.conv-approval[data-state="APPROVED"], .conv-approval[data-state="ANSWERED"] { border-color: var(--mb-color-border); background: var(--mb-surface-fill); }
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
.conv-notice { flex: none; width: 100%; max-width: var(--mb-column); margin: 0 auto var(--mb-space-1); box-sizing: border-box; border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-lg); background: var(--mb-surface-conversation); padding: var(--mb-space-1) var(--mb-space-3); }
.conv-notice[data-kind="error"] { border-color: var(--mb-color-danger); }
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
