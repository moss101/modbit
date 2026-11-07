// Opens a URL in the Electron that Modbit ships (apps/desktop's pinned electron), so every capture and every comparison
// uses the same Chromium. Used by capture.mjs, tokens.mjs and token-rgba.mjs; never part of the product.
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { _electron as electron } from "../../../apps/desktop/node_modules/@playwright/test/index.mjs";

const require = createRequire(new URL("../../../apps/desktop/package.json", import.meta.url));
const MAIN = fileURLToPath(new URL("./ref-shell/main.cjs", import.meta.url));

export async function withPage(url, { w = 1100, h = 900, scheme = "light" } = {}, fn) {
  const app = await electron.launch({
    executablePath: require("electron"),
    args: [MAIN],
    env: { ...process.env, REF_URL: url, REF_W: String(w), REF_H: String(h), REF_SCHEME: scheme },
  });
  try {
    const page = await app.firstWindow();
    await page.waitForLoadState("networkidle");
    return await fn(page, app);
  } finally {
    await app.close();
  }
}
