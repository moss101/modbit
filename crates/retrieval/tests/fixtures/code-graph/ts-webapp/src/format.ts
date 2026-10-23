import { User } from "./types";

export function format(u: User): string {
  return u.name.toUpperCase();
}

export function legacyFormat(): string {
  return "";
}
