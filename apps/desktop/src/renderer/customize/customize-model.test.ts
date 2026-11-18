import assert from "node:assert/strict";
import { test } from "node:test";
import { budgetText, extensionState, lastResult, offersCatalog, offersRevoke, offersShare, memoryWhere, shareWords, offersTrust, refusalWords, scopeWords, serverWords, shareText, shortHash, trustWords, whyLocked } from "./customize-model.ts";

test("PX-106: Trust is offered only where a trust decision can change something, and Revoke only for the owner's own", () => {
  assert.equal(offersTrust({ trust: "UNTRUSTED", scope: "USER" }), true);
  assert.equal(offersTrust({ trust: "CHANGED_SINCE_TRUSTED", scope: "PROJECT" }), true, "a changed file is untrusted again");
  for (const trust of ["TRUSTED_BY_OWNER", "SIGNED", "FORBIDDEN", "REVOKED", "DEV_FLAG", "SYSTEM"]) assert.equal(offersTrust({ trust, scope: "USER" }), false, trust);
  assert.equal(offersTrust({ trust: "UNTRUSTED", scope: "SYSTEM" }), false);
  assert.equal(offersRevoke({ trust: "TRUSTED_BY_OWNER" }), true);
  assert.equal(offersRevoke({ trust: "SIGNED" }), false);
});

test("PX-106: the words are the Core's facts, not new ones", () => {
  assert.equal(trustWords("UNTRUSTED"), "Not trusted: it does not reach the model");
  assert.equal(trustWords("SOMETHING_NEW"), "something new");
  assert.equal(shortHash("a".repeat(64)), "aaaaaaaaaaaa");
  assert.equal(budgetText({ indexBudgetTokens: 400, indexUsedTokens: 30, indexOmitted: 0 }), "30 of 400 index tokens used");
  assert.match(budgetText({ indexBudgetTokens: 400, indexUsedTokens: 400, indexOmitted: 2 }), /2 left out/);
  assert.equal(shareText({ indexed: true, indexTokens: 18, indexForm: "SHORT" }), "18 tokens (short)");
  assert.equal(shareText({ indexed: false, indexTokens: 0, indexForm: "NOT_INDEXED" }), "not in the index");
  // PX-207: the journal's outcome in words.
  assert.equal(lastResult({ invocations: 0, lastOutcome: "", lastDetail: "", lastDurationMs: "0" }), "Has not run yet");
  assert.equal(lastResult({ invocations: 2, lastOutcome: "OK", lastDetail: "", lastDurationMs: "7" }), "Last ran fine in 7 ms (2 runs)");
  assert.equal(lastResult({ invocations: 1, lastOutcome: "TIMEOUT", lastDetail: "slow", lastDurationMs: "9" }), "Last timed out in 9 ms (1 run): slow");
  assert.equal(scopeWords("ADMIN"), "administrator");
  assert.equal(whyLocked({ canDisable: true, scope: "USER" }), "");
  assert.match(whyLocked({ canDisable: false, scope: "ADMIN" }), /above you/);
  assert.match(serverWords({ state: "INERT", credentialRequired: true, credentialHeld: false }), /inert \(never started\), credential not held/);
  assert.match(serverWords({ state: "CONFIGURED", credentialRequired: false, credentialHeld: false }), /not probed/);
  assert.match(extensionState({ state: "QUARANTINED", quarantineReason: "unsigned" }), /Quarantined: unsigned/);
  assert.equal(extensionState({ state: "ACTIVE", quarantineReason: "" }), "Active");
  assert.match(refusalWords("HASH_MISMATCH"), /not the one you reviewed/);
  assert.match(refusalWords("HOOK_SCOPE_FORBIDS"), /above you/);
});

test("PX-106: no label offers a catalog, an install from the network or an update", () => {
  const ok = ["Trust this version", "Revoke", "Open source", "Switch off", "Switch on", "Unload", "Trust this manifest", "Import another agent's configuration…", "Refresh", "Close"];
  assert.deepEqual(offersCatalog(ok), []);
  for (const bad of ["Browse the marketplace", "Install from URL", "Update all", "Download", "Skill catalog", "Discover extensions"]) assert.deepEqual(offersCatalog([bad]), [bad], bad);
});

