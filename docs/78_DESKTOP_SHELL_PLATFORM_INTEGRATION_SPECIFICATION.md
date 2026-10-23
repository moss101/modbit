# Desktop shell platform integration specification (clean-room)

> **Authority:** DR-PX-2026-10-03-012 (`decisions/DR-PX-2026-10-03-012-desktop-shell-platform-integration.md`), status **proposed** until the owner ratifies it. Rows PX-094..PX-098 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` are NOT_STARTED and none may start before the record is accepted and DOC-PX-012 is COMPLETE.  
> **Nature:** a product specification in Modbit's own words and design. Provenance (clean-room posture), the four verification tags (LIVE, DISK, STATIC, UNVERIFIED) and the evidence key are those of `65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md` sections 2, 3 and 12 and are not repeated. It records functional facts and says how well each is known; it is not an implementation and not proof that anything exists.

## 1. Purpose

Modbit's desktop is an Electron main process that hosts the Core supervisor, a hardened renderer and the browser host, with a default window, no native menus, no tray, no badge, no protocol handler, no update flow and no pinned security fuses. The agent-first workspace of doc 65 needs more of the platform than a bare window: an in-app top bar over an inset title bar, native menus and badges that show attention, safe deep links, an update flow that never interrupts a run, and a main process whose hardening is proven rather than assumed. The owner has made an Electron change a real goal without saying which change. This document specifies the best-grounded set the research and the repository support, states its assumptions, and lists what it does not decide. It keeps Electron (MOD-DESK-001) and adds no new process, store or subsystem.

## 2. Constraints and ownership

No canonical subsystem is added. The renderer holds no authority and every effect goes through the Core (doc 81, REQ-EV-0076). Decision: `MOD-SHELL-001` in `02_AUTHORITY_AND_DECISIONS.md` (PROVISIONAL until the record is accepted). Common failure, cancellation, idempotency and restart semantics are those of doc 65 section 8.

| Row | Owner | Tier |
|---|---|---|
| PX-094 Window chrome: inset title bar, drag regions, window-control placement, optional translucency and persisted window state | desktop | release-critical |
| PX-095 Native menus, badge, tray icon, deep links with confirmation, single instance and native dialogs | desktop | release-critical |
| PX-096 Main-process hardening: fuses, ASAR integrity, permission and navigation denials, IPC inventory and main-thread responsiveness | desktop | release-critical |
| PX-097 Update application flow: apply at idle, staged and verified, with rollback and truthful states | desktop | release-critical |
| PX-098 Electron and Chromium version policy and upgrade qualification gate | governance | release-critical |

## 3. Existing implementation audit

Audit at `main` 5fdb47f, classes of doc 93.

| Area | Classification | What exists | First missing link |
|---|---|---|---|
| Window and renderer hardening | **IMPLEMENTED-PARTIAL** | `apps/desktop/src/main/main.ts`: `sandbox`, `contextIsolation`, no `nodeIntegration`, `webSecurity`, a CSP with `connect-src 'none'` set on every response, `will-navigate` prevented, `window.open` denied, sender validation on every IPC handler with an audit of refusals, renderer crash recovery with a new window and the same Core session; the browser host's views are sandboxed and grant no permission. Electron 44.3.0 is pinned in `apps/desktop/package.json`. | Reused unchanged; PX-096 adds proof and the missing pieces. |
| Window chrome | **NOT-FOUND** | A default frame, 1200 x 800, no minimum size (PX-049 adds one), no hidden title bar, no drag regions, no translucency, no persisted display-aware window state, no zoom model. | PX-094. |
| Menus, badge, tray, notifications, deep links | **IMPLEMENTED-PARTIAL** | OS notifications are delivered through main (`deliverNotification`, PX-023) with opt-in and a log. There is no native application menu, dock or taskbar badge, tray icon, `modbit://` protocol handler, single-instance lock or native dialog. | PX-095. |
| Electron fuses and ASAR integrity | **NOT-FOUND** | No fuse configuration, no ASAR integrity validation and no check of either in CI or in the packaged build; packaging is M10.2 (NOT_STARTED). | PX-096. |
| Updater, signing, SBOM | **NOT-FOUND** | M10.2 is NOT_STARTED and is the owner of signing, the update channel and the SBOM; nothing applies an update. | M10.2 stays the owner; PX-097 adds the apply-at-idle flow on top. |
| Version policy | **NOT-FOUND** | Electron is pinned exactly; there is no written policy for upgrade cadence, end-of-support, or an upgrade qualification gate. | PX-098. |

## 4. Requirements

