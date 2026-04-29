export function Unused(): string {
  return 'This export is never consumed by any app';
}

export function UnusedHelper(): void {
  console.log('dead code');
}
