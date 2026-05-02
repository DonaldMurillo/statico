import { Component, input } from '@angular/core';
import { RouterModule } from '@angular/router';
import { Toolbar, ToolbarWidget } from '@angular/aria/toolbar';

export interface NavLink {
  label: string;
  route?: string;
  url?: string;
  activeOptions?: { exact: boolean };
}

@Component({
  selector: 'app-nav',
  standalone: true,
  imports: [RouterModule, Toolbar, ToolbarWidget],
  template: `
    <nav class="nav" aria-label="Main navigation">
      <div class="nav-inner" ngToolbar aria-label="Site navigation toolbar">
        <a class="nav-brand" ngToolbarWidget [value]="'brand'" routerLink="/" aria-label="statico home">
          <span class="brand-prompt">$</span>
          <span class="brand-text">statico</span>
        </a>
        <div class="nav-links">
          @for (link of links(); track link.label) {
            @if (link.route) {
              <a class="nav-link"
                 ngToolbarWidget
                 [value]="link.label"
                 routerLinkActive="active"
                 [routerLinkActiveOptions]="link.activeOptions ?? { exact: false }"
                 [routerLink]="link.route">
                {{ link.label }}
              </a>
            } @else if (link.url) {
              <a class="nav-link"
                 ngToolbarWidget
                 [value]="link.label"
                 [href]="link.url"
                 target="_blank"
                 rel="noopener">
                {{ link.label }}<span class="link-external" aria-hidden="true">↗</span>
              </a>
            }
          }
        </div>
      </div>
    </nav>
  `,
  styles: [`
    .nav {
      position: sticky;
      top: 0;
      z-index: 100;
      background: var(--bg-nav);
      border-bottom: 1px solid var(--border);
      backdrop-filter: blur(8px);
    }

    .nav-inner {
      max-width: 1200px;
      margin: 0 auto;
      padding: 0 var(--sp-6);
      height: 56px;
      display: flex;
      align-items: center;
      justify-content: space-between;
    }

    .nav-brand {
      display: flex;
      align-items: baseline;
      gap: var(--sp-2);
      text-decoration: none;
      color: var(--text-primary);
      font-family: var(--font-mono);
    }

    .brand-prompt {
      color: var(--accent);
      font-weight: 600;
      font-size: 1rem;
    }

    .brand-text {
      font-weight: 700;
      font-size: 1.125rem;
      letter-spacing: -0.02em;
    }

    .nav-links {
      display: flex;
      gap: var(--sp-8);
      align-items: center;
    }

    .nav-link {
      color: var(--text-secondary);
      text-decoration: none;
      font-size: 0.875rem;
      font-weight: 500;
      font-family: var(--font-mono);
      padding: var(--sp-1) 0;
      transition: color 0.15s;
      display: inline-flex;
      align-items: center;
      gap: var(--sp-1);
    }

    .nav-link:hover,
    .nav-link.active {
      color: var(--text-primary);
    }

    .nav-link.active {
      color: var(--accent);
    }

    .link-external {
      font-size: 0.7em;
      opacity: 0.6;
    }

    :focus-visible {
      outline: 2px solid var(--accent);
      outline-offset: 2px;
      border-radius: 0;
    }
  `]
})
export class NavComponent {
  links = input.required<NavLink[]>();
}
