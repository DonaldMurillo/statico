import { User } from '@mono/core';

export function handleCommand(action: string, data: User[]): void {
  switch (action) {
    case 'list':
      console.table(data);
      break;
    default:
      console.log(`Unknown action: ${action}`);
  }
}
