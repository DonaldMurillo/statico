import { Component, DestroyRef, inject, signal } from '@angular/core';
import { takeUntilDestroyed } from '@angular/core/rxjs-interop';
import { ActivatedRoute } from '@angular/router';
import { DocPageComponent } from '../components/doc-page';
import { DocData } from '../resolvers/doc.resolver';

@Component({
  selector: 'app-doc-viewer',
  standalone: true,
  imports: [DocPageComponent],
  template: `
    @if (error()) {
      <div class="doc-error">
        <div class="error-terminal">
          <div class="error-header">
            <span class="error-label" aria-hidden="true">ERROR</span>
          </div>
          <div class="error-body">
            <p class="error-code">404: Document not found</p>
            <p class="error-msg">The requested documentation page could not be loaded.</p>
            <p class="error-hint">
              <span class="hint-prompt" aria-hidden="true">$</span> Try navigating from the sidebar.
            </p>
          </div>
        </div>
      </div>
    } @else if (docData(); as d) {
      <app-doc-page [htmlContent]="d.html" />
    }
  `,
  styles: [`
    .doc-error {
      padding: var(--sp-16) var(--sp-8);
      display: flex;
      justify-content: center;
    }

    .error-terminal {
      background: oklch(0.12 0.01 260);
      border: 1px solid oklch(0.22 0.01 260);
      border-radius: 0;
      width: 100%;
      max-width: 32rem;
      overflow: hidden;
    }

    .error-header {
      background: oklch(0.18 0.01 260);
      padding: var(--sp-2) var(--sp-4);
      border-bottom: 1px solid oklch(0.22 0.01 260);
    }

    .error-label {
      font-family: var(--font-mono);
      font-size: 0.7rem;
      font-weight: 600;
      text-transform: uppercase;
      letter-spacing: 0.1em;
      color: oklch(0.65 0.15 25);
    }

    .error-body {
      padding: var(--sp-6);
    }

    .error-code {
      font-family: var(--font-mono);
      font-size: 0.95rem;
      font-weight: 600;
      color: oklch(0.80 0.01 260);
      margin: 0 0 var(--sp-3);
    }

    .error-msg {
      font-family: var(--font-mono);
      font-size: 0.8rem;
      color: oklch(0.50 0.02 260);
      margin: 0 0 var(--sp-4);
    }

    .error-hint {
      font-family: var(--font-mono);
      font-size: 0.8rem;
      color: oklch(0.55 0.02 260);
      margin: 0;
      display: flex;
      align-items: center;
      gap: var(--sp-2);
    }

    .hint-prompt {
      color: oklch(0.75 0.18 160);
      font-weight: 600;
    }
  `]
})
export class DocViewerComponent {
  private route = inject(ActivatedRoute);

  docData = signal<DocData | null>(null);
  error = signal(false);

  constructor() {
    // Subscribe to route data so both initial SSG load and client-side
    // navigations update the content.
    this.route.data.pipe(takeUntilDestroyed()).subscribe((data) => {
      const doc = data['doc'] as DocData | null;
      if (doc) {
        this.docData.set(doc);
        this.error.set(false);
      } else {
        this.error.set(true);
      }
    });
  }
}
