import { RenderMode, ServerRoute } from '@angular/ssr';

const docSlugs = ['getting-started', 'ci-integration', 'plugins', 'configuration', 'output-formats'];

export const serverRoutes: ServerRoute[] = [
  {
    path: 'docs/:slug',
    renderMode: RenderMode.Prerender,
    getPrerenderParams: () => Promise.resolve(docSlugs.map(slug => ({ slug }))),
  },
  {
    path: '**',
    renderMode: RenderMode.Prerender,
  },
];
