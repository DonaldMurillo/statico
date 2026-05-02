import { Injectable, inject } from '@angular/core';
import { HttpClient } from '@angular/common/http';
import { marked } from 'marked';
import { firstValueFrom } from 'rxjs';

export interface DocEntry {
  slug: string;
  title: string;
  path: string;
  category: string;
}

@Injectable({ providedIn: 'root' })
export class DocsService {
  private http = inject(HttpClient);

  readonly docEntries: DocEntry[] = [
    { slug: 'getting-started', title: 'Getting Started', path: '/assets/docs/getting-started.md', category: 'Guides' },
    { slug: 'ci-integration', title: 'CI/CD Integration', path: '/assets/docs/ci-integration.md', category: 'Guides' },
    { slug: 'plugins', title: 'Plugin System', path: '/assets/docs/plugins.md', category: 'Guides' },
    { slug: 'configuration', title: 'Configuration', path: '/assets/docs/configuration.md', category: 'Reference' },
    { slug: 'output-formats', title: 'Output Formats', path: '/assets/docs/output-formats.md', category: 'Reference' },
  ];

  readonly navLinks = [
    { label: 'Home', route: '/', activeOptions: { exact: true } },
    { label: 'Docs', route: '/docs', activeOptions: { exact: false } },
    { label: 'GitHub', url: 'https://github.com/nickelc/statico' },
  ];

  private cache = new Map<string, string>();

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
      this.http.get(entry.path, { responseType: 'text' })
    );
    const html = await marked.parse(md, { async: true }) as string;
    this.cache.set(slug, html);
    return html;
  }

  getSlugs(): string[] {
    return this.docEntries.map(d => d.slug);
  }
}
