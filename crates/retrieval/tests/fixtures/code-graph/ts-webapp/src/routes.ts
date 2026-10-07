import { describeUser } from "./service";

export function handleGet(req: { params: { id: string } }): string {
  return describeUser(req.params.id);
}
