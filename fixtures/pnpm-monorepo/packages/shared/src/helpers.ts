export const helpers = {
  map: <T>(items: T[]): T[] => [...items],
  filter: <T>(items: T[], fn: (item: T) => boolean): T[] => items.filter(fn),
};
