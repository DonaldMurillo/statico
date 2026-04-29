import { Button } from '@repo/ui';

export function render(): void {
  console.log('Docs site loaded');
  Button({ children: 'Read Docs', variant: 'secondary' });
}
