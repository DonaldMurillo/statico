export function Card({ title }: { title: string }): string {
  return `[Card: ${title}]`;
}

export function List({ items }: { items: string[] }): string[] {
  return items.map((item, i) => `${i}: ${item}`);
}
