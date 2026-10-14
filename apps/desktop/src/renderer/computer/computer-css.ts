/**
 * Styles of native computer control's desktop surface (PX-074): the presence
 * bar and its Stop, the permission window, the action cards and the approval
 * card's body. Written against the design tokens only: no literal colour
 * appears here. State is never colour alone: each carries words and a glyph in
 * the markup, and the colours below only reinforce them.
 */
export const computerCss = `
.cc-spacer { flex: 1 1 auto; }
.cc-fine { margin: 0; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.cc-fine code, .cc-card code, .cc-appr code, .cc-perm code { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); }
.cc-bar { flex: none; display: flex; flex-direction: column; gap: var(--mb-space-1); padding: var(--mb-space-1) var(--mb-space-3); background: var(--mb-color-warn-tint); color: var(--mb-color-text); border-bottom: 2px solid var(--mb-color-warn); }
.cc-entry { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); font-size: var(--mb-type-chrome-size); }
.cc-entry[data-state="USER_ABORTED"] { }
.cc-what { min-width: 0; overflow-wrap: anywhere; }
.cc-what q { font-weight: 600; }
.cc-bar .cc-note { margin: 0; color: var(--mb-color-text); font-size: var(--mb-type-caption-size); }
.cc-bar .agents-link { color: var(--mb-color-text); text-decoration: underline; background: none; border: 0; padding: 0; font: inherit; cursor: pointer; }

.cc-perm { display: flex; flex-direction: column; gap: var(--mb-space-3); }
.cc-perm p { margin: 0; }
.cc-perm-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-2); }
.cc-perm-row { display: flex; flex-direction: column; gap: var(--mb-space-1); padding: var(--mb-space-2) var(--mb-space-3); border: 1px solid var(--mb-color-border); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); }
.cc-perm-row[data-status="ready"] { border-color: var(--mb-color-ok); }
.cc-perm-row[data-status="allow"] { border-color: var(--mb-color-warn); }
.cc-perm-head { display: flex; align-items: center; justify-content: space-between; gap: var(--mb-space-2); }
.cc-perm-actions { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); }
.cc-dialog-actions { display: flex; justify-content: flex-end; gap: var(--mb-space-2); margin-top: var(--mb-space-3); }

.cc-card { display: flex; flex-direction: column; align-items: stretch; gap: var(--mb-space-1); }
.cc-card-head { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); }
.cc-num { display: inline-flex; align-items: center; justify-content: center; min-width: 1.6em; height: 1.6em; padding: 0 var(--mb-space-1); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-5); font-size: var(--mb-type-caption-size); font-variant-numeric: tabular-nums; }
.cc-card-app q, .cc-card-target { overflow-wrap: anywhere; }
.cc-card-facts { margin: 0; display: grid; grid-template-columns: max-content minmax(0, 1fr); gap: var(--mb-space-0_5) var(--mb-space-3); font-size: var(--mb-type-chrome-size); }
.cc-card-facts dt { color: var(--mb-color-text-muted); }
.cc-card-facts dd { margin: 0; min-width: 0; overflow-wrap: anywhere; }
.cc-card[data-status="danger"] { border-left: 3px solid var(--mb-color-danger); }
.cc-card[data-status="warn"] { border-left: 3px solid var(--mb-color-warn); }
.cc-frame { margin: 0; display: flex; flex-direction: column; gap: var(--mb-space-1); }
.cc-frame img { max-width: 100%; height: auto; border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-3); background: var(--mb-color-sunken); align-self: flex-start; }
.cc-frame .cc-frame-full { max-width: 100%; }
.cc-latch { align-items: center; }

.cc-appr { display: flex; flex-direction: column; gap: var(--mb-space-2); }
`;
