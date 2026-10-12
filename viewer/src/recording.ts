import { z } from 'zod';

export const SCHEMA = 'mtg-kernel-viewer-recording/v1';
export const MAX_BYTES = 64 * 1024 * 1024;
const seat = z.enum(['p0', 'p1']);
const integer = z.number().int().nonnegative().max(Number.MAX_SAFE_INTEGER);
const text = z.string().max(4096);
const hash = z.string().regex(/^[0-9a-f]{64}$/);
const u32 = z.number().int().nonnegative().max(0xffffffff);
const i64 = z.string().regex(/^-?\d+$/).refine(s => BigInt(s) >= -(1n << 63n) && BigInt(s) < (1n << 63n), 'Value exceeds signed 64-bit capture range');
const safeImage = z.string().max(1024).refine(s => /^assets\/[a-zA-Z0-9/_\.\-]+$/.test(s) && !s.split('/').includes('..'), 'Invalid asset path');
const cardSchema = z.object({
  id: text, name: text, manaCost: text, type: text, rules: text,
  image: safeImage.nullable(), printing: text.nullable(), face: integer,
}).strict();
const objectSchema = z.object({
  id: text, card: text, owner: seat, controller: seat, tapped: z.boolean(),
  power: z.number().int().nullable(), toughness: z.number().int().nullable(),
  damage: integer, counters: z.record(z.string(), integer), token: z.boolean(),
}).strict();
const playerSchema = z.object({
  life: z.number().int(), handCount: integer, libraryCount: integer,
  mana: z.array(integer).length(6), battlefield: z.array(objectSchema),
  graveyard: z.array(objectSchema), hand: z.array(objectSchema),
}).strict();
const boardSchema = z.object({
  turn: integer, phase: text, active: seat, priority: seat,
  players: z.tuple([playerSchema, playerSchema]), exile: z.array(objectSchema),
  stack: z.array(z.object({ id: text, name: text, controller: seat, source: text.nullable(), targets: z.array(text) }).strict()),
  combat: z.array(z.object({ attacker: text, blockers: z.array(text) }).strict()),
}).strict();
const actionSchema = z.object({ index: integer, label: text, kind: text, refs: z.array(text) }).strict();
const keySchema = z.object({ episode: integer, step: integer, physical: integer, substep: integer, actor: seat }).strict();
const policySchema = z.object({
  kind: z.literal('hamilton_q64'), masses: z.array(z.string().regex(/^\d{1,20}$/)),
  sampled: integer, value: z.number().finite(), valueScale: text,
}).strict();
const replaySchema = z.object({
  schema: z.literal('mtg-kernel-search-replay/v1'), complete: z.literal(true), key: keySchema, worlds: text,
  nodes: z.array(z.object({ id: u32, actor: seat, visits: u32, priors: z.array(u32),
    actions: z.array(actionSchema).min(1), children: z.array(z.array(u32)),
    edgeVisits: z.array(u32), edgeValueSums: z.array(i64),
  }).strict()).min(1).max(513),
  simulations: z.array(z.object({ ordinal: u32, value: i64,
    reason: z.enum(['natural','expanded','depth','transition_budget','coverage']),
    choices: z.array(z.object({ node: u32, action: u32,
      mode: z.enum(['coverage','round_robin','unvisited','prior_free','puct']),
      scores: z.array(i64.nullable()), board: boardSchema,
    }).strict()).min(1).max(64), leaf: boardSchema,
  }).strict()).min(1).max(512),
}).strict();
export type SearchReplay = z.infer<typeof replaySchema>;
const searchSchema = z.object({
  digest: hash, stats: z.array(z.object({ index: u32, visits: u32, valueSum: i64, meanValue: i64 }).strict()),
  simulations: u32, transitions: u32, nodes: u32, tier: text, stabilityHalves: z.boolean(), valueScale: text,
  replay: replaySchema.optional(),
}).strict();
const frameSchema = z.object({
  id: text, key: keySchema, board: boardSchema, after: boardSchema,
  actions: z.array(actionSchema).min(1).max(65536), executed: integer,
  policy: policySchema.nullable(), search: searchSchema.nullable(),
  terminal: z.object({ kind: text, winner: seat.nullable() }).strict().nullable(),
}).strict();
const recordingSchema = z.object({
  schema: z.enum([SCHEMA,'mtg-kernel-viewer-recording/v2']), id: z.string().regex(/^[a-z0-9][a-z0-9-]{0,95}$/), title: text,
  description: text, perspective: seat, complete: z.boolean(), coverage: z.enum(['every-step', 'decision-snapshots']),
  identity: z.object({ source: text, runtime: text, checkpoint: text, cardDatabase: text, sourceSha256: hash }).strict(),
  seats: z.tuple([z.object({ name: text, deck: text }).strict(), z.object({ name: text, deck: text }).strict()]),
  cards: z.record(z.string(), cardSchema), frames: z.array(frameSchema).min(1).max(100000),
  bookmarks: z.array(z.object({ frame: text, title: text, situation: text, alternatives: text, consequence: text }).strict()).max(30),
}).strict();
export type Recording = z.infer<typeof recordingSchema>;
export type Frame = Recording['frames'][number];
export type Board = Frame['board'];
export type CardObject = Board['players'][number]['battlefield'][number];
export type Card = Recording['cards'][string];
export type Seat = 'p0' | 'p1';
export const seatIndex = (s: Seat) => s === 'p0' ? 0 : 1;
export const objectList = (b: Board) => [...b.players.flatMap(p => [...p.battlefield, ...p.hand, ...p.graveyard]), ...b.exile];
export const probability = (mass: string) => Number(BigInt(mass)) / 2 ** 64;
export const comparePolicyMasses = (a: string, b: string) => BigInt(a) > BigInt(b) ? -1 : BigInt(a) < BigInt(b) ? 1 : 0;
export const policyFavorite = (masses: string[]) => masses.reduce((best, mass, index) => comparePolicyMasses(mass, masses[best]) < 0 ? index : best, 0);
export const frameKey = (k: Frame['key']) => `${k.episode}:${k.step}:${k.physical}:${k.substep}:${k.actor}`;
export const isRoutine = (f: Frame) => ['pass', 'pass_priority'].includes(f.actions[f.executed].kind) &&
  JSON.stringify(f.board.players) === JSON.stringify(f.after.players) &&
  JSON.stringify(f.board.stack) === JSON.stringify(f.after.stack) && !f.terminal;

