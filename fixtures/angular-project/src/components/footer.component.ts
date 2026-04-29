import { Component } from '@angular/core';
import { AuthService } from '../services/auth.service';

@Component({
  selector: 'app-footer',
  standalone: true,
  template: `<footer>
    <p>&copy; 2024 My App</p>
    <p *ngIf="isLoggedIn">Logged in</p>
  </footer>`,
})
export class FooterComponent {
  isLoggedIn = false;

  constructor(private authService: AuthService) {
    this.isLoggedIn = authService.isAuthenticated();
  }
}
