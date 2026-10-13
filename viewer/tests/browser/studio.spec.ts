import { test, expect } from '@playwright/test';
import fixture from '../fixtures/actor-visible.json' with { type: 'json' };
import native from '../fixtures/native-root-excerpt.json' with { type: 'json' };
import replay from '../fixtures/search-replay.json' with { type: 'json' };
import graph from '../fixtures/search-graph.json' with {type:'json'};
import retained from '../fixtures/retained-search.json' with {type:'json'};
import uct from '../fixtures/uct-search.json' with {type:'json'};
import pooledUct from '../fixtures/uct-pooled.json' with {type:'json'};
import { createHash } from 'node:crypto';

async function openFixture(page: import('@playwright/test').Page) {
  await page.route('**/demo/manifest.json', route => route.fulfill({status:404}));
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({name:'recording.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(fixture))});
  await expect(page.getByRole('region', { name:'Game replay' })).toBeVisible();
}

test('full UCT tree exposes discovery, rollout moves and discarded attempts',async({page})=>{
  await openFixture(page);await page.locator('input[type=file]').setInputFiles({name:'uct.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(pooledUct))});
  await page.getByRole('button',{name:'Watch this search unfold'}).click();
  await expect(page.locator('.decision-evidence')).toContainText('Tree choices, rollout actions');
  await page.getByRole('button',{name:'Watch its discovery'}).click();
  await expect(page.locator('.search-discovery')).toContainText('attempt 1');
  await page.getByRole('button',{name:'Next search event',exact:true}).click();await expect(page.locator('[data-node]')).toHaveCount(1);await expect(page.locator('.search-caption')).toContainText('Selected at a UCT tree node');
  await page.getByRole('button',{name:'Next search event',exact:true}).click();await page.getByRole('button',{name:'Next search event',exact:true}).click();await expect(page.locator('[data-node]')).toHaveCount(2);
  await page.getByRole('button',{name:'See complete graph'}).click();await expect(page.locator('[data-node]')).toHaveCount(3);await expect(page.locator('.search-caption')).toContainText('without a backup');
  await page.getByText('Inspect every simulated move', {exact:false}).click();await page.getByLabel('Inspect rollout move').selectOption('1');
  await expect(page.locator('.search-caption')).toContainText('step 16');
  await page.setViewportSize({width:320,height:900});await page.emulateMedia({reducedMotion:'reduce'});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  await page.screenshot({path:'test-results/uct-search-320.png',fullPage:true});
});

test('retained search choices link to roots and show legal moves without fabricated scores',async({page})=>{
  await openFixture(page);
  await page.locator('input[type=file]').setInputFiles({name:'retained.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(retained))});
  await expect(page.locator('.decision-evidence')).toContainText('A search chose this move');
  await page.getByRole('tab',{name:'Search'}).click();
  await expect(page.locator('.action-list')).toContainText('completed rollouts');
  await expect(page.getByRole('button',{name:'Watch this search unfold'})).toHaveCount(0);
  await page.getByRole('button',{name:'Next decision',exact:true}).click();
  await expect(page.locator('.decision-evidence')).toContainText('Chosen from a searched continuation');
  await expect(page.locator('.action-list .action-probability')).toHaveCount(0);
  await expect(page.locator('.action-list .probability-track')).toHaveCount(0);
  await expect(page.locator('.action-list .action-option')).toHaveCount(retained.frames[1].actions.length);
  await page.getByRole('button',{name:'Inspect the source search'}).focus();await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/decision=d15/);
  await expect(page.locator('.action-list')).toContainText('completed rollouts');
  await page.setViewportSize({width:320,height:900});await page.emulateMedia({reducedMotion:'reduce'});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  await page.screenshot({path:'test-results/retained-search-320.png',fullPage:true});
});

test('a hash link switches manifest games and restores its search event', async ({page}) => {
  const games = [fixture, replay].map(recording => {
    const body = JSON.stringify(recording);
    return {id:recording.id, file:`${recording.id}.json`, sha256:createHash('sha256').update(body).digest('hex'), body};
  });
  await page.route('**/demo/manifest.json', route => route.fulfill({json:{games:games.map(({body:_, ...game})=>game)}}));
  for (const game of games) await page.route(`**/demo/${game.file}`, route => route.fulfill({body:game.body, contentType:'application/json'}));
  await page.goto(`/#game=${fixture.id}&decision=${fixture.frames[0].id}`);
  await expect(page.getByRole('heading',{name:fixture.title})).toBeVisible();
  await page.evaluate(hash => {window.location.hash=hash;}, `game=${replay.id}&decision=${replay.frames[0].id}&searchEvent=5`);
  await expect(page.getByRole('heading',{name:replay.title})).toBeVisible();
  await expect(page.getByRole('region',{name:'Recorded search replay'}).locator('.search-buttons')).toContainText('Event 6 / 6');
  await expect(page).toHaveURL(/searchEvent=5/);
  await page.evaluate(hash => {window.location.hash=hash;}, `game=${fixture.id}&decision=${fixture.frames[0].id}`);
  await expect(page.getByRole('heading',{name:fixture.title})).toBeVisible();
});
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
  const noPolicy=JSON.parse(JSON.stringify(replay));noPolicy.frames[0].policy=null;
  await page.locator('input[type=file]').setInputFiles({name:'search-only.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(noPolicy))});
  await expect(region.locator('.search-selection-summary')).toContainText('Policy sample: Not recorded');
});

test('the graph retains discovered branches, inspects positions and rewinds',async({page})=>{
  await openFixture(page);
  await page.locator('input[type=file]').setInputFiles({name:'graph.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(graph))});
  const region=page.getByRole('region',{name:'Recorded search replay'}),svg=region.getByRole('group',{name:'Accumulated explored search graph'});
  await expect(svg.locator('[data-node]')).toHaveCount(1);
  await region.getByRole('slider',{name:'Search event timeline'}).fill('4');
  await expect(svg.locator('[data-node]')).toHaveCount(2);
  await region.getByRole('slider',{name:'Search event timeline'}).fill('7');
  await expect(svg.locator('[data-node]')).toHaveCount(3);
  await expect(svg.locator('[data-edge="0:0:1"]')).toHaveCount(1);await expect(svg.locator('[data-edge="0:0:2"]')).toHaveCount(1);
  await region.getByRole('button',{name:'See complete graph'}).click();await expect(svg.locator('[data-node]')).toHaveCount(4);
  await region.getByRole('combobox',{name:'Inspect discovered position'}).selectOption('3');
  await expect(region.locator('.search-node-detail')).toContainText('First leaf estimate: 300');
  await region.getByRole('button',{name:'Zoom in search graph'}).click();await region.getByRole('button',{name:'Focus current simulation path'}).click();
  await svg.locator('[data-node="0"]').focus();await page.keyboard.press('ArrowDown');await page.keyboard.press('Enter');
  await expect(region.locator('.search-node-detail')).toContainText('Node 1');
  await region.getByRole('button',{name:'Play search replay'}).click();
  await expect(region.getByRole('slider',{name:'Search event timeline'})).not.toHaveValue('0');
  await region.getByRole('slider',{name:'Search event timeline'}).fill('0');
  await expect(svg.locator('[data-node]')).toHaveCount(1);await expect(region.getByRole('button',{name:'Play search replay'})).toBeVisible();
  await page.setViewportSize({width:320,height:900});await page.emulateMedia({reducedMotion:'reduce'});
  await region.getByRole('button',{name:'See complete graph'}).click();
  expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  await expect.poll(async()=>svg.locator('[data-node="0"]').evaluate(node=>{const n=node.getBoundingClientRect(),s=node.closest('svg')!.getBoundingClientRect();return n.left>=s.left&&n.right<=s.right;})).toBe(true);
  await region.screenshot({path:'test-results/search-graph-320.png'});
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
