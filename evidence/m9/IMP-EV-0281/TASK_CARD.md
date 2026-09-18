# Task Card — IMP-EV-0281 Site-declared structured-action precedence (M9.4)

## Identity

- Task ID: IMP-EV-0281 (moved to M9 by DR-M7-001: a site-declared structured tool is an MCP server, and the gateway that lists and calls one is M9.4)
- Milestone: M9 Engineering memory / effects / security hardening (P0/P1)
- Requirements: REQ-EV-0281 (prefer authenticated site-declared structured tool when available, then derived semantic action, primitive, vision); QUAL-EV-0281 (same task selects native tool when trust/policy allow; fallback works otherwise); docs/22 "Action hierarchy" rung 1.
- Qualification: the real Core, a real MCP server process bound to a site's origin, and a page with one protected action.
- Evidence tier: real-system.

## Goal

When a site offers a structured way to do the consequential thing, the agent does it that way — and when it does not, the agent drives the interface exactly as M7.4 built it.

## Existing-code audit

- classification: PARTIAL before this task. The ladder's rungs 2–4 are built and proven in M7 — derived semantic action by reference (M7.4), primitive structural CDP action (the host), targeted vision (M7.5) — with the precedence among them proven in `qual_ev_0082_0234_0277_0282_…`. Rung 1 did not exist: nothing bound a tool server to an origin, and nothing in `browser.act` knew a better path was available. DR-M7-001 moved the task to M9 because rung 1 is an external tool server and the hub that lists one arrived with M9.4.
- production entry points: `crates/mcp/src/config.rs` (`ServerConfig::sites`, `serves_site`, in the fingerprint); `crates/mcp/src/port.rs` (`SiteTools`, `SiteServerUnavailable`, `McpPort::for_site`); `services/modbit-core/src/mcp.rs` (`for_site` touches only the servers bound to the page's origin, so reading a page never starts an unrelated server); `crates/tools/src/browser.rs` (`site_tools_json` on every page read; the `SITE_TOOL_PREFERRED` refusal for a protected action and its entry in `FAILURE_TAXONOMY`).

## What "site-declared" means here, and why

The binding is the **host's** declaration: its configuration names the origins a trusted server serves. A page is untrusted content (docs/23), so a declaration read from a page could never by itself make a program callable — at most it is a *proposal*, which is exactly the path IMP-EV-0224 built (`ProposeExternalServer` → a person trusts it). The qualification takes that path: the server is proposed for `https://shop.test`, is untrusted, and only becomes the preferred way once a person trusts it. Reading a `<link rel="mcp-server">` from the page and turning it into a proposal automatically is a small, separate step this task does not take, because it would need the browser host to surface page metadata the accessibility tree does not carry.

## Ordering

The Capability Kernel judges a call's effect class before any effector runs, and it sees only the class — never the page. So for a protected action the approval is asked first and the `SITE_TOOL_PREFERRED` refusal comes from the effector after it. A model that read the page (where `site_tools.prefer` and the tool's name are stated) never reaches that point; the refusal is the backstop for one that acted against the read.

## Limitations

- Only a **protected** action prefers the site tool. A page-only action (filling a field, toggling) still goes through the interface: the preference is about the consequential thing, not about every interaction.
- The host does not map an entity to a particular site tool; it names every tool the bound server declares and leaves the choice to the agent. A per-entity mapping would need the server to describe the page, which is the site describing itself — untrusted again.
- `sites` is compared as an exact origin (scheme, host and port). No wildcards, no subdomain matching.
- An approval asked for a click that is then refused is wasted; see "Ordering".

## Verification

- `services/modbit-core/tests/surface_protocol.rs::qual_ev_0281_a_site_tool_is_preferred_for_a_protected_action_when_trust_allows_and_the_page_is_the_fallback`
- `crates/mcp` `config::tests::a_server_serves_only_the_origins_the_host_bound_it_to`, `config::tests::the_fingerprint_separates_what_makes_a_different_server`

## Evidence

- `evidence.json` in this directory (commits, hosted CI run, test names)
