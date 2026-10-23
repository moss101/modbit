import { Repository, User } from "./types";

export class InMemoryUserRepo implements Repository<User> {
  private items: User[] = [];

  find(id: string): User | undefined {
    return this.items.find((u) => u.id === id);
  }

  save(item: User): void {
    this.items.push(item);
  }
}

export class AuditedRepo extends InMemoryUserRepo {
  save(item: User): void {
    super.save(item);
  }
}

export function connect(): Repository<User> {
  return new InMemoryUserRepo();
}
