# Task Card — IMP-EV-0040 DevicePolicy / machine authority

## Identity

- Task ID: IMP-EV-0040 (REQ-EV-0040, ADAPT; owner Policy Kernel)
- Milestone: M9
- Qualification: QUAL-EV-0040 — a project file attempting to disable a device requirement is rejected.
- Evidence tier: release-critical (permissions and policy, security boundary)

## Existing-code audit

- classification: DOCUMENTED-ONLY for the device. The kernel's `PolicyEnvelope` doc named "admin/device authority", but the Core always used the compiled default envelope; configuration had admin, project and user layers and no machine layer; a `device` key in any configuration file was silently ignored.
- first missing link: no source for a machine policy and no layer above the organization that lower configuration cannot override.
- production entry points: `crates/policy/src/config.rs` (`Authority::Device` first in `ALL`; `DeviceConstraints`; `Layer.device` / `ResolvedConfig.device`; a non-device layer's device key recorded as a rejected attempt); `crates/policy/src/kernel.rs` (step 5: `sandbox_required` refuses `shell.exec` outside an isolated profile, `DEVICE_REQUIRES_SANDBOX`); `services/modbit-core/src/config.rs` (`device_policy_path`, the device layer read first); `server.rs` (`GetEffectivePolicy`).

## Verification

- `qual_ev_0040_a_project_file_cannot_disable_what_the_device_requires` (services/modbit-core, real Core, real terminal broker): with a managed device policy requiring sandboxed execution, telemetry off, a proxy, trust roots, an update floor and egress denied, a repository `.modbit/config.json` switching the sandbox and telemetry back and allowing egress and execution, and a user configuration naming its own proxy — `GetEffectivePolicy` shows the device constraints exactly as the device wrote them, read from its path, `network.egress=DENY by Device`, and every attempt from the project and the user refused; `shell.exec` under `local_trusted` is `POLICY_DENIED` / `DEVICE_REQUIRES_SANDBOX` and nothing runs.
- `crates/policy` unit `only_the_device_sets_device_constraints_and_its_denials_stand_below_it`.
- Regression: the full modbit-core suite (configuration, external servers, kernel), policy and core-runtime suites.

## Limitations

- Enforced here: the sandbox requirement and the device's permissions. Represented and shown, enforced by their owners later: the egress proxy and trust roots (the egress broker), the update channel and floor (M10.2 updater), the telemetry level (M10.1).
- The device file is read from `MODBIT_DEVICE_POLICY` or a fixed system path; no MDM profile format (e.g. macOS configuration profiles) is parsed.

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- docs/23, docs/30 and docs/16 carry the as-built paragraphs
