import { describe, expect, it } from 'vitest';
import fixture from './fixtures/actor-visible.json';
import native from './fixtures/native-root-excerpt.json';
import replayFixture from './fixtures/search-replay.json';
import graphFixture from './fixtures/search-graph.json';
import retained from './fixtures/retained-search.json';
import { graphAt, graphLayout } from '../src/search-graph';
import { initialSearchPlayback, searchReducer, searchEvents, rootHistory } from '../src/search-playback';
import { comparePolicyMasses, policyFavorite, parseRecording, verifySearchDigests } from '../src/recording';
import { initialPlayback, playbackReducer } from '../src/playback';

const copy = () => structuredClone(fixture);
describe('retained search-controlled decisions',()=>{
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
