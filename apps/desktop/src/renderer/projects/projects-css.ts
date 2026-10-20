/**
 * Styles of the project and worktree surfaces (REQ-PX-063, 064, 068). Written
 * against the design tokens only: no literal colour appears here, and no state
 * is carried by colour alone (every state has its words and a glyph).
 */
export const projectsCss = `
.agents-project { position: absolute; left: 0; right: 0; box-sizing: border-box; display: flex; align-items: stretch; padding: 0 var(--mb-space-1); }
.agents-project[data-drop="true"] { outline: 2px dashed var(--mb-color-accent); outline-offset: -2px; border-radius: var(--mb-radius-4); }
.agents-project[data-archived="true"] .agents-title { font-style: italic; }
.agents-project-toggle { flex: none; width: 24px; display: inline-flex; align-items: center; justify-content: center; font: inherit; border: 0; border-radius: var(--mb-radius-4); background: transparent; color: var(--mb-color-text-muted); padding: 0; }
.agents-project-toggle:hover:not(:disabled), .agents-project-open:hover:not(:disabled) { background: var(--mb-color-hover); }
.agents-project-open { flex: 1 1 0; min-width: 0; display: flex; align-items: center; gap: var(--mb-space-2); font: inherit; text-align: left; padding: var(--mb-space-1) var(--mb-space-2); border: 0; border-radius: var(--mb-radius-4); background: transparent; color: var(--mb-color-text); }
.agents-project[data-selected="true"] .agents-project-open { background: var(--mb-color-selected); }
.agents-project:hover .agents-actions, .agents-project:focus-within .agents-actions { display: inline-flex; }
.agents-project:hover .agents-dot, .agents-project:focus-within .agents-dot { visibility: hidden; }
.agents-newproject { position: absolute; left: 0; right: 0; display: flex; align-items: center; padding: 0 var(--mb-space-3); box-sizing: border-box; }
.agents-newproject-btn { font-size: var(--mb-type-chrome-size); }
.agents-row[draggable="true"] { cursor: grab; }
/* The row whose menu is open paints over the rows and headings that follow it (its actions form their own stacking context). */
.agents-row:hover, .agents-row:focus-within, .agents-project:hover, .agents-project:focus-within { z-index: 3; }

.proj-form, .wt-dialog { display: flex; flex-direction: column; gap: var(--mb-space-3); }
.proj-field { display: flex; flex-direction: column; gap: var(--mb-space-1); font-size: var(--mb-type-chrome-size); }
.proj-field input[type="text"], .proj-field select, .proj-field textarea { font: inherit; padding: var(--mb-space-1) var(--mb-space-2); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); color: var(--mb-color-text); }
.proj-appearance { display: flex; align-items: flex-end; gap: var(--mb-space-3); }
.proj-appearance .proj-field { flex: 1 1 0; }
.proj-preview { display: inline-flex; align-items: center; padding-bottom: var(--mb-space-1); }
.proj-problem { margin: 0; padding: var(--mb-space-1) var(--mb-space-2); border-radius: var(--mb-radius-3); background: var(--mb-color-danger-tint); color: var(--mb-color-text); border: 1px solid var(--mb-color-danger); }

.proj-page, .wt-page { padding: var(--mb-space-4) var(--mb-space-5); display: flex; flex-direction: column; gap: var(--mb-space-3); overflow-y: auto; min-height: 0; flex: 1 1 0; }
.proj-head, .wt-head { display: flex; align-items: center; gap: var(--mb-space-3); flex-wrap: wrap; }
.proj-title { margin: 0; font-size: var(--mb-type-title-size, 18px); }
.proj-actions { margin-left: auto; display: inline-flex; gap: var(--mb-space-2); }
.proj-h2 { margin: 0; font-size: var(--mb-type-chrome-size); color: var(--mb-color-text-muted); font-weight: 600; }
.proj-rollup { background: var(--mb-color-panel); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-5); padding: var(--mb-space-3); display: flex; flex-direction: column; gap: var(--mb-space-2); }
.proj-rollup p { margin: 0; }
.proj-counts { list-style: none; margin: 0; padding: 0; display: flex; flex-wrap: wrap; gap: var(--mb-space-2) var(--mb-space-4); }
.proj-counts li { display: inline-flex; align-items: center; gap: var(--mb-space-1); }
.proj-facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(160px, 1fr)); gap: var(--mb-space-2); margin: 0; }
.proj-facts div { display: flex; flex-direction: column; }
.proj-facts dt { font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.proj-facts dd { margin: 0; }
.proj-members { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.proj-member { display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1) var(--mb-space-2); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-4); background: var(--mb-color-panel); }
.proj-member-open { font: inherit; border: 0; background: transparent; color: var(--mb-color-accent-text); text-decoration: underline; padding: 0; text-align: left; }
.proj-member-open:hover:not(:disabled) { background: transparent; }
.proj-member .meta { flex: 1 1 0; min-width: 0; }

.wt-attention { border: 1px solid var(--mb-color-warn); background: var(--mb-color-warn-tint); border-radius: var(--mb-radius-5); padding: var(--mb-space-2) var(--mb-space-3); }
.wt-attention ul { margin: var(--mb-space-1) 0 0; padding-left: var(--mb-space-4); }
.wt-group { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.wt-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-2); }
.wt-row { background: var(--mb-color-panel); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-5); padding: var(--mb-space-2) var(--mb-space-3); display: flex; flex-direction: column; gap: var(--mb-space-1); }
.wt-row[data-running="true"] { border-color: var(--mb-color-info); }
.wt-main { display: flex; align-items: center; gap: var(--mb-space-2); flex-wrap: wrap; }
.wt-row p { margin: 0; }
.wt-path { overflow-wrap: anywhere; font-family: var(--mb-font-mono); }
.wt-actions { display: flex; gap: var(--mb-space-2); flex-wrap: wrap; margin-top: var(--mb-space-1); }

.apply { display: flex; flex-direction: column; gap: var(--mb-space-3); max-height: 70vh; overflow-y: auto; }
.apply-h { margin: 0; font-size: var(--mb-type-body-size); }
.apply-paths { border-collapse: collapse; width: 100%; font-size: var(--mb-type-chrome-size); }
.apply-paths th, .apply-paths td { text-align: left; padding: var(--mb-space-0_5) var(--mb-space-2); border-bottom: 1px solid var(--mb-color-border); }
.apply-paths tr[data-state="CONFLICT"] td { background: var(--mb-color-danger-tint); }
.apply-options { border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-5); padding: var(--mb-space-2) var(--mb-space-3); display: flex; flex-direction: column; gap: var(--mb-space-2); }
.apply-option { display: flex; align-items: flex-start; gap: var(--mb-space-2); }
.apply-option[data-available="false"] { opacity: 0.75; }
.apply-option[data-destructive="true"] strong { color: var(--mb-color-text); }
.apply-what { display: block; }
.apply-required { margin: 0; padding-left: var(--mb-space-4); font-family: var(--mb-font-mono); font-size: var(--mb-type-caption-size); max-height: 120px; overflow-y: auto; }
.apply-facts { display: grid; grid-template-columns: repeat(auto-fit, minmax(140px, 1fr)); gap: var(--mb-space-2); margin: 0; }
.apply-facts div { display: flex; flex-direction: column; }
.apply-facts dt { font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.apply-facts dd { margin: 0; }
.apply-remember { display: flex; align-items: center; gap: var(--mb-space-2); }
`;
