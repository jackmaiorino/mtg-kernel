import { test, expect } from '@playwright/test';
import fixture from '../fixtures/actor-visible.json' with { type: 'json' };

async function openFixture(page: import('@playwright/test').Page) {
  await page.route('**/demo/manifest.json', route => route.fulfill({status:404}));
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({name:'recording.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(fixture))});
  await expect(page.getByRole('region', { name:'Game replay' })).toBeVisible();
}
test('import stays local; policy, cards, playback and deep links work', async ({ page }) => {
  const unexpected: string[] = [];
  page.on('request', request => { if (request.method() !== 'GET') unexpected.push(request.url()); });
  await openFixture(page);
  await expect(page.getByText('Recorded selection probability')).toBeVisible();
  await page.getByRole('tab',{name:'Search'}).click();
  await expect(page.getByText('Search not recorded',{exact:true})).toBeVisible();
  await page.getByRole('button',{name:'Next decision'}).click();
  await expect(page).toHaveURL(/decision=d/);
  await page.getByRole('slider',{name:'Decision timeline'}).fill('0');
  await page.getByRole('button',{name:'Play recording'}).click();
  await page.getByRole('button',{name:'Pause playback'}).click();
  await expect(page.getByRole('button',{name:'Play recording'})).toBeVisible();
  expect(unexpected).toEqual([]);
});
for (const size of [{width:1440,height:900},{width:1920,height:1080},{width:320,height:900}]) {
  test(`layout fits ${size.width}px and reduced motion`, async ({page}) => {
    await page.setViewportSize(size); await page.emulateMedia({reducedMotion:'reduce'}); await openFixture(page);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.screenshot({path:`test-results/studio-${size.width}.png`,fullPage:true});
    await page.getByRole('tab',{name:'Policy'}).focus(); await expect(page.getByRole('tab',{name:'Policy'})).toBeFocused();
  });
}
