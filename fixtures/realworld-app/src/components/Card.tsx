import { useDebounce } from '../hooks/useDebounce';
import { cn } from '../lib/utils';

export function Card({ title }: { title: string }) {
  const debounced = useDebounce(title);
  return <div className={cn('card', debounced)}>{debounced}</div>;
}
