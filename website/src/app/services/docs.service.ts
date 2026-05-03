import { Injectable, inject } from '@angular/core';
import { Location } from '@angular/common';
import { HttpClient } from '@angular/common/http';
import { marked } from 'marked';
import { firstValueFrom } from 'rxjs';

export interface DocEntry {
  slug: string;
  title: string;
  /** Path under the asset root, e.g. `docs/getting-started.md`. The
   *  service joins this with `APP_BASE_HREF` + `assets/` at fetch time so
   *  the same entry works at `/` (dev) and `/statico/` (Pages). */
  assetPath: string;
  category: string;
}

@Injectable({ providedIn: 'root' })
export class DocsService {
  private http = inject(HttpClient);
  // `Location.prepareExternalUrl` joins a relative path with whatever
  // `<base href>` the app was bootstrapped with — `/` in dev,
  // `/statico/` on Pages. Works in both browser and SSR/prerender
  // contexts without needing an explicit `APP_BASE_HREF` provider.
  private location = inject(Location);

  readonly docEntries: DocEntry[] = [
    { slug: 'getting-started', title: 'Getting Started', assetPath: 'docs/getting-started.md', category: 'Guides' },
    { slug: 'ci-integration', title: 'CI/CD Integration', assetPath: 'docs/ci-integration.md', category: 'Guides' },
    { slug: 'plugins', title: 'Plugin System', assetPath: 'docs/plugins.md', category: 'Guides' },
    { slug: 'configuration', title: 'Configuration', assetPath: 'docs/configuration.md', category: 'Reference' },
    { slug: 'output-formats', title: 'Output Formats', assetPath: 'docs/output-formats.md', category: 'Reference' },
  ];

  readonly navLinks = [
    { label: 'Home', route: '/', activeOptions: { exact: true } },
    { label: 'Docs', route: '/docs', activeOptions: { exact: false } },
    { label: 'GitHub', url: 'https://github.com/DonaldMurillo/statico' },
  ];

  private cache = new Map<string, string>();

  /** Build the absolute URL for an asset, base-href aware. */
  resolveAssetUrl(assetPath: string): string {
    return this.location.prepareExternalUrl(`assets/${assetPath}`);
  }

  getDocBySlug(slug: string): DocEntry | undefined {
    return this.docEntries.find(d => d.slug === slug);
  }

  async getMarkdownHtml(slug: string): Promise<string> {
    if (this.cache.has(slug)) {
      return this.cache.get(slug)!;
    }

    const entry = this.getDocBySlug(slug);
    if (!entry) {
      throw new Error(`Doc not found: ${slug}`);
    }

    const md = await firstValueFrom(
      this.http.get(this.resolveAssetUrl(entry.assetPath), { responseType: 'text' })
    );
    const html = await marked.parse(md, { async: true }) as string;
    this.cache.set(slug, html);
    return html;
  }

  getSlugs(): string[] {
    return this.docEntries.map(d => d.slug);
  }
}
