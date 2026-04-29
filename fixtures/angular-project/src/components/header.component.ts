import { Component, Input, signal } from '@angular/core';
import { CurrencyPipe } from '../pipes/currency.pipe';
import { UserService } from '../services/user.service';

@Component({
  selector: 'app-header',
  standalone: true,
  imports: [CurrencyPipe],
  template: `<header>
    <h1>{{ title }}</h1>
    <span>{{ price | currency }}</span>
  </header>`,
})
export class HeaderComponent {
  @Input() title = '';
  price = signal(29.99);

  constructor(private userService: UserService) {}

  ngOnInit(): void {
    this.userService.getCurrentUser().subscribe();
  }
}
