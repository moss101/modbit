# Task Card — M8.6 credential broker + egress policy

## Identity

- Task ID: M8.6
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/21 "Sandbox substrate boundary" (deny-by-default sandbox-to-internal network; explicit egress domains/ports by capability; dynamic credential handles via broker injection), "`modbit-guest`" (cannot fetch secrets independently); docs/33 "Sandbox Gateway" (brokers dynamic secret handles and network policy); docs/11 (brokers dynamic secret handles and network policy); docs/02 MOD-SBX-001 (no secrets in the guest image or static env); REQ-EV-0287 (explicit grants only), REQ-EV-0286 (every RPC authenticated and tenant-bound — the audit per tenant), docs/23 (the forge token as a secret handle the lease names).
- Qualification: the conformance suite's egress steps on a real Firecracker MicroVM and on the reference backend on every OS; the cloud worker end to end — a `cloud_isolated` task's process reaches the forge through the broker with a token the guest never held, an ungranted destination is refused, both on the audit.
- Evidence tier: real-system.

## Goal

Egress from a sandbox only by explicit grant, decided and recorded by the gateway's broker on the host side of the private channel; credentials injected by the broker for granted virtual hosts so the guest never holds a secret.

## Existing-code audit

- classification: MISSING before this task: a guest had no way out at all (no interface, no broker); egress grants were refused rather than served.
- production entry points: `crates/sandbox/src/policy.rs` (`NetworkPolicy{egress, credentials}`, `CredentialGrant`, `admits`, `credential_for`; `CompiledPolicy.egress_proxy`), `crates/sandbox/src/egress.rs` (`EgressBroker`: the channel protocol `TUNNEL`/`HTTP`, the decisions, the tunnel relay, the credentialed forward with injection, `EgressAudit`/`MemoryAudit`), `crates/sandbox/src/backend/{mod.rs,reference.rs,microvm.rs}` (`Provisioned.egress`: the guest's channels — a loopback listener per sandbox; the host vsock port `5001` bound before the boot), `services/modbit-guest/src/proxy.rs` (the local HTTP proxy: CONNECT tunnels and absolute-URI requests to the broker over vsock or loopback), `services/modbit-guest/src/serve.rs` (`egress_proxy` → the proxy runs; `http_proxy`/`https_proxy` for every process unless the call names its own), `crates/protocol/proto/modbit/v1/guest.proto` (`GuestPolicy.egress_proxy`), `apps/sandbox-gateway/src/{lib.rs,routes.rs}` (the broker per sandbox with the provision's secrets in memory; `StoreAudit` persisting every decision; `GET /v1/sandboxes/{id}/egress`; the policy's grants on the provision response), `crates/event-store/src/cloud/{schema.rs,mod.rs}` (schema v4 `sandbox_egress`; `record_egress`, `egress_audit`), `crates/sandbox/src/client.rs` (grants and secrets on the provision request; the admitted hosts on the identity), `crates/policy/src/kernel.rs` (`cloud_isolated`'s default lease carries `network.egress` and `secret.use`), `services/modbit-core/src/sandboxes.rs` (the task's grants from its lease and the forge custody; the hosts on `SandboxLeaseAcquired`), `crates/domain/src/task.rs` (`SandboxLeaseAcquired.egress`/`credentials`), `apps/cloud-worker/src/{lib.rs,session.rs}` (`ConfigureForge` from the worker's configuration).
- proof: `apps/sandbox-gateway/tests/gateway.rs` conformance steps on both backends — `egress_broker_channel`, `egress_allowed_http` (a host the policy admits answers through the proxy), `egress_denied_http` and `egress_denied_tunnel` (a host it does not admit is refused, HTTP and CONNECT alike), `egress_credentialed` (the virtual host answers `authorized: true` — the target saw `Bearer <real secret>` — and the body carries no secret), `egress_secret_never_in_guest` (the process environment carries the proxy and nothing of the secret), `egress_audited` (an admitted HTTP, a refused HTTP, a refused tunnel and a credentialed admission with its capability); `apps/cloud-worker/tests/cloud_worker.rs::qual_m8_5_…` (a `cloud_isolated` task under its default lease: `wget http://forge.modbit.internal/user` in the guest answers through the broker with the worker-configured forge token the guest never held, `http://127.0.0.1:9/` is refused, `SandboxLeaseAcquired` names the admitted host and the virtual host and no secret, the gateway's audit has the credentialed admission and the refusal). Unit: `a_policy_has_no_network_unless_granted_and_protects_control_paths` (rules, wildcards, ports).

## Limitations

- Egress is brokered HTTP: CONNECT tunnels and plain HTTP through the local proxy; a process that ignores `http_proxy` (raw sockets, UDP, DNS) has no path out, by design.
- A credentialed virtual host is plain HTTP inside the guest (the broker terminates TLS to the target); the broker performs one request per connection.
- Grants come from the lease's symbolic resources and the Core's forge custody; per-task arbitrary allow-lists through the Cloud API are not yet exposed.

## Verification

- `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` and `qual_m8_3_the_reference_backend_passes_the_backend_contract` (the egress steps inside)
- `qual_m8_5_a_cloud_isolated_tasks_tools_act_inside_its_sandbox_and_the_sandbox_is_released_when_the_task_ends` (the forge and the refused destination inside)
- Unit: `a_policy_has_no_network_unless_granted_and_protects_control_paths`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside
