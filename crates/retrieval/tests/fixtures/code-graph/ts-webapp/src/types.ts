export type UserId = string;

export interface User {
  id: UserId;
  name: string;
}

export interface Repository<T> {
  find(id: string): T | undefined;
  save(item: T): void;
}
