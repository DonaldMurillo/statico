import { Injectable } from '@angular/core';

@Injectable({ providedIn: 'root' })
export class AnalyticsService {
  private events: string[] = [];

  trackEvent(name: string, payload?: Record<string, unknown>): void {
    this.events.push(name);
    console.log('Event tracked:', name, payload);
  }

  getEventCount(): number {
    return this.events.length;
  }

  generateReport(): string[] {
    return [...this.events];
  }
}
