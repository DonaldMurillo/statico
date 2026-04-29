export function query<T>(sql: string, params: unknown[] = []): T[] {
  console.log(`Executing: ${sql}`, params);
  return [];
}

export function transaction<T>(fn: () => T): T {
  console.log('BEGIN');
  try {
    const result = fn();
    console.log('COMMIT');
    return result;
  } catch (err) {
    console.log('ROLLBACK');
    throw err;
  }
}
