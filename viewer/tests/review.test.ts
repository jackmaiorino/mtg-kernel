import { describe, expect, it } from 'vitest';
import fixture from './fixtures/actor-visible.json';
import { parseRecording } from '../src/recording';
import { currentFlags, flagRecord, isReviewable, step, turnIndex, type Flag } from '../src/review';

const recording = () => {
  const r = parseRecording(structuredClone(fixture));
  return r;
};

describe('misplay review navigation', () => {
  it('steps only between featured decisions with a real choice when skipping trivial ones', () => {
    const r = recording();
    const reviewable = r.frames.flatMap((f, i) => isReviewable(r, f) ? [i] : []);
    expect(reviewable.length).toBeGreaterThan(0);
    for (const i of reviewable) expect(r.frames[i].actions.length).toBeGreaterThan(1);
    expect(step(r, -1, 1, true)).toBe(reviewable[0]);
    expect(step(r, reviewable[0], 1, false)).toBe(Math.min(reviewable[0] + 1, r.frames.length - 1));
    const last = r.frames.length - 1;
    expect(step(r, last, 1, true)).toBe(last);
    expect(step(r, 0, -1, true)).toBe(0);
  });
  it('jumps to the first decision of a turn and reports turns past the end', () => {
    const r = recording();
    const turn = r.frames.at(-1)!.board.turn;
    const index = turnIndex(r, turn, false)!;
    expect(r.frames[index].board.turn).toBe(turn);
    expect(index === 0 || r.frames[index - 1].board.turn < turn).toBe(true);
    expect(turnIndex(r, turn + 1000, false)).toBeNull();
  });
});

describe('misplay flags', () => {
  it('keeps the latest record per decision and honors removals', () => {
    const base = { game: 'g', index: 1, turn: 1, note: '' } as const;
    const flags = currentFlags([
      { ...base, decision: 'a', category: 'tactical' }, { ...base, decision: 'a', category: 'mana' },
      { ...base, decision: 'b', category: 'other' }, { ...base, decision: 'b', category: 'other', removed: true },
    ] as Flag[]);
    expect([...flags.values()].map(f => [f.decision, f.category])).toEqual([['a', 'mana']]);
  });
  it('records the played move, the model favorite and its value for the lead', () => {
    const r = recording();
    const index = r.frames.findIndex(f => f.policy);
    const record = flagRecord(r, index, 'strategic', 'should race');
    expect(record).toMatchObject({ schema: 'misplay-flag/v1', game: r.id, decision: r.frames[index].id, index, category: 'strategic', note: 'should race' });
    expect(record.alternatives.length).toBeGreaterThan(0);
    expect(record.played).not.toMatch(/\[c\d+\]/);
    expect(record.value).toBe(r.frames[index].policy!.value);
  });
});
