/** Settings' additions for REQ-PX-208: the list options under Appearance and the Shortcuts section. Sizes are tokens. */
export const settingsCss = `
.settings-select { font: inherit; font-size: var(--mb-type-chrome-size); height: var(--mb-control-sm); padding: 0 var(--mb-space-2); border: 1px solid var(--mb-color-border-control); border-radius: var(--radius-md); background: var(--color-panel); color: var(--color-foreground); }
.settings-shortcuts { display: flex; flex-direction: column; gap: var(--mb-space-2); }
.settings-shortcut-group { display: flex; flex-direction: column; gap: var(--mb-space-1); }
.settings-subheading { margin: var(--mb-space-2) 0 0; font-size: var(--mb-type-caption-size); font-weight: 600; color: var(--mb-color-text-muted); }
.settings-shortcut-list { list-style: none; margin: 0; padding: 0; }
.settings-keys { flex-wrap: wrap; justify-content: flex-end; gap: var(--mb-space-1); }
`;
