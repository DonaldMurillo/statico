import { Component } from '@angular/core';
import { RouterLink } from '@angular/router';
import { Grid, GridRow, GridCell } from '@angular/aria/grid';
import { DownloadCtaComponent } from '../components/download-cta';

@Component({
  selector: 'app-home',
  standalone: true,
  imports: [RouterLink, Grid, GridRow, GridCell, DownloadCtaComponent],
  template: `
    <main class="hero">
      <div class="hero-inner">

        <!-- Boot sequence -->
        <div class="boot-sequence">
          <div class="boot-line">
            <span class="prompt">$</span> statico --analyze ./src
          </div>
          <div class="boot-output">
            <span class="status-ok">OK</span> Scanning 247 files across 12 directories...
          </div>
          <div class="boot-output">
            <span class="status-ok">OK</span> Analysis complete.
          </div>
          <div class="boot-score">
            Health score: <span class="score-value">87</span><span class="score-max">/100</span>
          </div>
        </div>

        <!-- Headline -->
        <h1 class="hero-title">
          Ship healthier<br>
          <span class="hero-accent">TypeScript &amp; Rust</span>
        </h1>
        <p class="hero-desc">
          Detect dead code, unused exports, circular dependencies, code duplication,
          and framework-specific gotchas. Get a code health score from 0–100.
        </p>

        <!-- CTAs: smart-download primary, two secondary anchors -->
        <div class="hero-actions">
          <app-download-cta />
          <div class="hero-secondary-actions">
            <a class="cmd-btn cmd-btn-secondary" routerLink="/docs/getting-started">
              <span class="cmd-prompt" aria-hidden="true">$</span> get started
            </a>
            <a class="cmd-btn cmd-btn-secondary" href="https://github.com/DonaldMurillo/statico" target="_blank" rel="noopener">
              <span class="cmd-prompt" aria-hidden="true">//</span> view source
            </a>
          </div>
        </div>

        <!-- Features with 2D keyboard navigation -->
        <div class="features" ngGrid aria-label="Feature overview">
          <div class="features-row" ngGridRow>
            <div class="feature-block" ngGridCell>
              <div class="feature-header">
                <span class="feature-glyph" aria-hidden="true">//</span>
                <h3 class="feature-title">Dead Code Detection</h3>
              </div>
              <p class="feature-desc">Find files unreachable from any entry point with confidence scoring.</p>
            </div>

            <div class="feature-block" ngGridCell>
              <div class="feature-header">
                <span class="feature-glyph" aria-hidden="true">&#9633;</span>
                <h3 class="feature-title">Unused Exports</h3>
              </div>
              <p class="feature-desc">Flag named exports and TypeScript types never imported elsewhere.</p>
            </div>

            <div class="feature-block" ngGridCell>
              <div class="feature-header">
                <span class="feature-glyph" aria-hidden="true">&#8635;</span>
                <h3 class="feature-title">Circular Dependencies</h3>
              </div>
              <p class="feature-desc">Trace import cycles with full chain reporting and visualization.</p>
            </div>
          </div>

          <div class="features-row" ngGridRow>
            <div class="feature-block" ngGridCell>
              <div class="feature-header">
                <span class="feature-glyph" aria-hidden="true">&#8801;</span>
                <h3 class="feature-title">Code Duplication</h3>
              </div>
              <p class="feature-desc">Detect similar code blocks, clone groups, and mirrored directories.</p>
            </div>

            <div class="feature-block" ngGridCell>
              <div class="feature-header">
                <span class="feature-glyph" aria-hidden="true">&#9670;</span>
                <h3 class="feature-title">Plugin System</h3>
              </div>
              <p class="feature-desc">Extend analysis with custom rules in any language via JSON-RPC.</p>
            </div>

            <div class="feature-block" ngGridCell>
              <div class="feature-header">
                <span class="feature-glyph" aria-hidden="true">&#9642;</span>
                <h3 class="feature-title">Health Score</h3>
              </div>
              <p class="feature-desc">A single 0–100 metric combining issue density and duplication.</p>
            </div>
          </div>
        </div>
      </div>
    </main>
  `,
  styles: [`
    .hero {
      min-height: calc(100vh - 56px);
      display: flex;
      align-items: center;
      justify-content: center;
    }

    .hero-inner {
      max-width: 900px;
      padding: var(--sp-16) var(--sp-8);
      text-align: center;
    }

    /* --- Boot Sequence --- */
    .boot-sequence {
      background: oklch(0.12 0.01 260);
      border: 1px solid oklch(0.22 0.01 260);
      border-radius: 0;
      padding: var(--sp-4) var(--sp-6);
      margin-bottom: var(--sp-8);
      text-align: left;
      font-family: var(--font-mono);
      font-size: 0.8rem;
      line-height: 1.8;
    }

    .boot-line {
      color: oklch(0.88 0.01 260);
    }

    .prompt {
      color: oklch(0.75 0.18 160);
      font-weight: 600;
      margin-right: var(--sp-2);
    }

    .boot-output {
      color: oklch(0.55 0.02 260);
      padding-left: var(--sp-4);
    }

    .status-ok {
      color: oklch(0.75 0.18 160);
      font-weight: 600;
      margin-right: var(--sp-2);
    }

    .boot-score {
      color: oklch(0.88 0.01 260);
      font-weight: 600;
      padding-left: var(--sp-4);
      margin-top: var(--sp-1);
    }

    .score-value {
      color: oklch(0.75 0.18 160);
      font-size: 1.1em;
    }

    .score-max {
      color: oklch(0.45 0.02 260);
    }

    /* --- Headline --- */
    .hero-title {
      font-family: var(--font-mono);
      font-size: 3.25rem;
      font-weight: 700;
      line-height: 1.15;
      color: var(--text-primary);
      margin: 0 0 var(--sp-4);
      letter-spacing: -0.03em;
    }

    .hero-accent {
      color: var(--accent);
    }

    .hero-desc {
      font-size: 1.1rem;
      line-height: 1.7;
      color: var(--text-secondary);
      max-width: 600px;
      margin: 0 auto var(--sp-8);
    }

    /* --- CTAs --- */
    .hero-actions {
      display: flex;
      flex-direction: column;
      gap: var(--sp-4);
      align-items: center;
      justify-content: center;
      margin-bottom: var(--sp-16);
    }

    .hero-secondary-actions {
      display: flex;
      gap: var(--sp-4);
      flex-wrap: wrap;
      justify-content: center;
    }

    .cmd-btn {
      display: inline-flex;
      align-items: center;
      gap: var(--sp-2);
      padding: var(--sp-3) var(--sp-6);
      border-radius: 0;
      font-size: 0.9rem;
      font-weight: 600;
      font-family: var(--font-mono);
      text-decoration: none;
      transition: background 0.15s, color 0.15s;
      cursor: pointer;
      border: 1px solid transparent;
    }

    .cmd-prompt {
      font-weight: 700;
      opacity: 0.7;
    }

    .cmd-btn-primary {
      background: var(--accent);
      color: oklch(0.12 0.01 260);
    }

    .cmd-btn-primary:hover {
      background: var(--accent-hover);
    }

    .cmd-btn-secondary {
      background: transparent;
      color: var(--text-secondary);
      border: 1px solid var(--border-strong);
    }

    .cmd-btn-secondary:hover {
      background: var(--bg-sunken);
      color: var(--text-primary);
    }

    :focus-visible {
      outline: 2px solid var(--accent);
      outline-offset: 2px;
    }

    /* --- Feature Blocks --- */
    .features {
      display: grid;
      grid-template-columns: repeat(3, 1fr);
      gap: var(--sp-3);
      text-align: left;
    }

    .features-row {
      display: contents;
    }

    .feature-block {
      border: 1px solid var(--border);
      border-radius: 0;
      padding: var(--sp-4) var(--sp-4) var(--sp-4) var(--sp-6);
      background: var(--bg-surface);
      transition: border-color 0.15s;
    }

    .feature-block:hover {
      border-color: var(--border-strong);
    }

    .feature-header {
      display: flex;
      align-items: baseline;
      gap: var(--sp-2);
      margin-bottom: var(--sp-2);
    }

    .feature-glyph {
      color: var(--accent);
      font-family: var(--font-mono);
      font-weight: 700;
      font-size: 0.85rem;
    }

    .feature-title {
      font-family: var(--font-mono);
      font-size: 0.85rem;
      font-weight: 600;
      color: var(--text-primary);
      margin: 0;
    }

    .feature-desc {
      font-size: 0.825rem;
      color: var(--text-tertiary);
      margin: 0;
      line-height: 1.55;
      padding-left: var(--sp-4);
    }

    @media (max-width: 768px) {
      .hero-title {
        font-size: 2.25rem;
      }

      .features {
        grid-template-columns: 1fr;
      }

      .hero-actions {
        flex-direction: column;
        align-items: center;
      }
    }
  `]
})
export class HomeComponent {}
