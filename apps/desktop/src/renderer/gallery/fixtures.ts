/**
 * Fixtures for the model-free state gallery (AFW-K02, REQ-PX-044). Every value
 * here is invented for design review: there is no Core, no model and no task
 * behind it, and a gallery state is never evidence that a feature works (every
 * behaviour is proven against a real Core).
 */
export const GALLERY_STATES = ["approvals", "generating", "error", "empty", "offline"] as const;
export type GalleryState = (typeof GALLERY_STATES)[number];

export const APPROVAL_FIXTURE = {
  id: "approval-1",
  intent: "Run `cargo test -p example-crate` in the task worktree (effect class: process, intent a1b2c3d4e5f6…)",
  secondId: "approval-2",
  secondTitle: "Write outside the workspace",
  secondIntent: "Create ../scratch/notes.txt (effect class: filesystem outside the workspace; always asks)",
};

export const GENERATING_FIXTURE = { action: "Editing src/lib.rs", elapsed: "Working for 12 s", steps: ["Read 4 files", "Searched the repository", "Ran 2 commands"] };

export const ERROR_FIXTURE = { id: "turn-error", title: "The turn failed", detail: "The provider did not answer within the time limit. Your prompt is restored; nothing was changed.", requestId: "req-fixture-0001" };

export const EMPTY_FIXTURE = { title: "No tasks yet", detail: "Describe what you want done and Modbit will plan it, run it in an isolated worktree and bring the result back for review." };

export const OFFLINE_FIXTURE = { title: "You are offline", detail: "Open tasks stay readable. Sending fails until the connection returns; nothing you typed is lost." };

export const FIXTURE_ROWS = [
  { id: "r1", title: "Fix the flaky parser test", subtitle: "example/repo", status: "running" as const, age: "2m" },
  { id: "r2", title: "Add the export command", subtitle: "example/repo", status: "warn" as const, age: "9m" },
  { id: "r3", title: "Update the changelog", subtitle: "example/docs", status: "ok" as const, age: "1h" },
];
