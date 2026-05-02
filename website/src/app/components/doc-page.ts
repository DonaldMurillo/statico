import { Component, input, signal } from '@angular/core';

@Component({
  selector: 'app-doc-page',
  standalone: true,
  template: `
    <div class="doc-content">
      @if (loading()) {
        <div class="doc-loading">
          <span class="loading-prompt" aria-hidden="true">$</span> Loading document...
        </div>
      }
      <div class="prose" [innerHTML]="htmlContent()"></div>
    </div>
  `,
  styles: [`
    .doc-content {
      max-width: 48rem;
      margin: 0 auto;
      padding: var(--sp-8) var(--sp-8) var(--sp-16);
    }

    .doc-loading {
      color: var(--text-tertiary);
      font-family: var(--font-mono);
      font-size: 0.875rem;
      padding: var(--sp-16) 0;
      display: flex;
      align-items: center;
      gap: var(--sp-2);
    }

    .loading-prompt {
      color: var(--accent);
      font-weight: 600;
    }

    /* ================================================================
       Prose styles for rendered markdown HTML
       Terminal manual-page aesthetic
       ================================================================ */

    ::ng-deep .prose {
      color: var(--text-primary);
      line-height: 1.75;
      font-size: 0.95rem;
    }

    /* --- Headings (monospace display) --- */
    ::ng-deep .prose h1 {
      font-family: var(--font-mono);
      font-size: 2rem;
      font-weight: 700;
      color: var(--text-primary);
      margin: 0 0 var(--sp-4);
      line-height: 1.2;
      letter-spacing: -0.02em;
    }

    ::ng-deep .prose h1::before {
      content: '# ';
      color: var(--text-tertiary);
      font-weight: 400;
    }

    ::ng-deep .prose h2 {
      font-family: var(--font-mono);
      font-size: 1.4rem;
      font-weight: 600;
      color: var(--text-primary);
      margin: var(--sp-8) 0 var(--sp-3);
      padding-bottom: var(--sp-2);
      border-bottom: 1px solid var(--divider);
      letter-spacing: -0.01em;
    }

    ::ng-deep .prose h2::before {
      content: '## ';
      color: var(--text-tertiary);
      font-weight: 400;
    }

    ::ng-deep .prose h3 {
      font-family: var(--font-mono);
      font-size: 1.1rem;
      font-weight: 600;
      color: var(--text-primary);
      margin: var(--sp-6) 0 var(--sp-2);
    }

    ::ng-deep .prose h3::before {
      content: '### ';
      color: var(--text-tertiary);
      font-weight: 400;
    }

    /* --- Paragraphs --- */
    ::ng-deep .prose p {
      margin: var(--sp-3) 0;
    }

    /* --- Links --- */
    ::ng-deep .prose a {
      color: var(--accent);
      text-decoration: underline;
      text-underline-offset: 2px;
      text-decoration-thickness: 1px;
    }

    ::ng-deep .prose a:hover {
      color: var(--accent-hover);
    }

    /* --- Inline code --- */
    ::ng-deep .prose code {
      background: var(--bg-sunken);
      padding: 0.1em 0.35em;
      border-radius: 0;
      font-size: 0.85em;
      font-family: var(--font-mono);
      border: 1px solid var(--border);
    }

    /* --- Code blocks (terminal look, always dark bg) --- */
    ::ng-deep .prose pre {
      background: oklch(0.12 0.01 260);
      border: 1px solid oklch(0.22 0.01 260);
      border-radius: 0;
      padding: 0;
      overflow-x: auto;
      margin: var(--sp-4) 0;
      line-height: 1.6;
      position: relative;
    }

    ::ng-deep .prose pre::before {
      content: 'terminal';
      display: block;
      background: oklch(0.18 0.01 260);
      color: oklch(0.5 0.02 260);
      font-family: var(--font-mono);
      font-size: 0.7rem;
      padding: var(--sp-1) var(--sp-3);
      text-transform: uppercase;
      letter-spacing: 0.08em;
      border-bottom: 1px solid oklch(0.22 0.01 260);
    }

    ::ng-deep .prose pre code {
      display: block;
      background: none;
      padding: var(--sp-4) var(--sp-4);
      font-size: 0.85rem;
      border: none;
      color: oklch(0.82 0.02 160);
    }

    ::ng-deep .prose pre code,
    ::ng-deep .prose pre {
      color: oklch(0.82 0.02 160);
    }

    /* --- Lists --- */
    ::ng-deep .prose ul,
    ::ng-deep .prose ol {
      padding-left: var(--sp-6);
      margin: var(--sp-3) 0;
    }

    ::ng-deep .prose li {
      margin: var(--sp-1) 0;
    }

    ::ng-deep .prose ul li::marker {
      color: var(--accent);
    }

    /* --- Blockquote --- */
    ::ng-deep .prose blockquote {
      border-left: 1px solid var(--accent);
      padding-left: var(--sp-4);
      margin: var(--sp-4) 0;
      color: var(--text-secondary);
      font-style: italic;
    }

    /* --- Tables (ASCII-border feel) --- */
    ::ng-deep .prose table {
      width: 100%;
      border-collapse: collapse;
      margin: var(--sp-4) 0;
      font-size: 0.9rem;
      font-family: var(--font-mono);
    }

    ::ng-deep .prose th,
    ::ng-deep .prose td {
      border: 1px solid var(--border);
      padding: var(--sp-2) var(--sp-3);
      text-align: left;
    }

    ::ng-deep .prose th {
      background: var(--bg-sunken);
      font-weight: 600;
      font-size: 0.8rem;
      text-transform: uppercase;
      letter-spacing: 0.05em;
      color: var(--text-secondary);
    }

    /* --- Horizontal rule --- */
    ::ng-deep .prose hr {
      border: none;
      border-top: 1px solid var(--divider);
      margin: var(--sp-8) 0;
    }

    /* --- Images --- */
    ::ng-deep .prose img {
      max-width: 100%;
      border-radius: 0;
      border: 1px solid var(--border);
    }

    /* --- Strong / emphasis --- */
    ::ng-deep .prose strong {
      color: var(--text-primary);
      font-weight: 600;
    }
  `]
})
export class DocPageComponent {
  htmlContent = input.required<string>();
  loading = signal(false);
}
