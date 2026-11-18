/**
 * Styles of the Automations surface (REQ-PX-086) and of the two pages in the page shape of REQ-PX-207 (DR-PX-2026-11-11-021
 * MWS-14: Automations and Customize), written against the design tokens only: no literal colour and no literal text size appear
 * here. A page is a head the bar's height with the page's name and its one primary action, an optional section list at the left,
 * and lists of rows of at least 44 px with a 1 px edge; Automations has a details rail the width of the task's (MWS-09) beside its
 * list. State is never colour alone: every state, status and attention kind has a glyph and words in the markup.
 */
import { DETAILS_RAIL } from "../shell/layout.ts";

export const automationsCss = `
/* MWS-14: the page shape shared by Automations and Customize. */
.pg { height: 100%; min-height: 0; display: flex; flex-direction: column; overflow: hidden; background: var(--mb-surface-conversation); container-type: inline-size; container-name: page; }
.pg-head { flex: none; box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 var(--mb-space-2) 0 var(--mb-space-5); border-bottom: 1px solid var(--color-tab-border); font-size: var(--mb-type-chrome-size); }
.pg-head .mb-btn { white-space: nowrap; }
.pg-grow { flex: 1 1 auto; }
.pg-summary { margin: 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--mb-color-text-muted); }
.pg-split { flex: 1 1 auto; min-height: 0; display: flex; }
.pg-body { flex: 1 1 auto; min-width: 0; min-height: 0; overflow: auto; scrollbar-gutter: stable; box-sizing: border-box; padding: var(--mb-space-4) var(--mb-space-5) var(--mb-space-8); display: flex; flex-direction: column; gap: var(--mb-space-2); font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); }
.pg-body > * { flex: none; }
.pg-h { margin: var(--mb-space-2) 0 0 var(--mb-space-1_5); font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); font-weight: 600; color: var(--mb-color-text); }
.pg-intro { margin: 0 0 var(--mb-space-1) var(--mb-space-1_5); color: var(--mb-color-text-muted); }
.pg-list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); background: var(--mb-surface-conversation); overflow: hidden; }
.pg-list > li + li { border-top: 1px solid var(--color-tab-border); }
.pg-row, .pg-row-button.pg-row-button { width: 100%; box-sizing: border-box; display: flex; align-items: center; gap: var(--mb-space-3); min-height: 44px; padding: var(--mb-space-2) var(--mb-space-3) var(--mb-space-2) var(--mb-space-4); font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); outline: none; }
.pg-row:focus-visible, .pg-row[data-found="true"] { background: var(--color-selected); box-shadow: inset 2px 0 0 var(--mb-color-accent-text); }
.pg-row-button.pg-row-button { height: auto; border: 0; border-radius: 0; background: transparent; color: var(--mb-color-text); font: inherit; text-align: left; cursor: pointer; transition: none; }
.pg-row-button.pg-row-button:hover:not(:disabled) { background: var(--color-hover); }
.pg-ico { flex: none; display: inline-flex; color: var(--mb-color-text-muted); }
.pg-main { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; gap: 2px; }
.pg-title-line { overflow-wrap: anywhere; color: var(--mb-color-text); }
.pg-sub { font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); color: var(--mb-color-text-muted); overflow-wrap: anywhere; }
.pg-text { color: var(--mb-color-text); white-space: pre-wrap; }
.pg-dim { color: var(--mb-color-text-muted); }
.pg-warn { color: var(--mb-color-warn); }
.pg-mono { font-family: var(--mb-font-mono); font-size: var(--mb-type-mono-size); overflow-wrap: anywhere; }
.pg-aside { flex: none; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); color: var(--mb-color-text-muted); white-space: nowrap; }
.pg-tag { flex: none; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); padding: 1px var(--mb-space-2); border-radius: calc(infinity * 1px); background: var(--color-selected); color: var(--mb-color-text-muted); white-space: nowrap; }
.pg-tag[data-tone="ok"] { color: var(--mb-color-ok); }
.pg-tag[data-tone="warn"] { color: var(--mb-color-warn); }
.pg-tag[data-tone="danger"] { color: var(--mb-color-danger); }
.pg-actions { flex: none; display: flex; align-items: center; justify-content: flex-end; flex-wrap: wrap; gap: var(--mb-space-1_5); max-width: 50%; }
.pg-actions .mb-btn { white-space: nowrap; }
.pg-notes { list-style: none; margin: 0; padding: 0 0 0 var(--mb-space-1_5); display: flex; flex-direction: column; gap: 2px; }
.pg-form { display: flex; align-items: center; flex-wrap: wrap; gap: var(--mb-space-2); margin: var(--mb-space-1) 0 0 var(--mb-space-1_5); }
.pg-form input, .pg-form select { font: inherit; font-size: var(--mb-type-chrome-size); }
.pg-details { margin: 0 0 0 var(--mb-space-1_5); }
.pg-details summary { cursor: pointer; color: var(--mb-color-text-muted); }

/* Customize (REQ-PX-207): one search in the head, the section list at the left, the sections stacked in the body. */
.cz-search { flex: 0 1 320px; min-width: 160px; box-sizing: border-box; height: var(--mb-control-md); display: flex; align-items: center; gap: var(--mb-space-1_5); padding: 0 var(--mb-space-2); border: 1px solid var(--color-tab-border); border-radius: var(--radius-lg); background: var(--mb-surface-fill); color: var(--mb-color-text-muted); }
.cz-search:focus-within { outline: 2px solid var(--mb-color-accent-text); outline-offset: -1px; }
.cz-search input { flex: 1 1 auto; min-width: 0; height: 100%; border: 0; padding: 0; background: transparent; color: var(--mb-color-text); font: inherit; font-size: var(--mb-type-chrome-size); outline: none; }
.cz-nav { flex: none; width: 232px; box-sizing: border-box; overflow: auto; padding: var(--mb-space-3) var(--mb-row-inset); border-right: 1px solid var(--color-tab-border); display: flex; flex-direction: column; gap: var(--mb-row-gap); }
.cz-cap { padding: 0 var(--mb-space-2) var(--mb-space-1); font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); color: var(--mb-color-text-muted); }
.cz-nav-row.cz-nav-row { height: var(--mb-row); min-height: 0; display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 var(--mb-space-2); border: 0; border-radius: var(--mb-row-radius); background: transparent; color: var(--mb-color-text); font: inherit; font-size: var(--mb-type-chrome-size); text-align: left; cursor: pointer; }
.cz-nav-row.cz-nav-row:hover { background: var(--color-hover); }
.cz-nav-row.cz-nav-row[aria-current="true"] { background: var(--color-selected); }
.cz-nav-row > svg { flex: none; color: var(--mb-color-text-muted); }
.cz-nav-label { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cz-count { flex: none; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.cz-body > * { width: 100%; max-width: 920px; }
.cz-sec { display: flex; flex-direction: column; gap: var(--mb-space-2); scroll-margin-top: var(--mb-space-4); }
.cz-sec + .cz-sec { margin-top: var(--mb-space-5); }
.cz-results { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.cz-facts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-1) var(--mb-space-3); margin: 0; font-size: var(--mb-type-chrome-size); }
.cz-facts dt { color: var(--mb-color-text-muted); }
.cz-facts dd { margin: 0; overflow-wrap: anywhere; }
.cz-secondary { font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); color: var(--mb-color-text-muted); }
.cz-review { display: flex; flex-direction: column; gap: var(--mb-space-3); }
.cz-review-actions { display: flex; align-items: center; gap: var(--mb-space-2); }
@container page (max-width: 640px) { .cz-nav { width: 160px; } .pg-actions { max-width: none; } }

/* Automations (REQ-PX-086 in the page shape of REQ-PX-207): the list, and the details rail of the chosen one beside it. */
.autos-defs .autos-row { flex-wrap: wrap; row-gap: var(--mb-space-1); }
.autos-row[data-selected="true"] { background: var(--color-selected); }
.autos-row:hover:not([data-selected="true"]) { background: var(--color-hover); }
.autos-select { flex: 1 1 200px; min-width: 0; display: flex; align-items: center; gap: var(--mb-space-3); padding: 0; border: 0; background: transparent; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.autos-select:hover:not(:disabled) { background: transparent; }
.autos-cell { flex: none; box-sizing: border-box; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); color: var(--mb-color-text-muted); display: flex; flex-direction: column; }
.autos-cell-state { width: 128px; }
.autos-cell-trig { width: 168px; list-style: none; margin: 0; padding: 0; }
.autos-cell-trig li { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.autos-next { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.autos-cell-last { width: 112px; display: block; color: var(--mb-color-text); }
.autos-rail { flex: none; width: ${DETAILS_RAIL.width}px; min-width: ${DETAILS_RAIL.min}px; box-sizing: border-box; min-height: 0; overflow: auto; background: var(--mb-surface-conversation); border-left: 1px solid var(--color-tab-border); padding: var(--mb-space-1_5) 18px 18px; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); }
.autos-rail-head { gap: var(--mb-space-2); }
.autos-rail-title { flex: 1 1 auto; min-width: 0; margin: 0; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); font-weight: 600; overflow-wrap: anywhere; }
.autos-rail-state { margin: 0 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); }
.autos-rail-text { margin: 0 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); overflow-wrap: anywhere; }
.autos-rail .autos-note { margin: var(--mb-space-1) 0; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); }
.autos-runs > li + li { border-top: 1px solid var(--color-tab-border); }
.autos-run { padding: var(--mb-space-1) 0 var(--mb-space-2); }
.autos-run .rail-row { justify-content: space-between; }
@container page (max-width: 800px) { .autos .pg-split { flex-direction: column; } .autos-rail { width: auto; min-width: 0; max-height: 55%; border-left: 0; border-top: 1px solid var(--color-tab-border); } }
.autos-attention { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.autos-bar { display: flex; align-items: flex-end; gap: var(--mb-space-2); flex-wrap: wrap; margin: 0 0 var(--mb-space-2) var(--mb-space-1_5); }
.autos-field { display: flex; flex-direction: column; gap: var(--mb-space-0_5); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.autos-field-wide { flex: 1 1 22em; min-width: 14em; }
.autos-field input, .autos-field select, .autos-field textarea { font: inherit; font-size: var(--mb-type-chrome-size); color: var(--mb-color-text); }
.autos-note { margin: 0; padding: var(--mb-space-2) var(--mb-space-3); border: 1px solid var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--radius-lg); color: var(--mb-color-text); overflow-wrap: anywhere; }
.autos-note p { margin: 0 0 var(--mb-space-1); }
.autos-empty { padding: var(--mb-space-3) var(--mb-space-4); border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); display: flex; flex-direction: column; gap: var(--mb-space-1); }
.autos-empty p { margin: 0; }
.autos-h3 { margin: 0; font-size: var(--mb-type-body-size); }
.autos .meta { margin: 0; }
.autos-list { margin: 0; padding-left: var(--mb-space-4); }
.autos-facts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-1) var(--mb-space-3); margin: 0; font-size: var(--mb-type-chrome-size); }
.autos-facts dt { color: var(--mb-color-text-muted); }
.autos-facts dd { margin: 0; overflow-wrap: anywhere; }
.autos-hash { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); overflow-wrap: anywhere; }
.autos-event { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); overflow-wrap: anywhere; }
.autos-actions { display: flex; align-items: center; gap: var(--mb-space-1_5); flex-wrap: wrap; margin: var(--mb-space-2) 0; }
.autos-versions summary { cursor: pointer; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.autos-finding { margin: 0 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); font-weight: 600; }
.autos-editor { border: 1px solid var(--color-tab-border); border-radius: var(--radius-xl); background: var(--mb-surface-conversation); padding: var(--mb-space-3); }
.autos-editor-grid { display: grid; grid-template-columns: minmax(0, 3fr) minmax(0, 2fr); gap: var(--mb-space-3); }
@container page (max-width: 1000px) { .autos-editor-grid { grid-template-columns: minmax(0, 1fr); } }
.autos-editor-left, .autos-editor-right { display: flex; flex-direction: column; gap: var(--mb-space-2); min-width: 0; }
.autos-json { width: 100%; box-sizing: border-box; min-height: 12em; font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); white-space: pre; overflow: auto; }
.autos-verdict { margin: 0; display: flex; align-items: center; gap: var(--mb-space-1); flex-wrap: wrap; }
.autos-issues { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.autos-issues li { border-left: 3px solid var(--mb-color-danger); padding: var(--mb-space-0_5) var(--mb-space-2); display: flex; flex-wrap: wrap; gap: var(--mb-space-1); align-items: baseline; font-size: var(--mb-type-chrome-size); }
.autos-issue-code { font-size: var(--mb-type-caption-size); }
.autos-triggers { list-style: none; margin: 0; padding: 0; }
.autos-dialog { display: flex; flex-direction: column; gap: var(--mb-space-2); max-height: 70vh; overflow: auto; }
.autos-dialog p { margin: 0; }
.autos-ack { display: flex; gap: var(--mb-space-2); align-items: flex-start; }
`;
