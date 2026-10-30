# London mulligans for the default Pauper game (v1)

Status: revision 3, after round 2 (Fable max: proceed with changes 1-3;
GPT-6 Astra ultra: change 1). Round 1: Fable 1-6, Astra 1-6, all resolved. Every piece is opt-in; nothing changes a frozen study,
benchmark or checkpoint.

## 1. Goal and laws

Default Pauper games deal seven to each seat and both keep: training's
legacy reset, the `keep_seven_v2` evaluation opening, Spellbench
pauper-kernel-v2 (`mulligan: none`) and the model seat of the human client.
A 0-1 or 6-7 land seven is played out and the result mostly reflects the
deal. `docs/reports/pauper_forced_keep_noise_v1.md` measures this.

This design adds London mulligans (CR 103.5) as an opt-in game version whose
keep/mulligan and bottom choices are ordinary learned decisions:

- terminal win/loss is the only reward; pregame decisions get the same
  terminal return as every other decision;
- no keep rule, land threshold, bottom heuristic or hand-set prior;
- no reseed or retry is ever triggered by a hand or a mulligan sequence;
  existing retry rules are unchanged;
- default off: absent fields keep every byte, digest, seed and checkpoint
  identity, so EX1, reverse-search B1, OH2 panels and pauper-kernel-v2 are
  untouched. Adoption is per study, at its next design gate.

## 2. Game rules

The Pauper option reuses the engine's two-player London module unchanged
(`mtg-kernel/src/london_mulligan_v1.rs`, design
`docs/design/fdn_london_mulligans_v1.md`, XMage `LondonMulligan` parity):

- seven each from the existing deal (environment v2 initial library shuffle,
  ordinal 0, alternating draws);
- each round, seats that have not kept announce in starting-player order;
  every announcement precedes any redraw;
- a mulligan returns the hand, shuffles, draws seven, then bottoms one card
  per mulligan taken, one card per decision, before the next round;
- at seven mulligans the zero-card keep is forced; no free mulligan;
- turn 1 begins only when both have kept.

## 3. The option, the reset and the completion boundary

Fields and refusals:

- `ExpandedEpisodeV1.opening: Option<PauperOpeningV1>`, serialized
  `"london-v1"`, `#[serde(default, skip_serializing_if = "Option::is_none")]`
  as `runtime_rules` does (`expanded_deck_training_v1.rs` 966-1022). The
  run-level config that builds episodes carries the same field.
- Per-seat mulligan mode `learned | keep_seven`, carried by the episode and
  recorded in the trajectory output.
  - `learned`: the seat's policy answers every London decision.
  - `keep_seven`: the session keeps for the seat at its first
    announcement. This is today's behavior, for declared frozen opponents
    whose pregame rows are untrained. It is never a learned seat's fallback.
- Refusals: `opening` with `reset: Some` (recorded prefixes predate London);
  `opening` with a V3 or V4 feature identity on any seat (section 6);
  `opening` with `opponent_kind: Legacy` or with `opponent_search`, since
  v1 has no London search path (section 7).

Reset and completion. The existing constructors advance inside reset, and
the engine completes London inside `drain_or_decide`, then continues straight
into turn 1 in the same advance (`london_mulligan_v1.rs` 152-156, 176-181;
`engine.rs` 12006-12016). The forced zero-card keep is set only there. So
the boundary is owned by the session:

1. A new session constructor builds the same deck-pair state as the V4
   training reset and calls `enable_london_mulligans_v1` before any advance.
2. While London is pending, the session's advance calls
   `london_mulligan_v1::drain_or_decide` itself, before the engine advance:
   - `Some(decision)` for a `learned` seat is exposed as a decision;
   - `Some(ChooseLondonMulligan)` for a `keep_seven` seat is answered with
     keep inside the advance, like automatic engine progression;
   - `None` with `is_complete()` runs completion (step 3).
3. Completion, before the first engine advance into turn 1: zero
   `draws_this_turn`, clear `engine.event_log` (the `keep_seven_v2` handoff,
   `human_opening_v1.rs` 283-290); write the opening record; set
   `state.london_mulligans_v1 = None`; then advance normally.
   - Private knowledge of bottomed cards is untouched: it lives in the
     library-knowledge tables, not in the London struct.
   - Identity property (tested, both starting seats): if both keep seven,
     the first in-game `GameState` is byte-identical to the
     `keep_seven_v2` opening for the same seed, decks and starting player.
     So are the canonical observation, the tensors and the state hash.
     Session counters (policy step, physical decision id, environment
     revision) and their hashes differ.
   - Every in-game state therefore has the shape of a keep-seven game.
   - The legacy training reset leaves the seven setup draws in turn-1
     draw counters; `keep_seven_v2` corrected that. London inherits the
     corrected counters. This is a second, pre-existing difference from
     the training reset and is recorded as such.
