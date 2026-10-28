import { type Frame, type Recording, policyFavorite, probability } from './recording';

// Misplay review: a local server lists games in demo/manifest.json with `review` set and stores flags as JSONL.
export const CATEGORIES = [
  { key: 'tactical', label: 'Tactical / combat' },
  { key: 'sequencing', label: 'Sequencing' },
  { key: 'mana', label: 'Mana' },
  { key: 'misread', label: 'Card or board misread' },
  { key: 'strategic', label: 'Strategic' },
  { key: 'other', label: 'Other' },
] as const;
export type Category = typeof CATEGORIES[number]['key'];
export type Flag = { game: string; decision: string; index: number; turn: number; category: Category; note: string; removed?: boolean };
export type ReviewGame = { id: string; title?: string; subtitle?: string; decisions?: number };

const label = (s: string) => s.replace(/\s*\[c\d+\]/g, '');
// A decision worth reviewing: the featured seat chose between two or more moves.
export const isReviewable = (r: Recording, f: Frame) => f.key.actor === r.perspective && f.actions.length > 1;

export function step(r: Recording, index: number, direction: 1 | -1, skipTrivial: boolean): number {
  for (let i = index + direction; i >= 0 && i < r.frames.length; i += direction)
    if (!skipTrivial || isReviewable(r, r.frames[i])) return i;
  return index;
}

// First reviewable decision of the requested turn, else the first decision of any later turn.
export function turnIndex(r: Recording, turn: number, skipTrivial: boolean): number | null {
  const at = r.frames.findIndex(f => f.board.turn >= turn && (!skipTrivial || isReviewable(r, f)));
  if (at >= 0) return at;
  const any = r.frames.findIndex(f => f.board.turn >= turn);
  return any >= 0 ? any : null;
}

// Latest record per decision wins; a removal record clears it.
export function currentFlags(records: Flag[]): Map<string, Flag> {
  const flags = new Map<string, Flag>();
  for (const f of records) { const key = `${f.game}\u0000${f.decision}`; if (f.removed) flags.delete(key); else flags.set(key, f); }
  return flags;
}

// The record the lead reads: the decision, what was played, and the model's own view of it.
export function flagRecord(r: Recording, index: number, category: Category, note: string) {
  const f = r.frames[index], played = f.actions[f.executed];
  const ranked = f.policy ? f.actions.map(a => ({ action: label(a.label), p: probability(f.policy!.masses[a.index]) })).sort((a, b) => b.p - a.p).slice(0, 5) : [];
  return {
    schema: 'misplay-flag/v1', game: r.id, decision: f.id, index, turn: f.board.turn, phase: f.board.phase, category, note,
    played: label(played.label), playedProbability: f.policy ? probability(f.policy.masses[f.executed]) : null,
    policyFavorite: f.policy ? label(f.actions[policyFavorite(f.policy.masses)].label) : null,
    value: f.policy?.value ?? null, alternatives: ranked, legalChoices: f.actions.length,
  };
}
