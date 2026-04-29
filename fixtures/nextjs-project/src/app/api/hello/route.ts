import { connect } from '../../lib/db';

export async function GET() {
  const db = connect();
  return Response.json({ ok: true });
}
