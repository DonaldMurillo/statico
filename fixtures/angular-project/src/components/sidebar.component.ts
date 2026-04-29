import { Component } from '@angular/core';

@Component({
  selector: 'app-sidebar',
  standalone: true,
  template: `<aside class="sidebar">
    <nav>
      <a routerLink="/home">Home</a>
      <a routerLink="/settings">Settings</a>
    </nav>
  </aside>`,
})
export class SidebarComponent {
  collapsed = false;

  toggle(): void {
    this.collapsed = !this.collapsed;
  }
}