4. `engine.event_history` keeps the pregame zone changes, shuffles and
   draws (as Limited London does); `engine.event_log` is cleared. Scope of
   the enumeration: readers reachable from the nine registered decks' card
   programs in the Pauper build. Known so far: `eternal_cards_v1/e3.rs` has
   one reader; the `engine.rs` readers (about 3113, 16431, 18326, 19949,
   20762, 23052, 26350, 28145, 28882, 29652) slice from indices captured
   in-game; `trigger.rs` 6562 reads the cleared `event_log`. Outcome rule:
   if the enumeration shows no reader can count a pregame event as a turn-1
   event, nothing is bounded and the PR records the list. Otherwise
   implementation stops and this design is amended before code, because a
   boundary field on `GameState` would break the identity property above.

## 4. Accounting, determinism and RNG

- Exposed pregame decisions are ordinary physical decisions and policy
  steps (`rl_session.rs` 8696-8713) and count against
  `max_physical_decisions` and `max_policy_steps` (collector cap
  `expanded_deck_training_v1.rs` 2157-2160). Worst case is
  2 x (7 announcements + 28 bottoms) = 70. A London study declares its
  limits with that headroom at its design gate.
- Automatic keeps (`keep_seven` seats) and forced zero-card keeps consume no
  physical decision id, policy step, environment revision or RNG draw.
  Physical ids stay contiguous over exposed decisions only, so trajectory
  validation (`expanded_deck_training_v1.rs` 2751, 2824) is unchanged.
- One-row menus follow the session's existing single-legal-action rule,
  unchanged. An exposed decision, including a single remaining bottom
  choice, consumes the normal counters and its policy RNG draw.
- The session applies London answers through the engine's `apply_action`
  (`engine.rs` 23693) and then calls `drain_or_decide` itself.
- Any pregame decision other than `ChooseLondonMulligan` or
  `ChooseLondonBottom` fails closed. In the eternal-fixtures build,
  `drain_or_decide` can offer Serum Powder as `ChooseEffectOption`
  (`london_mulligan_v1.rs` 159-170); v1 refuses it instead of exposing it.
- The deal is unchanged. A mulligan shuffles through
  `GameState::shuffle_library`, consuming that owner's next environment v2
  in-game shuffle ordinal (`state.rs` 3406-3417), as human mulligans do.
  The other library is untouched; later shuffles by that owner use later
  ordinals.
- A learned seat's pregame decisions draw from its policy stream
  (`paired_policy_seeds_v1`) like any decision. A both-keep London game is
  state-identical to its keep-seven twin at turn 1 but not
  trajectory-identical.
- Snapshot and restore cover each pregame phase (announce; bottom with
  partial selections), including the session's per-seat modes and the
  pending opening record. Replay reproduces every pregame decision, shuffle
  and incarnation.
- Opening record, written into the trajectory and episode output: per seat
  mode, mulligans taken, kept hand size, exposed pregame decisions, and
  whether the zero-card keep was forced.

## 5. FlatV7 contract: observation and action menu

London decisions fail closed in the Pauper flat path today
(`rl_session.rs` 1694-1704, 3323, 3468). FlatV7 (Python `features_v10.py`,
`data/flat_policy_v7/`, new identity constants) forks FlatV4 and is
shape-equal (state 219, object 98, edge 41, action 195, action ref 25,
20 object groups). V4 checkpoints transfer by the explicit same-shape
transfer used from V3 to V4.

Columns reused. The first two are never nonzero in an admitted V3/V4
gameplay row; the third is reused only at pregame decisions:

- Action-kind column 15 (`choose_effect_number`). No decision builder
  constructs `ActionSemanticV1::ChooseEffectNumber` (only the enum,
  `rl.rs` 1234, and match arms); `rl_session.rs` 3586 is a further guard.
  Rust and Python test fixtures set it (`rl_session.rs` 17156,
  `features_v7.py` 3628); those are not gameplay rows. V7 renames the column
  `london_opening`, and the V7 Rust and Python encoders reject a numeric
  choice instead of aliasing it.
- Engine-stage slot 9 (`halted`). A halted engine returns `Halted`, which
  ends the session, so no scored decision has this slot set. In V7 it is
  `london_pregame`: 1 at pregame decisions, 0 elsewhere.
