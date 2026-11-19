/**
 * The Review page in the page shape (REQ-PX-208, DR-PX-2026-11-11-021 MWS-14): a head the bar's height with the one primary
 * action, the list at the left with rows of at least 44 px and a 1 px edge, the chosen task's change set, findings and comments
 * beside it. A task's own review keeps its body and takes the same head. Sizes are tokens or steps from them (MWS-03).
 */
export const reviewCss = `
.page.review-page { padding: 0; display: flex; flex-direction: column; overflow: hidden; font-size: var(--mb-type-chrome-size); }
.rp-head { flex: none; box-sizing: border-box; height: var(--mb-top-bar); display: flex; align-items: center; gap: var(--mb-space-2); padding: 0 var(--mb-space-2) 0 var(--mb-space-5); border-bottom: 1px solid var(--color-tab-border); }
.rp-spacer { flex: 1 1 auto; }
.rp-body { flex: 1 1 auto; min-height: 0; display: flex; overflow: hidden; }
.rp-col { flex: 0 0 360px; min-width: 280px; box-sizing: border-box; overflow: auto; padding: var(--mb-space-2) var(--mb-space-3) var(--mb-space-5); border-right: 1px solid var(--color-tab-border); }
.rp-detail { flex: 1 1 0; min-width: 0; box-sizing: border-box; overflow: auto; padding: var(--mb-space-3) var(--mb-space-5) var(--mb-space-8); display: flex; flex-direction: column; gap: var(--mb-space-1_5); }
.rp-label { margin: var(--mb-space-3) 0 var(--mb-space-1); font-size: var(--mb-type-caption-size); font-weight: 600; color: var(--mb-color-text-muted); }
.rp-col > .rp-label:first-child { margin-top: var(--mb-space-1); }
.rp-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; }
.rp-row.rp-row { box-sizing: border-box; width: 100%; height: auto; min-height: 44px; display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1_5) var(--mb-space-2); border: 0; border-bottom: 1px solid var(--color-tab-border); border-radius: 0; background: transparent; color: var(--mb-color-text); font: inherit; text-align: left; cursor: pointer; }
.rp-row.rp-row:hover { background: var(--color-hover); }
.rp-row.rp-row[aria-current="true"] { background: var(--color-selected); }
.rp-row.rp-row:focus-visible { outline: 2px solid var(--mb-color-focus); outline-offset: -2px; }
.rp-row > svg { flex: none; color: var(--mb-color-text-muted); }
.rp-row-main { flex: 1 1 0; min-width: 0; display: flex; flex-direction: column; }
.rp-row-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.rp-row-detail { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: var(--mb-type-caption-size); color: var(--mb-color-text-muted); }
.rp-size { flex: none; font-variant-numeric: tabular-nums; font-size: var(--mb-type-caption-size); }
.rp-add { color: var(--ok); }
.rp-del { color: var(--bad); }
.rp-detail-head { display: flex; align-items: center; gap: var(--mb-space-2); }
.rp-detail-title { flex: 1 1 auto; min-width: 0; margin: 0; font-size: var(--mb-type-chrome-size); font-weight: 600; overflow-wrap: anywhere; }
.rp-file, .rp-finding { box-sizing: border-box; min-height: 44px; display: flex; align-items: center; gap: var(--mb-space-2); padding: var(--mb-space-1_5) var(--mb-space-1); border-bottom: 1px solid var(--color-tab-border); }
.rp-path { flex: 1 1 auto; min-width: 0; overflow-wrap: anywhere; font-family: var(--mb-font-mono); font-size: var(--mb-type-caption-size); }
.rp-finding { align-items: flex-start; }
.rp-finding-text { flex: 1 1 auto; min-width: 0; overflow-wrap: anywhere; display: flex; flex-direction: column; gap: 2px; }
.rp-finding-text .rp-path { flex: none; }
.rp-tag { flex: none; min-width: 88px; font-size: var(--mb-type-caption-size); font-weight: 600; color: var(--mb-color-text-muted); }
.rp-finding[data-open="true"] .rp-tag[data-kind="check"], .rp-finding[data-open="true"] .rp-tag[data-kind="forge"], .rp-finding[data-open="true"] .rp-tag[data-kind="rule"] { color: var(--mb-color-danger); }
.rp-actions { display: flex; flex-wrap: wrap; align-items: center; gap: var(--mb-space-2); margin-top: var(--mb-space-2); }
.rp-foot { margin-top: var(--mb-space-3); }
/* A task's own review in the page shape: its head is the bar's height with a 1 px edge under it. */
.review.review { padding: 0 var(--mb-space-5) var(--mb-space-5); }
.review > .review-head { box-sizing: border-box; height: var(--mb-top-bar); margin: 0 calc(-1 * var(--mb-space-5)) var(--mb-space-3); padding: 0 var(--mb-space-2) 0 var(--mb-space-5); gap: var(--mb-space-2); border-bottom: 1px solid var(--color-tab-border); font-size: var(--mb-type-chrome-size); }
.review > .review-head h2 { font-size: var(--mb-type-chrome-size); font-weight: 600; }
.review > .review-head > .meta { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.review > .review-head button { margin-left: 0; }
`;
