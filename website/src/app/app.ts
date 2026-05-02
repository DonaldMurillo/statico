import { Component, inject } from '@angular/core';
import { RouterOutlet } from '@angular/router';
import { NavComponent, NavLink } from './components/nav';
import { DocsService } from './services/docs.service';

@Component({
  selector: 'app-root',
  standalone: true,
  imports: [RouterOutlet, NavComponent],
  template: `
    <app-nav [links]="navLinks" />
    <router-outlet />
  `,
  styles: [`
    :host {
      display: block;
      min-height: 100vh;
      background: var(--bg-root);
      color: var(--text-primary);
    }
  `]
})
export class App {
  private docs = inject(DocsService);
  navLinks: NavLink[] = this.docs.navLinks;
}
