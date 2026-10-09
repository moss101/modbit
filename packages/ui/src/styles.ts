/**
 * Component styles for the primitives, written against the design tokens
 * (`--mb-*`) only: no literal colour appears here, so a theme change or the
 * high-contrast mode reaches every primitive. Under prefers-reduced-motion the
 * token stylesheet removes every transition and animation.
 */
export const uiCss = `
.mb-sr-only { position: absolute; width: 1px; height: 1px; margin: -1px; padding: 0; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; border: 0; }
.mb-btn { font: inherit; font-size: var(--mb-type-chrome-size); line-height: var(--mb-type-chrome-line); display: inline-flex; align-items: center; justify-content: center; gap: var(--mb-space-1_5); min-height: var(--mb-control-md); padding: 0 var(--mb-space-3); border-radius: var(--mb-radius-3); border: 1px solid var(--mb-color-border-control); background: var(--mb-color-raised); color: var(--mb-color-text); cursor: pointer; transition: background var(--mb-motion-fast), transform var(--mb-motion-fast); }
.mb-btn[data-size="sm"] { min-height: var(--mb-control-sm); padding: 0 var(--mb-space-2); }
.mb-btn[data-size="lg"] { min-height: var(--mb-control-lg); }
.mb-btn:hover:not(:disabled) { background: var(--mb-color-hover); }
.mb-btn:active:not(:disabled) { transform: scale(var(--mb-press-scale)); }
.mb-btn[data-variant="primary"] { background: var(--mb-color-accent); color: var(--mb-color-text-on-accent); border-color: var(--mb-color-accent); }
.mb-btn[data-variant="primary"]:hover:not(:disabled) { background: var(--mb-color-accent-hover); }
.mb-btn[data-variant="danger"] { color: var(--mb-color-danger); }
.mb-btn[data-variant="ghost"] { border-color: transparent; background: transparent; }
.mb-btn[data-variant="ghost"]:hover:not(:disabled) { background: var(--mb-color-hover); }
.mb-btn[aria-pressed="true"] { background: var(--mb-color-selected); }
.mb-btn:disabled, .mb-btn[aria-disabled="true"] { color: var(--mb-color-text-disabled); cursor: default; border-color: var(--mb-color-border); }
.mb-btn[data-variant="primary"]:disabled { background: var(--mb-color-hover); }
.mb-icon-btn { padding: 0; width: var(--mb-control-md); }
.mb-icon-btn[data-size="sm"] { width: var(--mb-control-sm); }
.mb-icon { display: inline-flex; width: 16px; height: 16px; align-items: center; justify-content: center; }
:where(button, a, input, select, textarea, [tabindex]):focus-visible { outline: 2px solid var(--mb-color-focus); outline-offset: 2px; }
.mb-kbd { font: var(--mb-type-mono-size)/var(--mb-type-mono-line) var(--mb-font-mono); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-2); padding: 0 var(--mb-space-1); color: var(--mb-color-text-muted); background: var(--mb-color-sunken); white-space: nowrap; }
.mb-badge { display: inline-flex; align-items: center; font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); padding: 0 var(--mb-space-1_5); border-radius: var(--mb-radius-full); border: 1px solid var(--mb-color-border-control); color: var(--mb-color-text); background: var(--mb-color-sunken); }
.mb-badge[data-tone="ok"] { background: var(--mb-color-ok-tint); color: var(--mb-color-ok); border-color: var(--mb-color-ok); }
.mb-badge[data-tone="warn"] { background: var(--mb-color-warn-tint); color: var(--mb-color-warn); border-color: var(--mb-color-warn); }
.mb-badge[data-tone="danger"] { background: var(--mb-color-danger-tint); color: var(--mb-color-danger); border-color: var(--mb-color-danger); }
.mb-badge[data-tone="info"], .mb-badge[data-tone="accent"] { background: var(--mb-color-info-tint); color: var(--mb-color-info); border-color: var(--mb-color-info); }
.mb-dot { display: inline-flex; align-items: center; gap: var(--mb-space-1); color: var(--mb-color-text-muted); }
.mb-dot-glyph { display: inline-flex; width: 14px; height: 14px; }
.mb-dot-glyph svg { width: 100%; height: 100%; }
.mb-dot[data-status="ok"] .mb-dot-glyph { color: var(--mb-color-ok); }
.mb-dot[data-status="warn"] .mb-dot-glyph { color: var(--mb-color-warn); }
.mb-dot[data-status="danger"] .mb-dot-glyph { color: var(--mb-color-danger); }
.mb-dot[data-status="info"] .mb-dot-glyph, .mb-dot[data-status="running"] .mb-dot-glyph { color: var(--mb-color-info); }
@keyframes mb-spin { to { transform: rotate(360deg); } }
@keyframes mb-shimmer { from { background-position: 200% 0; } to { background-position: -200% 0; } }
.mb-spin { animation: mb-spin 1s linear infinite; }
.mb-shimmer { background: linear-gradient(90deg, var(--mb-color-text-muted) 30%, var(--mb-color-text) 50%, var(--mb-color-text-muted) 70%) 200% 0 / 200% 100%; -webkit-background-clip: text; background-clip: text; color: transparent; animation: mb-shimmer 2s linear infinite; }
.mb-popover { position: absolute; z-index: var(--mb-z-popover); min-width: 200px; padding: var(--mb-space-1); background: var(--mb-color-raised); color: var(--mb-color-text); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-5); box-shadow: var(--mb-elevation-2); }
.mb-menu-host { position: relative; display: inline-flex; }
.mb-menu[data-placement="down"] { top: calc(100% + var(--mb-space-1)); }
.mb-menu[data-placement="up"] { bottom: calc(100% + var(--mb-space-1)); }
.mb-menu[data-align="start"] { left: 0; }
.mb-menu[data-align="end"] { right: 0; }
.mb-menu-item { display: flex; align-items: center; gap: var(--mb-space-2); min-height: var(--mb-control-md); padding: 0 var(--mb-space-2); border-radius: var(--mb-radius-3); cursor: pointer; font-size: var(--mb-type-chrome-size); }
.mb-menu-item:hover, .mb-menu-item:focus { background: var(--mb-color-hover); }
.mb-menu-item[aria-disabled="true"] { color: var(--mb-color-text-disabled); cursor: default; }
.mb-menu-label { flex: 1; white-space: nowrap; }
.mb-menu-check { width: 14px; }
.mb-menu-sep { height: 1px; margin: var(--mb-space-1) 0; background: var(--mb-color-border); }
.mb-tooltip-host { position: relative; display: inline-flex; }
.mb-tooltip { position: absolute; z-index: var(--mb-z-tooltip); top: calc(100% + var(--mb-space-1)); left: 50%; transform: translateX(-50%); white-space: nowrap; padding: var(--mb-space-1) var(--mb-space-2); border-radius: var(--mb-radius-3); background: var(--mb-color-inverse); color: var(--mb-color-text-on-inverse); font-size: var(--mb-type-caption-size); line-height: var(--mb-type-caption-line); box-shadow: var(--mb-elevation-1); }
.mb-tooltip[hidden] { display: none; }
.mb-scrim { position: fixed; inset: 0; z-index: var(--mb-z-modal); display: flex; align-items: flex-start; justify-content: center; padding-top: 12vh; background: color-mix(in srgb, var(--mb-color-scrim) 55%, transparent); }
.mb-dialog { width: min(560px, calc(100vw - 32px)); max-height: 76vh; overflow: auto; background: var(--mb-color-raised); color: var(--mb-color-text); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-6); box-shadow: var(--mb-elevation-3); padding: var(--mb-space-4); }
.mb-dialog[data-size="sm"] { width: min(400px, calc(100vw - 32px)); }
.mb-dialog[data-size="lg"] { width: min(760px, calc(100vw - 32px)); }
.mb-dialog-title { margin: 0 0 var(--mb-space-3); font-size: var(--mb-type-title-size); line-height: var(--mb-type-title-line); }
.mb-tablist { display: flex; gap: var(--mb-space-1); border-bottom: 1px solid var(--mb-color-border); }
.mb-tab { font: inherit; font-size: var(--mb-type-chrome-size); min-height: var(--mb-control-lg); padding: 0 var(--mb-space-3); background: transparent; color: var(--mb-color-text-muted); border: 0; border-bottom: 2px solid transparent; cursor: pointer; }
.mb-tab[aria-selected="true"] { color: var(--mb-color-text); border-bottom-color: var(--mb-color-accent); }
.mb-tab:disabled { color: var(--mb-color-text-disabled); cursor: default; }
.mb-tabpanel { padding: var(--mb-space-3) 0; }
.mb-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: var(--mb-space-0_5); }
.mb-row { font: inherit; display: flex; align-items: center; gap: var(--mb-space-2); width: 100%; min-height: 30px; padding: var(--mb-space-1) var(--mb-space-2); border: 0; border-radius: var(--mb-radius-4); background: transparent; color: var(--mb-color-text); text-align: left; cursor: pointer; }
.mb-row:hover { background: var(--mb-color-hover); }
.mb-row[aria-current="true"] { background: var(--mb-color-selected); }
.mb-row-text { display: flex; flex-direction: column; min-width: 0; flex: 1; }
.mb-row-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-chrome-size); }
.mb-row-subtitle { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.mb-row-trailing, .mb-row-leading { display: inline-flex; align-items: center; color: var(--mb-color-text-muted); font-size: var(--mb-type-caption-size); }
.mb-tray-host { display: flex; flex-direction: column; gap: var(--mb-space-1); padding: 0 var(--mb-space-3); z-index: var(--mb-z-tray); }
.mb-tray-host:empty, .mb-tray-host[data-count="0"] { display: none; }
.mb-tray { background: var(--mb-color-raised); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-5); padding: var(--mb-space-2) var(--mb-space-3); box-shadow: var(--mb-elevation-1); }
.mb-tray[data-tone="error"] { background: var(--mb-color-danger-tint); border-color: var(--mb-color-danger); }
.mb-tray[data-tone="warn"] { background: var(--mb-color-warn-tint); border-color: var(--mb-color-warn); }
.mb-tray[data-tone="attention"], .mb-tray[data-tone="info"] { background: var(--mb-color-info-tint); border-color: var(--mb-color-info); }
.mb-tray-head { display: flex; align-items: center; justify-content: space-between; gap: var(--mb-space-2); }
.mb-tray-body { margin-top: var(--mb-space-1); color: var(--mb-color-text); }
.mb-tray-actions { display: flex; gap: var(--mb-space-2); margin-top: var(--mb-space-2); flex-wrap: wrap; }
.mb-tray-chip { font: inherit; font-size: var(--mb-type-caption-size); text-align: left; padding: var(--mb-space-1) var(--mb-space-3); border: 1px solid var(--mb-color-border-control); border-radius: var(--mb-radius-full); background: var(--mb-color-raised); color: var(--mb-color-text); cursor: pointer; }
.mb-tray-chip-tone { color: var(--mb-color-text-muted); }
`;
