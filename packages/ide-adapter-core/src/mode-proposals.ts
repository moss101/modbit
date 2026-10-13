/**
 * Typed commands of the mode-switch proposal (REQ-PX-055; docs/65 AFW-D06),
 * over the one SurfaceProtocol client. A proposal is made by the agent through
 * the Core's own tool and is data until the person accepts it: there is no
 * function here that creates one. Deciding takes the session lease the client
 * holds; the Core refuses a client that does not hold it, a proposal that is
 * no longer pending (it expires to skipped after 15 s, never to accepted) and
 * an accept for a task that moved to another mode meanwhile.
 */
import { create, fromBinary, toBinary } from "@bufbuild/protobuf";
import {
  DecideModeProposalSchema,
  ListModeProposalsSchema,
  ModeProposalDecidedSchema,
  ModeProposalListSchema,
  type ModeProposalDecided,
  type ModeProposalList,
} from "@modbit/surface-protocol";
import { unhex, type CoreClient } from "./client.ts";

const id = (hex: string) => ({ value: unhex(hex) });

/** The proposals of a task, oldest first, with the Core's own clock for the time left. */
export async function listModeProposals(c: CoreClient, taskId: string): Promise<ModeProposalList> {
  const payload = toBinary(ListModeProposalsSchema, create(ListModeProposalsSchema, { taskId: id(taskId) }));
  return fromBinary(ModeProposalListSchema, (await c.command("ListModeProposals", payload)).result);
}

/** The person's answer. Accepting changes the mode through the Core's one mode path. */
export async function decideModeProposal(c: CoreClient, sessionId: string, taskId: string, proposalId: string, accept: boolean, commandId?: Uint8Array): Promise<ModeProposalDecided> {
  const payload = toBinary(DecideModeProposalSchema, create(DecideModeProposalSchema, { taskId: id(taskId), proposalId, accept }));
  const ack = await c.command("DecideModeProposal", payload, commandId, c.leaseGeneration(sessionId), sessionId);
  return fromBinary(ModeProposalDecidedSchema, ack.result);
}
