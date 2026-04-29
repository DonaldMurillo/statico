export function legacyFormatDate(date: Date): string {
  return date.toISOString();
}

export function legacyCalculateHash(data: string): string {
  let hash = 0;
  for (let i = 0; i < data.length; i++) {
    const char = data.charCodeAt(i);
    hash = (hash << 5) - hash + char;
    hash |= 0;
  }
  return hash.toString(16);
}

export function legacyTransformInput(input: unknown): string {
  return JSON.stringify(input);
}
