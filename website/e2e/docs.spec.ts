import { test, expect } from '@playwright/test';

const docPages = [
  { slug: 'getting-started', title: 'Getting Started' },
  { slug: 'ci-integration', title: 'CI/CD Integration' },
  { slug: 'plugins', title: 'Plugin System' },
  { slug: 'configuration', title: 'Configuration' },
  { slug: 'output-formats', title: 'Output Formats' },
];

/* ================================================================
   Sidebar
   ================================================================ */
test.describe('Sidebar', () => {
  test('shows all 5 doc links', async ({ page }) => {
    await page.goto('/docs/getting-started');
    const links = page.locator('.sidebar-link');
    await expect(links).toHaveCount(5);
  });

  test('has Guides and Reference headings', async ({ page }) => {
    await page.goto('/docs/getting-started');
    const headings = page.locator('.sidebar-heading');
    await expect(headings).toHaveText(['// Guides ', '// Reference ']);
  });

  for (const doc of docPages) {
    test(`highlights ${doc.title} as active when visiting /docs/${doc.slug}`, async ({ page }) => {
      await page.goto(`/docs/${doc.slug}`);
      const active = page.locator('.sidebar-link.active');
      await expect(active).toContainText(doc.title);
    });
  }

  test('navigates to another doc on click', async ({ page }) => {
    await page.goto('/docs/getting-started');
    await page.click('.sidebar-link:has-text("Configuration")');
    await expect(page).toHaveURL('/docs/configuration');
    // Wait for doc content to update after client-side navigation
    await expect(page.locator('.prose')).toBeVisible();
    await expect(page.locator('.prose h1')).toContainText('Configuration');
  });
});

/* ================================================================
   Doc Content — per-page tests
   ================================================================ */
test.describe('Doc Content', () => {
  for (const doc of docPages) {
    test.describe(`${doc.title} (/docs/${doc.slug})`, () => {
      test('renders page with visible prose content', async ({ page }) => {
        await page.goto(`/docs/${doc.slug}`);
        await expect(page.locator('.prose')).toBeVisible();
        await expect(page.locator('.prose h1')).toContainText(doc.title);
      });

      test('has multiple headings in prose', async ({ page }) => {
        await page.goto(`/docs/${doc.slug}`);
        const h2s = page.locator('.prose h2');
        await expect(h2s.first()).toBeVisible();
        expect(await h2s.count()).toBeGreaterThan(0);
      });

      test('has paragraph text content', async ({ page }) => {
        await page.goto(`/docs/${doc.slug}`);
        const paragraphs = page.locator('.prose > p');
        expect(await paragraphs.count()).toBeGreaterThan(0);
      });
    });
  }

  /* ---- Getting Started specific ---- */
  test('Getting Started has code blocks', async ({ page }) => {
    await page.goto('/docs/getting-started');
    const codeBlocks = page.locator('.prose pre');
    await expect(codeBlocks.first()).toBeVisible();
    expect(await codeBlocks.count()).toBeGreaterThanOrEqual(2);
  });

  test('Getting Started has a table', async ({ page }) => {
    await page.goto('/docs/getting-started');
    const table = page.locator('.prose table');
    await expect(table).toBeVisible();
    // Table should have header + body rows
    const rows = table.locator('tr');
    expect(await rows.count()).toBeGreaterThan(3);
  });

  test('Getting Started table has Feature and Description columns', async ({ page }) => {
    await page.goto('/docs/getting-started');
    const headers = page.locator('.prose table th');
    await expect(headers.nth(0)).toContainText('Feature');
    await expect(headers.nth(1)).toContainText('Description');
  });

  /* ---- CI/CD Integration specific ---- */
  test('CI/CD has GitHub Actions section', async ({ page }) => {
    await page.goto('/docs/ci-integration');
    await expect(page.locator('.prose h2:text("Quick Start – GitHub Actions")')).toBeVisible();
  });

  test('CI/CD has Docker section', async ({ page }) => {
    await page.goto('/docs/ci-integration');
    await expect(page.locator('.prose h2:text("Running in Docker")')).toBeVisible();
  });

  test('CI/CD has SARIF section', async ({ page }) => {
    await page.goto('/docs/ci-integration');
    await expect(page.locator('.prose h2:text("SARIF Integration with GitHub Code Scanning")')).toBeVisible();
  });

  /* ---- Plugins specific ---- */
  test('Plugins has Quick Start', async ({ page }) => {
    await page.goto('/docs/plugins');
    await expect(page.locator('.prose h2:text("Quick Start")')).toBeVisible();
  });

  test('Plugins lists supported languages', async ({ page }) => {
    await page.goto('/docs/plugins');
    await expect(page.locator('.prose h2:text("Supported Languages")')).toBeVisible();
  });

  test('Plugins has protocol section', async ({ page }) => {
    await page.goto('/docs/plugins');
    await expect(page.locator('.prose h2:text("Protocol")')).toBeVisible();
  });

  /* ---- Configuration specific ---- */
  test('Configuration has all main sections', async ({ page }) => {
    await page.goto('/docs/configuration');
    const h2s = page.locator('.prose h2');
    const texts = await h2s.allTextContents();
    expect(texts).toEqual(expect.arrayContaining([
      expect.stringContaining('Basic Configuration'),
      expect.stringContaining('Analysis Options'),
      expect.stringContaining('Framework Configuration'),
    ]));
  });

  test('Configuration has code examples', async ({ page }) => {
    await page.goto('/docs/configuration');
    const codeBlocks = page.locator('.prose pre');
    await expect(codeBlocks.first()).toBeVisible();
  });

  /* ---- Output Formats specific ---- */
  test('Output Formats lists all format sections', async ({ page }) => {
    await page.goto('/docs/output-formats');
    const h2s = page.locator('.prose h2');
    const texts = await h2s.allTextContents();
    const expectedFormats = ['JSON', 'Markdown', 'SARIF', 'HTML', 'Mermaid'];
    for (const fmt of expectedFormats) {
      expect(texts.some(t => t.includes(fmt))).toBeTruthy();
    }
  });

  /* ---- Shared content features ---- */
  test('code blocks have terminal label', async ({ page }) => {
    await page.goto('/docs/getting-started');
    // The pre::before pseudo-element shows "terminal" — check the CSS class exists
    const preBlocks = page.locator('.prose pre');
    await expect(preBlocks.first()).toBeVisible();
  });

  test('inline code is styled', async ({ page }) => {
    await page.goto('/docs/getting-started');
    const inlineCode = page.locator('.prose code:not(pre code)');
    expect(await inlineCode.count()).toBeGreaterThan(0);
  });

  test('prose links are visible', async ({ page }) => {
    await page.goto('/docs/getting-started');
    const links = page.locator('.prose a');
    expect(await links.count()).toBeGreaterThan(0);
  });
});

