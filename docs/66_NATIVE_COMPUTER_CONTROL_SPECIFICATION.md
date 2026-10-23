# Native computer control specification (clean-room)

> **Authority:** DR-PX-2026-10-03-008 (`decisions/DR-PX-2026-10-03-008-native-computer-control.md`), status **accepted** on 2026-10-05 (owner instruction, goal: implement research/audit/01-TASK-LIST.md). Rows PX-069..PX-076 in `62_PRODUCT_EXTENSION_REQUIREMENTS_TASKS_AND_QUALIFICATIONS.md` are NOT_STARTED; with DOC-PX-008 COMPLETE each is startable as its other prerequisites (including rows of DR-PX-2026-10-03-007) complete.  
> **Nature:** a product specification in Modbit's own words and design. Provenance (clean-room posture), the four verification tags (LIVE, DISK, STATIC, UNVERIFIED) and the evidence key are those of `65_AGENT_FIRST_WORKSPACE_SPECIFICATION.md` sections 2, 3 and 12 and are not repeated. It records functional facts and says how well each is known; it is not an implementation and not proof that anything exists.

## 1. Purpose

Modbit's browser runtime controls the exact live Chromium session the user sees, structurally first (docs 22 and 17). The agent cannot yet operate any other application on the user's computer: it cannot test a native app it just built, drive a desktop tool, or help with a task that lives outside a browser. The owner has made local operating-system control a real goal. This document specifies it as a semantic-first Computer Runtime under the existing Browser and Computer Runtime owner: the application's accessibility tree first, targeted coordinates only against a fresh observation, raw input last, every action approved per call. It is not a pixel-only computer-use product, which stays a non-goal of doc 10.

## 2. Constraints and ownership

No canonical subsystem is added. The renderer holds no authority and every effect goes through the Core (doc 81, REQ-EV-0076). Decision: `MOD-COMP-001` in `02_AUTHORITY_AND_DECISIONS.md` (PROVISIONAL; the record was accepted on 2026-10-05). Common failure, cancellation, idempotency and restart semantics are those of doc 65 section 8.

| Row | Owner | Tier |
|---|---|---|
| PX-069 Computer Runtime contract: typed computer tools, scopes, handles, refusal taxonomy and unknown-outcome latch | browser | release-critical |
| PX-070 Per-call exact-intent approvals for native control, never allowlistable | effects-security | release-critical |
| PX-071 macOS actuator helper: separate signed process, authenticated local RPC, accessibility-first background control, takeover lease and stop | browser | release-critical |
| PX-072 Actuator supply chain: bundled signed helper, hash-pinned manifest, rollback and process identity | browser | release-critical |
| PX-073 Browser control hardening: CDP deny list, per-call origin gate, certificate trust, permissions and per-task view ownership | browser | release-critical |
| PX-074 Computer-control desktop surface: permission window, presence and Stop, action cards and approval card | desktop | release-critical |
| PX-075 Computer-use subagent profile with an isolated target, constrained tools and stall rules | core-runtime | release-critical |
| PX-076 Screenshot and accessibility artifacts: window capture, masking, content-aware codec, retention and audit | media | release-critical |

## 3. Existing implementation audit

Audit at `main` 5fdb47f, classes of doc 93.

