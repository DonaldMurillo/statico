export function old(): void {
  console.warn('This function is deprecated');
}

export function legacyFormat(data: unknown): string {
  return JSON.stringify(data);
}
