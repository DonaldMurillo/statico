import { cn } from '../../src/lib/utils';
export function Button(props: { className?: string }) {
  return <button className={cn('btn', props.className)} />;
}
