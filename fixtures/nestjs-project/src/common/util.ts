export function formatResponse<T>(data: T): { success: boolean; data: T } {
  return { success: true, data };
}

export function sanitizeInput(input: string): string {
  return input.replace(/[<>"'&]/g, '');
}

export function generateId(): string {
  return Math.random().toString(36).substring(2, 11);
}
