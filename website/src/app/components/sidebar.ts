import { Component, input } from '@angular/core';
import { RouterModule } from '@angular/router';
import { DocEntry } from '../services/docs.service';
import { Tree, TreeItem } from '@angular/aria/tree';

@Component({
  selector: 'app-sidebar',
  standalone: true,
  imports: [RouterModule, Tree, TreeItem],
  template: `
    <aside class="sidebar" aria-label="Documentation">
      <div class="sidebar-inner" ngTree [nav]="true" #tree="ngTree" aria-label="Documentation navigation">
        @for (group of groupedEntries(); track group.category) {
          <div class="sidebar-group" role="group" [attr.aria-label]="group.category">
            <h3 class="sidebar-heading" role="presentation">
              <span class="heading-icon" aria-hidden="true">//</span> {{ group.category }}
            </h3>
            <ul class="sidebar-list">
              @for (entry of group.entries; track entry.slug) {
                <li>
                  <a class="sidebar-link"
                     ngTreeItem
                     [value]="entry.slug"
                     [parent]="tree"
                     routerLinkActive="active"
                     [routerLink]="['/docs', entry.slug]">
                    <span class="link-prefix" aria-hidden="true"></span>
                    {{ entry.title }}
                  </a>
                </li>
              }
            </ul>
          </div>
        }
      </div>
    </aside>
  `,
  styles: [`
    .sidebar {
      width: 14rem;
      flex-shrink: 0;
      border-right: 1px solid var(--border);
      background: var(--bg-sidebar);
      height: calc(100vh - 56px);
      position: sticky;
      top: 56px;
      overflow-y: auto;
    }

    .sidebar-inner {
      padding: var(--sp-6) var(--sp-2);
    }

    .sidebar-group {
      margin-bottom: var(--sp-8);
    }

    .sidebar-heading {
      font-size: 0.65rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.1em;
      color: var(--text-tertiary);
      margin: 0 0 var(--sp-2) 0;
      padding-left: var(--sp-4);
      font-family: var(--font-mono);
      display: flex;
      align-items: center;
      gap: var(--sp-1);
    }

    .heading-icon {
      color: var(--accent);
      font-size: 0.65rem;
    }

    .sidebar-list {
      list-style: none;
      margin: 0;
      padding: 0;
    }

    .sidebar-link {
      display: flex;
      align-items: center;
      gap: var(--sp-1);
      padding: var(--sp-1) var(--sp-2) var(--sp-1) var(--sp-4);
      color: var(--text-secondary);
      text-decoration: none;
      font-size: 0.8rem;
      font-family: var(--font-mono);
      transition: background 0.1s, color 0.1s;
      cursor: pointer;
    }

    .link-prefix::before {
      content: '·';
      color: var(--text-tertiary);
      font-weight: 700;
    }

    .sidebar-link:hover {
      background: var(--bg-sidebar-hover);
      color: var(--text-primary);
    }

    .sidebar-link.active {
      background: var(--bg-sidebar-active);
      color: var(--accent);
    }

    .sidebar-link.active .link-prefix::before {
      content: '▸';
      color: var(--accent);
    }

    :focus-visible {
      outline: 2px solid var(--accent);
      outline-offset: -2px;
      border-radius: 0;
    }
  `]
})
export class SidebarComponent {
  entries = input.required<DocEntry[]>();

  get groupedEntries() {
    return () => {
      const groups = new Map<string, DocEntry[]>();
      for (const entry of this.entries()) {
        const cat = entry.category;
        if (!groups.has(cat)) groups.set(cat, []);
        groups.get(cat)!.push(entry);
      }
      return Array.from(groups.entries()).map(([category, entries]) => ({ category, entries }));
    };
  }
}
