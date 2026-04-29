import { Card } from '@mono/ui';
import { fetchData } from '@mono/data';

const card = Card({ title: 'Dashboard' });
const response = fetchData<string>('/api/items');

export { card, response };
