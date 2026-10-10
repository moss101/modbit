import { test } from "node:test";
import assert from "node:assert/strict";
import { workspaceCss } from "./workspace-css.ts";

test("WCAG 1.4.3: new streamed text is marked without ever fading the glyphs (partial opacity is text below AA contrast while it lasts)", () => {
  const keyframes = /@keyframes conv-fade\s*\{([^]*?\})\s*\}/.exec(workspaceCss);
  assert.ok(keyframes, "the fade-in keyframes exist");
  assert.ok(!/opacity|color\s*:|filter/.test(keyframes[1]!.replace(/text-decoration-color/g, "")), `the fade animates only the underline: ${keyframes[1]}`);
  const rule = /\.conv-fresh\s*\{([^}]*)\}/.exec(workspaceCss);
  assert.ok(rule && !/opacity/.test(rule[1]!), "the fresh span itself is never given an opacity");
});