| Area | Classification | What exists | First missing link |
|---|---|---|---|
| Browser session, semantic compiler, structural action ladder | **PRODUCTION-WORKING** | `apps/desktop/src/main/browser.ts`, `crates/browser` (M7.1 to M7.8): sandboxed `WebContentsView` per session, entities with stable ids, `browser.act`, targeted capture, credential handle fill, takeover lease (`ControlLease`), host-owned emergency stop, injection scanner; typed failures such as `TARGET_STALE`, `HUMAN_ACTIVE`, `ACTION_UNSAFE`. | Reused as the pattern and as the shared lease and stop machinery. |
| Native application control (`computer.*`) | **DOCUMENTED-ONLY** | Doc 17 lists `computer.observe` and `computer.action` under the Computer Runtime; REQ-EV-0083 to 0090 specify exact application identity, a single controller lock, a watchdog, human preemption and a typed failure taxonomy. No crate, protocol message, helper process or tool implements them for native applications. | No Computer Runtime for native applications. |
| Raw CDP and per-call origin gate for browser control | **IMPLEMENTED-PARTIAL** | The host never exposes raw CDP to the model; navigation is limited to http and https and `will-navigate`, `will-redirect` and new windows are blocked; every page permission is denied and named. There is no deny list enforced at the CDP bridge for future power tools, no per-call origin policy on the page a tool acts on, and no certificate trust flow. | Origin gate and CDP deny list at the host boundary (PX-073). |
| Typed refusal contract and unknown-outcome latch | **IMPLEMENTED-PARTIAL** | Browser failures are typed and reconcile through the effect ledger; the unknown-outcome rule exists for protected effects (doc 23). A latch that blocks further input after an input whose outcome is unknown is not implemented for any input channel. | Latch and the native taxonomy (PX-069). |
| Native permission flow and presence UI | **NOT-FOUND** | No macOS Accessibility or Screen Recording handling, no first-run window, no Stop overlay, no presence indicator exist in `apps/desktop`. | Desktop surface (PX-074). |
| Actuator helper process, supply chain | **NOT-FOUND** | No helper binary, local RPC, process-identity pinning or signing path exists; the updater, signing and SBOM task M10.2 is NOT_STARTED. | Actuator and its supply chain (PX-071, PX-072). |

## 4. Requirements

### A. Architecture and scopes

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-A01 | Native control is a Computer Runtime under the existing Browser and Computer Runtime owner (REQ-EV-0075, REQ-EV-0081). The model sees typed `computer.*` tools in the Tool Registry; the runtime in the Core owns leases, handles, refusals, the latch and the audit; a separate actuator process, never the model-facing layer, touches the screen or input. No tool or UI code calls an operating-system capture or input API directly. | STATIC | PX-069, PX-071 |
| CUC-A02 | There are two scopes. APP scope observes and acts on one named application in the background through its accessibility tree, without moving the real cursor or taking the display; it is the default and covers observation and element actions. SCREEN scope takes over the display with the real cursor under an exclusive lease and a visible Stop control; it is an escalation that needs its own approval and is for cases the accessibility tree cannot serve. | STATIC | PX-069, PX-071, PX-074 |
| CUC-A03 | Computer control exists only for `local_trusted` tasks on the user's own machine. It is denied in `cloud_isolated` execution, where the sandbox's own browser and desktop are the existing surfaces, and it is unavailable on any platform whose actuator has not passed its own packaged E2E (macOS first; other platforms answer ACTUATOR_UNAVAILABLE). | UNVERIFIED (design choice) | PX-069, PX-071 |
| CUC-A04 | The action hierarchy of doc 22 holds: a structured accessibility action on a resolved element, then a coordinate action bound to a fresh observation, then raw input. Each fall to a lower rung records the modality and the reason and the action is verified against the observed state afterwards (REQ-EV-0082, REQ-EV-0086). | STATIC | PX-069 |
| CUC-A05 | Computer-use work runs in a dedicated subagent profile with a constrained toolset (read, search, shell for non-GUI work, and `computer.*`), its own view of the target, a specialist model slot chosen by policy, no access to the parent's transcript, and results returned as a typed envelope. A long GUI loop never runs in the main context. It starts only once the environment is believed healthy, and it is resumable. | STATIC | PX-075 |

