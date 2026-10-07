import { handleGet } from "./routes";

export function start(): string {
  return handleGet({ params: { id: "1" } });
}
