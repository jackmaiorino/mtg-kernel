import { test, expect } from '@playwright/test';
import fixture from '../fixtures/actor-visible.json' with { type: 'json' };
import native from '../fixtures/native-root-excerpt.json' with { type: 'json' };
import replay from '../fixtures/search-replay.json' with { type: 'json' };
import graph from '../fixtures/search-graph.json' with {type:'json'};
import rootChoices from '../fixtures/root-choices.json' with {type:'json'};
import retained from '../fixtures/retained-search.json' with {type:'json'};
import uct from '../fixtures/uct-search.json' with {type:'json'};
import pooledUct from '../fixtures/uct-pooled.json' with {type:'json'};
import lifecycle from '../fixtures/uct-lifecycle.json' with {type:'json'};
import { createHash } from 'node:crypto';

async function openFixture(page: import('@playwright/test').Page) {
  await page.route('**/demo/manifest.json', route => route.fulfill({status:404}));
  await page.goto('/');
  await page.locator('input[type=file]').setInputFiles({name:'recording.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(fixture))});
  await expect(page.getByRole('region', { name:'Game replay' })).toBeVisible();
}

test('actual guide lifecycle distinguishes refresh from reuse and locates the game without changing its decision',async({page})=>{
  await openFixture(page);await page.locator('input[type=file]').setInputFiles({name:'lifecycle.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(lifecycle))});
  const status=page.getByRole('region',{name:'Actual game search lifecycle'});
  await expect(status.getByRole('heading',{name:'Starts a new search'})).toBeVisible();
  await page.getByRole('button',{name:'Next decision',exact:true}).click();
  await expect(status.locator('.lifecycle-refresh')).toContainText('no move had the eight completed rollouts');
  await expect(status.locator('.lifecycle-refresh')).toContainText('node 1');
  await page.getByRole('button',{name:'Next decision',exact:true}).click();
  await expect(status.getByRole('heading',{name:'Follows the existing tree'})).toBeVisible();
  await expect(page.locator('[data-actual-node="1"]')).toBeVisible();
  await expect(page).toHaveURL(/decision=d17/);
  await page.getByRole('button',{name:'Return to search discovery'}).click();
  await expect(page.locator('[data-actual-node="1"]')).toHaveCount(0);
  await page.getByRole('button',{name:'Locate actual game in tree'}).click();
  await expect(page.locator('[data-actual-node="1"]')).toBeVisible();await expect(page).toHaveURL(/decision=d17/);
  await expect(page).toHaveURL(/searchView=actual/);
  await page.setViewportSize({width:320,height:900});await page.emulateMedia({reducedMotion:'reduce'});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  await status.screenshot({path:'test-results/search-lifecycle-320.png'});
});

test('a link between reuse decisions of one tree restores its requested search event',async({page})=>{
  await openFixture(page);const r=structuredClone(lifecycle),next=structuredClone(r.frames[2]);
  next.id='d18';next.key.step=18;next.key.physical=18;r.frames.push(next);
  await page.locator('input[type=file]').setInputFiles({name:'reuse-links.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(r))});
  await page.evaluate(id=>{window.location.hash=`game=${id}&decision=d17&searchEvent=5`;},r.id);
  await expect(page.getByRole('slider',{name:'Search event timeline'})).toHaveValue('5');
  await page.evaluate(id=>{window.location.hash=`game=${id}&decision=d18&searchEvent=2`;},r.id);
  await expect(page.getByRole('slider',{name:'Search event timeline'})).toHaveValue('2');
  await expect(page).toHaveURL(/decision=d18&searchEvent=2/);
});

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

test('every root move remains visible, including evaluations without stored children',async({page})=>{
  await openFixture(page);
  await page.locator('input[type=file]').setInputFiles({name:'root-choices.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(rootChoices))});
  const region=page.getByRole('region',{name:'Recorded search replay'}),svg=region.getByRole('group',{name:'Accumulated explored search graph'});
  await expect(svg.locator('.search-graph-root-choice')).toHaveCount(4);
  await expect(svg.locator('[data-node]')).toHaveCount(1);
  await expect(svg.locator('[data-root-evaluation]')).toHaveCount(0);
  await expect(region.locator('.search-root-options')).toContainText('Not evaluated yet');
  await region.getByRole('button',{name:'See complete graph'}).click();
  await expect(svg.locator('[data-node]')).toHaveCount(2);
  await expect(svg.locator('[data-root-evaluation]')).toHaveCount(3);
  await expect(region.locator('.search-root-status').filter({hasText:'Evaluated once · not expanded'})).toHaveCount(3);
  const lava=svg.locator('.search-graph-root-choice[data-root-choice="1"]');
  await lava.focus();await page.keyboard.press('Enter');
  await expect(region.locator('.search-node-detail')).toContainText('Initial evaluation from simulation 2: -7786');
  await expect(region.locator('.search-preview-label')).toContainText('Initial evaluation of move 1');
  await expect(region.getByRole('slider',{name:'Search event timeline'})).toHaveValue('14');
  await region.screenshot({path:'test-results/root-choices-desktop.png'});
  await page.setViewportSize({width:320,height:900});await page.emulateMedia({reducedMotion:'reduce'});
  expect(await page.evaluate(()=>document.documentElement.scrollWidth)).toBeLessThanOrEqual(320);
  await expect(svg.locator('.search-graph-root-choice')).toHaveCount(4);
  await region.screenshot({path:'test-results/root-choices-320.png'});
  await region.getByRole('slider',{name:'Search event timeline'}).fill('0');
  await expect(svg.locator('[data-root-evaluation]')).toHaveCount(0);
  await expect(region.locator('.search-preview-label')).toHaveCount(0);
  await expect(svg.locator('.search-graph-root-choice')).toHaveCount(4);
});

test('the graph retains discovered branches, inspects positions and rewinds',async({page})=>{
  await openFixture(page);
  await page.locator('input[type=file]').setInputFiles({name:'graph.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(graph))});
  const region=page.getByRole('region',{name:'Recorded search replay'}),svg=region.getByRole('group',{name:'Accumulated explored search graph'});
  await expect(svg.locator('[data-node]')).toHaveCount(1);
  await region.getByRole('slider',{name:'Search event timeline'}).fill('4');
  await expect(svg.locator('[data-node]')).toHaveCount(2);
  await region.getByRole('slider',{name:'Search event timeline'}).fill('7');
  await expect(svg.locator('[data-node]')).toHaveCount(1);
  await expect(svg.locator('[data-outcome-group="0:0"]')).toHaveAttribute('data-outcome-count','2');
  await svg.locator('[data-outcome-group="0:0"]').focus();await page.keyboard.press('Enter');
  await expect(region.locator('[data-outcome-position]')).toHaveCount(2);
  await expect(region.locator('.search-outcome-list')).toContainText('Evaluated before a response');
  await expect(region.getByRole('slider',{name:'Search event timeline'})).toHaveValue('7');
  await region.getByRole('button',{name:'See complete graph'}).click();await expect(svg.locator('[data-node]')).toHaveCount(4);
  await expect(svg.locator('[data-search-action="1:0"]')).toContainText('OPPONENT');
  await svg.locator('[data-search-action="0:0"]').focus();await page.keyboard.press('Enter');
  await region.locator('.search-simulation-responses summary').click();
  await expect(region.locator('.search-simulation-responses')).toContainText('Simulation 4 · Opponent tried');
  await expect(region.locator('.search-simulation-responses')).toContainText('Pass priority');
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
test('an interior evaluation previews its result without changing search time',async({page})=>{
  await openFixture(page);
  const recording=structuredClone(graph),frame=recording.frames[0],replay=frame.search!.replay!;
  recording.title='Synthetic interior evaluation regression';
  const evaluated=structuredClone(replay.simulations[3]);
  evaluated.ordinal=5;evaluated.value='-7000';evaluated.reason='depth';evaluated.leafNode=null;
  evaluated.choices.push({node:3,action:0,mode:'puct',scores:[null],board:structuredClone(evaluated.leaf)});
  evaluated.leaf.players[0].life=7;replay.simulations.push(evaluated);
  for(const n of replay.nodes){n.visits=0;n.edgeVisits.fill(0);n.edgeValueSums.fill('0');}
  let transitions=0;
  for(const s of replay.simulations){for(const c of s.choices){const n=replay.nodes[c.node];n.visits++;n.edgeVisits[c.action]++;n.edgeValueSums[c.action]=String(BigInt(n.edgeValueSums[c.action])+BigInt(s.value));transitions++;}if(s.leafNode!=null)replay.nodes[s.leafNode].visits++;}
  const search=frame.search!;search.transitions=transitions;search.simulations=replay.simulations.length;
  search.stats.forEach((s,i)=>{s.visits=replay.nodes[0].edgeVisits[i];s.valueSum=replay.nodes[0].edgeValueSums[i];s.meanValue=s.visits?String(BigInt(s.valueSum)/BigInt(s.visits)):'0';});
  const bytes=Buffer.alloc(24+search.stats.length*24);bytes.writeBigUInt64LE(BigInt(search.stats.length));
  search.stats.forEach((s,i)=>{const p=8+i*24;bytes.writeUInt32LE(s.index,p);bytes.writeUInt32LE(s.visits,p+4);bytes.writeBigInt64LE(BigInt(s.valueSum),p+8);bytes.writeBigInt64LE(BigInt(s.meanValue),p+16);});
  [frame.executed,search.transitions,search.simulations,search.nodes].forEach((n,i)=>bytes.writeUInt32LE(n,8+search.stats.length*24+i*4));
  search.digest=createHash('sha256').update(bytes).digest('hex');
  await page.locator('input[type=file]').setInputFiles({name:'interior.json',mimeType:'application/json',buffer:Buffer.from(JSON.stringify(recording))});
  const region=page.getByRole('region',{name:'Recorded search replay'});
  await region.getByRole('button',{name:'See complete graph'}).click();
  await region.locator('[data-item="e3:0"]').focus();await page.keyboard.press('Enter');
  await expect(region.locator('.search-preview-label')).toContainText('move 0 at position 3 · simulation 6');
  await expect(region.locator('.search-seat-heading strong').last()).toContainText('7');
  await expect(region.getByRole('slider',{name:'Search event timeline'})).toHaveValue('20');
  await region.getByRole('slider',{name:'Search event timeline'}).fill('0');
  await expect(region.locator('.search-preview-label')).toHaveCount(0);
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
