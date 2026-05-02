import { inject } from '@angular/core';
import { ResolveFn } from '@angular/router';
import { HttpClient } from '@angular/common/http';
import { firstValueFrom } from 'rxjs';
import { marked } from 'marked';
import { DocsService } from '../services/docs.service';

export interface DocData {
  html: string;
  title: string;
  slug: string;
}

export const docResolver: ResolveFn<DocData | null> = async (route) => {
  const docs = inject(DocsService);
  const http = inject(HttpClient);
  const slug = route.paramMap.get('slug');

  if (!slug) return null;

  const entry = docs.getDocBySlug(slug);
  if (!entry) return null;

  // HttpClient works on both server (prerender) and browser (hydration).
  // TransferCache automatically serializes the response from server → client.
  const md = await firstValueFrom(
    http.get(entry.path, { responseType: 'text' })
  );
  const html = await marked.parse(md, { async: true }) as string;

  return { html, title: entry.title, slug };
};
