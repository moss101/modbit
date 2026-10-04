import { test } from "node:test";
import assert from "node:assert/strict";
import { StopRegistry } from "./stop.ts";

test("FIX-19: an emergency stop holds until a new session lease is acquired above the one it was raised under, then the host's fence lifts", () => {
  const r = new StopRegistry();
  assert.equal(r.leaseAcquired("s1", 1), false, "no stop: nothing to lift");
  r.stop("s1", "stopped from the Browser panel");
  assert.equal(r.reason("s1"), "stopped from the Browser panel");
  // Another session is unaffected, both ways.
  assert.equal(r.reason("s2"), null);
  assert.equal(r.leaseAcquired("s2", 5), false);
  assert.equal(r.reason("s1"), "stopped from the Browser panel");
  // The same stop restated by the Core's event does not move what it lifts above.
  r.stop("s1", "stopped from the Browser panel");
  // A replayed or stale lease event (at or below the generation seen when the stop was raised) lifts nothing.
  assert.equal(r.leaseAcquired("s1", 1), false);
  assert.equal(r.reason("s1"), "stopped from the Browser panel");
  // A new lease lifts it.
  assert.equal(r.leaseAcquired("s1", 2), true);
  assert.equal(r.reason("s1"), null);
  // And a later stop is its own: it needs a lease above generation 2.
  r.stop("s1", "again");
  assert.equal(r.leaseAcquired("s1", 2), false);
  assert.equal(r.reason("s1"), "again");
  assert.equal(r.leaseAcquired("s1", 3), true);
  assert.equal(r.reason("s1"), null);
});

test("FIX-19: a stop raised after leases were seen lifts only above the highest of them; a malformed generation lifts nothing", () => {
  const r = new StopRegistry();
  r.leaseAcquired("s", 1);
  r.leaseAcquired("s", 4);
  r.stop("s", "");
  assert.equal(r.reason("s"), "emergency stop", "an empty reason still names the stop");
  assert.equal(r.leaseAcquired("s", 3), false);
  assert.equal(r.leaseAcquired("s", Number.NaN), false);
  assert.equal(r.leaseAcquired("s", 0), false);
  assert.equal(r.reason("s"), "emergency stop");
  assert.equal(r.leaseAcquired("s", 5), true);
  assert.equal(r.reason("s"), null);
});