export function parseRecording(raw: unknown): Recording {
  const result = recordingSchema.safeParse(raw);
  if (!result.success) {
    const issue = result.error.issues[0];
    throw new Error(`Recording cannot be opened: ${issue.path.join('.')} ${issue.message}`);
  }
  const r = result.data;
  const ids = new Set<string>();
  const keys = new Set<string>();
  for (const [id, card] of Object.entries(r.cards)) if (id !== card.id) throw new Error('Card identifier mismatch');
  for (const f of r.frames) {
    if (ids.has(f.id) || keys.has(frameKey(f.key))) throw new Error('Duplicate decision identity');
    ids.add(f.id); keys.add(frameKey(f.key));
    if (f.key.actor !== r.perspective && (f.policy || f.search || f.actions.length !== 1)) throw new Error('Opposing private decision diagnostics are not permitted');
    if (f.actions.some((a, i) => a.index !== i) || f.executed >= f.actions.length) throw new Error('Executed action or engine order is invalid');
    for (const b of [f.board, f.after]) {
      const objects = objectList(b);
      if (new Set(objects.map(o => o.id)).size !== objects.length) throw new Error('Duplicate object reference in a snapshot');
      if (objects.some(o => !r.cards[o.card])) throw new Error('Unknown card reference');
      if (b.players[1 - seatIndex(r.perspective)].hand.length) throw new Error('Recording exposes the opposing hand');
      const present = new Set(objects.map(o => o.id));
      if (b.combat.some(c => !present.has(c.attacker) || c.blockers.some(x => !present.has(x)))) throw new Error('Broken combat reference');
    }
    const visible = new Set([...objectList(f.board), ...objectList(f.after)].map(o => o.id).concat([...f.board.stack, ...f.after.stack].flatMap(s => s.source ? [s.source] : [])));
    if (f.actions.some(a => a.refs.some(ref => !visible.has(ref)))) throw new Error('Broken action reference');
    if (f.policy) {
      if (f.policy.masses.length !== f.actions.length || f.policy.sampled >= f.actions.length ||
        f.policy.masses.reduce((sum, n) => sum + BigInt(n), 0n) !== 1n << 64n) throw new Error('Invalid exact sampling distribution');
      if (f.key.actor !== r.perspective) throw new Error('Policy belongs to a different viewing perspective');
    }
    if (f.search) {
      const indices = f.search.stats.map(s => s.index);
      if (indices.length !== f.actions.length || new Set(indices).size !== indices.length || indices.some(i => i >= f.actions.length)) throw new Error('Search action mapping is invalid');
      if (f.search.stats.some(s => s.meanValue !== (s.visits ? BigInt(s.valueSum) / BigInt(s.visits) : 0n).toString())) throw new Error('Search mean differs from captured visit/value sum');
      if (f.search.replay) {
        if (r.schema === SCHEMA) throw new Error('Search replay requires recording format v2');
        validateReplay(f, r);
      }
    }
  }
  if (r.bookmarks.some(b => !ids.has(b.frame))) throw new Error('Bookmark points to a missing decision');
  if (r.complete && !r.frames.at(-1)?.terminal) throw new Error('Complete game has no recorded terminal state');
  return r;
}

