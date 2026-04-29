export function orphanFn(): void {
  console.log('Nobody calls me');
}

export function orphanHelper(value: string): string {
  return value.toUpperCase();
}
