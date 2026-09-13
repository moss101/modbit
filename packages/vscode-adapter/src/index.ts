/**
 * @modbit/vscode-adapter — the first IDE adapter (PX-002; docs/29): a thin
 * SurfaceProtocol client on `@modbit/ide-adapter-core` hosted in the VS Code
 * extension host. `adapter.ts` is the editor-independent part; `extension.ts`
 * adds only VS Code hosting (commands, the task panel, diagnostics
 * forwarding, persisted session and cursor). It owns nothing.
 */
export * from "./adapter.ts";
