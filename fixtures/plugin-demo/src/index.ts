// A simple TypeScript project to test the no-console-log plugin

export function greet(name: string): string {
  console.log(`Greeting ${name}`); // should be flagged
  return `Hello, ${name}!`;
}

export function add(a: number, b: number): number {
  return a + b;
}

export function debug(): void {
  console.log("debugging"); // should be flagged
  console.log("more debug"); // should be flagged
}

// This is a comment with console.log — should NOT be flagged
