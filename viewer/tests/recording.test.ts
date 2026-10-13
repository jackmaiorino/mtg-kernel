import { describe, expect, it } from 'vitest';
import fixture from './fixtures/actor-visible.json';
import native from './fixtures/native-root-excerpt.json';
import replayFixture from './fixtures/search-replay.json';
import graphFixture from './fixtures/search-graph.json';
import retained from './fixtures/retained-search.json';
import uctFixture from './fixtures/uct-search.json';
import pooledUct from './fixtures/uct-pooled.json';
import { uctEvents, uctGraphAt } from '../src/uct-playback';
import rootChoicesFixture from './fixtures/root-choices.json';
import { graphAt, graphLayout, rootChoicesAt, rootChoiceStatus } from '../src/search-graph';
import { initialSearchPlayback, searchReducer, searchEvents, rootHistory } from '../src/search-playback';
import { actionsAt, actionLayout, responseRecords, responseSummary } from '../src/search-actions';
import { comparePolicyMasses, policyFavorite, parseRecording, verifySearchDigests } from '../src/recording';
import { initialPlayback, playbackReducer } from '../src/playback';

const copy = () => structuredClone(fixture);
describe('full UCT discovery traces',()=>{
  it('restores pooled snapshots losslessly and rejects broken or unused pooled states',async()=>{
    const r=parseRecording(pooledUct);await verifySearchDigests(r);
    expect(r.frames[0].search!.uctReplay!.boards[0]).toEqual(r.frames[0].board);
    const broken=structuredClone(pooledUct);broken.frames[0].search.uctReplay.boards[0].players[0].battlefield=[9];
    expect(()=>parseRecording(broken)).toThrow(/pooled snapshot/);
    const unused=structuredClone(pooledUct);unused.frames[0].search.uctReplay.objects.push(unused.frames[0].search.uctReplay.objects[0]);
    expect(()=>parseRecording(unused)).toThrow(/pooled snapshot/);
  });
  it('reconciles natural wins, draws and discarded proposals, revealing branches before backup',async()=>{
    const r=parseRecording(uctFixture);await verifySearchDigests(r);const u=r.frames[0].search!.uctReplay!;
    expect(u.firstCombo).toBe(0);expect(uctEvents(u).filter(e=>e.phase==='rollout')).toHaveLength(8);
    const early=uctEvents(u)[1];expect(early.phase).toBe('rollout');expect(uctGraphAt(u,early).nodes.size).toBe(1);
    const pending=uctGraphAt(u,{simulation:0,phase:'select',choice:1});expect(pending.nodes.size).toBe(2);expect(pending.completed).toBe(0);
    const natural=uctGraphAt(u,{simulation:0,phase:'backup',choice:1});expect(natural.nodes.get(1)?.visits).toBe(1);
    const final=uctGraphAt(u,{simulation:2,phase:'backup',choice:1});expect(final.completed).toBe(2);expect(final.nodes.get(0)?.sum).toBe(1000000n);expect(final.nodes.get(2)?.visits).toBe(0);
    expect(uctGraphAt(u,{simulation:0,phase:'select',choice:0}).nodes.size).toBe(1);
  });
  it('rejects invented source, combo, move attribution, labels and early backups',()=>{
    const mutate=[(r:typeof uctFixture)=>{r.frames[0].decisionSource.artifactSha256='b'.repeat(64);},
      (r:typeof uctFixture)=>{r.frames[0].search.uctReplay.simulations[0].rollout[0][5]=0;},
      (r:typeof uctFixture)=>{r.frames[0].search.uctReplay.simulations[0].choices[0].live=0;},
      (r:typeof uctFixture)=>{r.frames[0].search.uctReplay.nodes[0].actions[0].label='Invented move';},
      (r:typeof uctFixture)=>{r.frames[0].search.uctReplay.nodes[2].visits=1;},
      (r:typeof uctFixture)=>{Object.assign(r.frames[0].search.uctReplay.nodes[1].actions[0],{refs:['absent']});},
      (r:typeof uctFixture)=>{Object.assign(r.frames[0].search.uctReplay.boards[0],{combat:[{attacker:'absent',blockers:[]}]});},
      (r:typeof uctFixture)=>{Object.assign(r.frames[0].search.uctReplay.actions[0],{refs:['absent']});}];
    for(const change of mutate){const r=structuredClone(uctFixture);change(r);expect(()=>parseRecording(r)).toThrow(/UCT/);}
  });
  it('verifies its compact public table digest independently',async()=>{
    const r=structuredClone(uctFixture);r.frames[0].search.uctReplay.tableDigest='b'.repeat(64);
    await expect(verifySearchDigests(parseRecording(r))).rejects.toThrow(/public table digest/);
  });
});
describe('retained search-controlled decisions',()=>{
  it('accepts original Git source identities and binds the reconstruction artifact',()=>{
    const reconstruction={kind:'recorded-actions',originalSource:'c'.repeat(40),originalRuntime:'d'.repeat(64),
      sourceArtifactSha256:'b'.repeat(64),openingArtifactSha256:'e'.repeat(64),runtimeRules:'resolution-boundary-v1'};
    expect(parseRecording({...retained,reconstruction}).reconstruction?.originalSource).toBe(reconstruction.originalSource);
    expect(()=>parseRecording({...retained,reconstruction:{...reconstruction,sourceArtifactSha256:'f'.repeat(64)}})).toThrow(/different reconstruction source/);
  });
  it('loads a bound root table and continuation without inventing optional diagnostics',async()=>{
    const r=parseRecording(retained);await verifySearchDigests(r);
    expect(r.frames[0].search?.algorithm).toBe('persistent-uct');
    expect(r.frames[1].decisionSource?.root).toBe(r.frames[0].id);
    expect(r.frames.every(f=>f.policy===null)).toBe(true);
    expect(r.frames[0].search?.replay).toBeUndefined();
  });
  it('rejects missing roots, changed tree identities and older format provenance',()=>{
    const missing=structuredClone(retained);missing.frames[1].decisionSource!.root='absent';
    expect(()=>parseRecording(missing)).toThrow(/binding/);
    const mismatch=structuredClone(retained);mismatch.frames[1].decisionSource!.treeDigest='c'.repeat(64);
    expect(()=>parseRecording(mismatch)).toThrow(/binding/);
    const old=structuredClone(retained);old.schema='mtg-kernel-viewer-recording/v2';
    expect(()=>parseRecording(old)).toThrow(/format v3/);
    const episode=structuredClone(retained);episode.frames[1].key.episode++;
    expect(()=>parseRecording(episode)).toThrow(/binding/);
  });
});
describe('growing search graph',()=>{
  it('groups distinct prefix frontiers by action without merging their identities',()=>{
    const replay=parseRecording(graphFixture).frames[0].search!.replay!,event={simulation:2,phase:'backup' as const,choice:0};
    const graph=graphAt(replay,event),model=actionsAt(replay,event,graph),layout=actionLayout(model,graph);
    expect(graph.nodes.size).toBe(3);expect(model.groups.get('0:0')).toEqual([1,2]);
    expect(model.menus).toEqual(new Set([0]));expect(layout.destinations.get(1)).toBe('g0:0');
    expect(layout.destinations.get(2)).toBe('g0:0');expect(model.actions.get('0:0')!.visits).toBe(3);
    expect(responseRecords(1,replay,event)).toEqual([]);expect(responseSummary(1,replay,model)).toBe('Evaluated before a response');
  });
  it('unfolds an explored opponent and records its response with exact simulation timing',()=>{
    const replay=parseRecording(graphFixture).frames[0].search!.replay!,selected={simulation:3,phase:'select' as const,choice:1};
    const graph=graphAt(replay,selected),model=actionsAt(replay,selected,graph);
    expect(model.groups.size).toBe(0);expect(model.menus.has(1)).toBe(true);expect(model.actions.get('1:0')!.visits).toBe(0);
    expect(responseRecords(1,replay,selected)).toEqual([{simulation:3,index:0,label:'Pass priority',stage:'selected'}]);
    const evaluated={...selected,phase:'evaluate' as const},backed={...selected,phase:'backup' as const};
    expect(responseRecords(1,replay,evaluated)[0].stage).toBe('tried');expect(responseRecords(1,replay,backed)[0].stage).toBe('backed up');
    expect(actionsAt(replay,backed,graphAt(replay,backed)).actions.get('1:0')!.visits).toBe(1);
    const rewind={simulation:0,phase:'select' as const,choice:0};expect(actionsAt(replay,rewind,graphAt(replay,rewind)).actions.has('1:0')).toBe(false);
  });
  it('keeps a hypothetical chosen spell distinct from hidden hand availability',()=>{
    const replay=parseRecording(graphFixture).frames[0].search!.replay!;
    replay.nodes[1].actions[0].label='Cast Lightning Bolt';
    const end={simulation:4,phase:'backup' as const,choice:0};
    expect(responseRecords(1,replay,end)[0].label).toBe('Cast Lightning Bolt');
    expect(responseRecords(2,replay,end)).toEqual([]);
  });
  it('keeps all four legal moves before search visits or evaluates them',()=>{
    const replay=parseRecording(rootChoicesFixture).frames[0].search!.replay!;
    const event={simulation:0,phase:'select' as const,choice:0},graph=graphAt(replay,event);
    const choices=rootChoicesAt(replay,event,graph);
    expect(choices.map(c=>c.index)).toEqual([0,1,2,3]);
    expect(choices.every(c=>c.visits===0&&c.evaluations===0&&c.children.length===0&&c.firstEvaluation===null)).toBe(true);
    expect(graph.nodes.size).toBe(1);
  });
  it('aligns root results beneath the action that produced them',()=>{
    const replay=parseRecording(rootChoicesFixture).frames[0].search!.replay!,event={simulation:4,phase:'backup' as const,choice:0};
    const graph=graphAt(replay,event),layout=actionLayout(actionsAt(replay,event,graph),graph);
    expect(layout.items.get('n1')!.x).toBe(layout.items.get('a0:0')!.x);
    for(const index of [1,2,3])expect(layout.items.get(`e0:${index}`)!.x).toBe(layout.items.get(`a0:${index}`)!.x);
  });
  it('retains evaluation-only alternatives and advances visits at backup',()=>{
    const replay=parseRecording(rootChoicesFixture).frames[0].search!.replay!;
    const evaluated={simulation:1,phase:'evaluate' as const,choice:0};
    const probe=rootChoicesAt(replay,evaluated,graphAt(replay,evaluated))[1];
    expect(probe).toMatchObject({visits:0,evaluations:1,children:[],firstEvaluation:{value:'-7786',simulation:1}});
    expect(rootChoiceStatus(probe)).toContain('backup pending');
    const end={simulation:4,phase:'backup' as const,choice:0},graph=graphAt(replay,end),choices=rootChoicesAt(replay,end,graph);
    expect(choices.map(c=>c.visits)).toEqual([2,1,1,1]);
    expect(choices[0].children).toEqual([1]);
    expect(choices.slice(1).map(rootChoiceStatus)).toEqual(Array(3).fill('Evaluated once · not expanded'));
    expect(graph.nodes.size).toBe(2);
    const rewind={simulation:0,phase:'select' as const,choice:0};
    expect(rootChoicesAt(replay,rewind,graphAt(replay,rewind)).every(c=>c.firstEvaluation===null)).toBe(true);
  });
  it('does not label an older expansion with missing identity as unexpanded',()=>{
    const replay=parseRecording(rootChoicesFixture).frames[0].search!.replay!;
    replay.schema='mtg-kernel-search-replay/v1';delete replay.simulations[4].leafNode;
    const event={simulation:4,phase:'backup' as const,choice:0},choice=rootChoicesAt(replay,event,graphAt(replay,event))[0];
    expect(choice.unidentifiedExpansions).toBe(1);
    expect(rootChoiceStatus(choice)).toBe('Expansion recorded · child identity unavailable');
  });
  it('reveals evaluated positions at expansion and updates visits only at backup',async()=>{
    const r=parseRecording(graphFixture);await verifySearchDigests(r);const replay=r.frames[0].search!.replay!;
    const before=graphAt(replay,{simulation:1,phase:'select',choice:0});expect([...before.nodes.keys()]).toEqual([0]);
    const evaluated=graphAt(replay,{simulation:1,phase:'evaluate',choice:0});expect([...evaluated.nodes.keys()]).toEqual([0,1]);
    expect(evaluated.nodes.get(1)?.visits).toBe(0);expect(evaluated.frontier).toBe(1);
    expect(evaluated.explored.has(1)).toBe(false);
    const backedUp=graphAt(replay,{simulation:1,phase:'backup',choice:0});expect(backedUp.nodes.get(1)?.visits).toBe(1);
    const branched=graphAt(replay,{simulation:2,phase:'evaluate',choice:0});expect([...branched.edges.keys()]).toEqual(['0:0:1','0:0:2']);
    const final=graphAt(replay,{simulation:4,phase:'backup',choice:0});expect(final.nodes.size).toBe(4);expect(final.edges.size).toBe(3);
    expect(final.nodes.get(0)).toMatchObject({visits:5,sum:11600n});expect(final.nodes.get(1)).toMatchObject({visits:2,sum:1200n});
    expect(graphLayout(replay).get(3)?.depth).toBe(2);
    expect(graphAt(replay,{simulation:3,phase:'evaluate',choice:1}).explored.has(1)).toBe(true);
    expect(graphAt(replay,{simulation:0,phase:'select',choice:0}).nodes.size).toBe(1);
  });
  it('rejects wrong leaf identities, early node visits and unrecorded graph connections',()=>{
    const wrong=structuredClone(graphFixture);wrong.frames[0].search!.replay!.simulations[1].leafNode=2;
    expect(()=>parseRecording(wrong)).toThrow(/leaf identity/);
    const early=structuredClone(graphFixture);early.frames[0].search!.replay!.simulations[0].choices[0].node=1;
    expect(()=>parseRecording(early)).toThrow(/order/);
    const extra=structuredClone(graphFixture);extra.frames[0].search!.replay!.nodes[2].children[0].push(3);
    expect(()=>parseRecording(extra)).toThrow(/coverage/);
  });
});
describe('recorded search replay',()=>{
  it('loads v2 and reconciles exact root statistics',async()=>{
    const r=parseRecording(replayFixture);await verifySearchDigests(r);
    const replay=r.frames[0].search!.replay!,history=rootHistory(replay);
    expect(history[0].visits).toEqual([0,0]);expect(history.at(-1)?.sums).toEqual([900n,-500n]);
    expect(searchEvents(replay).map(e=>e.phase)).toEqual(['select','evaluate','backup','select','evaluate','backup']);
  });
  it('rejects another decision, broken trace sums and exposed sampled hands',()=>{
    const key=structuredClone(replayFixture);key.frames[0].search!.replay!.key.step++;
    expect(()=>parseRecording(key)).toThrow(/different decision/);
    const sums=structuredClone(replayFixture);sums.frames[0].search!.replay!.simulations[0].value='901';
    expect(()=>parseRecording(sums)).toThrow(/statistics/);
    const privateCards=parseRecording(replayFixture);privateCards.frames[0].search!.replay!.simulations[0].leaf.players[0].hand=structuredClone(parseRecording(native).frames[0].board.players[0].hand);
    expect(()=>parseRecording(privateCards)).toThrow(/sampled hand/);
  });
  it('seeking cancels playback and stops at the last event',()=>{
    const playing={...initialSearchPlayback,playing:true};
    expect(searchReducer(playing,{type:'seek',event:4})).toMatchObject({event:4,playing:false,epoch:1});
    expect(searchReducer({...playing,event:5},{type:'advance',length:6}).playing).toBe(false);
  });
});
describe('versioned recordings', () => {
  it('ranks nearly equal Q64 masses without losing their exact favorite', () => {
    const masses = ['9223372036854775807', '9223372036854775809'];
    expect(Number(masses[0])).toBe(Number(masses[1]));
    expect(masses.reduce((sum, mass) => sum + BigInt(mass), 0n)).toBe(1n << 64n);
    expect(policyFavorite(masses)).toBe(1);
    expect([0,1].sort((a,b) => comparePolicyMasses(masses[a], masses[b]))).toEqual([1,0]);
    expect(policyFavorite(['9223372036854775808','9223372036854775808'])).toBe(0);
  });
  it('verifies actual recorded root digests, including their work-count suffix', async () => {
    await expect(verifySearchDigests(parseRecording(native))).resolves.toBeUndefined();
  });
  it('loads a real actor-visible historical excerpt', () => {
    const r = parseRecording(copy()); expect(r.frames.length).toBeGreaterThan(0);
    expect(r.frames.every(f => f.policy && !f.search)).toBe(true);
  });
  it('rejects opposing hand exposure even if a file claims to be public', () => {
    const r = copy(); const own = r.perspective === 'p0' ? 0 : 1;
    r.frames[0].board.players[1 - own].hand = structuredClone(r.frames[0].board.players[own].hand);
    r.frames[0].board.players[1 - own].hand.forEach((o, i) => o.id = `opposing-${i}`);
    expect(() => parseRecording(r)).toThrow('opposing hand');
  });
  it('rejects altered sampler masses', () => {
    const r = copy(); r.frames[0].policy!.masses[0] = '0';
    expect(() => parseRecording(r)).toThrow('sampling distribution');
  });
  it('rejects incompatible versions, extra private fields, and broken execution', () => {
    expect(() => parseRecording({ ...copy(), schema:'v2' })).toThrow();
    expect(() => parseRecording({ ...copy(), seed:123 })).toThrow();
    const r = copy(); r.frames[0].executed = 9999; expect(() => parseRecording(r)).toThrow('Executed');
  });
  it('does not allow a partial recording to claim a completed game', () => {
    expect(() => parseRecording({ ...copy(), complete:true })).toThrow('terminal');
  });
  it('rejects fabricated search tables using their committed digest', async () => {
    const r = parseRecording(copy());
    r.frames[0].search = { digest:'0'.repeat(64), simulations:1, transitions:1, nodes:1, tier:'test', stabilityHalves:false, valueScale:'test',
      stats:r.frames[0].actions.map((a, i) => ({index:a.index,visits:i === 0 ? 1 : 0,valueSum:'0',meanValue:'0'})) };
    await expect(verifySearchDigests(r)).rejects.toThrow('digest');
  });
});
describe('playback', () => {
  it('seeking invalidates motion and stops playback', () => {
    const next = playbackReducer({ ...initialPlayback, playing:true, animate:true }, { type:'seek',index:3 });
    expect(next).toMatchObject({ index:3, playing:false, animate:false, after:false, epoch:1 });
  });
  it('displays the recorded consequence before advancing and stops at terminal', () => {
    const playing = { ...initialPlayback, playing:true };
    const after = playbackReducer(playing, { type:'advance',length:1 }); expect(after.after).toBe(true);
    expect(playbackReducer(after, { type:'advance',length:1 }).playing).toBe(false);
  });
});
