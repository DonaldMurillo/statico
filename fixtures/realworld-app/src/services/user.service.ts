import type { User } from '../types/user';
import { db } from '../lib/db';

export function getUser(id: number): User {
  return { id, name: '', email: '' };
}
