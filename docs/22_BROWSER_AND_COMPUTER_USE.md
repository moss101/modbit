# Browser and Computer-Use Architecture

> **Authority date:** 2026-09-03  
> **Product:** Modbit — clean-slate implementation dossier  
> **Status vocabulary:** **LOCKED**, **PROVISIONAL**, **EXPERIMENT**, **DEFERRED**, **REJECTED**  
> **Source-of-truth rule:** latest explicit Modbit decision > locked decisions > current dossier > older project documents. Older Code-OSS/Modbit Lite material is historical only when it conflicts with this dossier.


## Locked direction

Modbit does **not** build a new browser engine and does not operate primarily from screenshots. It builds an agent-native semantic runtime over Chromium while exposing the exact same live session to the user.

## Local browser

Electron main creates a sandboxed `WebContentsView` in a dedicated session partition with Node disabled and strict context isolation. A Browser Bridge controls that same `webContents` through Chrome DevTools Protocol. The renderer only hosts/choreographs the view; untrusted page content never gets Modbit privileged APIs.

As built (M7.1): `apps/desktop/src/main/browser.ts` (`BrowserHost`) creates one `WebContentsView` per browser session in the session's own persisted partition (`persist:modbit-browser-<id>`) with `sandbox`, `contextIsolation`, no `nodeIntegration` (also none in workers or subframes), no preload, no `webviewTag`; the view opens no windows, downloads nothing, is granted no permission or device, and is never navigated anywhere but http(s) (`will-navigate` / `will-redirect` blocked). The host attaches the view to the Core's session over the desktop's authenticated surface connection (`AttachBrowserHost`, a `browser.host` client capability only the desktop kind holds — REQ-EV-0110 — stating the view's isolation; the Core refuses a view without it, `HOST_NOT_ISOLATED`, and a partition other than the session's) and answers the Core's `BrowserHostRequest` frames on that connection (`BrowserHostResponse`) through `webContents.debugger` (CDP 1.3): `navigate` (`loadURL`, the settled page state), `state`, `snapshot` (`Accessibility.getFullAXTree`, bounded, unignored nodes with role, name, value and depth), `capture` (`Page.captureScreenshot`, a clip), `isolation` (asked of the page: `process`, `require`, `window.electron` all absent), `close`. `crates/browser` is the protocol (`BrowserSessionId`, `ControlLease` with `Controller` and a fencing generation, `HostRequest` / `HostResponse`, `PageState` with its fingerprint, `AxNode`, `BrowserPort`, `navigable`); `services/modbit-core/src/browser.rs` owns the sessions (open per task, journaled `BrowserSessionOpened` / `BrowserHostAttached` / `BrowserNavigated` / `BrowserSessionClosed`, rebuilt from the task's events after a restart so a host attaches again to the same partition; a host that disconnects fails every request it owed `HOST_GONE`; a request under the user's control lease is refused `USER_HAS_CONTROL` before it reaches the host); `crates/tools/src/browser.rs` is the family (`browser.navigate`, `browser.snapshot`: ReadOnly, `browser.control` in the trusted and autonomous leases, `NAVIGATION_BLOCKED` for anything but http(s) before anything is sent, every page-derived string tagged `UNTRUSTED_WEB_CONTENT`; a navigation is progress, reading the page again is not). The renderer's Browser panel places the view over its placeholder and shows only what the host and the Core report (URL, title, state version, the lease, the isolation probe) — page content never enters the window. Proven on the real Core with a fake host over the real protocol (`qual_m7_1_…`) and in the real app against a real page (`apps/desktop/e2e/browser.spec.ts`): the agent's navigation and snapshot reach the model tagged untrusted with the page's instruction-shaped title left as data; the view the person opens is the one the agent drove. Semantic entities and stable references (M7.2), deltas (M7.3), semantic actions (M7.4), the visual fallback (M7.5), the takeover (M7.6; the lease and its fencing exist) follow.

## Cloud browser

