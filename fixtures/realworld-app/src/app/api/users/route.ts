import { db } from '../../../lib/db';
import { getUser } from '../../../services/user.service';
import type { User } from '../../../types/user';

export async function GET() {
  const users: User[] = await db.query('SELECT * FROM users');
  return Response.json(users);
}