### B. Tool contract and handles

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-B01 | Tool names, parameters and results are Modbit's own. Observation tools: list applications, resolve an application (without activating it), read application state (an accessibility tree as text, one element per line with id, role, name, value, settable and available actions), screenshot, wait. Action tools: press (an element action), set value, click, move, drag, type, key, scroll. Arguments are validated strictly: an unknown or misspelt argument is refused with the advertised names, never dropped. | STATIC | PX-069 |
| CUC-B02 | The model-facing coordinate canvas is fixed at 1280 x 800 with the origin at the top left. The runtime letterboxes the real display or window to it with a single scaler and rejects any frame whose size or aspect differs beyond a tolerance of 0.02. 1280 x 800 is Modbit's own choice; the reference product's per-platform canvases are noted, not adopted. | STATIC | PX-069, PX-076 |
| CUC-B03 | Handles are short-lived and typed. An element id is valid only with the snapshot id of the tree it came from and is invalidated by a new tree read, a window or application change, or an edit that opens a sheet or replaces the tree. A coordinate token is valid only until the next screenshot or until the window moves or resizes. A stale handle is refused TARGET_STALE with the changed facts; nothing is acted on. | STATIC | PX-069 |
| CUC-B04 | Observation and action are coupled by scope: a SCREEN-scope action returns a fresh screenshot; an APP-scope action returns the typed state delta and requires an explicit re-observation before the next coordinate use. A wait that finds the screen unchanged says so and does not re-encode an image. A visible change is evidence that something happened, not proof that this action caused it, and no visible change is not failure. | STATIC | PX-069, PX-076 |
| CUC-B05 | Typing prefers reversible entry: set the value of a settable element; otherwise typed input to a verified focus. Newlines, tabs and destructive keys (Delete, Backspace on selection, quit, close, cut, lock) are separate arguments, and destructive keys are ACTION_UNSAFE unless the approval named them. Clipboard use preserves and restores the user's clipboard and never lets its contents enter model context or evidence (REQ-EV-0090). | STATIC | PX-069 |
| CUC-B06 | A secure or password field is never typed into from model text. A credential is entered only through the credential broker's handle bound to the exact application identity and field, per call, and the value never reaches the model, the transcript, an event or a screenshot (REQ-EV-0090, M7.8 pattern). | STATIC | PX-069, PX-070 |