function validateReplay(frame: Frame, recording: Recording) {
  const search = frame.search!, replay = search.replay!;
  if (frameKey(replay.key) !== frameKey(frame.key)) throw new Error('Search replay belongs to a different decision');
  if (replay.nodes.length !== search.nodes || replay.simulations.length !== search.simulations ||
      JSON.stringify(replay.nodes[0].actions) !== JSON.stringify(frame.actions) || replay.nodes[0].actor !== frame.key.actor) throw new Error('Search replay root mapping differs');
  const visits = replay.nodes.map(n => n.actions.map(() => 0));
  const sums = replay.nodes.map(n => n.actions.map(() => 0n));
  for (const [i,n] of replay.nodes.entries()) {
    if (n.id !== i || n.actions.some((a,j) => a.index !== j) ||
      [n.priors,n.children,n.edgeVisits,n.edgeValueSums].some(a => a.length !== n.actions.length) ||
      n.priors.reduce((s,v) => s+v,0) !== 1000000 || n.children.flat().some(c => c >= replay.nodes.length)) throw new Error('Broken search node or action reference');
  }
  let transitions = 0;
  for (const [i,s] of replay.simulations.entries()) {
    if (s.ordinal !== i || s.choices[0].node !== 0) throw new Error('Search simulation order is invalid');
    for (const [j,c] of s.choices.entries()) {
      const n = replay.nodes[c.node];
      if (!n || c.action >= n.actions.length || c.scores.length !== n.actions.length) throw new Error('Broken search choice reference');
      if (j > 0) { const p = s.choices[j-1]; if (!replay.nodes[p.node].children[p.action].includes(c.node)) throw new Error('Search path is not in the captured graph'); }
      visits[c.node][c.action]++; sums[c.node][c.action] += BigInt(s.value); transitions++;
    }
    for (const b of [...s.choices.map(c => c.board),s.leaf]) {
      if (b.players.some(p => p.hand.length)) throw new Error('Hypothetical search exposes a sampled hand');
      const objects = objectList(b), ids = new Set(objects.map(o => o.id));
      if (ids.size !== objects.length || objects.some(o => !recording.cards[o.card]) ||
        b.combat.some(c => !ids.has(c.attacker) || c.blockers.some(id => !ids.has(id)))) throw new Error('Broken hypothetical board reference');
    }
  }
  if (transitions !== search.transitions) throw new Error('Search replay transition count differs');
  for (const [i,n] of replay.nodes.entries()) {
    if (n.edgeVisits.some((v,j) => v !== visits[i][j]) || n.edgeValueSums.some((v,j) => BigInt(v) !== sums[i][j])) throw new Error('Replay differs from final search node statistics');
  }
  if (search.stats.some(s => s.visits !== visits[0][s.index] || BigInt(s.valueSum) !== sums[0][s.index])) throw new Error('Replay differs from its root digest table');
}

export async function verifySearchDigests(r: Recording): Promise<void> {
  for (const f of r.frames) {
    if (!f.search) continue;
    const stats = f.search.stats;
    const bytes = new ArrayBuffer(24 + stats.length * 24);
    const data = new DataView(bytes);
    data.setBigUint64(0, BigInt(stats.length), true);
    stats.forEach((s, i) => {
      const offset = 8 + i * 24;
      data.setUint32(offset, s.index, true); data.setUint32(offset + 4, s.visits, true);
      data.setBigInt64(offset + 8, BigInt(s.valueSum), true); data.setBigInt64(offset + 16, BigInt(s.meanValue), true);
    });
    const suffix = 8 + stats.length * 24;
    [f.executed, f.search.transitions, f.search.simulations, f.search.nodes].forEach((n, i) => data.setUint32(suffix + i * 4, n, true));
    const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))].map(n => n.toString(16).padStart(2, '0')).join('');
    if (digest !== f.search.digest) throw new Error('Search table does not match its recorded digest');
  }
}
