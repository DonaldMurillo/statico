export interface User {
  id: string;
  name: string;
  email: string;
}

export function getUsers(): User[] {
  return [
    { id: '1', name: 'Alice', email: 'alice@example.com' },
    { id: '2', name: 'Bob', email: 'bob@example.com' },
  ];
}

export function getUserById(id: string): User | undefined {
  return getUsers().find((u) => u.id === id);
}
