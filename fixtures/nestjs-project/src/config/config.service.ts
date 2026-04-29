import { Injectable } from '@nestjs/common';

@Injectable()
export class ConfigService {
  private readonly env: Record<string, string> = {
    DB_HOST: 'localhost',
    DB_PORT: '5432',
    JWT_SECRET: 'secret',
    PORT: '3000',
  };

  get(key: string): string {
    return this.env[key] ?? '';
  }

  getNumber(key: string): number {
    return Number(this.env[key]);
  }

  isProduction(): boolean {
    return this.env.NODE_ENV === 'production';
  }
}
