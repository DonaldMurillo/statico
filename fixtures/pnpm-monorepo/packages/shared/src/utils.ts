export const utils = {
  id: (): string => crypto.randomUUID(),
  slug: (text: string): string => text.toLowerCase().replace(/\s+/g, '-'),
};
