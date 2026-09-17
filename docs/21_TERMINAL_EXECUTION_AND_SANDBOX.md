# Terminal, Execution Router, and Sandbox Architecture

> **Authority date:** 2026-09-05  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Execution profiles

- `local_trusted` — runs against user-approved local workspace under host policy.
- `cloud_isolated` — runs inside tenant-bound isolated MicroVM.
- `review_isolated` — Isolated Non-Committing Reviewer profile (ADR-R-053, EPR-018). Processes run only inside a disposable review worktree: the canonical tracked tree is read-only, ephemeral writes are confined to that worktree, network and secret handles are denied by default, and Git commit/push, deploy and every external or persistent effect are refused. The process/tool budget is bounded and priced into the plan. The profile is realized by the same Execution Router on top of `local_trusted` or `cloud_isolated` isolation primitives and adds no broker or gateway; accept, cancel, timeout or crash kills its processes, revokes its handles and disposes the worktree. Trusted Core may export bounded evidence from the worktree through the existing artifact owner; the reviewer itself cannot persist anything.
- `review_isolated` as built (EPR-018): the broker confines every process of this profile with the host's real sandbox (macOS `sandbox-exec`: no network, writes only under the worktree and the temp dirs; Linux: a seccomp filter the broker installs by re-executing itself as the launcher — `socket(2)` refused for every family but `AF_UNIX`, `io_uring_setup(2)` refused, locked in with `no_new_privs`; no network, no user namespace needed; `AF_UNIX` stays open, so a local daemon with network of its own is reachable over its socket, as under a network namespace) and starts it with no inherited environment and no secret-looking variable; a host with no sandbox refuses the process (`SANDBOX_UNAVAILABLE`) and the Core admits no review environment there (`ProbeSandbox`). The Core provisions the disposable worktree, the review task and the confined lease (docs/27 "As built (EPR-018)").
- `plan` — plan mode (REQ-EV-0117, as built): a `READ_ONLY` ceiling and a lease of `fs.read` / `git.read`; no write, shell or worktree tool is compiled into its surface and the policy port refuses any non-read effect under it; the task's product is its recorded plan, reviewed like any candidate (docs/14 "As built (REQ-EV-0117)").
- Future profiles may be added through the same Execution Router; no tool changes required.

## Structured command contract

```text
ExecRequest {
  argv[]
  cwd
  env_handles[]
  timeout_ms
  pty: bool
  stdin_mode
  output_budget_bytes
  execution_profile
  capability_lease_id
  terminal_session_id?
}

ExecResult {
  process_id
  exit_code?
  signal?
  stdout_ref
  stderr_ref
  duration_ms
  terminal_cursor
  effect_receipt_id?
  checkpoint_before?
  checkpoint_after?
}
```

Shell-string convenience is parsed into an argv-aware request and clearly marked; internal deterministic tools prefer argv.

## Durable `modbit-execd`

A small broker owns local PTYs/processes and a bounded replay log. Core sends authenticated commands over local socket. UI can detach/reconnect without losing output. Core acknowledges output cursor; broker retains a sliding replay window and spills large output to OutputRef/object store. Broker is not authorized to create capabilities or decide policy. Processes started under `review_isolated` carry their review worktree and plan/leg IDs so cleanup and accounting are exact.

As built (M4.5): the broker outlives the Core (doc 33: durable terminal resources are detached, never killed). It writes an owner-only `execd.ready` in its data directory; a restarted Core reattaches through it instead of spawning a second broker, so a running command keeps running with the same handle and its output resumes from the cursor the run acknowledged (doc 51 E2E-008). Every session's metadata lives beside its log, so a broker that is itself killed (doc 54 fault 12) comes back knowing its sessions: those still running when it died are `LOST` — exit unknown, log replayable and sealed under an OutputRef — and a retry of the same request replays them rather than starting again. A Core holds one connection open for its lifetime; a broker with no Core connected past its orphan grace (`--orphan-grace-secs`, 60 by default) stops its processes as cancelled and exits. Attaches carry the terminal replay generation (doc 13; the Core's boot generation): an older reader is refused `STALE_GENERATION` and a newer one ends older attachments. Because the broker outlives the Core, it must inherit nothing from whoever spawned the Core: before spawning it the Core makes every handle it holds non-inheritable (Windows handle flags, Unix `FD_CLOEXEC`), so a supervising client waiting for the Core's pipes to close never waits for the broker's orphan grace instead; the broker's own stdin is null and its stderr is the profile's `execd/execd.log`. The Core records `TerminalCreated`, `TerminalOutputAdvanced` and `ProcessExited` on the task, and the protocol state (doc 19 layer 2) carries each handle with its last acknowledged cursor.

## Command failure semantics

Non-zero exit is a valid tool result. It emits `CommandExited` with status and output; Agent Runtime may inspect, repair and retry. `ToolCallFailed` means execution infrastructure/schema/policy failure. `TurnFailed` occurs only when runtime can no longer make progress under policy/budget.

## Sandbox substrate boundary

Cloud isolated execution uses a Modbit-owned Sandbox Gateway in front of the MicroVM substrate:
- authenticated tenant-bound sandbox lease;
- deny-by-default sandbox-to-internal network;
- explicit egress domains/ports by capability;
- dynamic credential handles via broker injection;
- protected filesystem paths;
- typed guest RPC with task/turn/call/effect IDs;
- resource quotas and emergency stop;
- guest image is immutable/versioned and contains no tenant secrets.

