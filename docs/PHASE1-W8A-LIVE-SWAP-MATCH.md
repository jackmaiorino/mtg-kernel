# W8a: live-swap BO3 match path and self-play skeleton

Status: engineering only. This implements the ratified deck-model build order's W8a
(`docs/superpowers/specs/2026-09-09-deck-model-sideboarding-and-brewing-design.md`
section 8, ruling 9, RULED 2026-09-10: "approved as the engineering prerequisite").
Ruling 10 ("W8 ships imitation-only until a later ruling") is still open. Nothing in
this change trains a model, writes a checkpoint, or dispatches anything: it is the
connective path ruling 9 approved, built ahead of ruling 10 per the standing
efficiency delegation and `BO3-LEARNING-DESIGN-001.md` revision 2's owner question 2
("the lead is treating Ruling 9's 'approved as the engineering prerequisite' as a
yes and will proceed on that reading absent objection").

Before this change, `bo3_session::prepare_game_v1` hardcoded post-board sideboarding
to `sideboard_policy.apply_v1` for both seats with no override point, and no
reusable, tested, versioned path connected a live per-seat swap policy to a real BO3
match for RL-refinement purposes (spec section 6). `prepare_game_with_configurations_v1`
already existed and `learned_bo3_v1.rs`/`phase1_bo3_collection_v1` already drove it
by hand with deterministic-argmax sideboard heads (`score_v1`), but that pattern was
ad hoc, duplicated across two files, and had no sampled (non-argmax) entry a
policy-gradient objective could use. W8a formalizes and versions that pattern.

## 1. The live-swap match path (`src/bo3_session.rs`)

`BestOfThreeDeckMatchV1::prepare_game_with_live_policies_v1` is a `prepare_game_v1`
sibling that takes an optional live policy per seat:

```rust
pub fn prepare_game_with_live_policies_v1(
    &mut self,
    chooser: PlayerId,
    choice: PlayDrawChoiceV1,
    live: [Option<LiveSideboardConsultationV1<'_>>; 2],
) -> Result<LiveGamePreparationV1, Bo3SessionErrorV1>
```

A seat left `None` keeps this match's own installed static/Keep policy
(`DeterministicSideboardPolicyV1::apply_v1`, byte-for-byte unchanged); a `Some` seat
is consulted through a new trait instead:

```rust
pub trait LiveSideboardSwapPolicyV1 {
    fn select_live_v1(
        &mut self,
        evidence: &LearnedSideboardInputV1,
        current: &DeckConfigurationV1,
    ) -> Result<(DeckConfigurationV1, Vec<SideboardActionV1>), String>;
}
```

Game one is never sideboarded, live or static, matching every other constructor in
this file. Whenever at least one seat is live, both seats' resolved configurations
(live or static) are validated and committed through the existing
`prepare_game_with_configurations_v1`, never a separate ad hoc path, so the live path
inherits that method's registered-multiset and game-one-immutability checks for free.

**Unchanged-path byte-identity.** When neither seat is live for a given call, and the
match has an installed static policy, `prepare_game_with_live_policies_v1` delegates
verbatim to `prepare_game_v1` (the literal same function call), so its output,
including per-seat `AppliedSideboardReceiptV1`, matches `prepare_game_v1` exactly.
`bo3_session::tests::unchanged_static_path_is_byte_identical_to_prepare_game_v1_across_games`
proves this across a decided game one, two, and three by running both methods on
independently constructed but identical matches and asserting `PartialEq` on the
result. A `new_live_v1` match (no installed static policy) has no `prepare_game_v1`
to be identical to; `bo3_session::tests::live_only_match_advances_game_one_without_any_live_policy`
covers that case separately, including the `LiveConfigurationsRequired` error when
game two arrives with neither a live nor a static source.

## 2. Evidence boundary

A live policy can never see hidden information; this is enforced by the type of the
input it receives, not by a runtime filter. `select_live_v1`'s `evidence` parameter
is `&LearnedSideboardInputV1` (`learned_sideboard_v1.rs`), the same restricted type
the deck model (`learned_sideboard_v1::score_v1`/`sample_v1`) and the existing
evaluation harness (`learned_bo3_v1::VisibleSideboardPolicyV1`) already use. That
type carries only: the policy's own registered 75, its own completed-game card
outcomes, opponent evidence reduced to `(card_id, game_index, first_seen_turn, zone)`
per public card identity (never a hidden object id or a true opponent deck id),
resource summaries, and the match score. There is no field for a `GameSummaryV1`
(which carries both seats' private outcomes), the opponent's `RegisteredDeckV1`, or
any hidden zone contents. Building this restricted input from a real match's
completed-game history is the caller's job, using the existing
`learned_bo3_v1::project_sideboard_input_v1` projector (or an equivalent), exactly as
`learned_bo3_v1::run_learned_bo3_session_v1` and `phase1_bo3_collection_v1::play_game`
already do; `prepare_game_with_live_policies_v1` never constructs this input itself
and never adds anything to what the caller supplies.
`bo3_session::tests::live_policy_receives_exactly_the_supplied_restricted_evidence`
confirms the pass-through is faithful: a policy spy receives exactly the
`LearnedSideboardInputV1` value the caller passed in, unchanged.

## 3. The sampled entry (`src/learned_sideboard_v1.rs`)

`LearnedSideboardModelV1::score_v1` already returns per-action logits and an argmax
selection. `sample_v1` is its new sampled sibling, sharing the same context/logit
computation but selecting via `fast_sampler::WideCategoricalScratchV1`'s exact
Hamilton apportionment and first SplitMix64 draw (`SIDEBOARD_LIVE_SAMPLER_VERSION_V1
= "kernel-learned-sideboard-sample-f32-q8-expq63-hamilton-splitmix64/v1"`), keyed by
a caller-supplied `seed: u64`. It records the actual Hamilton mass of the selected
action divided by `2**64` as `sampled_probability`, the real behavior probability a
policy-gradient `log p_selected` term needs, not a plain softmax value.
`deliberate_sampled_v1` is `deliberate_v1`'s sampled sibling: it runs the same
swap-then-`Done` deliberation loop, calling `sample_v1` once per decision with a
seed from a caller-supplied `seed_for_step: impl FnMut(u32) -> u64` closure, and
returns every step's `SampledSideboardDecisionV1`.

This method owns no RNG state; seeding is entirely the caller's responsibility. The
self-play driver (below) derives it as a domain-separated hash of the match seed,
physical game index, seat, and decision ordinal, so any decision's seed is
reproducible from those four values alone with no side stream to replay.

## 4. Self-play driver skeleton (`src/phase1_w8a_live_swap_self_play_v1.rs`)

`run_w8a_self_play_matches_v1` plays `config.match_count` matches between a learner
seat (live, sampled) and a fixed opponent (the match's own installed static/Keep
policy), through `prepare_game_with_live_policies_v1`. This is deliberately a
*skeleton*, not a production driver: playing one physical game is a caller-supplied
`W8aPhysicalGamePlayerV1`,

```rust
pub trait W8aPhysicalGamePlayerV1 {
    fn play_physical_game_v1(
        &mut self,
        start: GameStartV1,
        mainboards: [Vec<u16>; 2],
        environment_seed: u64,
    ) -> Result<GameSummaryV1, String>;
}
```

not a fixed binding to `FastActorSessionV1`/a production checkpoint. A real caller
would implement this trait over `learned_bo3_v1`'s existing episode construction and
`game_summary_v1::try_run_fast_episode_with_summary_v1`, exactly as
`learned_bo3_v1::run_learned_bo3_session_v1` already does for evaluation; this module
does not do that binding itself, so it owns no weights and cannot accidentally run a
real game on its own. The function, and everything it calls, is a plain library
call: there is no CLI or binary entry point, and no call anywhere in this module or
`bo3_session.rs`/`learned_sideboard_v1.rs`'s new code reaches a training step or an
optimizer. Ruling 10 stays exactly as open as before this change.

Both the per-match seed (`w8a_match_seed_v1`) and each sideboard decision's sampler
seed (`w8a_sideboard_sample_seed_v1`) are deterministic SHA-256 domain separations
(the same style as `sideboard_search_campaign_v1::candidate_seed_v1`), reproducible
from their inputs alone. `run_w8a_self_play_matches_v1` run twice with the same
config, decks, model, and scripted game outcomes produces byte-identical receipts;
`phase1_w8a_live_swap_self_play_v1::tests::self_play_receipts_record_sampled_probabilities_and_terminal_outcome`
proves this.

### Receipt schema

```rust
pub struct W8aSelfPlayMatchReceiptV1 {
    pub schema: String,               // "kernel-w8a-live-sideboard-self-play-receipt/v1"
    pub match_ordinal: u32,
    pub match_seed: u64,
    pub learner_seat: PlayerSeatV1,
    pub deck_ids: [String; 2],
    pub sampler_version: &'static str,
    pub game_starts: Vec<GameStartV1>,
    pub sideboard_decisions: Vec<Bo3DecisionRecordV1>,
    pub outcome: MatchOutcomeV1,       // the sole RL reward (ruling 11: terminal
                                       // match win/loss, never a batched delta)
}
```

Each `sideboard_decisions` entry reuses `phase1_agent_v1`'s exact
`Bo3DecisionRecordV1`/`ActorVisibleDecisionV1::Sideboard`/`BehaviorDistributionV1`
vocabulary: `visible` is `ActorVisibleDecisionV1::Sideboard { input, ordered_actions }`
(the same shape `phase1_bo3_collection_v1::play_game` already produces for a greedy
sideboard head), and `behavior` is `BehaviorDistributionV1::HamiltonQ64`, built by
`BehaviorDistributionV1::hamilton_from_logits_v1` from the sampled decision's own
logits and selected index, the identical construction `phase1_agent_v1::trajectory`
already uses for gameplay decisions. This is deliberate: a later
`phase1_bo3_learning_v1` capture extension for sideboard RL (`BO3-LEARNING-DESIGN-001.md`
revision 2, "what remains" items 3-6) can fold this receipt in without inventing a
translation layer, only by extending capture to also emit `Bo3DecisionRecordV1`s in
this shape. This receipt is not itself wired into `phase1_bo3_collection_v1` or
`phase1_bo3_learning_v1`'s capture: nothing reads it back into a trainer.

**Scope limit.** `sideboard_decisions` is sideboard-only. `phase1_bo3_collection_v1`'s
existing "gameplay updates" block already captures mulligan/play-draw/gameplay
decisions separately (`BO3-LEARNING-DESIGN-001.md` block 1); `decision_index` here is
therefore contiguous only across this receipt's own sideboard records, not across
the whole match's decisions the way `Bo3DecisionRecordV1`'s own doc comment
describes for a full trajectory capture. A receipt that needed both would need to
merge this module's sideboard records with a separate gameplay capture by
`(match, game, physical_decision_id)`, which this skeleton does not attempt.

## 5. What this does not do

- No training step, gradient, optimizer, or checkpoint write anywhere in this
  change. `sample_v1`/`deliberate_sampled_v1`/`prepare_game_with_live_policies_v1`/
  `run_w8a_self_play_matches_v1` are all pure library functions over caller-supplied
  state.
- No CLI or binary dispatches any of this code. `run_w8a_self_play_matches_v1` is
  reachable only by another Rust caller that explicitly imports and calls it.
- No native training binary, WSL, Docker, or cloud command was run to build or test
  this change; only `cargo test`/`cargo build` inside this worktree.
- Ruling 10 (whether and when to spend a training cycle on RL refinement) is
  untouched by this change and stays open. Building W8a does not itself authorize
  anything past it; per the Gate in `BO3-LEARNING-DESIGN-001.md` revision 2, no
  gradient step through the sideboard head's parameters, sampled or otherwise, runs
  before Ruling 10 is lifted for that specific slice.
- The self-play driver's `W8aPhysicalGamePlayerV1` is unimplemented against the real
  engine in this change; only a deterministic, scripted test double exists (used in
  this module's own tests). Wiring a real implementation (over `FastActorSessionV1`
  and a real checkpoint) is future, separately reviewed work, not part of W8a.
- Draw-extended matches beyond game three are exercised (tests below), but the
  self-play driver's own test script only runs three physical games; a longer
  self-play campaign's cost/throughput is out of scope here, matching W8a's
  "engineering prerequisite," not "campaign," framing.

## 6. Tests

All new/changed tests run under
`cargo test --offline --locked --jobs 4 -p mtg-kernel --lib <filter> -- --test-threads=1`
from the workspace root (`E:/mtg-kernel-phase1-w8a`) with
`CARGO_TARGET_DIR=E:/cargo-target-phase1-w8a`.

- `learned_sideboard_v1::tests::sample_v1_is_deterministic_and_matches_fast_sampler_exactly`:
  same seed reselects the same action/probability; cross-checks `sample_v1`'s output
  directly against an independent `WideCategoricalScratchV1` call over the same
  logits, proving it is a pass-through, not a reimplementation.
- `learned_sideboard_v1::tests::sampled_deliberation_finishes_and_conserves_the_registered_75`:
  `deliberate_sampled_v1` terminates within the existing 31-decision practical bound,
  conserves the registered 75, and reproduces its exact trace given the same
  per-step seed closure.
- `bo3_session::tests::unchanged_static_path_is_byte_identical_to_prepare_game_v1_across_games`,
  `..._live_only_match_advances_game_one_without_any_live_policy`: requirement (1).
- `bo3_session::tests::one_live_seat_overrides_only_its_own_configuration`,
  `..._live_policy_receives_exactly_the_supplied_restricted_evidence`: requirement (2).
- `bo3_session::tests::starting_player_matches_play_draw_choice_across_games`: Play
  keeps the chooser on the play, Draw hands it to the opponent, through the live
  path, matching `bo3_match.rs`'s own rule.
- `bo3_session::tests::live_policy_is_consulted_through_a_draw_extended_match`: the
  learner is consulted at every physical game from two through five (four draws),
  and the match completes only on two real wins, never on a game-count ceiling.
- `bo3_session::tests::failing_live_policy_leaves_match_state_unchanged`,
  `..._live_policy_cannot_change_the_registered_multiset`: transactional guarantees
  and existing conservation checks still hold on the live path.
- `phase1_w8a_live_swap_self_play_v1::tests::seed_derivations_are_pure_and_domain_separated`:
  both seed functions are pure and change with every disambiguating input.
- `phase1_w8a_live_swap_self_play_v1::tests::self_play_receipts_record_sampled_probabilities_and_terminal_outcome`:
  end-to-end driver run (two matches, a draw-extended one included) with a scripted
  game player, asserting the receipt schema, recorded probabilities, terminal
  `MatchOutcomeV1`, and full determinism across a repeated run.
- `phase1_w8a_live_swap_self_play_v1::tests::different_base_seeds_can_change_the_sampled_trace`:
  distinct base seeds draw from distinct domain-separated streams.
