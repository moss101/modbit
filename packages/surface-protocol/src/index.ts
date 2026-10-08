/**
 * @modbit/surface-protocol — generated SurfaceProtocol/API TypeScript types.
 *
 * Canonical owner: core-runtime owner (protocol). Schema source is
 * `crates/protocol/proto/modbit/v1/*.proto`; `src/gen` is produced by
 * `pnpm generate` (M0.3) and must match the source (CI verifies).
 *
 * Only types and codecs live here. No transport, authentication or command
 * handling exists in this package; that arrives with M1.3 behind Core.
 */
export * from "./gen/modbit/v1/domain_pb.js";
export * from "./gen/modbit/v1/envelope_pb.js";
export * from "./gen/modbit/v1/tool_pb.js";
export * from "./gen/modbit/v1/output_ref_pb.js";
export * from "./gen/modbit/v1/negotiation_pb.js";
export * from "./gen/modbit/v1/surface_pb.js";
export * from "./gen/modbit/v1/stream_pb.js";
export * from "./gen/modbit/v1/terminal_pb.js";
export * from "./gen/modbit/v1/tasking_pb.js";
export * from "./gen/modbit/v1/run_control_pb.js";
export * from "./gen/modbit/v1/checkpoints_pb.js";
export * from "./gen/modbit/v1/context_knowledge_pb.js";
export * from "./gen/modbit/v1/conversation_search_pb.js";
export * from "./gen/modbit/v1/terminal_control_pb.js";
export * from "./gen/modbit/v1/skills_budgets_pb.js";
export * from "./gen/modbit/v1/slash_inventory_pb.js";
export * from "./gen/modbit/v1/security_runtime_pb.js";
export * from "./gen/modbit/v1/worktrees_pb.js";
export * from "./gen/modbit/v1/browser_runtime_pb.js";
export * from "./gen/modbit/v1/composer_pb.js";

/** The protocol version this build speaks (mirrors `modbit_protocol::PROTOCOL_VERSION`). */
export const PROTOCOL_VERSION = { major: 1, minor: 0 } as const;
export * from "./gen/modbit/v1/workspace_files_pb.js";
