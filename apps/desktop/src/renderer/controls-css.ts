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
/* The docked approval (MWS-08): one card the width of the composer, on the editor surface, with the 3:1 edge: the words, the command, why it asks, one disclosure, then the three decisions. */
.conv > .mb-tray-host { width: calc(100% - 2 * var(--mb-space-5)); max-width: var(--mb-column); margin-inline: auto; padding: 0; box-sizing: border-box; flex: none; }
.mb-tray[data-tray-id="approvals"] { background: var(--color-panel); border-color: var(--mb-color-border-control); border-radius: var(--mb-composer-radius); padding: 0; overflow: hidden; }
.mb-tray[data-tray-id="approvals"] .mb-tray-head { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); }
.mb-tray[data-tray-id="approvals"] .mb-tray-body { margin: 0; }
.appr { display: flex; flex-direction: column; gap: 0; outline: none; font-size: var(--mb-type-chrome-size); }
.appr > * { padding-inline: 14px; }
.appr:focus-visible { outline: 2px solid var(--mb-color-accent); outline-offset: 2px; border-radius: var(--radius-md); }
.appr-deckbar { padding-block: 6px; display: flex; align-items: center; gap: var(--mb-space-2); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); border-bottom: 1px solid var(--color-tab-border); }
.appr-deckbar .appr-spacer, .appr-foot .appr-spacer { flex: 1 1 auto; }
.appr-title { margin: 0; padding-block: 12px 0; display: flex; align-items: center; gap: var(--mb-space-2); font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); font-weight: 500; }
.appr-title-words { flex: 1 1 auto; min-width: 0; }
.appr-body { display: flex; flex-direction: column; gap: var(--mb-space-2); padding-block: 10px 0; }
.appr-command-line { margin: 0; }
.appr-command { display: block; padding: var(--mb-space-2) var(--mb-space-3); background: var(--mb-surface-fill); border-radius: var(--radius-lg); font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 9em; overflow: auto; }
.appr-intent { margin: 0; display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-0_5) var(--mb-space-3); font-size: var(--mb-type-caption-size); }
.appr-intent:empty { display: none; }
.appr-intent dt, .appr-exact-list dt { color: var(--mb-color-text-muted); }
.appr-intent dd, .appr-exact-list dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
.appr-why { margin: 0; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); color: var(--mb-color-text-muted); }
.appr-exact-wrap { padding-block: var(--mb-space-1_5) 12px; }
.appr-disc.appr-disc { display: inline-flex; align-items: center; gap: var(--mb-space-1); min-height: 0; padding: 2px 0; border: 0; border-radius: var(--radius-sm); background: transparent; font: inherit; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); cursor: pointer; }
.appr-disc.appr-disc:hover:not(:disabled) { background: transparent; color: var(--mb-color-text); }
.appr-exact:not(.mb-sr-only) { display: flex; flex-direction: column; gap: var(--mb-space-1_5); margin-top: var(--mb-space-1_5); font-size: var(--mb-type-caption-size); }
.appr-exact p { margin: 0; color: var(--mb-color-text-muted); }
.appr-exact-list { margin: 0; display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-0_5) var(--mb-space-3); }
.appr-exact code, .appr-fine code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.appr-exact pre { margin: 0; padding: var(--mb-space-1_5) var(--mb-space-3); background: var(--mb-surface-fill); border-radius: var(--radius-lg); font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); white-space: pre-wrap; overflow-wrap: anywhere; max-height: 10em; overflow: auto; }
.appr-fine { margin: 0; padding-block: 0 8px; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.appr-deckbar + .appr-fine { padding-block: 8px 0; }
.appr > .cc-appr { padding-block: 10px 0; }
.appr-foot { display: flex; flex-wrap: nowrap; align-items: center; gap: var(--mb-space-2); padding-block: 10px; border-top: 1px solid var(--color-tab-border); }
.appr-foot .mb-btn { white-space: nowrap; }
.appr-foot + .appr-fine { padding-block: 0 10px; }
.appr-warn { margin: 0; font-size: var(--mb-type-chrome-size); font-weight: 600; }
.appr-note { margin: 0; padding-block: 0 8px; font-size: var(--mb-type-chrome-size); }
/* Always allow: one menu holding the prefix and where the rule applies. It is anchored to its button and drawn over the page, so the card's own edge never clips it. */
.appr-always-host { display: inline-flex; }
.appr-always-host > .mb-btn[aria-expanded="true"] { anchor-name: --appr-always; }
.appr-always { position: fixed; position-anchor: --appr-always; right: anchor(right); bottom: anchor(top); margin-bottom: 6px; position-try-fallbacks: flip-block; z-index: var(--mb-z-popover); box-sizing: border-box; width: 340px; max-width: calc(100vw - 2 * var(--mb-space-4)); max-height: min(420px, 60vh); overflow-y: auto; display: flex; flex-direction: column; gap: var(--mb-space-1); padding: 6px; background: var(--color-menu); color: var(--color-foreground); border: 1px solid var(--color-popover-border); border-radius: var(--radius-xl); box-shadow: var(--mb-elevation-2); font-size: var(--mb-type-chrome-size); }
.appr-always-group { border: 0; margin: 0; padding: 0; min-width: 0; display: flex; flex-direction: column; gap: 1px; }
.appr-always-group legend { padding: 6px 10px 4px; font-size: var(--mb-type-caption-size); font-weight: 600; color: var(--mb-color-text-muted); }
.appr-always-item { display: grid; grid-template-columns: auto minmax(0, 1fr); column-gap: var(--mb-space-2); align-items: center; padding: 6px 10px; border-radius: var(--radius-lg); cursor: pointer; }
.appr-always-item:hover, .appr-always-item[data-checked="true"] { background: var(--color-menu-hover); }
.appr-always-item:focus-within { outline: 2px solid var(--mb-color-focus); outline-offset: -2px; }
.appr-always-item input { grid-row: 1 / span 2; margin: 0; }
.appr-always-a { min-width: 0; overflow-wrap: anywhere; font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); color: var(--mb-color-text); }
.appr-always-a.appr-always-plain { font: inherit; }
.appr-always-b { grid-column: 2; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.appr-always-foot { display: flex; justify-content: flex-end; gap: var(--mb-space-2); padding: 6px 4px 2px; border-top: 1px solid var(--color-border); margin-top: 2px; }

.rm-btn { font-size: var(--mb-type-chrome-size); }
.rm { display: flex; flex-direction: column; gap: var(--mb-space-3); max-height: 70vh; overflow: auto; }
.rm h3 { margin: 0 0 var(--mb-space-1); font-size: var(--mb-type-body-size); }
.rm fieldset { border: 0; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.rm legend { padding: 0; font-weight: 600; margin-bottom: var(--mb-space-1); }
.rm-mode { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0 var(--mb-space-2); align-items: start; padding: var(--mb-space-1) var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--radius-md); }
.rm-mode[data-current="true"] { border-color: var(--mb-color-accent); background: var(--mb-color-selected); }
.rm-mode input { margin-top: 3px; }
.rm-mode .meta { grid-column: 2; margin: 0; }
.rm-classes { margin: 0; padding-left: var(--mb-space-4); font-size: var(--mb-type-chrome-size); }
.rm-rules { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.rm-rule { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--radius-md); }
.rm-rule code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.rm-rule .rm-spacer { flex: 1 1 auto; }
.rm-form { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.rm-form label { display: flex; flex-direction: column; gap: var(--mb-space-0_5); font-size: var(--mb-type-chrome-size); }
.rm-form label.rm-check { flex-direction: row; align-items: flex-start; gap: var(--mb-space-2); }
.rm-form select { font: inherit; padding: var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.rm-actions { display: flex; gap: var(--mb-space-2); justify-content: flex-end; flex-wrap: wrap; }

.ring { display: inline-flex; align-items: center; gap: var(--mb-space-1); font: inherit; font-size: var(--mb-type-caption-size); padding: 1px var(--mb-space-2); border-radius: calc(infinity * 1px); border: 1px solid var(--mb-color-border-control); background: transparent; color: var(--mb-color-text); cursor: pointer; }
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
.ctx-bar { display: block; height: 6px; border-radius: calc(infinity * 1px); background: var(--mb-color-accent-text); min-width: 1px; }
.ctx-bar-cell { width: 28%; }
.ctx-usage { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-0_5) var(--mb-space-3); margin: 0; }
.ctx-usage dt { color: var(--mb-color-text-muted); }
.ctx-usage dd { margin: 0; }
.ctx-budget { display: flex; flex-wrap: wrap; align-items: end; gap: var(--mb-space-2); }
.ctx-budget label { display: flex; flex-direction: column; gap: var(--mb-space-0_5); font-size: var(--mb-type-caption-size); }

.ckpt { display: inline-flex; align-items: center; flex-wrap: wrap; gap: var(--mb-space-1); font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.ckpt-actions { display: flex; flex-wrap: wrap; gap: var(--mb-space-1); align-items: center; }
.conv-user .ckpt-actions { margin-top: var(--mb-space-1); justify-content: flex-end; }
/* AWL-07: a message's hover-only actions overlay the bubble's corner instead of reserving a line. */
.conv-user .ckpt-actions[data-hover-only="true"] { position: absolute; right: var(--mb-space-2); bottom: var(--mb-space-1); margin: 0; background: var(--mb-surface-fill); border-radius: var(--radius-md); }
.conv-user:not(:hover):not(:focus-within) .ckpt-actions[data-hover-only="true"] { opacity: 0; }
.ckpt-redo { display: flex; align-items: center; gap: var(--mb-space-2); font-size: var(--mb-type-chrome-size); padding: var(--mb-space-1) var(--mb-space-3); border: 1px dashed var(--mb-color-border-control); border-radius: var(--radius-md); }
.ckpt-dialog { display: flex; flex-direction: column; gap: var(--mb-space-2); max-height: 65vh; overflow: auto; }
.ckpt-files { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 2px; font-size: var(--mb-type-chrome-size); max-height: 14em; overflow: auto; }
.ckpt-files li { display: flex; flex-wrap: wrap; gap: var(--mb-space-2); align-items: baseline; }
.ckpt-files code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); overflow-wrap: anywhere; }
/* In the review gallery every row is measured at once: off-screen rows are not skipped, so no colour is read mid-transition. */
[data-testid="gallery-controls"] .conv-row, [data-testid="gallery-conversation"] .conv-row { content-visibility: visible; }
.ckpt-edit { display: flex; flex-direction: column; gap: var(--mb-space-1); }
.ckpt-edit textarea { min-height: 4.5em; width: 100%; box-sizing: border-box; }
`;