test("PX-188: Share is offered for live private items only, and the words say where an item lives and came from", () => {
  assert.equal(offersShare({ scope: "user:u1", status: "curated" }), true);
  assert.equal(offersShare({ scope: "session:s", status: "proposed" }), true);
  assert.equal(offersShare({ scope: "organization:t", status: "curated" }), false, "already broad");
  assert.equal(offersShare({ scope: "user:u1", status: "superseded" }), false, "not live");
  assert.equal(memoryWhere({ scope: "agent_profile:p", source: "agent_observed", author: "agent:x", status: "proposed" }), "agent profile scope · from agent observed by agent:x · proposed");
  assert.match(shareWords({ outcome: "shared", refusalCode: "", scope: "organization:t", existed: false }), /personal detail removed/);
  assert.match(shareWords({ outcome: "refused", refusalCode: "SHARE_NOT_OWNER", scope: "", existed: false }), /SHARE_NOT_OWNER/);
});

// ------------------------------------------------------------------ PX-207

import { baseName, customizeHits, hookName, hookWhen, layerWords, ruleSize, ruleWhen, rulesOf, signatureWords, skillKey, skillScopeWords, skillTag, type RuleLayer } from "./customize-model.ts";
import type { HookRow, SkillRow } from "../../shared/customize-types.ts";

const layer = (o: Partial<RuleLayer>): RuleLayer => ({ layer: "project", id: "x", source: "/r/.modbit/rules/x.md", contentHash: "a".repeat(64), reason: "GLOBAL", loaded: true, notLoadedReason: "", bytes: "10", truncated: false, scannerFindings: [], ...o });
const skillRow = (o: Partial<SkillRow>): SkillRow => ({ name: "s", version: "1", description: "", scope: "USER", contentHash: "a".repeat(64), trust: "UNTRUSTED", trustDetail: "", enabled: false, invocation: "BOTH", paths: [], pathsActive: false, indexTokens: 0, indexed: false, indexForm: "", source: "", provenanceSource: "", provenanceAuthor: "", provenanceLicense: "", lifecycle: "", ...o });
const hookRow = (o: Partial<HookRow>): HookRow => ({ id: "config:user/h", source: "", point: "before_tool", mode: "observe", failPolicy: "closed", timeoutMs: "0", tools: [], command: ["sh"], scope: "USER", enabled: true, canDisable: true, trustedWorkspaceGate: false, lastOutcome: "", lastDetail: "", lastDurationMs: "0", invocations: 0, ...o });

test("PX-207: Rules shows only the layers the Core recorded as loaded, in its order; the rest stand apart", () => {
  const all = [layer({ source: "AGENTS.md", layer: "repo-instructions" }), layer({ source: "/r/b.md", loaded: false, layer: "", notLoadedReason: "unreadable" }), layer({ source: "/u/rules/mine.md", layer: "user" })];
  const { loaded, notLoaded } = rulesOf(all);
  assert.deepEqual(loaded.map((l) => l.source), ["AGENTS.md", "/u/rules/mine.md"]);
  assert.deepEqual(notLoaded.map((l) => l.source), ["/r/b.md"]);
  assert.equal(layerWords("repo-instructions"), "This repository's instructions");
  assert.equal(layerWords("extension:kit"), "From the extension kit");
  assert.equal(ruleWhen("GLOBAL"), "Always in force");
  assert.equal(ruleWhen("PATH:src/a.rs matched src/**"), "In force because src/a.rs matches src/**");
  assert.equal(ruleSize({ bytes: "2048", truncated: true }), "2.0 KB, cut to fit the prompt");
  assert.equal(baseName("/r/.modbit/rules/style.md"), "style.md");
  assert.equal(baseName("C:\\r\\rules\\style.md"), "style.md");
});

