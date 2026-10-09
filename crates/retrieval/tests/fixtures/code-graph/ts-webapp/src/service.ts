import { connect } from "./db";
import { format } from "./format";
import { User } from "./types";

export function getUser(id: string): User | undefined {
  return connect().find(id);
}

export function describeUser(id: string): string {
  const u = getUser(id);
  return u ? format(u) : "unknown";
}