Chromium runs inside the isolated MicroVM. Browser Bridge connects through authenticated remote CDP; user view is streamed through remote display/WebRTC/VNC-style transport. Structural and visual control share one BrowserSessionId.

## Semantic Browser Compiler

```text
Chromium/CDP
  ├─ Accessibility tree
  ├─ DOM/layout metadata
  ├─ URL/navigation/network state
  ├─ form/control state
  └─ targeted screenshot regions
          ↓
Semantic Compiler
  ├─ persistent semantic entity IDs
  ├─ page classification
  ├─ action derivation
  ├─ intent filtering
  ├─ state fingerprint
  ├─ delta/diff generation
  └─ page transition graph
          ↓
Agent-facing actions
```

Inspired mechanisms include accessibility snapshots/stable refs, page diffs, semantic action grouping, state graphs and targeted visual fallback. Full AX snapshots are not repeatedly dumped into context; after initial state, incremental semantic patches are preferred.

As built (M7.2, the first stage): `crates/browser/src/compiler.rs` fuses the host's raw tree — each accessible node with the DOM node behind it (`backendDOMNodeId`), its landmark ancestry and, for actionable nodes, the layout box the host fetched (`DOM.getBoxModel`, bounded) — into a compact `PageEntities`: actions (buttons, links, menu items, tabs…), fields (text boxes, combo boxes, check boxes, radios…) and landmarks (main, navigation, form, dialog, headings…), each with a **stable reference** (12 hex of sha256 over role, accessible name, landmark path and ordinal among look-alikes — never a DOM node id, which is per state version and never leaves the Core), plus the page's visible text (headings, paragraphs; bounded) as data, and an entity hash that is the same for two versions with the same identities. `browser.snapshot` returns the compiled page; `browser.inspect {ref}` compiles the page again and resolves the reference by identity: the same element at its new version (new node id, current value and box), or `TARGET_STALE` naming the look-alikes that remain (same role and name, by reference) — never the node that now sits where the element was (REQ-EV-0278). `BrowserPort::remember_page` / `known_entity` keep what every compiled page named, so a stale reference is explained even after several re-renders. Proven with a fake host that re-renders the page with fresh node ids and renames the form's button (`qual_m7_2_…`), and in the real app on a real page re-served with the button renamed (`browser.spec.ts`): the field's reference resolves again with its box, the button's reference is stale with the outer look-alike named. Deltas (M7.3), actions on references with postconditions (M7.4) and the page transition graph follow.

## Action hierarchy

1. Native website-declared or browser-native structured action when trusted and policy-allowed.
2. Derived semantic action (`fill login.email`, `click checkout.submit`).
3. Primitive structural CDP action.
4. Targeted screenshot/vision action for canvas/unlabeled/visual-only region.
5. Full screenshot only as last-resort diagnostic evidence.

## Live user takeover

Browser session has a control lease. User takeover immediately blocks agent input, not observation. Returning control increments lease generation to fence stale input events. The session does not restart.

## Prompt-injection isolation

Page text, ARIA labels, DOM attributes, downloads and site-provided tools are untrusted evidence. Browser content cannot:
- change system policy;
- request hidden secrets;
- widen capabilities;
- approve effects;
- instruct the agent to ignore the user/system goal.

Context Compiler tags source and strips/segments browser content from trusted instruction channels.

## Credentials

Login data is supplied only through an explicit user-approved credential broker. The model receives field handles/status, not password values. Browser Bridge can fill a credential handle directly into a bound origin/field under policy.

## Verification

Actions can declare postconditions: URL/state fingerprint change, DOM/AX value, network response, downloaded artifact or screenshot region. Protected external actions require evidence before the Effect Ledger receipt closes.


## V2 media interaction

Browser screenshots/regions, uploaded screenshots and tool-returned images all use the same `MediaEnvelope`/Artifact Store path. Full-page screenshots are not the normal perception loop. Structural browser state remains primary; vision receives only targeted regions/pages when semantic state is insufficient, and any visual fallback records reason, source state version and post-action verification.
