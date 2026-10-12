import { test, expect } from '@playwright/test';
import fixture from '../fixtures/actor-visible.json' with { type: 'json' };
import native from '../fixtures/native-root-excerpt.json' with { type: 'json' };
import replay from '../fixtures/search-replay.json' with { type: 'json' };

async function openFixture(page: import('@playwright/test').Page) {
  await page.route('**/demo/manifest.json', route => route.fulfill({status:404}));
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({name:'recording.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(fixture))});
  await expect(page.getByRole('region', { name:'Game replay' })).toBeVisible();
}
test('search replay stepping, seeking, pause and deep link restore',async({page})=>{
  await openFixture(page);
  const load=()=>page.locator('input[type=file]').setInputFiles({name:'replay.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(replay))});
  await load();const region=page.getByRole('region',{name:'Recorded search replay'});
  await expect(region).toBeVisible();await region.getByRole('button',{name:'Next search event'}).click();
  await expect(region.locator('.search-phase')).toContainText('Initial move coverage');
  await region.getByRole('button',{name:'Next search event'}).click();
  await expect(region.locator('.search-branches')).toContainText('1 visits');
  await region.getByRole('button',{name:'Play search replay'}).click();
  await region.getByRole('slider',{name:'Search event timeline'}).fill('5');
  await expect(region.getByRole('button',{name:'Play search replay'})).toBeVisible();
  await expect(page).toHaveURL(/searchEvent=5/);
  await load();await expect(region.locator('.search-buttons')).toContainText('Event 6 / 6');
  await region.getByRole('slider',{name:'Search event timeline'}).fill('0');
  await region.getByRole('button',{name:'Next search event'}).focus();await page.keyboard.press('Enter');
  await expect(region.locator('.search-buttons')).toContainText('Event 2 / 6');
  await page.setViewportSize({width:320,height:900});await page.emulateMedia({reducedMotion:'reduce'});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=window.innerWidth)).toBe(true);
  await region.screenshot({path:'test-results/search-replay-320.png'});
});
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
test('recorded block and stack targeting relationships render', async ({page}) => {
  await openFixture(page);
  await page.locator('input[type=file]').setInputFiles({name:'native.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(native))});
  await expect(page.getByRole('heading',{name:native.title})).toBeVisible();
  await expect(page.locator('.connections > path')).toHaveCount(1);
  await page.getByRole('tab',{name:'Search'}).click();
  await expect(page.getByText('Share of search visits',{exact:true})).toBeVisible();
  await expect(page.getByText('Search changed the policy sample',{exact:true})).toBeVisible();
  await page.getByRole('slider',{name:'Decision timeline'}).fill('1');
  await expect(page.locator('.connections > path')).toHaveCount(1);
  await expect(page.locator('.stack-zone')).toContainText('Lightning Bolt');
});
test('exact policy favorite survives numeric display rounding', async ({page}) => {
  await openFixture(page);
  const recording=structuredClone(native);
  recording.frames[0].policy!.masses=['9223372036854775807','9223372036854775809'];
  await page.locator('input[type=file]').setInputFiles({name:'precision.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(recording))});
  await expect(page.getByRole('heading',{name:native.title})).toBeVisible();
  await expect(page.locator('.action-option').first()).toContainText('Policy favorite');
  await expect(page.locator('.action-option').first()).toContainText('Assign Voldaren Epicure to block Clockwork Percussionist');
  await expect(page.locator('.action-option').nth(1)).not.toContainText('Policy favorite');
});
for (const size of [{width:1440,height:900},{width:1920,height:1080},{width:320,height:900}]) {
  test(`layout fits ${size.width}px and reduced motion`, async ({page}) => {
    await page.setViewportSize(size); await page.emulateMedia({reducedMotion:'reduce'}); await openFixture(page);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
    await page.screenshot({path:`test-results/studio-${size.width}.png`,fullPage:true});
    await page.getByRole('tab',{name:'Policy'}).focus(); await expect(page.getByRole('tab',{name:'Policy'})).toBeFocused();
  });
}
