export const env = {
  NODE_ENV: 'test' as const,
  API_URL: 'http://localhost:3000',
  VERSION: '1.0.0',
};

export type Env = typeof env;
