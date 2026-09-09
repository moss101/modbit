# Task Card — M1.3 Implement local authenticated SurfaceProtocol

## Identity

- Task ID: M1.3
- Milestone: M1
- Canonical owner: core-runtime owner. Wire types, framing and client in `modbit-protocol` (doc 81 `protocol-state-wire`); server in the `modbit-core` daemon (`services/modbit-core`, the deployment unit named in docs/11 "Deployment units"); thin client `modbit-cli` (docs/29)
- Requirement IDs: docs/30 "Local SurfaceProtocol" (framed Protobuf over Unix socket / named pipe; boot-scoped secret; command envelope; idempotent commands); docs/33 startup steps 4 and 10; docs/29 thin-client rules. Capabilities registered against REQ-EV-0103 (narrow authenticated IPC), REQ-EV-0108 (bounded IPC), REQ-EV-0192 (multi-client event model), REQ-EV-0010 (offset resume) — their own IMP tasks follow and will cite this substrate
- Risk class: high (security boundary, protocol)
- Evidence tier: release-critical
- Release membership: ALPHA, BETA, RELEASE_ZERO

## Goal

A spawned Core prints a one-line ready message carrying its endpoint and a boot-scoped random secret; only a client presenting that secret is served; frames are bounded; session/task commands are idempotent by `command_id`; any number of clients subscribe to a session's events and resume losslessly from an offset.

## Non-goals

- Electron main/preload wiring (M1.4); snapshot/replay recovery orchestration (M1.5); the remaining docs/30 commands (their milestones); cloud HTTP/WSS (M8); long-running Core attach from the CLI (PX-000).

## Required reading

`AGENTS.md`, `docs/11` (deployment units), `docs/29`, `docs/30`, `docs/32` (security settings), `docs/33`, `docs/81`.

## Existing-code audit

- classification: NOT-FOUND (wire types only)
- production entry point: `modbit-core --data-dir` → `server::run`; `modbit-cli` → `Client`
- real effector/storage boundary: real Unix socket (0600, in a private 0700 runtime dir) / Windows named pipe; `core.db` through the M1.2 store
- tests found: none
- first missing/broken link: no framing
- duplicate/drift risks: a second command path bypassing the idempotency ledger; prevented by routing every mutating command through `EventStore::execute_command`

## Invariants

- The boot secret leaves the process only via the ready line to the spawner; comparison is constant-time; a wrong or missing secret gets an `UNAUTHENTICATED` error and a closed connection; each Core start mints a new secret and endpoint.
- First frame must be `ClientHello`; anything else, malformed bytes, or a frame above 4 MiB ends the connection with a typed `ProtocolError` before allocation.
- Protocol major mismatch → `HelloAck{compatible:false, upgrade_required:true}` and no commands.
- `CreateSession`/`CreateTask` derive their aggregate id from `command_id`, so a replay names the same aggregate and returns `REPLAYED`; a reused id with a different request is `IDEMPOTENCY_CONFLICT`.
- Subscriptions deliver `offset > after_offset` in bounded batches of 256, then live; multiple clients see the same stream.
- One Core per data directory (`core.lock` with liveness check).
- Sockets live in a short private runtime dir, never in the data dir (SUN_LEN).

## Implementation slice

- domain/API: `proto/modbit/v1/surface.proto` (`SurfaceFrame`, `ClientHello`/`Auth`, `CreateSession`, `CreateTask`, `GetSessionSnapshot`, `SubscribeEvents`, `CommandAck`, results, `StoredEventFrame`, `ProtocolError`); TS bindings regenerated
- service/effector: `modbit-protocol::{framing, local, client}`; `services/modbit-core` (`server.rs`); `apps/cli`
- failure/recovery: typed rejections; connection-level protocol errors; kill/restart proven
- evidence/observability: connection end reasons logged to stderr

## Verification

- unit: framing round trip / oversized / malformed / empty; ready line; endpoint length
- integration (real binary, real socket): commands, two clients, live delivery, disconnect and resume from offset, replay and conflict, snapshot, unknown session, unsupported command; wrong/empty secret, pre-handshake command, malformed bytes, oversized frame after handshake with the Core still healthy, major mismatch; hard kill of the Core, second instance refused, restart with new secret, old secret refused, data intact, replay recognised after restart
- CLI smoke (real `modbit-cli` spawning real `modbit-core`, long data dir): create, task, show, tail from 0 and from 2, bad id
- E2E: hosted CI on macOS/Linux (Unix sockets) and Windows (named pipes)

## Completion evidence

See `evidence.json`.