test("PX-207: the one search finds rows by their words and names the section each opens; a rule the Core did not load is never a result", () => {
  const d = {
    skills: [skillRow({ name: "kestrel-notes", description: "Tidies notes" }), skillRow({ name: "other", description: "Drafts merged work" })],
    rules: [layer({ source: "/r/.modbit/rules/kestrel.md" }), layer({ source: "/r/kestrel-broken.md", loaded: false })],
    hooks: [hookRow({ id: "config:user/kestrel-watch" })],
    extensions: [],
    memory: [{ id: "m1", topic: "Kestrel", content: "facts", scope: "user:me" }],
  };
  const hits = customizeHits("KESTREL", d);
  assert.deepEqual(hits.map((h) => `${h.section}:${h.key}`), ["skills:USER/kestrel-notes", "rules:/r/.modbit/rules/kestrel.md", "hooks:config:user/kestrel-watch", "memory:m1"]);
  assert.deepEqual(customizeHits("merged work", d).map((h) => h.key), ["USER/other"], "every word must match, in any field");
  assert.deepEqual(customizeHits("   ", d), []);
  assert.equal(skillKey({ scope: "PROJECT", name: "a" }), "PROJECT/a");
});

test("PX-207: the pages say the Core's codes in words", () => {
  assert.equal(skillScopeWords("USER"), "yours");
  assert.deepEqual(skillTag({ trust: "UNTRUSTED", enabled: false }), { text: "Not trusted", tone: "warn" });
  assert.deepEqual(skillTag({ trust: "TRUSTED_BY_OWNER", enabled: true }), { text: "Trusted", tone: "ok" });
  assert.deepEqual(skillTag({ trust: "FORBIDDEN", enabled: false }), { text: "Forbidden", tone: "danger" });
  assert.equal(hookWhen({ point: "before_tool", mode: "intercept", failPolicy: "open" }), "Before a tool runs · can stop or change the step · if it fails the step goes ahead");
  assert.equal(hookName("config:user/rec"), "rec");
  assert.equal(signatureWords("UNSIGNED"), "not signed");
  assert.equal(signatureWords("VERIFIED:acme"), "signed by acme");
});

import { commandWords, folderOfRules, notLoadedWords, shortPath, toolWords, transportWords } from "./customize-model.ts";

test("PX-207: paths are short in a row and whole in its title; a hook says what it runs and when in words", () => {
  assert.equal(shortPath("/work/repo/.modbit/rules/a.md", { root: "/work/repo" }), ".modbit/rules/a.md");
  assert.equal(shortPath("/Users/ann/Library/Modbit/rules/mine.md", { user: "ann" }), "~/Library/Modbit/rules/mine.md");
  assert.equal(shortPath("/home/ann/x.md", { user: "ann" }), "~/x.md");
  assert.equal(shortPath("/var/folders/ab/T/profile/rules/mine.md"), "…/profile/rules/mine.md");
  assert.equal(shortPath("AGENTS.md", { root: "/work/repo" }), "AGENTS.md");
  assert.equal(folderOfRules([{ layer: "repo-instructions", source: "AGENTS.md" }, { layer: "project", source: "/work/repo/.modbit/rules/a.md" }]), "/work/repo");
  assert.equal(hookWhen({ point: "before_tool", mode: "observe", failPolicy: "closed", tools: ["fs.read"] }), "Before reading files · only watches · if it fails the step does not go ahead");
  assert.equal(toolWords(["fs.read", "shell.exec"]), "reading files or running commands");
  assert.equal(commandWords(["sh", "/tmp/a/kestrel-watch.sh"]), "kestrel-watch.sh");
  assert.equal(commandWords(["cargo", "fmt"]), "cargo with 1 argument");
  assert.equal(transportWords("stdio"), "runs on this machine");
  assert.equal(notLoadedWords("not loaded: a repository's AGENTS.md is in force only once the session trusts the repository (TrustRepository)"), "a repository's AGENTS.md is in force only once the session trusts the repository");
});
