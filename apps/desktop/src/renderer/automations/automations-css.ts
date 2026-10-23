/**
 * Styles of the Automations surface (REQ-PX-086), written against the design
 * tokens only: no literal colour appears here. State is never colour alone:
 * every state, status and attention kind has a glyph and words in the markup.
 */
export const automationsCss = `
.autos { padding: var(--mb-space-4); display: flex; flex-direction: column; gap: var(--mb-space-3); max-width: 1180px; }
.autos-scroll { flex: 1 1 0; min-height: 0; overflow-y: auto; }
.autos-head { display: flex; align-items: center; gap: var(--mb-space-2); flex-wrap: wrap; }
.autos-title { margin: 0; font-size: var(--mb-type-title-size, 18px); }
.autos-spacer { flex: 1 1 0; }
.autos-h3 { margin: 0; font-size: var(--mb-type-body-size); }
.autos .meta { margin: 0; }
.autos-bar { display: flex; align-items: flex-end; gap: var(--mb-space-2); flex-wrap: wrap; }
.autos-field { display: flex; flex-direction: column; gap: var(--mb-space-0_5); font-size: var(--mb-type-chrome-size); }
.autos-field-wide { flex: 1 1 22em; min-width: 14em; }
.autos-field input, .autos-field select, .autos-field textarea { font: inherit; font-size: var(--mb-type-chrome-size); }
.autos-note { margin: 0; padding: var(--mb-space-2) var(--mb-space-3); border: 1px solid var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--mb-radius-4); color: var(--mb-color-text); overflow-wrap: anywhere; }
.autos-note p { margin: 0 0 var(--mb-space-1); }
.autos-attention { border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-4); background: var(--mb-color-panel); padding: var(--mb-space-2) var(--mb-space-3); display: flex; flex-direction: column; gap: var(--mb-space-1); }
.autos-attention-item { display: flex; align-items: center; gap: var(--mb-space-2); flex-wrap: wrap; padding: var(--mb-space-0_5) 0; }
.autos-plain { list-style: none; margin: 0; padding: 0; }
.autos-list { margin: 0; padding-left: var(--mb-space-4); }
.autos-empty { padding: var(--mb-space-3); }
.autos-table-wrap { overflow-x: auto; border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-4); background: var(--mb-color-panel); }
.autos-table { width: 100%; border-collapse: collapse; font-size: var(--mb-type-chrome-size); }
.autos-table th, .autos-table td { text-align: left; vertical-align: top; padding: var(--mb-space-2); border-bottom: 1px solid var(--mb-color-border); }
.autos-table thead th { font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); font-weight: 600; }
.autos-table tbody th { font-weight: 600; }
.autos-table tr[data-selected="true"] { background: var(--mb-color-selected); }
.autos-link { font: inherit; font-weight: 600; background: transparent; border: 0; padding: 0; color: var(--mb-color-accent-text); text-decoration: underline; text-align: left; cursor: pointer; }
.autos-link:hover:not(:disabled) { background: transparent; }
.autos-detail { border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-4); background: var(--mb-color-panel); padding: var(--mb-space-3); display: flex; flex-direction: column; gap: var(--mb-space-2); }
.autos-detail-head { display: flex; align-items: center; gap: var(--mb-space-2); flex-wrap: wrap; }
.autos-facts { display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-1) var(--mb-space-3); margin: 0; font-size: var(--mb-type-chrome-size); }
.autos-facts dt { color: var(--mb-color-text-muted); }
.autos-facts dd { margin: 0; overflow-wrap: anywhere; }
.autos-hash { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); overflow-wrap: anywhere; }
.autos-event { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); overflow-wrap: anywhere; }
.autos-actions { display: flex; align-items: center; gap: var(--mb-space-2); flex-wrap: wrap; }
.autos-versions summary { cursor: pointer; font-size: var(--mb-type-chrome-size); }
.autos-finding { font-size: var(--mb-type-caption-size); font-weight: 600; margin-top: var(--mb-space-0_5); }
.autos-editor { border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-4); background: var(--mb-color-panel); padding: var(--mb-space-3); }
.autos-editor-grid { display: grid; grid-template-columns: minmax(0, 3fr) minmax(0, 2fr); gap: var(--mb-space-3); }
@media (max-width: 1000px) { .autos-editor-grid { grid-template-columns: minmax(0, 1fr); } }
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