### C. Approvals and policy

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-C01 | Every action tool call is approved per call with an exact-intent approval bound to: the target application's identity (bundle identifier, executable path, code-signing identity and process id), the window, the snapshot id or coordinate token, the action and its arguments (typed text shown in full), the scope, and the approval's expiry. Any change to those invalidates the approval. These approvals can never be allowlisted, never be covered by any run mode (including Run everything), never be created by a durable rule, and never be given by a background or detached agent (REQ-EV-0046). | STATIC (reference approval class: OS-level control always asks); UNVERIFIED (no live observation) | PX-070 |
| CUC-C02 | Observation is gated by an observation grant: an exact-intent approval for one named application and APP scope, bound to its identity, valid for the current turn and at most 15 minutes, with a call cap, never allowlisted, never inherited by a subagent or a later turn. Per-call approval of every screenshot would make observation unusable; the grant is the narrowest unit that keeps it exact. The owner may instead require per-call observation approval (question recorded in the DR). | UNVERIFIED (design choice) | PX-070 |
| CUC-C03 | SCREEN scope needs a separate approval that states it takes the display and the real cursor and shows the Stop control; it expires at turn end and is never combined with an APP-scope grant in one approval. | STATIC | PX-070, PX-074 |
| CUC-C04 | Administrative policy can forbid computer control, restrict it to an application allow-list by bundle identity, or forbid SCREEN scope. An unknown policy verdict fails closed (REQ-EV-0031). | STATIC | PX-070 |
| CUC-C05 | Applications that hold secrets or control the machine (the credential store, the system settings and security panes, the terminal and shell hosts, the operating system's own authentication dialogs, and Modbit itself) are not drivable: the runtime answers WINDOW_UNVERIFIABLE or ACTION_UNSAFE and no approval can lift it. The list is data owned by policy, versioned and shown to the user. | UNVERIFIED (design choice) | PX-070 |

### D. Failure, outcome and stop semantics

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-D01 | Every refusal is typed and carries a code, a message, and a recommended escalation of one of four kinds: retry (nothing was actuated, the same call is safe), ask the user (only the person at the machine can clear it), use a different tool (observe first), or stop (no further input or control this turn). The codes extend REQ-EV-0089: TARGET_STALE, TARGET_OCCLUDED, WINDOW_UNVERIFIABLE, ACCESSIBILITY_UNAVAILABLE, HUMAN_ACTIVE, MODAL_BLOCKING, TARGET_NOT_EDITABLE, ACTION_UNSAFE, PERMISSION_REQUIRED, SECURE_DESKTOP, TARGET_ELEVATED, CAPTURE_FAILED, SCREENSHOT_REQUIRED, INVALID_ARGUMENTS, UNKNOWN_TOOL, INPUT_FAILED, TIMEOUT, UNSUPPORTED_REQUEST, ACTUATOR_UNAVAILABLE, OUTCOME_UNKNOWN, SESSION_REQUIRED, SESSION_BUSY, USER_ABORTED, APPROVAL_DENIED, GRANT_EXPIRED. The contract is explained to the model from the same tables the code uses. | STATIC | PX-069 |
| CUC-D02 | Calls are serialised per control session; there are no parallel computer calls. An input is never retried after delivery: only a failure provably before the write (the actuator not running, the connection refused) may relaunch and retry. | STATIC | PX-069 |
| CUC-D03 | After an input whose outcome is unknown (a timeout or disconnect after delivery) the control session latches: further input is refused OUTCOME_UNKNOWN until a new control session with a fresh observation and a new approval. The unknown outcome is recorded as an effect receipt with outcome UNKNOWN and reversibility NONE, and is reconciled only by observation. | STATIC | PX-069 |
| CUC-D04 | Real human input (a physical key or pointer event) parks the controller at once: the next agent input is HUMAN_ACTIVE and control must be re-acquired after a cooldown (REQ-EV-0087). The actuator excludes its own synthetic events from this detection. | STATIC | PX-071 |
| CUC-D05 | Stop is host-owned and independent of the model loop. The actuator stops injecting input within 500 ms of a Stop from the on-screen control, the desktop app, the session emergency stop (M9.5) or a Core disconnect; a user stop is a typed, sticky, per-turn state (USER_ABORTED) after which no input, start-control or release-control call is accepted that turn. Leases release automatically at turn end, on stop and on failure. | STATIC | PX-069, PX-071, PX-074 |
| CUC-D06 | An action that is no longer wanted when a failure repeats stops: after four failed attempts or when progress stalls the subagent stops and reports what it observed, what blocked it and the best next step, and it treats logins, passkeys, captchas, permission prompts and destructive confirmations as blockers to report and not to improvise. The no-progress rule of PX-039 applies. | STATIC | PX-075 |

### E. Actuator process and supply chain

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-E01 | The actuator is a separate, signed, notarised helper process with its own operating-system permissions (Accessibility and Screen Recording on macOS, granted to the helper and not to the app). It can be killed independently. The Core verifies the helper's code signature and designated requirement before connecting and the helper verifies its parent. | STATIC | PX-071, PX-072 |
| CUC-E02 | The local RPC is authenticated and bound to the launcher: a per-launch random token delivered over an inherited descriptor or a file readable only by the user; peer-credential and process-id checks in both directions; the helper creates the endpoint exclusively and aborts if the name is taken; the Core accepts a welcome only from the process it launched; the protocol has a major version and a mismatch refuses. Requests carry a deadline the helper honours. | STATIC | PX-071 |
| CUC-E03 | Before signalling a helper process its executable path is verified against the expected bundle. The Core never replaces a helper that is running. Install and update keep a previous version for rollback and recover from a crash mid-update. | STATIC | PX-072 |
| CUC-E04 | Modbit ships the helper inside its signed, notarised application bundle and updates it only with the application (M10.2), with a hash-pinned entry in the build manifest and the SBOM. Unlike the reference product, which downloads its helper at run time against a pinned manifest, Modbit adds no run-time download path, so the helper's supply chain is the application's. | STATIC (reference download path); UNVERIFIED (Modbit choice) | PX-072 |
| CUC-E05 | Permissions are requested by minimum labelled capability in plain language: access to application interfaces (accessibility) and use of screenshots as context (screen recording). The first-run window shows each as checking, allow, or ready; one helper restart after a grant makes it effective; a blocked episode routes the window once, in the root workspace window only. | STATIC | PX-074 |

### F. Observation, privacy and evidence

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-F01 | Screenshots and accessibility trees are sensitive artifacts. APP scope captures the target window only. Secure fields are masked before capture leaves the actuator. A secure desktop or a locked screen is SECURE_DESKTOP and nothing is captured. Artifacts follow the task's retention policy, are untrusted input to the model, and never leave the machine for a cloud task. | STATIC (privacy classification of screenshots as sensitive) | PX-076 |
| CUC-F02 | Frames use a content-aware codec: flat user-interface frames are lossless, others lossy at a quality ladder that never changes the pixel size (every frame is a coordinate space), and an identical frame is memoised and not re-encoded. | STATIC | PX-076 |
| CUC-F03 | The audit record holds counts and kinds, never contents: per session the actions by kind, the screenshot count, the duration, the outcome, the target application identity and the approval ids. Each approved action produces an effect receipt (reversibility NONE for input) bound to the intent hash. | STATIC | PX-070, PX-076 |

### G. Browser control hardening

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-G01 | Any raw DevTools-protocol access in the browser host is filtered by a deny list enforced before attach: the domains Browser, Input, Storage, SystemInfo, Target and Tethering, and the methods that set or clear cookies, clear the cache, set file-input files, navigate or read navigation history. The host's own structural actions are the only Input users. Large responses are spilled to an OutputRef. | STATIC | PX-073 |
| CUC-G02 | Every browser tool call checks the page's current origin against policy, not only navigation: `file:` is always refused; with an allow-list configured, a call on a page whose origin is not allowed or cannot be determined is refused with the allowed list; an unknown origin fails closed. The default policy admits http and https only. | STATIC | PX-073 |
| CUC-G03 | A certificate error holds the request for 60 s awaiting the user and shows issuer, subject, validity and fingerprint with Reject and Trust (remembered per workspace). Page permissions are a minimal allow-list; every other permission is denied and named. A sign-out wipes cache, cookies, storage, service workers and downloads of the browser partitions. | STATIC | PX-073, PX-074 |
| CUC-G04 | Browser views have an owner task: a task lists and selects only its own; hidden views are capped (six) and reclaimed least recently used (not locked, not loading) with a stated reset; navigation never steals focus unless the person asked to see the browser. Agent device emulation is scoped to the turn, visible with a Reset control, reverted at turn end, and never overrides the user's own emulation. | STATIC | PX-073 |
| CUC-G05 | The existing takeover and lock keep working: a person's own input or the lock overlay's take-control action revokes the agent's input at once and returns control only by an explicit hand-back that increments the lease generation. | STATIC | PX-073 |

### H. Desktop surface and agents

| ID | Requirement | Tag | Rows |
|---|---|---|---|
| CUC-H01 | While control is held the desktop shows presence in every window: who holds it (agent or person), the target application, the scope, and a Stop control that is always one click or one chord away. A transcript card shows a numbered action list with a screenshot thumbnail and the approval that bound each action. | STATIC | PX-074 |
| CUC-H02 | Computer use is available to a task only when its mode and profile allow it; the AGENT mode may request it, ASK and PLAN modes cannot, and a mode switch never carries a grant across. | UNVERIFIED (design choice) | PX-070, PX-075 |

## 5. Rows and evidence tiers

Every row is release-critical: each changes effect-bearing behaviour, permissions or policy, execution, recovery, protocol or schema, a security boundary or evidence semantics (doc 83); none is eligible for the iteration tier.

| Row | Title | Owner | Prerequisites |
|---|---|---|---|
| PX-069 | Computer Runtime contract: typed computer tools, scopes, handles, refusal taxonomy and unknown-outcome latch | browser | M7.6, M5.1, DOC-PX-008 |
| PX-070 | Per-call exact-intent approvals for native control, never allowlistable | effects-security | M2.5, M9.2, PX-057, DOC-PX-008 |
| PX-071 | macOS actuator helper: separate signed process, authenticated local RPC, accessibility-first background control, takeover lease and stop | browser | PX-069, M7.6, M9.5, DOC-PX-008 |
| PX-072 | Actuator supply chain: bundled signed helper, hash-pinned manifest, rollback and process identity | browser | PX-071, M10.2, DOC-PX-008 |
| PX-073 | Browser control hardening: CDP deny list, per-call origin gate, certificate trust, permissions and per-task view ownership | browser | M7.6, M7.7, DOC-PX-008 |
| PX-074 | Computer-control desktop surface: permission window, presence and Stop, action cards and approval card | desktop | PX-058, PX-069, PX-070, PX-071, DOC-PX-008 |
| PX-075 | Computer-use subagent profile with an isolated target, constrained tools and stall rules | core-runtime | M6.3, PX-039, PX-051, PX-069, DOC-PX-008 |
| PX-076 | Screenshot and accessibility artifacts: window capture, masking, content-aware codec, retention and audit | media | PX-069, M2.10, DOC-PX-008 |

## 6. Supersessions and clarifications

Nothing sealed changes beyond the scope below (in force from 2026-10-05). The explicit records, also entered in docs 02 and 03:

| Earlier decision, row or text | Kind | Scope and effect | What survives |
|---|---|---|---|
| doc 10 non-goal: pixel-only computer-use product | CLARIFIED, NOT SUPERSEDED | Native control is semantic-first (accessibility tree, then coordinates against a fresh observation, then raw input). A pixel-only product stays a non-goal. | The non-goal as written; doc 22's locked direction that Modbit does not operate primarily from screenshots. |
| REQ-EV-0083 to REQ-EV-0090 (ADOPT, Computer Runtime) | FULFILLED, NOT CHANGED | The rows already require exact application identity, a single controller lock, a watchdog, human preemption and a typed failure taxonomy; PX-069 to PX-076 implement them for native applications. | Every row text, disposition and owner. |
| doc 17 `computer.observe` and `computer.action` rows | IMPLEMENTED BY | The tool family of PX-069 registers under the existing Computer Runtime owner (the `browser` subsystem). | The rows. |

## 7. Traceability: requirement to row, tag and source

| Requirement | Rows | Verification tag | Source report section |
|---|---|---|---|
| CUC-A01 | PX-069, PX-071 | STATIC | F03 §1 F-A1, doc 17, doc 81 |
| CUC-A02 | PX-069, PX-071, PX-074 | STATIC | F03 §2 F-B2, F03 §1 F-A2 |
| CUC-A03 | PX-069, PX-071 | UNVERIFIED (design choice) | doc 76, doc 75 |
| CUC-A04 | PX-069 | STATIC | F03 §2 F-B2, F03 §2 F-B3 |
| CUC-A05 | PX-075 | STATIC | F03 §1 F-A3 |
| CUC-B01 | PX-069 | STATIC | F03 §2 F-B1, F03 §2 F-B5 |
| CUC-B02 | PX-069, PX-076 | STATIC | F03 §2 F-B6 |
| CUC-B03 | PX-069 | STATIC | F03 §2 F-B2, F03 §2 F-B3 |
| CUC-B04 | PX-069, PX-076 | STATIC | F03 §2 F-B1, F03 §2 F-B9 |
| CUC-B05 | PX-069 | STATIC | F03 §2 F-B5, doc 40 REQ-EV-0090 |
| CUC-B06 | PX-069, PX-070 | STATIC | F03 §6 F-F8, doc 22 |
| CUC-C01 | PX-070 | STATIC (reference approval class: OS-level control always asks); UNVERIFIED (no live observation) | F03 §3 F-C2, doc 23 |
| CUC-C02 | PX-070 | UNVERIFIED (design choice) | F03 §3 F-C2 |
| CUC-C03 | PX-070, PX-074 | STATIC | F03 §2 F-B2, F03 §2 F-B10 |
| CUC-C04 | PX-070 | STATIC | F03 §3 F-C3 |
| CUC-C05 | PX-070 | UNVERIFIED (design choice) | doc 52 |
| CUC-D01 | PX-069 | STATIC | F03 §2 F-B8, doc 40 REQ-EV-0089 |
| CUC-D02 | PX-069 | STATIC | F03 §2 F-B9 |
| CUC-D03 | PX-069 | STATIC | F03 §2 F-B9, doc 23 |
| CUC-D04 | PX-071 | STATIC | F03 §2 F-B10, doc 40 REQ-EV-0087 |
| CUC-D05 | PX-069, PX-071, PX-074 | STATIC | F03 §2 F-B10, F03 §3 F-C4, doc 40 REQ-EV-0085 |
| CUC-D06 | PX-075 | STATIC | F03 §6 F-F9, doc 28 |
| CUC-E01 | PX-071, PX-072 | STATIC | F03 §4 F-D1, F03 §4 F-D2 |
| CUC-E02 | PX-071 | STATIC | F03 §4 F-D2, F03 §4 F-D3 |
| CUC-E03 | PX-072 | STATIC | F03 §4 F-D1 |
| CUC-E04 | PX-072 | STATIC (reference download path); UNVERIFIED (Modbit choice) | F03 §4 F-D1, doc 70 |
| CUC-E05 | PX-074 | STATIC | F03 §3 F-C1 |
| CUC-F01 | PX-076 | STATIC (privacy classification of screenshots as sensitive) | F03 §5 F-E4, doc 25 |
| CUC-F02 | PX-076 | STATIC | F03 §2 F-B7 |
| CUC-F03 | PX-070, PX-076 | STATIC | F03 §5 F-E4, doc 23 |
| CUC-G01 | PX-073 | STATIC | F03 §6 F-F5 |
| CUC-G02 | PX-073 | STATIC | F03 §3 F-C3 |
| CUC-G03 | PX-073, PX-074 | STATIC | F03 §6 F-F8 |
| CUC-G04 | PX-073 | STATIC | F03 §6 F-F5, F03 §6 F-F7 |
| CUC-G05 | PX-073 | STATIC | F03 §6 F-F6, doc 22 |
| CUC-H01 | PX-074 | STATIC | F03 §1, F01 §4.3 |
| CUC-H02 | PX-070, PX-075 | UNVERIFIED (design choice) | doc 65 AFW-D05 |

### Row to requirements

| Row | Requirements |
|---|---|
| PX-069 | CUC-A01, CUC-A02, CUC-A03, CUC-A04, CUC-B01, CUC-B02, CUC-B03, CUC-B04, CUC-B05, CUC-B06, CUC-D01, CUC-D02, CUC-D03, CUC-D05 |
| PX-070 | CUC-B06, CUC-C01, CUC-C02, CUC-C03, CUC-C04, CUC-C05, CUC-F03, CUC-H02 |
| PX-071 | CUC-A01, CUC-A02, CUC-A03, CUC-D04, CUC-D05, CUC-E01, CUC-E02 |
| PX-072 | CUC-E01, CUC-E03, CUC-E04 |
| PX-073 | CUC-G01, CUC-G02, CUC-G03, CUC-G04, CUC-G05 |
| PX-074 | CUC-A02, CUC-C03, CUC-D05, CUC-E05, CUC-G03, CUC-H01 |
| PX-075 | CUC-A05, CUC-D06, CUC-H02 |
| PX-076 | CUC-B02, CUC-B04, CUC-F01, CUC-F02, CUC-F03 |

## 8. Non-goals

| Not in scope | Why |
|---|---|
| A pixel-only computer-use product | Doc 10 non-goal stands. Native control is semantic-first; screenshots and raw input are the lower rungs. |
| Native control inside cloud sandboxes | The cloud sandbox's own browser and desktop are existing surfaces (M8); this record is local only. |
| Windows and Linux actuators | No evidence base; separate promotion (doc 76). |
| Mobile and simulator control | MOD-MOBILE-001 (DEFERRED) stands. |
| Screen recording and polished replay video | Not needed for control; a later record if evidence-video is wanted. |
| Design mode and element-pick annotation in the browser | A UI authoring feature, not control; not in these rows. |
| A server-side next-action decider for GUI steps | Would add a second planning loop outside the Core; not adopted. |

## 9. Still unverified

| ID | Unverified fact | Evidence that would close it |
|---|---|---|
| V01 | How the native actuator captures and injects on the platform: capture API, input injection, overlay design, accessibility walker timing | Modbit's own actuator built and measured on fixture applications; the reference helper is not in the shipped bundle and was never run. |
| V02 | Accessibility tree completeness under load (a truncated walk was reported in the reference tests) | Fixture applications with large trees; compare element counts across repeated reads and under CPU load. |
| V03 | Latency and image-token cost of observation per scope | Measured sessions on fixtures; budgets recorded in the benchmark plan. |
| V04 | Behaviour on the secure desktop, locked screen, elevated windows and full-screen spaces | Real-machine fault injection with a locked screen and an elevated application. |
| V05 | Whether per-grant observation approval is acceptable or per-call observation approval is required | Owner decision (question in DR-PX-2026-10-03-008). |
| V06 | Windows and Linux actuators | Per-platform research and a packaged E2E; promotion by a Decision Record (doc 76). The reference's Windows facts are STATIC only and Linux has no reference backend. |
| V07 | Whether any reference approval wording exists for native control | A live observation with the reference product's computer use enabled; it was gated off in the sampled profile (DISK). |