### A. Scope and interpretation

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| ESH-A01 | This record specifies the main-process platform integration of the existing Electron shell: window chrome, native integration, hardening, update application and version policy. It does not replace Electron or change the renderer's isolation model, the preload bridge's allow-list, the CSP or the Core protocol. A framework replacement has no evidence in the research or the repository and is not specified; if the owner meant one, the question is recorded in the DR. | UNVERIFIED (interpretation of the owner's request) | PX-094, PX-095, PX-096, PX-097, PX-098 |
| ESH-A02 | macOS is the release platform. Windows and Linux behaviour is implemented behind capability checks and stays CI_COMPATIBLE until their own packaged E2E passes (docs 75 and 76); nothing here promotes a platform. | UNVERIFIED (design choice); STATIC (reference per-platform window options) | PX-094, PX-095 |

### B. Window chrome

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| ESH-B01 | On macOS the title bar is inset and hidden: the 40 pt top bar of doc 65 is the drag region and the window controls sit inside the agent-list strip. The window controls were observed inside the sidebar at about 20 pt from the top; the placement formula (offset derived from the top-bar height, a reserved spacer so content never collides, a different constant on newer operating systems) is the reference's and was inferred from code, not seen on screen, so Modbit measures its own positions on each supported OS version. | LIVE (controls inside the sidebar strip); STATIC (formula, spacer constants); UNVERIFIED (newer-OS constants) | PX-094 |
| ESH-B02 | Interactive controls inside the drag region opt out of dragging; double-click on the bar zooms or maximises per platform convention; the window has a hairline border where the platform draws none and a corner radius of about 10 pt matches the platform. Keyboard focus reaches every control in the bar. | LIVE (corner radius about 10 pt); STATIC (border) | PX-094 |
| ESH-B03 | Translucency (vibrancy on macOS, a similar material on Windows) is opt-in. The default is opaque. A Reduce transparency preference follows the operating system and overrides the opt-in, and an opaque fallback exists for every translucent surface. The reference light theme showed no visible translucency in the sampled window, so no look is promised. | STATIC (vibrancy values and fallbacks); LIVE (no visible tint in the light theme) | PX-094 |
| ESH-B04 | Window state (position, size, maximised, fullscreen) is persisted per display configuration and validated on restore so a window never opens off every screen. A UI zoom model scales the interface from a 13 px baseline in 8 percent steps per px with a clamp, persisted per profile. | STATIC (zoom formula and clamp); UNVERIFIED (Modbit persistence) | PX-094 |

### C. Native integration

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| ESH-C01 | A native application menu (macOS standard roles plus File, Edit, View, Window and Help) draws its accelerators from the shortcut registry of PX-045, so the menu and the keyboard never disagree; items that act on a task are disabled when no task is selected. The reference's Agents window registers menu accelerators for new agent, open folder, terminal, changes, browser, files, palette, shortcut help, settings and zoom. | DISK (menu items and accelerators cached by the reference); UNVERIFIED (Modbit menu) | PX-095 |
| ESH-C02 | The dock or taskbar badge shows the count of items needing attention, never routine progress. A menu-bar or tray icon has template variants for attention, running and unread in light and dark, redrawn for Modbit, and offers the five fleet views and quit. | STATIC (template icon states); UNVERIFIED (Modbit design) | PX-095 |
| ESH-C03 | OS notifications stay opt-in with quiet hours and deep-link to the exact state (PX-023); a notification click focuses the right window and task. | UNVERIFIED (extends existing behaviour) | PX-095 |
| ESH-C04 | A `modbit://` protocol handler with a single-instance lock opens tasks, approvals, settings and install confirmations. Routes are an allow-list; names and identifiers are validated; a link can never perform a state-changing action by itself: an install of a package or an external server always opens the confirmation of PX-088 or PX-090, and an approval link only focuses the card. A link from an untrusted page never reaches the handler with the user's session implied. | STATIC (the reference's install deep link opens a confirmation, validates names and has an admin switch) | PX-095 |
| ESH-C05 | Native file and folder dialogs run in main and return a path only to be shown; the Core re-validates and applies path policy and trust before using it (the existing repository-trust flow). | UNVERIFIED (design choice) | PX-095 |

