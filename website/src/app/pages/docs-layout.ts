import { Component, inject, signal, ViewChild } from '@angular/core';
import { RouterOutlet, RouterLink } from '@angular/router';
import { SidebarComponent } from '../components/sidebar';
import { DocsService, DocEntry } from '../services/docs.service';
import { MenuTrigger, Menu, MenuItem } from '@angular/aria/menu';

@Component({
  selector: 'app-docs-layout',
  standalone: true,
  imports: [RouterOutlet, SidebarComponent, RouterLink, MenuTrigger, Menu, MenuItem],
  template: `
    <div class="docs-layout">
      <app-sidebar [entries]="docsService.docEntries" />

      <main class="docs-main">
        <router-outlet />
      </main>
    </div>

    <!-- Mobile floating menu button -->
    <button class="mobile-menu-btn"
            ngMenuTrigger
            [menu]="docMenu"
            aria-label="Open documentation navigation">
      <span class="hamburger" aria-hidden="true">☰</span>
    </button>

    <!-- Accessible dropdown menu for mobile -->
    @if (menuOpen()) {
      <div class="mobile-menu-overlay" (click)="menuOpen.set(false)" aria-hidden="true"></div>
    }
    <div ngMenu
         #docMenu="ngMenu"
         class="mobile-menu"
         [class.open]="menuOpen()"
         aria-label="Documentation navigation"
         role="menu">
      @for (group of menuGroups; track group.category) {
        <div class="mobile-menu-group" role="presentation">
          <div class="mobile-menu-heading" role="presentation">{{ group.category }}</div>
          @for (entry of group.entries; track entry.slug) {
            <a class="mobile-menu-item"
               ngMenuItem
               [value]="entry.slug"
               [routerLink]="['/docs', entry.slug]"
               (click)="menuOpen.set(false)">
              {{ entry.title }}
            </a>
          }
        </div>
      }
    </div>
  `,
  styles: [`
    .docs-layout {
      display: flex;
      min-height: calc(100vh - 3rem);
    }
    .docs-main {
      flex: 1;
      min-width: 0;
    }

    /* ── Mobile menu button ─────────────────────────────── */
    .mobile-menu-btn {
      display: none;
      position: fixed;
      bottom: 1rem;
      right: 1rem;
      z-index: 200;
      width: 48px;
      height: 48px;
      border-radius: 0.375rem;
      border: 1px solid var(--border);
      background: var(--bg-nav);
      color: var(--text-primary);
      font-size: 1.25rem;
      cursor: pointer;
      align-items: center;
      justify-content: center;
      box-shadow: 0 4px 12px oklch(0 0 0 / 0.3);
    }
    .hamburger { line-height: 1; }

    /* ── Mobile menu overlay ────────────────────────────── */
    .mobile-menu-overlay {
      display: none;
      position: fixed;
      inset: 0;
      z-index: 290;
      background: oklch(0 0 0 / 0.5);
    }

    /* ── Mobile menu ────────────────────────────────────── */
    .mobile-menu {
      display: none;
      position: fixed;
      bottom: 4rem;
      right: 1rem;
      z-index: 300;
      min-width: 220px;
      max-height: 70vh;
      overflow-y: auto;
      background: var(--bg-nav);
      border: 1px solid var(--border);
      border-radius: 0.5rem;
      box-shadow: 0 8px 24px oklch(0 0 0 / 0.4);
      padding: 0.5rem 0;
    }
    .mobile-menu.open {
      display: block;
    }
    .mobile-menu-group {
      padding: 0.25rem 0;
    }
    .mobile-menu-group + .mobile-menu-group {
      border-top: 1px solid var(--border);
    }
    .mobile-menu-heading {
      font-family: var(--font-mono, monospace);
      font-size: 0.65rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.08em;
      color: var(--text-muted);
      padding: 0.5rem 1rem 0.25rem;
    }
    .mobile-menu-item {
      display: block;
      padding: 0.5rem 1rem;
      color: var(--text-secondary);
      text-decoration: none;
      font-family: var(--font-mono, monospace);
      font-size: 0.85rem;
    }
    .mobile-menu-item:hover,
    .mobile-menu-item[data-active] {
      background: var(--bg-hover);
      color: var(--text-primary);
    }

    @media (max-width: 768px) {
      .docs-layout {
        flex-direction: column;
      }
      .mobile-menu-btn {
        display: flex;
      }
      .mobile-menu-overlay {
        display: block;
      }
    }
  `]
})
export class DocsLayoutComponent {
  docsService = inject(DocsService);
  menuOpen = signal(false);

  get menuGroups(): { category: string; entries: DocEntry[] }[] {
    const groups = new Map<string, DocEntry[]>();
    for (const entry of this.docsService.docEntries) {
      const cat = entry.category;
      if (!groups.has(cat)) groups.set(cat, []);
      groups.get(cat)!.push(entry);
    }
    return Array.from(groups.entries()).map(([category, entries]) => ({ category, entries }));
  }
}
