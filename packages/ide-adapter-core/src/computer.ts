/**
 * Typed commands of native computer control's desktop surface (PX-074; docs/66
 * CUC-D05, CUC-E05, CUC-H01), over the one SurfaceProtocol client. Thin: the
 * Core decides who holds the machine, which operating-system permissions the
 * actuator reports, what a Stop does and whether a latch may be resolved; these
 * return its answer unchanged. Stop and the latch resolution take the session
 * lease the client holds, so a client that does not hold it is refused by the
 * Core, never by this file. There is no capture or input call here: the
 * actuator is the Core's, and the desktop only asks.
 */
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  ComputerActivityViewSchema,
  ComputerLatchResolvedSchema,
  ComputerPresenceViewSchema,
  ComputerRuntimeViewSchema,
  ComputerStopSchema,
  ComputerStoppedSchema,
  GetComputerPresenceSchema,
  GetComputerRuntimeSchema,
  ListComputerActivitySchema,
  ResolveComputerLatchSchema,
  type ComputerActivityView,
  type ComputerLatchResolved,
  type ComputerPresenceView,
  type ComputerRuntimeView,
  type ComputerStopped,
} from "@modbit/surface-protocol";
import { unhex, type CoreClient } from "./client.ts";

const id = (hex: string) => ({ value: unhex(hex) });

/** Who holds the machine (every task), the actuator, and the permissions it reports. Not task-scoped. */
export async function getComputerPresence(c: CoreClient): Promise<ComputerPresenceView> {
  const ack = await c.command("GetComputerPresence", toBinary(GetComputerPresenceSchema, create(GetComputerPresenceSchema, {})));
  return fromBinary(ComputerPresenceViewSchema, ack.result);
}

/** One task's runtime: its control session, grant, latch and counters. */
export async function getComputerRuntime(c: CoreClient, taskId: string): Promise<ComputerRuntimeView> {
  const ack = await c.command("GetComputerRuntime", toBinary(GetComputerRuntimeSchema, create(GetComputerRuntimeSchema, { taskId: id(taskId) })));
  return fromBinary(ComputerRuntimeViewSchema, ack.result);
}

/** The task's computer events from the log: kinds, counts, outcomes and masked frame references, never contents. */
export async function listComputerActivity(c: CoreClient, taskId: string): Promise<ComputerActivityView> {
  const ack = await c.command("ListComputerActivity", toBinary(ListComputerActivitySchema, create(ListComputerActivitySchema, { taskId: id(taskId) })));
  return fromBinary(ComputerActivityViewSchema, ack.result);
}

/**
 * The on-screen Stop: the Core stops the actuator at once, closes the task's
 * control session and makes the stop sticky for the rest of the run
 * (USER_ABORTED).
 */
export async function computerStop(c: CoreClient, sessionId: string, taskId: string, reason: string): Promise<ComputerStopped> {
  const payload = toBinary(ComputerStopSchema, create(ComputerStopSchema, { taskId: id(taskId), reason }));
  const ack = await c.command("ComputerStop", payload, undefined, c.leaseGeneration(sessionId), sessionId);
  return fromBinary(ComputerStoppedSchema, ack.result);
}

/** The person looked at the machine and resolves an unknown-outcome latch. The model still needs a new session, a fresh observation and a new approval to act. */
export async function resolveComputerLatch(c: CoreClient, sessionId: string, taskId: string, note: string): Promise<ComputerLatchResolved> {
  const payload = toBinary(ResolveComputerLatchSchema, create(ResolveComputerLatchSchema, { taskId: id(taskId), note }));
  const ack = await c.command("ResolveComputerLatch", payload, undefined, c.leaseGeneration(sessionId), sessionId);
  return fromBinary(ComputerLatchResolvedSchema, ack.result);
}
