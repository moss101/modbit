/**
 * The composer's styles (REQ-PX-054..056), written against the design tokens
 * only: no literal colour appears here, so a theme change or the high-contrast
 * mode reaches every part of it. The raw buttons override the legacy global
 * `button` rule explicitly, including its hover state.
 */
export const composerCss = `
.cmp { flex: none; display: flex; flex-direction: column; gap: var(--mb-space-2); padding: var(--mb-space-2) var(--mb-space-3) var(--mb-space-3); border-top: 1px solid var(--mb-color-border); background: var(--mb-color-bg); }
.cmp > * { width: 100%; max-width: 840px; margin-inline: auto; box-sizing: border-box; }
.cmp-dock { display: flex; flex-direction: column; gap: var(--mb-space-1_5); }
.cmp-dock:empty { display: none; }
.cmp-chips { display: flex; flex-wrap: wrap; gap: var(--mb-space-1_5); align-items: center; }
.cmp-chips:empty { display: none; }
.cmp-box { position: relative; border: 1px solid var(--mb-color-border-control); border-radius: 17px; background: var(--mb-color-raised); padding: var(--mb-space-1_5) var(--mb-space-2); display: flex; flex-direction: column; gap: var(--mb-space-1); }
.cmp-box:focus-within { border-color: var(--mb-color-focus); }
.cmp-box[data-tint="ask"] { border-color: var(--mb-color-mode-ask); }
.cmp-box[data-tint="plan"] { border-color: var(--mb-color-mode-plan); }
.cmp-box[data-tint="debug"] { border-color: var(--mb-color-mode-debug); }
.cmp-box[data-tint="multitask"] { border-color: var(--mb-color-mode-multitask); }
.cmp-box[data-mode-status="unconfirmed"] { border-style: dashed; }
.cmp-row { display: flex; align-items: flex-end; gap: var(--mb-space-1_5); min-width: 0; }
.cmp-row > * { flex: none; }
.cmp-input { position: relative; flex: 1 1 0; min-width: 0; }
.cmp-textarea { display: block; width: 100%; box-sizing: border-box; min-height: 0; max-height: 40vh; resize: none; border: 0; border-radius: var(--mb-radius-3); background: transparent; color: var(--mb-color-text); padding: 9px 6px; line-height: 1.35; }
.cmp-textarea::placeholder { color: var(--mb-color-text-muted); opacity: 1; }
.mb-btn.cmp-plus { width: var(--mb-control-md); padding: 0; border-radius: var(--mb-radius-full); border-color: var(--mb-color-border-control); font-size: 18px; }
.cmp-chip, .cmp-seg { font: inherit; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); display: inline-flex; align-items: center; gap: var(--mb-space-1); min-height: var(--mb-control-sm); padding: 0 var(--mb-space-2); border-radius: var(--mb-radius-full); border: 1px solid var(--mb-color-border-control); background: var(--mb-color-raised); color: var(--mb-color-text); cursor: pointer; }
.cmp-chip:hover:not(:disabled), .cmp-seg:hover:not(:disabled) { background: var(--mb-color-hover); border-color: var(--mb-color-border-control); color: var(--mb-color-text); }
.cmp-seg[aria-pressed="true"] { background: var(--mb-color-selected); border-color: var(--mb-color-accent); }
.cmp-seg[aria-pressed="true"]:hover:not(:disabled) { background: var(--mb-color-selected); }
.cmp-chip:disabled { color: var(--mb-color-text-disabled); cursor: default; }
.cmp-dim { color: var(--mb-color-text-muted); }
.cmp-model { max-width: 240px; white-space: nowrap; }
.cmp-model > span:first-child { overflow: hidden; text-overflow: ellipsis; }
.cmp-mode { display: inline-flex; align-items: center; }
.cmp-mode[data-mode="ASK"] .cmp-mode-btn { color: var(--mb-color-mode-ask); border-color: var(--mb-color-mode-ask); }
.cmp-mode[data-mode="PLAN"] .cmp-mode-btn { color: var(--mb-color-mode-plan); border-color: var(--mb-color-mode-plan); }
.cmp-mode[data-mode="DEBUG"] .cmp-mode-btn { color: var(--mb-color-mode-debug); border-color: var(--mb-color-mode-debug); }
.cmp-mode[data-mode="MULTITASK"] .cmp-mode-btn { color: var(--mb-color-mode-multitask); border-color: var(--mb-color-mode-multitask); }
.mb-btn.cmp-mode-btn { border-radius: var(--mb-radius-full); min-height: var(--mb-control-sm); padding: 0 var(--mb-space-2); background: var(--mb-color-raised); }
.cmp-stop { width: var(--mb-control-md); height: var(--mb-control-md); padding: 0; display: inline-flex; align-items: center; justify-content: center; border-radius: var(--mb-radius-full); border: 1px solid var(--mb-color-border-control); background: var(--mb-color-raised); cursor: pointer; }
.cmp-stop:hover:not(:disabled) { background: var(--mb-color-hover); border-color: var(--mb-color-border-control); }
.cmp-stop span { width: 12px; height: 12px; background: var(--mb-color-text); border-radius: 2px; }
.cmp-stop:disabled { opacity: 0.5; }
.mb-btn.cmp-send { border-radius: var(--mb-radius-full); }
.mb-btn.cmp-send[data-tint="ask"] { background: var(--mb-color-raised); color: var(--mb-color-mode-ask); border: 2px solid var(--mb-color-mode-ask); }
.cmp-attach-row { display: flex; flex-wrap: wrap; gap: var(--mb-space-1); }
.cmp-token { display: inline-flex; align-items: center; gap: var(--mb-space-1); font-size: var(--mb-type-caption-size); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-full); padding: 1px var(--mb-space-2); background: var(--mb-color-sunken); max-width: 100%; }
.cmp-token img { border-radius: var(--mb-radius-2); object-fit: cover; }
.cmp-token button { font: inherit; padding: 0 var(--mb-space-1); border: 0; background: transparent; color: var(--mb-color-text); cursor: pointer; }
.cmp-token button:hover:not(:disabled) { background: var(--mb-color-hover); }
.cmp-notes { margin: 0; padding-left: var(--mb-space-4); font-size: var(--mb-type-caption-size); color: var(--mb-color-danger); }
.cmp-reason { margin: 0; font-size: var(--mb-type-caption-size); color: var(--mb-color-danger); }
.cmp-suggest { display: flex; gap: var(--mb-space-1_5); flex-wrap: wrap; }
.cmp-spacer { flex: 1 1 0; }
.cmp-tray { background: var(--mb-color-raised); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-5); padding: var(--mb-space-1_5) var(--mb-space-3); font-size: var(--mb-type-chrome-size); }
.cmp-attn[data-tone="error"] { background: var(--mb-color-danger-tint); border-color: var(--mb-color-danger); }
.cmp-attn[data-tone="warn"] { background: var(--mb-color-warn-tint); border-color: var(--mb-color-warn); }
.cmp-attn[data-tone="info"] { background: var(--mb-color-info-tint); border-color: var(--mb-color-info); }
.cmp-tray p { margin: var(--mb-space-1) 0; }
.cmp-tray-head { display: flex; align-items: center; gap: var(--mb-space-2); flex-wrap: wrap; }
.cmp-actions { display: flex; flex-wrap: wrap; gap: var(--mb-space-2); align-items: center; margin-top: var(--mb-space-1_5); }
.cmp-lines { margin: var(--mb-space-1) 0; padding-left: var(--mb-space-4); }
.cmp-qlist { list-style: none; margin: var(--mb-space-1) 0 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-0_5); max-height: 168px; overflow: auto; }
.cmp-qrow { display: flex; align-items: center; gap: var(--mb-space-2); min-height: 28px; }
.cmp-qmain { flex: 1 1 0; min-width: 0; display: flex; align-items: center; gap: var(--mb-space-2); text-align: left; font: inherit; font-size: var(--mb-type-chrome-size); padding: 0 var(--mb-space-1); min-height: 26px; border: 1px solid transparent; border-radius: var(--mb-radius-3); background: transparent; color: var(--mb-color-text); cursor: pointer; }
.cmp-qmain:hover:not(:disabled) { background: var(--mb-color-hover); border-color: transparent; color: var(--mb-color-text); }
.cmp-static { cursor: default; }
.cmp-qline { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cmp-qmode { flex: none; max-width: 40%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cmp-qactions { display: inline-flex; gap: var(--mb-space-1); flex: none; }
.cmp-qedit { display: flex; flex: 1 1 0; gap: var(--mb-space-2); align-items: flex-start; }
.cmp-qedit textarea { flex: 1 1 0; min-height: 0; padding: var(--mb-space-1) var(--mb-space-2); }
.cmp-menu { position: absolute; z-index: var(--mb-z-popover); left: 0; right: 0; bottom: calc(100% + 8px); max-height: 300px; overflow: auto; background: var(--mb-color-raised); color: var(--mb-color-text); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-5); box-shadow: var(--mb-elevation-2); padding: var(--mb-space-1); }
.cmp-slash { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 220px); gap: var(--mb-space-2); }
.cmp-list { list-style: none; margin: 0; padding: 0; }
.cmp-group { font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); text-transform: uppercase; letter-spacing: 0.04em; padding: var(--mb-space-1) var(--mb-space-2) 0; }
.cmp-divider { height: 1px; margin: var(--mb-space-1) 0; background: var(--mb-color-border); }
.cmp-option { display: flex; align-items: center; gap: var(--mb-space-2); min-height: 28px; padding: 0 var(--mb-space-2); border-radius: var(--mb-radius-3); cursor: pointer; font-size: var(--mb-type-chrome-size); }
.cmp-option[data-active="true"] { background: var(--mb-color-selected); }
.cmp-option[aria-disabled="true"] .cmp-option-label { color: var(--mb-color-text-muted); }
.cmp-option-label { flex: none; max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.cmp-option-detail { flex: 1 1 0; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; text-align: right; }
.cmp-check { width: 14px; flex: none; }
.cmp-empty { margin: 0; padding: var(--mb-space-2); }
.cmp-detail { border-left: 1px solid var(--mb-color-border); padding: var(--mb-space-1) var(--mb-space-2); font-size: var(--mb-type-chrome-size); overflow-wrap: anywhere; }
.cmp-detail p { margin: var(--mb-space-1) 0; }
.cmp-picker-host { position: relative; display: inline-flex; }
.cmp-picker { position: absolute; z-index: var(--mb-z-popover); right: 0; bottom: calc(100% + 8px); width: 300px; max-height: 420px; overflow: auto; background: var(--mb-color-raised); color: var(--mb-color-text); border: 1px solid var(--mb-color-border-control); border-radius: 14px; box-shadow: var(--mb-elevation-2); padding: var(--mb-space-2); }
.cmp-picker-search { width: 100%; box-sizing: border-box; }
.cmp-auto { margin-top: var(--mb-space-2); }
.cmp-auto-title { margin: 0 0 var(--mb-space-1); }
.cmp-objectives { display: flex; gap: var(--mb-space-1); flex-wrap: wrap; }
.cmp-sep { border: 0; border-top: 1px solid var(--mb-color-border); margin: var(--mb-space-2) 0; }
.cmp-models .cmp-option-label { max-width: 45%; }
.cmp-side-row { display: flex; gap: var(--mb-space-2); }
.cmp-side-row input { flex: 1 1 0; min-width: 0; }
.cmp-side-answer { white-space: pre-wrap; overflow-wrap: anywhere; }
.cmp-fieldset { border: 0; margin: var(--mb-space-1) 0; padding: 0; }
.cmp-radio { display: flex; align-items: center; gap: var(--mb-space-2); min-height: 26px; }
.cmp-stopped-text { overflow-wrap: anywhere; }
`;
