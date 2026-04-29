import { Injectable, signal } from '@angular/core';
import { User } from '../models/user.model';

@Injectable({ providedIn: 'root' })
export class UserService {
  private currentUser = signal<User | null>(null);

  getCurrentUser() {
    return this.currentUser.asReadonly();
  }

  setUser(user: User): void {
    this.currentUser.set(user);
  }

  clearUser(): void {
    this.currentUser.set(null);
  }
}