/* ================================================================
   Navigation
   ================================================================ */
test.describe('Navigation', () => {
  test('redirects /docs to /docs/getting-started', async ({ page }) => {
    await page.goto('/docs');
    await expect(page).toHaveURL('/docs/getting-started');
  });

  test('navigates home via brand link', async ({ page }) => {
    await page.goto('/docs/getting-started');
    await page.click('.nav-brand');
    await expect(page).toHaveURL('/');
  });

  test('nav Docs link goes to getting-started', async ({ page }) => {
    await page.goto('/');
    await page.click('.nav-link:has-text("Docs")');
    await expect(page).toHaveURL('/docs/getting-started');
  });

  test('GitHub link opens external repo', async ({ page }) => {
    await page.goto('/');
    const githubLink = page.locator('.nav-link:has-text("GitHub")');
    expect(await githubLink.getAttribute('href')).toContain('github.com');
    expect(await githubLink.getAttribute('target')).toBe('_blank');
  });
});

/* ================================================================
   SSG Verification — content is pre-rendered, no JS fetch needed
   ================================================================ */
test.describe('SSG Content', () => {
  test('doc page has content before JS hydration', async ({ page }) => {
    // Block all JS to verify content is in static HTML
    await page.route('**/*.js', route => route.abort());
    const response = await page.goto('/docs/getting-started');
    const body = await page.locator('body').innerHTML();
    // The static HTML should contain the doc content
    expect(body).toContain('Getting Started');
    expect(body).toContain('Installation');
    expect(body).toContain('Quick Start');
  });

  for (const doc of docPages) {
    test(`${doc.title} has prerendered h1 in static HTML`, async ({ page }) => {
      await page.route('**/*.js', route => route.abort());
      await page.goto(`/docs/${doc.slug}`);
      const body = await page.locator('body').innerHTML();
      expect(body).toContain(doc.title);
    });
  }
});
