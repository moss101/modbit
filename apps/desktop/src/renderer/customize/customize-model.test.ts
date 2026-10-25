import assert from "node:assert/strict";
import { test } from "node:test";
import { budgetText, extensionState, lastResult, offersCatalog, offersRevoke, offersTrust, refusalWords, scopeWords, serverWords, shareText, shortHash, trustWords, whyLocked } from "./customize-model.ts";

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
  assert.equal(lastResult({ invocations: 0, lastOutcome: "", lastDetail: "", lastDurationMs: "0" }), "never ran");
  assert.equal(lastResult({ invocations: 2, lastOutcome: "OK", lastDetail: "", lastDurationMs: "7" }), "OK after 7 ms (2 runs)");
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
