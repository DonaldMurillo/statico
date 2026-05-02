# statico — Documentation Website

Static documentation site for [statico](https://github.com/nickelc/statico), built with Angular 21 SSG.

## Quick Start

```bash
npm install
npm start          # dev server at http://localhost:4000
```

## Build

```bash
npm run build                # prerendered static site → dist/website/browser/
npm run serve:ssg            # serve the built site on :4000
```

## E2E Tests

```bash
npm run build:e2e            # build + run Playwright tests
npm run e2e                  # run tests against existing build
npm run e2e:headed           # run tests with browser visible
```

## Architecture

- **SSG (Static Site Generation)** — all pages prerendered at build time
- **Route resolver** fetches Markdown via HttpClient, parsed with `marked`
- **`@angular/aria`** headless primitives for accessibility
- **Playwright** for end-to-end testing

## Structure

```
src/app/
├── components/     nav, sidebar, doc-page
├── pages/          home, doc-viewer, docs-layout
├── resolvers/      doc.resolver (markdown → HTML)
├── services/       docs.service
└── styles.css      global theme (CSS custom properties)
```

## Docs Content

Markdown files live in `src/assets/docs/`:

- `getting-started.md`
- `ci-integration.md`
- `plugins.md`
- `configuration.md`
- `output-formats.md`
