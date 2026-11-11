/**
 * The measured workspace's stylesheet rules (DR-PX-2026-11-11-021 MWS-01..03, QUAL-PX-198), checked on the text the build
 * concatenates into app.css: a density is the token block and nothing else, no component writes a text size of its own, and
 * the measured surfaces read their sizes and their surface from the tokens. Each check is shown failing on a mutated sheet.
 */
import { test } from "node:test";
import assert from "node:assert/strict";
import { densityVariables, sharedVariables, tokensCss } from "@modbit/design-tokens";
import { uiCss } from "@modbit/ui/styles";
import { legacyCss, shellCss } from "./styles.ts";
import { workspaceCss } from "./workspace-css.ts";
import { composerCss } from "./composer/composer-css.ts";
import { controlsCss } from "./controls-css.ts";
import { computerCss } from "./computer/computer-css.ts";
import { automationsCss } from "./automations/automations-css.ts";
import { projectsCss } from "./projects/projects-css.ts";

/** Every stylesheet a component owns: all of app.css but the generated token sheet (and xterm's own). */
const COMPONENT_SHEETS: Record<string, string> = { uiCss, legacyCss, shellCss, workspaceCss, composerCss, controlsCss, computerCss, automationsCss, projectsCss };

/** The sheets that select on the shell's density (`data-density="compact"` on the root); the conversation's own density attribute is upper case and is not this. */
const densitySelectors = (sheets: Record<string, string>): string[] => Object.entries(sheets).flatMap(([name, css]) => (/data-density\s*[~|^$*]?=\s*["']?(compact|comfortable)/.test(css) ? [name] : []));

/**
 * Text sizes and line pitches written as a number of px instead of a token. A size that is a token, or a step from one
 * (`calc(var(--ui-font-size) - 2px)`), follows the density; in the `font` shorthand the only token allowed is the family.
 */
function literalTextSizes(css: string): string[] {
  const out: string[] = [];
  for (const m of css.matchAll(/(?:^|[;{\s])(font-size|line-height)\s*:\s*([^;}]*)/g)) if (/\d(?:\.\d+)?px/.test(m[2]!) && !m[2]!.includes("var(--")) out.push(`${m[1]}: ${m[2]!.trim()}`);
  for (const m of css.matchAll(/(?:^|[;{\s])font\s*:\s*([^;}]*)/g)) if (/\d(?:\.\d+)?px/.test(m[1]!.replace(/var\([^)]*\)/g, ""))) out.push(`font: ${m[1]!.trim()}`);
  return out;
}

/** The value a rule gives a property: the rule is the one whose selector list is exactly `selector`. */
function declared(sheet: string, selector: string, property: string): string | null {
  const css = sheet.replace(/\/\*[^]*?\*\//g, "");
  for (const m of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (m[1]!.trim() !== selector) continue;
    const d = new RegExp(`(?:^|;)\\s*${property}\\s*:\\s*([^;]*)`).exec(m[2]!);
    if (d) return d[1]!.trim();
  }
  return null;
}

/** Where each measured size and surface must come from: [sheet, selector, property, the token it reads]. */
const MEASURED: [keyof typeof COMPONENT_SHEETS, string, string, string][] = [
  // MWS-01: the top bar and every pane head are one bar height.
  ["shellCss", ".topbar", "height", "var(--mb-top-bar)"],
  ["shellCss", ".agent-region-head", "height", "var(--mb-top-bar)"],
  ["shellCss", ".apps-panel .mb-tablist", "height", "var(--mb-top-bar)"],
  ["shellCss", ".changes-head", "height", "var(--mb-top-bar)"],
  // Navigation rows: the row height, 2 apart, radius 8, inset 8.
  ["shellCss", ".agent-nav-row.agent-nav-row", "height", "var(--mb-row)"],
  ["shellCss", ".agent-nav-row.agent-nav-row", "border-radius", "var(--mb-row-radius)"],
  ["shellCss", ".agent-nav", "gap", "var(--mb-row-gap)"],
  ["shellCss", ".agent-nav", "padding", "var(--mb-space-1) var(--mb-row-inset) 0"],
  ["shellCss", ".agent-foot", "height", "var(--mb-list-foot)"],
  // Task rows share the row's radius and inset; their heights are the list's layout, which reads the same tokens (list-model.test.ts).
  ["workspaceCss", ".agents-row", "padding", "0 var(--mb-row-inset)"],
  ["workspaceCss", ".agents-open", "border-radius", "var(--mb-row-radius)"],
  // The conversation's column: 768, its text at 736; the composer's cap and radius.
  ["workspaceCss", ".conv-inner", "max-width", "var(--mb-column-text)"],
  ["workspaceCss", ".conv-notice", "max-width", "var(--mb-column)"],
  ["composerCss", ".cmp > *", "max-width", "var(--mb-column)"],
  ["composerCss", ".cmp-box", "border-radius", "var(--mb-composer-radius)"],
  ["controlsCss", ".conv > .mb-tray-host", "max-width", "var(--mb-column)"],
  // MWS-02: the conversation, the top bar and the panel on the editor surface, the list on the sidebar, chrome as the fill.
  ["shellCss", ".shell-frame", "background", "var(--mb-surface-conversation)"],
  ["shellCss", ".shell-panel", "background", "var(--mb-surface-conversation)"],
  ["shellCss", ".shell-list", "background", "var(--mb-surface-list)"],
  ["workspaceCss", ".conv-user", "background", "var(--mb-surface-fill)"],
  ["workspaceCss", ".conv-code", "background", "var(--mb-surface-fill)"],
];

const measuredDrift = (sheets: Record<string, string>): string[] => MEASURED.flatMap(([sheet, selector, property, want]) => {
  const got = declared(sheets[sheet]!, selector, property);
  // The last rule wins where a sheet states a property twice; the measured rules are stated once.
  return got !== null && got.startsWith(want) ? [] : [`${sheet} ${selector} { ${property}: ${got ?? "(not set)"} } is not ${want}`];
});

test("MWS-03: Compact is the token block and nothing else: no component stylesheet selects on the density", () => {
  assert.deepEqual(densitySelectors(COMPONENT_SHEETS), []);
  assert.deepEqual(densitySelectors({ tokens: tokensCss() }), ["tokens"], "the token sheet is where a density lives");
  // The retained mutation: a size written inside a component for one density fails.
  const mutated = { ...COMPONENT_SHEETS, workspaceCss: `${workspaceCss}\n:root[data-density="compact"] .agents-open { height: 30px; }` };
  assert.deepEqual(densitySelectors(mutated), ["workspaceCss"]);
  // The conversation's own density (COMPACT, BALANCED, DETAILED on the conversation) is a different thing and is not caught.
  assert.deepEqual(densitySelectors({ x: '.conv[data-density="COMPACT"] .conv-row { gap: 0; }' }), []);
});

test("MWS-03: every text size and line pitch a component uses is a token, so a density reaches all of it", () => {
  for (const [name, css] of Object.entries(COMPONENT_SHEETS)) assert.deepEqual(literalTextSizes(css), [], name);
  // The retained mutations: a size written inside a component fails, in each of the ways it can be written.
  assert.deepEqual(literalTextSizes(".x { font-size: 13px; }"), ["font-size: 13px"]);
  assert.deepEqual(literalTextSizes(".x { color: red; line-height: 18px }"), ["line-height: 18px"]);
  assert.deepEqual(literalTextSizes(".x { font: 600 11.5px var(--mb-font-mono); }"), ["font: 600 11.5px var(--mb-font-mono)"]);
  assert.deepEqual(literalTextSizes(".x { font: 12px/20px var(--mb-font-mono); }"), ["font: 12px/20px var(--mb-font-mono)"]);
  // A token, a ratio, a calc over a token and `inherit` are not literals.
  assert.deepEqual(literalTextSizes(".x { font: inherit; font-size: var(--mb-type-chrome-size); line-height: 1.5; } .y { font-size: calc(var(--ui-font-size) - 2px); line-height: var(--mb-type-chrome-line); }"), []);
});

test("MWS-01/02: the measured surfaces take their sizes and their surface from the tokens, and a size or surface written in place fails", () => {
  assert.deepEqual(measuredDrift(COMPONENT_SHEETS), []);
  // Every token the rules name is one the token sheet defines, and the density overrides only such names.
  const shared = sharedVariables();
  for (const [, , , want] of MEASURED) for (const name of want.match(/--[a-z0-9-]+/g) ?? []) assert.ok(name in shared, `${name} is a shared token`);
  for (const name of Object.keys(densityVariables("compact"))) assert.ok(name in shared, name);
  // The retained mutations: each superseded number written back into its component is caught.
  const mutate = (sheet: keyof typeof COMPONENT_SHEETS, from: string, to: string) => {
    assert.ok(COMPONENT_SHEETS[sheet]!.includes(from), `${sheet} holds ${from}`);
    return measuredDrift({ ...COMPONENT_SHEETS, [sheet]: COMPONENT_SHEETS[sheet]!.replace(from, to) });
  };
  assert.match(mutate("shellCss", ".topbar { flex: none; box-sizing: border-box; height: var(--mb-top-bar);", ".topbar { flex: none; box-sizing: border-box; height: 40px;").join("\n"), /\.topbar \{ height: 40px \}/);
  assert.match(mutate("shellCss", "height: var(--mb-row); min-height: 0; display: flex; align-items: center; gap: 10px;", "height: 28px; min-height: 0; display: flex; align-items: center; gap: 10px;").join("\n"), /\.agent-nav-row\.agent-nav-row \{ height: 28px \}/);
  assert.match(mutate("workspaceCss", "max-width: var(--mb-column-text);", "max-width: 640px;").join("\n"), /\.conv-inner \{ max-width: 640px \}/);
  assert.match(mutate("composerCss", "max-width: var(--mb-column);", "max-width: 640px;").join("\n"), /\.cmp > \* \{ max-width: 640px \}/);
  assert.match(mutate("composerCss", "border-radius: var(--mb-composer-radius);", "border-radius: var(--radius-xl);").join("\n"), /\.cmp-box \{ border-radius: var\(--radius-xl\) \}/);
  // The superseded mapping (the conversation on chrome, the message on the editor surface) is caught too.
  assert.match(mutate("shellCss", "overflow: hidden; background: var(--mb-surface-conversation); }", "overflow: hidden; background: var(--color-background); }").join("\n"), /\.shell-frame \{ background: var\(--color-background\) \}/);
  assert.match(mutate("workspaceCss", ".conv-user { align-self: stretch; background: var(--mb-surface-fill);", ".conv-user { align-self: stretch; background: var(--color-panel);").join("\n"), /\.conv-user \{ background: var\(--color-panel\) \}/);
});
