import { test, expect } from '@playwright/test';

test.describe('Homepage', () => {
  test('loads and shows hero title', async ({ page }) => {
    await page.goto('/');
    await expect(page.locator('.hero-title')).toContainText('Ship healthier');
  });

  test('shows boot sequence terminal', async ({ page }) => {
    await page.goto('/');
    const boot = page.locator('.boot-sequence');
    await expect(boot).toBeVisible();
    await expect(boot).toContainText('statico --analyze');
    await expect(boot).toContainText('Health score');
  });

  test('shows hero description', async ({ page }) => {
    await page.goto('/');
    const desc = page.locator('.hero-desc');
    await expect(desc).toBeVisible();
    await expect(desc).toContainText('dead code');
  });

  test('shows 6 feature blocks', async ({ page }) => {
    await page.goto('/');
    const features = page.locator('.feature-block');
    await expect(features).toHaveCount(6);
  });

  test('feature blocks have expected titles', async ({ page }) => {
    await page.goto('/');
    const titles = page.locator('.feature-title');
    const texts = await titles.allTextContents();
    expect(texts).toEqual([
      'Dead Code Detection',
      'Unused Exports',
      'Circular Dependencies',
      'Code Duplication',
      'Plugin System',
      'Health Score',
    ]);
  });

  test('navigates to docs from primary CTA', async ({ page }) => {
    await page.goto('/');
    await page.click('.cmd-btn-primary');
    await expect(page).toHaveURL('/docs/getting-started');
  });

  test('GitHub link points to repo', async ({ page }) => {
    await page.goto('/');
    const link = page.locator('.cmd-btn-secondary');
    expect(await link.getAttribute('href')).toContain('github.com/nickelc/statico');
  });

  test('nav has Home, Docs, GitHub links', async ({ page }) => {
    await page.goto('/');
    const navLinks = page.locator('.nav-link');
    await expect(navLinks).toHaveCount(3);
    await expect(navLinks.nth(0)).toContainText('Home');
    await expect(navLinks.nth(1)).toContainText('Docs');
    await expect(navLinks.nth(2)).toContainText('GitHub');
  });

  test('Home nav link is active on homepage', async ({ page }) => {
    await page.goto('/');
    const active = page.locator('.nav-link.active');
    await expect(active).toContainText('Home');
  });
});

test.describe('Homepage SSG', () => {
  test('content exists in static HTML without JS', async ({ page }) => {
    await page.route('**/*.js', route => route.abort());
    await page.goto('/');
    const body = await page.locator('body').innerHTML();
    expect(body).toContain('Ship healthier');
    expect(body).toContain('Dead Code Detection');
    expect(body).toContain('statico --analyze');
  });
});
