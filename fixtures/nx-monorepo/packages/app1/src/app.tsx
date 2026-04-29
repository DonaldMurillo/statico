import { Card, List } from '@mono/ui';
import { fetchData } from '@mono/data';

export function renderApp(): void {
  const card = Card({ title: 'Nx App' });
  const items = List({ items: ['one', 'two', 'three'] });
  const data = fetchData<string>('/api/data');

  console.log(card, items, data);
}
