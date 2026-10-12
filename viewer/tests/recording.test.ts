import { describe, expect, it } from 'vitest';
import fixture from './fixtures/actor-visible.json';
import { parseRecording, verifySearchDigests } from '../src/recording';
import { initialPlayback, playbackReducer } from '../src/playback';

const copy = () => structuredClone(fixture);
describe('versioned recordings', () => {
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
