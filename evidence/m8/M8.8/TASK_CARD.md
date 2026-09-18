# Task Card — M8.8 cloud browser remote stream/CDP

## Identity

- Task ID: M8.8
- Milestone: M8 Cloud isolated execution (P0)
- Requirements: docs/22 "Cloud browser" (Chromium inside the isolated MicroVM; the Browser Bridge over authenticated remote CDP; the user view streamed; structural and visual control on one BrowserSessionId); docs/02 MOD-BROWSE-001 (structural CDP/AX control, user observe/takeover in the same session); docs/21 "Sandbox substrate boundary" (the guest's browser leaves only through the egress broker; browser endpoint control as a narrow guest method); docs/24 (a worker's outbound link; no second source of truth); docs/33 "Sandbox Gateway".
- Qualification: the backend conformance steps on a real Firecracker MicroVM (Chrome for Testing's headless shell in the signed image) and on the reference backend (the host's Chrome); the cloud worker end to end — the agent's browser tools on a page through the broker, the person's stream, input and control through the Cloud API.
- Evidence tier: real-system.

## Goal

A `cloud_isolated` task's browser is the Chromium inside its sandbox, driven by the Core over the gateway's DevTools relay with the same tools, compiler and control lease as the local browser; the person watches it and takes over remotely through the Cloud API.

## Existing-code audit

- classification: MISSING before this task: the browser tools were `local_trusted`/`local_autonomous` only (the desktop's Electron view); a cloud task had no browser and no path to one.
- production entry points: `services/modbit-guest/src/browser.rs` (headless Chromium: start on the policy's grant, DevTools on the loopback, the egress proxy with the loopback bypass removed, stop; a term stops it), `services/modbit-guest/src/serve.rs` (`browser.start` / `browser.stop` / `browser.forward` under the `browser` capability; the forwarded link becomes a byte relay), `crates/protocol/proto/modbit/v1/guest.proto` (`GuestPolicy.browser`, the browser calls), `crates/sandbox/src/{policy.rs,port.rs,client.rs,link.rs,backend/reference.rs}` (`SandboxSpec.browser`, `CompiledPolicy.browser`, `SandboxPort::browser_start/browser_stop/browser_connect`, `CdpTransport`, the WebSocket relay client, `GuestLink::browser_*`, the reference backend's Chromium and graceful termination), `apps/sandbox-gateway/src/routes.rs` (`browser.start|stop` calls; `GET /v1/sandboxes/{id}/browser/cdp` — the WebSocket relay over a forwarded admitted link; a destroy stops the browser), `services/modbit-core/src/browser_cloud.rs` (the Browser Bridge: the CDP client, the `cloud-cdp` host answering navigate/state/snapshot/capture/act/isolation/close, the screencast to watchers, the person's input), `services/modbit-core/src/server.rs` (the host attached at `StartTask` when the sandbox grants a browser; `WatchBrowserView` / `UnwatchBrowserView` binding a connection to the frames; `BrowserViewInput` under the control lease), `services/modbit-core/src/browser.rs` (`HostLink.view`), `crates/protocol/proto/modbit/v1/surface.proto` (the view messages; `SurfaceFrame.browser_frame`), `crates/protocol/src/client.rs` (`next_browser_frame`), `crates/tools/src/browser.rs` (the tools under `cloud_isolated`), `crates/policy/src/kernel.rs` (`browser.control` in the cloud profile's default lease), `apps/cloud-api/src/{browser_view.rs,routes.rs,lib.rs}` (the workers' links, the person's stream, `:input`, `:control`, `CLOUD_CAPABILITIES` serving `browser.control`), `apps/cloud-worker/src/{link.rs,lib.rs,session.rs}` (the outbound link; watch/unwatch/input/control against the session's Core), `tools/guest-image/build.sh` and `.github/workflows/ci.yml` (the headless shell, pinned and checksummed, in the MicroVM image with its library closure, NSS modules and a font).
- proof: `apps/sandbox-gateway/tests/gateway.rs` conformance steps `browser_started` (a DevTools endpoint on the guest's loopback), `browser_answers_cdp` (`Browser.getVersion` over a forwarded link names Chrome), `browser_stopped` — on the MicroVM and the reference backend; `apps/cloud-worker/tests/cloud_worker.rs::qual_m8_8_…`: `BrowserHostAttached {host_kind: cloud-cdp, partition: sandbox:<id>}` and `SandboxLeaseAcquired.browser` on the cloud log; `browser.navigate` to the greeter on the admitted host (through the broker — every other request Chromium makes is refused and audited), `browser.snapshot` compiling the page's textbox and link with their boxes from the guest's accessibility tree, `browser.act fill` (`inserted 3 chars`), `browser.act click` navigating to the greeting with `url_contains` held; `:input` under the agent's control refused `409 AGENT_ACTIVE`; `:control USER`; the API stream's `watching` opener naming `cloud-cdp` and `USER`, a JPEG frame (`FFD8`, page width ≥ 320, url of the greeting); the person's click at the compiled box of the `Again` link, delivered, and a later frame of the form; `:control AGENT`; the agent's next snapshot reading the form the person navigated to; `BrowserControlChanged` USER then AGENT on the log.

## Limitations

- The desktop does not yet render the API stream or forward the person's input (the transport is proven with a protocol client); WebRTC/VNC-style transport is not built — frames are JPEG over WebSocket at the screencast's rate.
- `fill_credential` is refused in the cloud (`CREDENTIAL_UNAVAILABLE`): page credentials stay in the desktop broker's custody.
- One browser per sandbox, one page target; the browser image is the job's (a per-tenant choice is not exposed).
- The worker's link to the API is a transport for the view; the log stays the only source of truth (nothing of the stream is stored).

## Verification

- `qual_m8_3_a_firecracker_microvm_boots_the_guest_and_passes_the_backend_contract` and `qual_m8_3_the_reference_backend_passes_the_backend_contract` (the browser steps inside)
- `qual_m8_8_a_cloud_tasks_browser_runs_inside_its_sandbox_over_cdp_and_streams_its_view_to_the_person`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
- CI run json copy alongside: `ci-run-35290929921.json` — hosted run 35290929921 at c20494f on main, green on macOS, Linux and Windows; the cloud job on a real Firecracker MicroVM with Chrome for Testing's headless shell (`HeadlessChrome/153.0.8010.47`) answering CDP inside the guest