- The two per-player `has_lost` columns, pregame only. They are not dead in
  gameplay: a draw continuation can run state-based actions, set
  `has_lost`, and expose priority before the next terminal check (Highway
  Robbery in Burn: `engine.rs` 13558, 12188, 12391; `trigger.rs` 5109;
  the session does not check `has_lost`, `rl_session.rs` 9733). Their
  weights may therefore be trained, so they carry a prior that pregame
  learning must overcome; that is acceptable and stated, not hidden. At no
  pregame decision can `has_lost` be set (`enable_london_mulligans_v1`
  refuses a lost seat, `london_mulligan_v1.rs` 89). In V7, at pregame
  decisions only, the self column carries "opponent announced mulligan this
  round" and the opponent column carries "opponent has kept". In-game rows
  encode `has_lost` exactly as V4 does. That engine behavior (priority after
  a loss) is a separate defect, reported and left unchanged here because
  fixing it would change frozen studies.

State at pregame decisions (visible to both heads):

- `london_pregame` flag (engine-stage slot 9).
- Each seat's mulligans taken are natural state: London redraws are draws,
  so `draws_this_turn` = 7 x (1 + mulligans) for each seat, and hand counts
  follow. Cards left to bottom follow from those two. Turn-1 untap has no
  other Pauper decision.
- The opponent's announcement status uses the two `has_lost` columns above.
  CR 103.5 makes it public, and the second announcer needs it.
- The canonical observation gains a `london_pregame` extension (phase,
  public counts, kept flags, this-round announcements) only while London is
  pending. It changes the state hash of pregame decisions only. Coverage
  tests count explicit columns, never digest differences.
- Hidden information is unchanged: own hand objects are visible, the
  opponent's hand is a count, and bottomed cards are known library cards to
  their owner only.

Encoder paths V7 forks (each gets the pregame encoding, and a pregame-row
equality test runs across all of them and Python): the production state
encoders `native_flat_tensorizer_v2.rs` about 3943 and 4027 (the
`cfg(test)` encoder near 2010-2114 follows them), `streaming_v3.rs` about
202 (`has_lost` by field name), `streaming_v3/context_v3.rs` about 165
(`current_stage`), the action-row encoder
`explicit_action_features_contract_v3` (about 7880), the flat session
admission points `rl_session.rs` 1694-1704, 3323 and 3468, and Python
`features_v10.py`. Replay rescoring stores tensors
(`expanded_deck_training_v1.rs` 2934-2937), so a path mismatch would not be
caught there; the cross-path test is what catches it.

Action rows:

- `ChooseLondonMulligan` gives keep and mulligan rows; `ChooseLondonBottom`
  gives one row per hand card (existing menus, `rl.rs` 7284-7316). All use
  kind column 15.
- Keep/mulligan rows: flag column 69 (`VALUE`) = 1 for mulligan; column 63
  = own mulligans taken; column 66 = opponent mulligans taken; column 52 =
  opponent has kept; column 53 = opponent announced mulligan this round. No
  card reference.
- Bottom rows: role `Card` reference to the exact hand object (card
  features 30-42 and an action-reference row to the hand object); column 49
  = cards still to bottom; column 63 = own mulligans taken.
- The action hash covers the canonical London semantic, as for any action.

Function preservation (tested):

- In-game decisions of a London game give V7 tensors byte-identical to V4
  tensors for the same state, because the state is canonicalized at
  completion and pregame-only encodings never fire in-game.
- A V4 checkpoint transferred to V7 scores every in-game decision exactly
  as before; only pregame rows are new.
- Counterfactual test: the same deal with the first announcer keeping
  versus mulliganing gives different explicit state columns at the second
  announcer, with the state digest disabled.

## 6. Policy network, warm start and training

- The network is a per-action scorer (`native_policy_value_net_v1.rs`
  1005-1019): each row is embedded, joined to the state embedding and scored
  to one logit. The value head reads the state embedding, which now
  includes the pregame facts. No new outputs or dimensions.
- Warm start: an explicit V4-to-V7 transfer of a checkpoint (for example
  OH2 r2-s1 b512), recorded like the V3-to-V4 transfer.
- Kind column 15 and stage slot 9 never received gradient from gameplay
  rows in the source's training (decayed under AdamW, not trained); the
  `has_lost` columns may be trained (section 5). Initial pregame behavior is
  whatever the transferred weights give; no prior is set by hand. A study
  pre-registers a deal set (seeds, count) in its design and reports on it,
  before launch, the warm-start mulligan rate, kept-size distribution,
  forced-zero rate and pregame share of rows.