### D. Main-process hardening and updates

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| ESH-D01 | The packaged binary sets Electron fuses to the hardened values: run-as-node off, the node-options environment variable off, the node inspector arguments off, embedded ASAR integrity validation on, only-load-from-ASAR on, cookie encryption on, file-protocol extra privileges off. CI reads the fuse state from the packaged binary and fails on any difference. No research source covers this; it is standard Electron practice and Modbit's own requirement. | UNVERIFIED (design choice; no research evidence) | PX-096 |
| ESH-D02 | The app's own session denies every permission request and check. Browser partitions allow only a minimal list and name every denial (the existing behaviour, M7.1). Window attach of web views is denied and navigation of the app window is limited to the app's own origin. A test pins the CSP including `connect-src 'none'`. | STATIC (reference permission allow-list for web views) | PX-096 |
| ESH-D03 | Every IPC channel has a schema and a sender check; a generated inventory of channels is pinned by a test so a new channel is a reviewed change. The preload exposes only typed functions by name. | UNVERIFIED (extends existing behaviour) | PX-096 |
| ESH-D04 | An update is applied only when no run is active and no approval is pending, or after the quit prompt of PX-049; it is staged, its signature and digest are verified (M10.2), the previous version is kept for rollback, and the update states (available, downloading, ready, applying, applied, failed) are shown with cause and next action. A failed update leaves the running version intact. M10.2 owns signing, channel and SBOM; this record adds the apply behaviour only. | UNVERIFIED (design choice) | PX-097 |
| ESH-D05 | The main process never blocks its event loop on a Core call: main-thread lag stays at or under 50 ms at p95 under load, so the operating system's not-responding dialog never appears. The reference product showed that dialog during a slow command probe. | LIVE (the not-responding dialog observed once); UNVERIFIED (budget) | PX-096 |

### E. Versioning

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| ESH-E01 | Electron and its Chromium are pinned exactly and recorded in the build manifest. A written policy sets the cadence (track a supported stable major, never run one past end of support), a security-update response window, and an upgrade qualification gate: the browser host E2E, the CDP deny list and origin gate suites, the security suites, the fuse check, the accessibility suite and the packaged smoke run must pass before a version moves. The reference product runs one Electron major below Modbit's pin. | LIVE (the reference runs Electron 42.10.0 with Chromium 148, in a signed-out instance) | PX-098 |