As built (M8.3): the boundary is `crates/sandbox` (the canonical `sandbox-gateway-abstraction`): a `SandboxSpec` (tenant, session, task, the workspace source, protected paths, egress grants, resource bounds) compiles to a `CompiledPolicy` — substrate-side, whether the guest gets a network interface at all (none unless egress is granted) and the mounts; guest-side, the one writable tree (`/workspace`), the protected paths (the guest's control paths `/init`, `/etc`, `/proc`, `/sys`, `/dev`, `/run/modbit` always, plus the spec's under the workspace), the readable roots, the output, process and timeout bounds. A `SandboxBackend` boots `modbit-guest` under that policy and returns a private byte channel; `GuestLink` negotiates (the guest's hello — protocol version, methods; another major or a missing required method is refused before admission), admits it with a fresh 32-byte credential and the policy, and thereafter sends typed calls each signed with an HMAC over the call under that credential, verifying the signed replies; a call id is a nonce the guest answers once. Two backends: the MicroVM backend drives Firecracker over KVM through its API socket — the kernel and the immutable root image are the gateway's (the root is attached read-only; it is `modbit-guest` as `/init` over a static BusyBox userland, built by `tools/guest-image/build.sh`, no tenant data, no secret), the workspace is a per-sandbox ext4 image built from the workspace source and attached read-write, the channel is vsock; an egress grant is refused rather than served open (a tap device and a host firewall arrive with M8.6), so a guest has no network interface and the control plane is unreachable from inside. The reference backend runs the same guest as a child process on the host over loopback: it implements the whole contract so the conformance suite runs anywhere and isolates nothing — it says so (`isolated: false`) and a gateway serves it only when configured to. The conformance suite (`modbit_sandbox::conformance`) runs the same steps on both: negotiation, health, exec with exit code, stdin, timeout, timeout ceiling and output bound, the workspace present, write-then-read, a protected path, a path outside the workspace, a traversal and a control path refused, the control plane unreachable (asserted of an isolating backend), an unsigned and a replayed call refused, destroy twice. Proven on a real Firecracker MicroVM in the hosted `cloud` job and on the reference backend on every OS (`apps/sandbox-gateway/tests/gateway.rs`).

As built (M8.4, signed and versioned guest images): a publisher signs an `ImageManifest` (`modbit_sandbox::image`) — the image's SHA-256 and size, the guest version and protocol it carries, the kernel it was built for, its provenance — with an Ed25519 key (`guest-image-tool keygen | sign | verify`; the signing key never reaches a gateway). A gateway is configured with the publisher keys it trusts (`MODBIT_GUEST_IMAGE_KEYS`) and the signed manifest of its root image (`MODBIT_GUEST_IMAGE_MANIFEST`; the reference guest binary likewise, `MODBIT_GUEST_BIN_MANIFEST`): the MicroVM backend refuses to start (`IMAGE_UNVERIFIED`) unless the manifest verifies under a trusted key, the image on disk hashes to what it names and the kernel is the one it was built for, and it re-checks the image's hash before every boot; the guest that comes up must report the version and protocol the manifest names or it is refused at admission (`IMAGE_VERSION_MISMATCH`) before any credential is issued. Every sandbox the gateway issues names the verified image (`image: {kind, sha256, guest_version}`). The hosted `cloud` job mints a publisher key per run, signs the root image and the reference guest it built, and proves: the signed image boots and the guest is the one named; a manifest under another key, a tampered image and a manifest naming another guest version are refused — the last only after the guest said who it is.

## `modbit-guest`

Guest RPC methods are narrow: process start/wait/cancel, PTY attach/replay, file operations within mounted workspace, Git helpers, artifact upload/download, browser endpoint control and health. Gateway supplies a capability token scoped to sandbox lease and call.

As built (M8.3): `services/modbit-guest` — as `/init` of a MicroVM it mounts `/proc`, `/sys`, `/dev`, `/tmp`, `/run` and the workspace block device the kernel command line names at `/workspace`, sets the hostname and listens on the vsock port the command line names; as the reference backend's child it listens on loopback and maps `/workspace` onto a host directory. It speaks first (`GuestHello`: protocol 1.0, its version, its methods `health`, `exec`, `fs.read`, `fs.write`, `net.probe`, a boot id), accepts the gateway's admission (protocol check, the credential, the policy) and thereafter answers only calls whose HMAC verifies, each id once, each naming the task, the effect and the capability the body exercises (a mismatch is `BAD_CALL`); it enforces the admitted policy — a write only under the workspace and never on a protected path after resolving the symlinks it can, a read only under the workspace and the readable roots, an exec with exactly the environment the call names (nothing inherited), bounded output per stream, a timeout under the ceiling, a process count under the bound — and refuses the rest with a typed refusal (`UNAUTHENTICATED`, `REPLAYED`, `PROTECTED_PATH`, `OUTSIDE_WORKSPACE`, `LIMIT_EXCEEDED`, `BAD_CALL`). It mints nothing and fetches nothing. Process start/wait/cancel as separate calls, PTY, Git helpers, artifact transfer and browser endpoint discovery follow with M8.5 and M8.8.

## Handoff local → cloud

Handoff bundle contains immutable workspace checkpoint, Git metadata, context/index generation references, task/protocol state, tool capability requirements and encrypted/opaque secret handle references. It never contains raw secret values. Cloud admission verifies capability parity before switching execution owner.

## Sandbox recovery

If a sandbox dies, Core marks in-flight tool calls unknown, reconciles protected effects, provisions a fresh sandbox, restores latest valid checkpoint and resumes. External side effects are never replayed automatically.
