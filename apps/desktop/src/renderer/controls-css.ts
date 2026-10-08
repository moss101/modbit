/**
 * Styles of the approval stack (PX-058), the run-mode dialog (PX-057), the
 * context ring and its trays (PX-060) and the checkpoint controls (PX-062),
 * written against the design tokens only: no literal colour appears here.
 * State is never colour alone: each state carries words and a glyph in the
 * markup, and the colours below only reinforce them.
 */
export const controlsCss = `
/* The one tray host sits directly above the composer in the conversation; a tall approval deck scrolls inside it instead of squeezing the transcript out. */
.conv > .mb-tray-host { flex: none; max-height: 45%; overflow-y: auto; padding-top: var(--mb-space-1); }
.appr { display: flex; flex-direction: column; gap: var(--mb-space-2); outline: none; }
.appr:focus-visible { outline: 2px solid var(--mb-color-accent); outline-offset: 2px; border-radius: var(--mb-radius-3); }
.appr-deckbar { display: flex; align-items: center; gap: var(--mb-space-2); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.appr-deckbar .appr-spacer { flex: 1 1 auto; }
.appr-title { margin: 0; font-size: var(--mb-type-body-size); display: flex; flex-wrap: wrap; align-items: baseline; gap: var(--mb-space-2); }
.appr-title code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); color: var(--mb-color-text-muted); }
.appr-intent { margin: 0; display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-0_5) var(--mb-space-3); font-size: var(--mb-type-chrome-size); }
.appr-intent dt { color: var(--mb-color-text-muted); }
.appr-intent dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
.appr-command { display: block; padding: var(--mb-space-1) var(--mb-space-2); background: var(--mb-color-sunken); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-3); font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 9em; overflow: auto; }
.appr-args pre { margin: var(--mb-space-1) 0 0; padding: var(--mb-space-1) var(--mb-space-2); background: var(--mb-color-sunken); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-3); font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 10em; overflow: auto; }
.appr-why { margin: 0; font-size: var(--mb-type-chrome-size); }
.appr-fine { margin: 0; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.appr-fine code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.appr-foot { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); }
.appr-foot .appr-spacer { flex: 1 1 auto; }
.appr-prefix { font: inherit; font-size: var(--mb-type-chrome-size); max-width: 26ch; padding: var(--mb-space-0_5) var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.appr-warn { margin: 0; font-size: var(--mb-type-chrome-size); font-weight: 600; }
.appr-note { margin: 0; font-size: var(--mb-type-chrome-size); }

.rm-btn { font-size: var(--mb-type-chrome-size); }
.rm { display: flex; flex-direction: column; gap: var(--mb-space-3); max-height: 70vh; overflow: auto; }
.rm h3 { margin: 0 0 var(--mb-space-1); font-size: var(--mb-type-body-size); }
.rm fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.rm legend { padding: 0; font-weight: 600; margin-bottom: var(--mb-space-1); }
.rm-mode { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0 var(--mb-space-2); align-items: start; padding: var(--mb-space-1) var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-3); }
.rm-mode[data-current="true"] { border-color: var(--mb-color-accent); background: var(--mb-color-selected); }
.rm-mode input { margin-top: 3px; }
.rm-mode .meta { grid-column: 2; margin: 0; }
.rm-classes { margin: 0; padding-left: var(--mb-space-4); font-size: var(--mb-type-chrome-size); }
.rm-rules { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.rm-rule { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-3); }
.rm-rule code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.rm-rule .rm-spacer { flex: 1 1 auto; }
.rm-form { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.rm-form label { display: flex; flex-direction: column; gap: var(--mb-space-0_5); font-size: var(--mb-type-chrome-size); }
.rm-form label.rm-check { flex-direction: row; align-items: flex-start; gap: var(--mb-space-2); }
.rm-form select { font: inherit; padding: var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.rm-actions { display: flex; gap: var(--mb-space-2); justify-content: flex-end; flex-wrap: wrap; }

.ring { display: inline-flex; align-items: center; gap: var(--mb-space-1); font: inherit; font-size: var(--mb-type-caption-size); padding: 1px var(--mb-space-2); border-radius: var(--mb-radius-full); border: 1px solid var(--mb-color-border-control); background: transparent; color: var(--mb-color-text); cursor: pointer; }
.ring:hover:not(:disabled) { background: var(--mb-color-hover); border-color: var(--mb-color-border-control); }
.ring svg { width: 16px; height: 16px; flex: none; }
.ring-track { fill: none; stroke: var(--mb-color-border-control); stroke-width: 3; }
.ring-fill { fill: none; stroke: var(--mb-color-accent-text); stroke-width: 3; stroke-linecap: round; transform: rotate(-90deg); transform-origin: 50% 50%; }
.ring[data-level="warn"] .ring-fill { stroke: var(--mb-color-warn); }
.ring[data-level="danger"] .ring-fill { stroke: var(--mb-color-danger); }
.ctx { display: flex; flex-direction: column; gap: var(--mb-space-2); font-size: var(--mb-type-chrome-size); }
.ctx table { border-collapse: collapse; width: 100%; }
.ctx th, .ctx td { text-align: left; padding: 2px var(--mb-space-2) 2px 0; font-weight: normal; vertical-align: middle; }
.ctx td.num, .ctx th.num { text-align: right; font-variant-numeric: tabular-nums; }
.ctx tfoot td, .ctx tfoot th { border-top: 1px solid var(--mb-color-border); font-weight: 600; }
.ctx-bar { display: block; height: 6px; border-radius: var(--mb-radius-full); background: var(--mb-color-accent-text); min-width: 1px; }
.ctx-bar-cell { width: 28%; }
.ctx-usage { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-0_5) var(--mb-space-3); margin: 0; }
.ctx-usage dt { color: var(--mb-color-text-muted); }
.ctx-usage dd { margin: 0; }
.ctx-budget { display: flex; flex-wrap: wrap; align-items: end; gap: var(--mb-space-2); }
.ctx-budget label { display: flex; flex-direction: column; gap: var(--mb-space-0_5); font-size: var(--mb-type-caption-size); }

.ckpt { display: inline-flex; align-items: center; flex-wrap: wrap; gap: var(--mb-space-1); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.ckpt-actions { display: flex; flex-wrap: wrap; gap: var(--mb-space-1); align-items: center; }
.conv-user .ckpt-actions { margin-top: var(--mb-space-1); justify-content: flex-end; }
.conv-user:not(:hover):not(:focus-within) .ckpt-actions[data-hover-only="true"] { opacity: 0; }
.ckpt-redo { display: flex; align-items: center; gap: var(--mb-space-2); font-size: var(--mb-type-chrome-size); padding: var(--mb-space-1) var(--mb-space-3); border: 1px dashed var(--mb-color-border-control); border-radius: var(--mb-radius-3); }
.ckpt-dialog { display: flex; flex-direction: column; gap: var(--mb-space-2); max-height: 65vh; overflow: auto; }
.ckpt-files { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; font-size: var(--mb-type-chrome-size); max-height: 14em; overflow: auto; }
.ckpt-files li { display: flex; flex-wrap: wrap; gap: var(--mb-space-2); align-items: baseline; }
.ckpt-files code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); overflow-wrap: anywhere; }
/* In the review gallery every row is measured at once: off-screen rows are not skipped, so no colour is read mid-transition. */
[data-testid="gallery-controls"] .conv-row { content-visibility: visible; }
.ckpt-edit { display: flex; flex-direction: column; gap: var(--mb-space-1); }
.ckpt-edit textarea { min-height: 4.5em; width: 100%; box-sizing: border-box; }
`;
