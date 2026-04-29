import { Injectable } from '@nestjs/common';

@Injectable()
export class LoggerService {
  private context?: string;

  setContext(context: string): void {
    this.context = context;
  }

  log(message: string): void {
    console.log(`[${this.context || 'App'}] ${message}`);
  }

  error(message: string, trace?: string): void {
    console.error(`[${this.context || 'App'}] ${message}`, trace);
  }

  warn(message: string): void {
    console.warn(`[${this.context || 'App'}] ${message}`);
  }
}
