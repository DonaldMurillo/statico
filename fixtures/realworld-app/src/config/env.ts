export const env = {
  dbUrl: process.env.DATABASE_URL || 'localhost',
  jwtSecret: process.env.JWT_SECRET || 'secret',
};