### F. Process model

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| ESH-F01 | Heavy and background work stays in the Core (single owner): full-text search, indexing, diffing and analysis are Core services. The reference product hosts conversation search in a separate Electron utility process; Modbit does not adopt that, so the main process holds only windows, IPC, credential custody, the browser host and the updater. | DISK (the reference's process graph: main, shared, search, extension hosts, pty host, utility workers) | PX-096 |
| ESH-F02 | A renderer responsiveness watchdog pings the renderer at an interval and treats a block over a threshold as a hang with recovery (the existing crash recovery opens a new window on the same session). Lifecycle participants start and stop in a defined order and shutdown waits for them with a bounded timeout. | DISK (ping 5 s, block threshold 15 s, ordered start and reverse stop) | PX-096, PX-097 |

## 5. Rows and evidence tiers

Every row is release-critical: each changes effect-bearing behaviour, permissions or policy, execution, recovery, protocol or schema, a security boundary or evidence semantics (doc 83); none is eligible for the iteration tier.

| Row | Title | Owner | Prerequisites |
|---|---|---|---|
| PX-094 | Window chrome: inset title bar, drag regions, window-control placement, optional translucency and persisted window state | desktop | PX-049, PX-045, DOC-PX-012 |
| PX-095 | Native menus, badge, tray icon, deep links with confirmation, single instance and native dialogs | desktop | PX-088, PX-090, PX-094, PX-045, DOC-PX-012 |
| PX-096 | Main-process hardening: fuses, ASAR integrity, permission and navigation denials, IPC inventory and main-thread responsiveness | desktop | PX-049, M10.2, DOC-PX-012 |
| PX-097 | Update application flow: apply at idle, staged and verified, with rollback and truthful states | desktop | M10.2, PX-049, PX-096, DOC-PX-012 |
| PX-098 | Electron and Chromium version policy and upgrade qualification gate | governance | PX-096, PX-073, DOC-PX-012 |

## 6. Supersessions and clarifications

Nothing sealed changes until the record is accepted. The explicit records, also entered in docs 02 and 03:

| Earlier decision, row or text | Kind | Scope and effect | What survives |
|---|---|---|---|
| MOD-DESK-001 (PROVISIONAL): Electron plus React and TypeScript native shell for v1 | RETAINED, SCOPE CLARIFIED | Main-process platform integration is specified; Electron is not replaced. SurfaceProtocol keeps architectural lock-in away. | The decision. |
| PX-049 invariant: webPreferences, CSP and preload byte-identical | RETAINED | PX-094 keeps the same invariant; PX-095 and PX-096 add main-process code (menus, protocol handler, fuses, session denials) under their own gates and pinned inventories. | The renderer isolation model. |

## 7. Traceability: requirement to row, tag and source

| Requirement | Rows | Verification tag | Source report section |
|---|---|---|---|
| ESH-A01 | PX-094, PX-095, PX-096, PX-097, PX-098 | UNVERIFIED (interpretation of the owner's request) | F06 §14.1, doc 02 MOD-DESK-001 |
| ESH-A02 | PX-094, PX-095 | UNVERIFIED (design choice); STATIC (reference per-platform window options) | doc 76, F06 §14.1 |
| ESH-B01 | PX-094 | LIVE (controls inside the sidebar strip); STATIC (formula, spacer constants); UNVERIFIED (newer-OS constants) | L14 §3.1, F06 §14.1, F06 §7.1 |
| ESH-B02 | PX-094 | LIVE (corner radius about 10 pt); STATIC (border) | L14 §3.1, F06 §3.3 |
| ESH-B03 | PX-094 | STATIC (vibrancy values and fallbacks); LIVE (no visible tint in the light theme) | F06 §3.3, F06 §14.1, L14 §3.1 |
| ESH-B04 | PX-094 | STATIC (zoom formula and clamp); UNVERIFIED (Modbit persistence) | F06 §4.4, F06 §13.4 |
| ESH-C01 | PX-095 | DISK (menu items and accelerators cached by the reference); UNVERIFIED (Modbit menu) | L12 §3, L12 §2.1 |
| ESH-C02 | PX-095 | STATIC (template icon states); UNVERIFIED (Modbit design) | F06 §14.3, doc 10 |
| ESH-C03 | PX-095 | UNVERIFIED (extends existing behaviour) | doc 39 |
| ESH-C04 | PX-095 | STATIC (the reference's install deep link opens a confirmation, validates names and has an admin switch) | F04 §7.6, F04 §10 |
| ESH-C05 | PX-095 | UNVERIFIED (design choice) | doc 32 |
| ESH-D01 | PX-096 | UNVERIFIED (design choice; no research evidence) | doc 70 |
| ESH-D02 | PX-096 | STATIC (reference permission allow-list for web views) | F03 §6 F-F8, doc 32 |
| ESH-D03 | PX-096 | UNVERIFIED (extends existing behaviour) | doc 32 |
| ESH-D04 | PX-097 | UNVERIFIED (design choice) | doc 70, F03 §4 F-D1 |
| ESH-D05 | PX-096 | LIVE (the not-responding dialog observed once); UNVERIFIED (budget) | L14 §P3.7, doc 53 |
| ESH-E01 | PX-098 | LIVE (the reference runs Electron 42.10.0 with Chromium 148, in a signed-out instance) | L11, F07 |
| ESH-F01 | PX-096 | DISK (the reference's process graph: main, shared, search, extension hosts, pty host, utility workers) | L13 §3.13, doc 81 |
| ESH-F02 | PX-096, PX-097 | DISK (ping 5 s, block threshold 15 s, ordered start and reverse stop) | L13 §3.13 |

### Row to requirements

| Row | Requirements |
|---|---|
| PX-094 | ESH-A01, ESH-A02, ESH-B01, ESH-B02, ESH-B03, ESH-B04 |
| PX-095 | ESH-A01, ESH-A02, ESH-C01, ESH-C02, ESH-C03, ESH-C04, ESH-C05 |
| PX-096 | ESH-A01, ESH-D01, ESH-D02, ESH-D03, ESH-D05, ESH-F01, ESH-F02 |
| PX-097 | ESH-A01, ESH-D04, ESH-F02 |
| PX-098 | ESH-A01, ESH-E01 |

## 8. Non-goals

| Not in scope | Why |
|---|---|
| Replacing Electron or adopting another shell framework | No evidence and MOD-DESK-001 stands; a separate record if ever wanted. |
| Weakening the renderer sandbox, context isolation, the CSP or the preload allow-list | Forbidden; every row here narrows or preserves them. |
| Owning the updater, signing and SBOM | M10.2. |
| Platform promotion of Windows or Linux | Doc 76; a Decision Record after packaged E2E. |
| Extension hosts, a workbench or a webview-based UI for plugins | MOD-SURF-001 and MOD-SURF-002. |
| Telemetry crash reporting to a remote service | Not specified; local diagnostics export exists (docs 34 and 71). |

## 9. Still unverified

| ID | Unverified fact | Evidence that would close it |
|---|---|---|
| V01 | What the owner means by the Electron change | The owner's answer (question in the DR). This record assumes window chrome and platform integration, hardening, update application and version policy. |
| V02 | Window-control placement constants on the operating-system versions Modbit supports | Measured captures of Modbit's own window on each version. |
| V03 | Which fuse values the browser host and the Core supervisor tolerate | A packaged build with the fuses set, run through the full desktop E2E. |
| V04 | Whether translucency is worth shipping at all | Owner decision after a prototype; it is opt-in and off by default regardless. |
| V05 | Windows and Linux chrome behaviour | Per-platform packaged E2E and promotion by a Decision Record (doc 76). |
| V06 | Main-thread lag under load against the 50 ms budget | Packaged-app traces with a busy Core. |