- Pregame decisions are ordinary learner rows: same loss and group return
  (`expanded_deck_training_v1.rs` 2945).
- Generation consistency: collection requires one feature generation for
  both seats (`expanded_deck_training_v1.rs` 2080), and replay scores both
  seats under it (2920). So every seat in a London episode is V7. Frozen
  opponents (pool, panel, benchmark legacy policies) enter as
  function-preserving V4-to-V7 copies with `keep_seven` mode. Their
  in-game play is unchanged.
- The run refuses `london-v1` with a V3 or V4 identity. A V7 identity on
  keep-seven episodes is legal and plays in-game exactly as V4.

## 7. Search: staged

- v1 (this PR) has no London search path. The search state encodes its
  root through V4 scoring and action encoders (`flat_action_v4/search_state.rs`
  377), which keep rejecting London. Episodes with `opening` refuse
  `opponent_search`, and s4a/B1 bank and plan specs refuse `opening`.
- v2 (next PR, before any search study adopts London): V7 search state
  (observation, action binding, scorer dispatch, rollout handoff) for
  pregame and in-game roots of London games, opening identity in bank and
  replay records, and a sampler hook. The V4 sampler permutes unknown hand
  objects (`sampler.rs` 541-549, 696) while London binds exact hand
  incarnations (`london_mulligan_v1.rs` 126-141), so after a resample the
  pending London hand bindings are rebound to the current hands. The
  conflict scan must not treat those bindings as historical references.
  Without London the hook is a no-op.
  Known approximation: an opponent's privately bottomed cards are resampled
  as unknown library cards anywhere in the library. Actor-known positions
  are preserved.

## 8. Versioning into consumers

| Consumer | Version and adoption |
|---|---|
| Kernel | `opening: "london-v1"`; FlatV7 identity. Absent option and V4 identity unchanged. |
| Training | Run option `opening` with V7 transfer; a new study or stage at its design gate, never mid-study. |
| Search (s4a, B1) | After v2 search; bank and plan specs carry `opening`; adopt at a design gate. |
| Eval panels (Bo3) | New `Bo3OpeningProtocolV1::LondonV1`; `keep_seven_v2` stays the frozen panel protocol. |
| Spellbench | New benchmark pauper-kernel-v3 (`mulligan: ["london"]`) with its adapter; pauper-kernel-v2 never changes. |
| Human client | Model seat uses learned mulligans when its checkpoint has the V7 identity, via engine London in standard announcement order; otherwise keep-seven as now. `human_opening_v1` is a separate London implementation, so the follow-up replaces it for V7 games rather than wrapping it. |

## 9. Build scope (this PR)

1. Session constructor, per-seat modes, session-owned completion boundary,
   opening record in trajectory and episode output, refusals.
2. FlatV7 contract (Python generator, contract JSON, identity), V7 flat
   encoding of pregame state and rows, tensorizer V7, V4-to-V7 transfer
   admission, run-level `opening` with identity checks.
3. Tests:
   - both keep, both starting seats: first in-game `GameState` equals
     `keep_seven_v2`;
   - announcement order, simultaneous redraw, bottom counts, forced
     zero-card keep and its completion;
   - a mulligan shuffles only its owner's ordinal;
   - snapshot/restore and replay at each pregame phase;
   - stale and invalid actions rejected without mutation;
   - V7 menus and rows for both seats; opponent hand never exposed;
   - in-game V7 tensors and logits equal V4 over a seeded game with
     mulligans, plus a loss-bearing row with `has_lost` set (Highway
     Robbery case);
   - pregame-row equality across every forked Rust encoder path and Python;
   - counterfactual announcement test (explicit columns, digest off);
   - V4 identity still refuses London; V7 refuses numeric choice;
   - `keep_seven` seat: no step, id, revision or RNG draw consumed;
   - decision caps count pregame rows; contiguous physical ids;
   - event-history enumeration recorded (section 3, step 4);
   - refusals: `opening` with `reset: Some`, with `opponent_kind: Legacy`,
     with `opponent_search`; s4a and B1 bank and plan specs with `opening`;
     a non-London pregame decision (Serum Powder) fails closed;
   - absent option: existing goldens and digests unchanged.

Follow-ups, each its own PR: v2 search (section 7), Bo3 `LondonV1`,
Spellbench pauper-kernel-v3 adapter, human-client model-seat mulligans.
