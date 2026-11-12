/**
 * A key that arrives between a commit and its effects is answered for the screen that is showing. The review opens by itself
 * when a task is ready; the app's key listener used to read the state of the render before (it was re-registered in an effect),
 * so an Escape pressed the moment the review appeared found no review to close and left it open. That was seen twice on the
 * Linux runner (CI runs 43167558623 and 43181893787, in context.spec.ts and wave4.spec.ts). Here the Escape is dispatched from
 * a mutation observer, in the same task as the review's first commit and before any effect has run, on the real app and Core.
 */
import { expect, test } from "@playwright/test";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { streamingModel } from "./support/stream-model.ts";
import { runTask } from "./support/new-task.ts";
import { closeApp, launch, makeRepo, setContentSize } from "./support/ui-harness.ts";

test("Escape pressed the moment the review opens by itself closes the review (the key listener reads the latest render)", async () => {
  const repo = makeRepo(mkdtempSync(join(tmpdir(), "modbit-race-repo-")));
  const model = await streamingModel([
    { parts: [{ call: { name: "plan.update", args: { outcome: "edit notes", expected_files: ["notes.txt"] } } }] },
    { parts: [{ call: { name: "fs.read", args: { path: "notes.txt" } } }] },
    { parts: [{ call: { name: "change.apply", args: { path: "notes.txt", op: "replace", content: "line 1\nline 2 changed\nline 3\n" } } }] },
    { parts: [{ call: { name: "task.complete", args: { summary: "edited", self_review: { findings: [] } } } }] },
  ]);
  const { app, page } = await launch(mkdtempSync(join(tmpdir(), "modbit-e2e-race-")), { env: { MODBIT_OPENAI_BASE_URL: model.url } });
  try {
    await setContentSize(app, page, 1280, 800);
    await page.evaluate(() => {
      const w = window as unknown as { __raced?: string };
      new MutationObserver(() => {
        if (w.__raced || !document.querySelector('[data-testid="review"]')) return;
        w.__raced = "sent";
        document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
      }).observe(document.body, { childList: true, subtree: true });
    });
    await runTask(page, "Edit the notes", repo);
    await expect.poll(() => page.evaluate(() => (window as unknown as { __raced?: string }).__raced), { timeout: 90_000 }).toBe("sent");
    await expect(page.getByTestId("review"), "Escape at the review's first commit closes it").toHaveCount(0, { timeout: 5_000 });
  } finally {
    await closeApp(app);
    model.server.close();
  }
});
