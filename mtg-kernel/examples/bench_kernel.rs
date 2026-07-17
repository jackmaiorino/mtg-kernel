//! PERFORMANCE-ONLY kernel benchmark suite (external review Sol #93 scope:
//! measure raw engine speed, do not claim correctness/training/search value
//! -- that's a separate, already-tested concern covered by the ~150 green
//! tests elsewhere in this crate). Every number this binary prints is a raw
//! wall-clock measurement of the mtg-kernel engine alone: no NN inference,
//! no Python, no training loop.
//!
//! Sections (see the design brief this was commissioned against):
//!   1. snapshot/restore cost at ~80/200/350/500-object states
//!   2. step throughput: (a) the deterministic Burn goldfish script,
//!      (b) full Burn-mirror games via a seeded random-legal-action policy,
//!      both driven through the raw `engine::advance_until_decision`/`step`
//!      API (every priority window counted, none suppressed)
//!   3. self-play throughput: 1/4/8/16 threads, each running seeded
//!      random-policy Burn-mirror games through `HarnessSurfaceV2` (the
//!      H-visible decision surface a training loop would actually consume)
//!   4. legal-actions enumeration cost on a captured mid-game cast window
//!   5. allocator profile (only with `--features count_allocs`)
//!
//! Run: cargo run --release --example bench_kernel
//! Alloc profile: cargo run --release --example bench_kernel --features count_allocs

use mtg_kernel::card_def::{card_id_by_name, CARD_DEFS};
use mtg_kernel::engine::{self, Action, Decision, OptionalCostChoice};
use mtg_kernel::event::{self, ProposedEvent};
use mtg_kernel::ids::{ObjectId, PlayerId};
use mtg_kernel::runtime_decks::{runtime_deck_by_id, RuntimeDeckDefinition};
use mtg_kernel::state::{Counters, GameObject, GameState, SplitMix64, Target, Zone};
use mtg_kernel::surface_v2::{HarnessSurfaceV2, SurfaceAction, SurfaceDecision};

use std::collections::HashSet;
use std::sync::{Arc, Barrier, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

// ------------------------------------------------------------- tuning knobs

const SNAPSHOT_WARMUP_ITERS: u64 = 1_000;
const SNAPSHOT_TIMED_ITERS: u64 = 20_000;
const GOLDFISH_DURATION: Duration = Duration::from_millis(1_500);
const RANDOM_GAME_DURATION: Duration = Duration::from_millis(2_500);
const SELFPLAY_DURATION_PER_CONFIG: Duration = Duration::from_millis(4_000);
const SELFPLAY_THREAD_COUNTS: [usize; 4] = [1, 4, 8, 16];
const LEGAL_ACTIONS_TIMED_ITERS: u64 = 50_000;
#[cfg(feature = "count_allocs")]
const ALLOC_PROFILE_GAMES: u64 = 20;
/// Safety valve only: a bug in the random policy (or a real engine stall)
/// aborts the game instead of hanging the whole suite. Never expected to
/// fire -- typical games run a few hundred to a few thousand decisions.
const DECISION_SAFETY_CAP: u64 = 200_000;

const ATTACK_INCLUDE_CHANCE: (u64, u64) = (1, 2);
const BLOCK_CHANCE: (u64, u64) = (35, 100);

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--ceiling-json-v1") {
        match CeilingConfigV1::parse(&args[1..]) {
            Ok(config) => run_ceiling_json_v1(config),
            Err(message) => {
                eprintln!("{message}");
                eprintln!(
                    "usage: bench_kernel --ceiling-json-v1 --git-commit HEX40 [--deck Burn|Rally] [--actors 1,4,8,16] [--warmup-ms N] [--measure-ms N] [--seed N]"
                );
                std::process::exit(2);
            }
        }
        return;
    }
    if !args.is_empty() {
        eprintln!("usage: bench_kernel [--ceiling-json-v1 ...]");
        std::process::exit(2);
    }
    println!("=== mtg-kernel PERFORMANCE-ONLY benchmark suite (Sol #93 scope) ===");
    println!("Every number below is engine-only wall-clock time: no NN inference, no training.\n");

    section1_snapshot_scaling();
    section2_step_throughput();
    section3_selfplay_threading();
    section4_legal_actions_cost();
    section5_alloc_profile();
}

// ------------------------------------------------------ shared: burn deck

/// Mono-Red Burn mainboard, exactly as played in the golden-corpus mirror
/// (`Mage.Server.Plugins/.../decks/Pauper/Deck - Mono-Red Burn.dek`,
/// `Sideboard="false"` entries only). All 12 of these have a real, fully
/// implemented `spell_effect`/`mana_ability` program in this kernel (see
/// `card_def.rs`'s module doc), so a full random-legal-action self-play
/// game never touches an unimplemented mechanic.
const BURN_MAINBOARD: &[(&str, u32)] = &[
    ("Sneaky Snacker", 4),
    ("Faithless Looting", 2),
    ("Highway Robbery", 4),
    ("Masked Meower", 4),
    ("Lightning Bolt", 4),
    ("Mountain", 18),
    ("Grab the Prize", 4),
    ("Fireblast", 4),
    ("Guttersnipe", 4),
    ("Fiery Temper", 4),
    ("Voldaren Epicure", 4),
    ("Lava Dart", 4),
];

fn burn_deck_ids() -> Vec<u16> {
    let mut ids = Vec::with_capacity(60);
    for &(name, qty) in BURN_MAINBOARD {
        let id = card_id_by_name(name).unwrap_or_else(|| panic!("{name} missing from CARD_DEFS"));
        for _ in 0..qty {
            ids.push(id);
        }
    }
    assert_eq!(
        ids.len(),
        60,
        "Mono-Red Burn mainboard should be exactly 60 cards"
    );
    ids
}

fn shuffled(ids: &[u16], rng: &mut SplitMix64) -> Vec<u16> {
    let mut v = ids.to_vec();
    for i in (1..v.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        v.swap(i, j);
    }
    v
}

fn debug_name(card_id: u16) -> String {
    CARD_DEFS[card_id as usize].name.to_string()
}

/// A fresh Burn-mirror game: both players shuffle the same 60-card Burn
/// list independently (seeded), then draw their opening 7.
fn build_mirror_state(seed: u64) -> GameState {
    let deck = burn_deck_ids();
    build_mirror_state_from_ids(&deck, seed)
}

fn build_mirror_state_from_ids(deck: &[u16], seed: u64) -> GameState {
    let mut shuffle_rng = SplitMix64::seed(seed);
    let lib0 = shuffled(deck, &mut shuffle_rng);
    let lib1 = shuffled(deck, &mut shuffle_rng);
    let mut state = GameState::new_from_libraries(&lib0, &lib1, debug_name, seed);
    for _ in 0..7 {
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P0));
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P1));
    }
    state
}

// -------------------------------------------------- shared: random policy

fn rng_below(rng: &mut SplitMix64, n: usize) -> usize {
    if n == 0 {
        0
    } else {
        (rng.next_u64() % n as u64) as usize
    }
}

fn rng_chance(rng: &mut SplitMix64, num: u64, den: u64) -> bool {
    rng.next_u64() % den < num
}

/// Uniformly random pick among every legal action `decision` currently
/// offers (Pass included as one of the candidates for `CastSpellOrPass`).
/// Never called on `Decision::GameOver` (the driver loops break before
/// reaching here for that variant).
fn random_action_for_decision(
    decision: &Decision,
    state: &GameState,
    rng: &mut SplitMix64,
) -> Action {
    match decision {
        Decision::CastSpellOrPass {
            castable_spells,
            mana_abilities,
            land_drops,
            activatable_abilities,
            plot_actions,
            ..
        } => {
            let mut candidates: Vec<Action> = Vec::with_capacity(
                castable_spells.len()
                    + mana_abilities.len()
                    + land_drops.len()
                    + activatable_abilities.len()
                    + plot_actions.len()
                    + 1,
            );
            candidates.extend(castable_spells.iter().map(|&id| Action::CastSpell(id)));
            candidates.extend(
                mana_abilities
                    .iter()
                    .map(|&id| Action::ActivateManaAbility(id)),
            );
            candidates.extend(land_drops.iter().map(|&id| Action::PlayLand(id)));
            candidates.extend(
                activatable_abilities
                    .iter()
                    .map(|&(id, idx)| Action::ActivateAbility(id, idx)),
            );
            candidates.extend(plot_actions.iter().map(|&id| Action::PlotSpell(id)));
            candidates.push(Action::Pass);
            let i = rng_below(rng, candidates.len());
            candidates.swap_remove(i)
        }
        Decision::ChooseTargets { legal_targets, .. } => {
            assert!(
                !legal_targets.is_empty(),
                "a real ChooseTargets window must have at least one legal target"
            );
            Action::ChooseTarget(legal_targets[rng_below(rng, legal_targets.len())])
        }
        Decision::ChooseCostTargets { candidates, .. } => {
            Action::ChooseCostTarget(candidates[rng_below(rng, candidates.len())])
        }
        Decision::ChooseCastMode { options, .. } => {
            Action::ChooseCastMode(options[rng_below(rng, options.len())])
        }
        Decision::ChooseKicker { .. } => Action::ChooseKicker(rng_chance(rng, 1, 2)),
        Decision::ChooseSpellMode { mode_count, .. } => {
            Action::ChooseSpellMode(rng_below(rng, *mode_count as usize) as u8)
        }
        Decision::ChooseEffectOption { option_count, .. } => {
            Action::ChooseEffectOption(rng_below(rng, *option_count as usize) as u16)
        }
        Decision::ChooseEffectBoolean { .. } => Action::ChooseEffectBoolean(rng_chance(rng, 1, 2)),
        Decision::ChooseEffectTargets {
            legal_targets,
            can_finish,
            ..
        } => {
            let choice_count = legal_targets.len() + usize::from(*can_finish);
            let choice = rng_below(rng, choice_count);
            if choice < legal_targets.len() {
                Action::ChooseEffectTarget(legal_targets[choice])
            } else {
                Action::FinishEffectSelection
            }
        }
        Decision::ChooseOptionalCost { .. } => {
            // Real payable flags, not this decision's own -- the H2 surface
            // reshape re-presents `ChooseOptionalCost` with a presentation-
            // only sentinel at its `Use` stage (see `HarnessSurfaceV2::
            // pending_optional_cost_payable`'s doc); reading `state.engine.
            // pending_optional_cost` directly is accurate for the raw-engine
            // path too (`play_one_game_raw`/`hunt_max_legal_actions`, which
            // never goes through the reshape at all).
            let (discard_payable, sacrifice_payable) =
                HarnessSurfaceV2::pending_optional_cost_payable(state).unwrap_or((false, false));
            let mut options = vec![OptionalCostChoice::Decline];
            if discard_payable {
                options.push(OptionalCostChoice::Discard);
            }
            if sacrifice_payable {
                options.push(OptionalCostChoice::SacrificeLand);
            }
            Action::ChooseOptionalCost(options[rng_below(rng, options.len())])
        }
        Decision::ChooseSpellCopyPayment { .. } => {
            Action::ChooseSpellCopyPayment(rng_chance(rng, 1, 2))
        }
        Decision::ChooseSpellCopyRetarget { .. } => {
            Action::ChooseSpellCopyRetarget(rng_chance(rng, 1, 2))
        }
        Decision::ChooseMadnessCast { .. } => Action::ChooseMadnessCast(rng_chance(rng, 1, 2)),
        Decision::Discard { count, choices, .. } => {
            let mut pool = choices.clone();
            let mut picked = Vec::new();
            for _ in 0..*count {
                if pool.is_empty() {
                    break;
                }
                picked.push(pool.swap_remove(rng_below(rng, pool.len())));
            }
            Action::Discard(picked)
        }
        Decision::DeclareAttackers { eligible, .. } => {
            let (num, den) = ATTACK_INCLUDE_CHANCE;
            Action::DeclareAttackers(
                eligible
                    .iter()
                    .copied()
                    .filter(|_| rng_chance(rng, num, den))
                    .collect(),
            )
        }
        Decision::DeclareBlockers { legal_blockers, .. } => {
            let mut used: HashSet<ObjectId> = HashSet::new();
            let mut pairs = Vec::new();
            let (num, den) = BLOCK_CHANCE;
            for (attacker, blockers) in legal_blockers {
                if !rng_chance(rng, num, den) {
                    continue;
                }
                let avail: Vec<ObjectId> = blockers
                    .iter()
                    .copied()
                    .filter(|b| !used.contains(b))
                    .collect();
                if !avail.is_empty() {
                    let b = avail[rng_below(rng, avail.len())];
                    used.insert(b);
                    pairs.push((b, *attacker));
                }
            }
            Action::DeclareBlockers(pairs)
        }
        Decision::OrderTriggers { pending, .. } => {
            let mut idx: Vec<usize> = (0..pending.len()).collect();
            for i in (1..idx.len()).rev() {
                let j = rng_below(rng, i + 1);
                idx.swap(i, j);
            }
            Action::OrderTriggers(idx)
        }
        Decision::GameOver { .. } => {
            unreachable!("caller must check for GameOver before requesting an action")
        }
        Decision::Halted { .. } => {
            unreachable!("caller must check for Halted before requesting an action")
        }
    }
}

fn warn_safety_cap_once() {
    use std::sync::atomic::{AtomicBool, Ordering};
    static WARNED: AtomicBool = AtomicBool::new(false);
    if !WARNED.swap(true, Ordering::Relaxed) {
        eprintln!("WARNING: a game hit DECISION_SAFETY_CAP ({DECISION_SAFETY_CAP}) and was aborted early; it is excluded from natural-terminal throughput.");
    }
}

/// Plays one full Burn-mirror game through the raw `engine` API (every
/// priority window counted, nothing suppressed). Returns decisions taken.
fn play_one_game_raw(seed: u64) -> u64 {
    let mut state = build_mirror_state(seed);
    let mut rng = SplitMix64::seed(seed ^ 0x5EED_1234_ABCD_0001);
    let mut decisions = 0u64;
    loop {
        let decision = engine::advance_until_decision(&mut state);
        decisions += 1;
        if matches!(decision, Decision::GameOver { .. }) {
            break;
        }
        let action = random_action_for_decision(&decision, &state, &mut rng);
        engine::step(&mut state, action)
            .expect("random policy only picks actions the decision itself listed as legal");
        if decisions >= DECISION_SAFETY_CAP {
            warn_safety_cap_once();
            break;
        }
    }
    decisions
}

/// Plays one full Burn-mirror game through `HarnessSurfaceV2` (the
/// H-visible decision surface). Returns H-visible decisions taken.
fn play_one_game_surface(seed: u64) -> u64 {
    let deck = runtime_deck_by_id("Burn").expect("built-in Burn deck exists");
    play_one_game_surface_for_deck(seed, deck).decisions
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SurfaceGameOutcomeV1 {
    NaturalTerminal,
    SafetyCap,
    Halted,
    ApplyError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SurfaceGameResultV1 {
    decisions: u64,
    outcome: SurfaceGameOutcomeV1,
}

fn play_one_game_surface_for_deck(
    seed: u64,
    deck: &'static RuntimeDeckDefinition,
) -> SurfaceGameResultV1 {
    let mut state = build_mirror_state_from_ids(deck.card_ids, seed);
    let mut rng = SplitMix64::seed(seed ^ 0x5EED_1234_ABCD_0002);
    let mut surface = HarnessSurfaceV2::new();
    let mut decisions = 0u64;
    loop {
        let sd = surface.next_decision(&mut state);
        decisions += 1;
        match &sd {
            SurfaceDecision::Decision(Decision::GameOver { .. }) => {
                return SurfaceGameResultV1 {
                    decisions,
                    outcome: SurfaceGameOutcomeV1::NaturalTerminal,
                };
            }
            SurfaceDecision::Decision(Decision::Halted { .. }) => {
                return SurfaceGameResultV1 {
                    decisions,
                    outcome: SurfaceGameOutcomeV1::Halted,
                };
            }
            SurfaceDecision::Decision(d) => {
                let action = random_action_for_decision(d, &state, &mut rng);
                if surface
                    .apply(&mut state, SurfaceAction::Action(action))
                    .is_err()
                {
                    return SurfaceGameResultV1 {
                        decisions,
                        outcome: SurfaceGameOutcomeV1::ApplyError,
                    };
                }
            }
            SurfaceDecision::DeclareBlockersForAttacker { legal_blockers, .. } => {
                let (num, den) = BLOCK_CHANCE;
                let picks = if !legal_blockers.is_empty() && rng_chance(&mut rng, num, den) {
                    vec![legal_blockers[rng_below(&mut rng, legal_blockers.len())]]
                } else {
                    Vec::new()
                };
                if surface
                    .apply(&mut state, SurfaceAction::DeclareBlockersForAttacker(picks))
                    .is_err()
                {
                    return SurfaceGameResultV1 {
                        decisions,
                        outcome: SurfaceGameOutcomeV1::ApplyError,
                    };
                }
            }
        }
        if decisions >= DECISION_SAFETY_CAP {
            warn_safety_cap_once();
            return SurfaceGameResultV1 {
                decisions,
                outcome: SurfaceGameOutcomeV1::SafetyCap,
            };
        }
    }
}

// ------------------------------------------ machine-readable raw ceiling v1

const RAW_CEILING_SCHEMA_V1: &str = "kernel_rl_raw_ceiling/v1";
const CEILING_SEED_PARTITION_STRIDE: u64 = 1u64 << 55;
const CEILING_WARMUP_PARTITION_OFFSET: u64 = 256;

fn ceiling_partition_seed(base_seed: u64, warmup: bool, actor: usize, index: u64) -> u64 {
    assert!(actor < 256, "ceiling actor must fit its seed partition");
    assert!(
        index < CEILING_SEED_PARTITION_STRIDE,
        "ceiling seed partition exhausted"
    );
    let partition = actor as u64
        + if warmup {
            CEILING_WARMUP_PARTITION_OFFSET
        } else {
            0
        };
    base_seed
        .wrapping_add(partition.wrapping_mul(CEILING_SEED_PARTITION_STRIDE))
        .wrapping_add(index)
}

#[derive(Debug)]
struct CeilingConfigV1 {
    git_commit: String,
    deck_id: String,
    actors: Vec<usize>,
    warmup: Duration,
    measure: Duration,
    seed: u64,
}

impl CeilingConfigV1 {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut git_commit = None;
        let mut deck_id = "Burn".to_string();
        let mut actors = vec![1, 4, 8, 16];
        let mut warmup_ms = 150u64;
        let mut measure_ms = 4_000u64;
        let mut seed = 0x9E37_0000u64;
        let mut seen = HashSet::new();
        let mut index = 0;
        while index < args.len() {
            let flag = args[index].as_str();
            if !seen.insert(flag.to_string()) {
                return Err(format!("duplicate option: {flag}"));
            }
            let value = args
                .get(index + 1)
                .ok_or_else(|| format!("missing value for {flag}"))?;
            match flag {
                "--git-commit" => {
                    if value.len() != 40
                        || !value
                            .bytes()
                            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                    {
                        return Err(
                            "--git-commit must be 40 lowercase hexadecimal characters".to_string()
                        );
                    }
                    git_commit = Some(value.clone());
                }
                "--deck" => {
                    if runtime_deck_by_id(value).is_none() {
                        return Err("--deck must be exact Burn or Rally".to_string());
                    }
                    deck_id = value.clone();
                }
                "--actors" => {
                    let parsed = value
                        .split(',')
                        .map(|part| {
                            part.parse::<usize>()
                                .map_err(|_| "--actors must be a comma-separated integer list")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    if parsed.is_empty()
                        || parsed.iter().any(|count| !(1..=256).contains(count))
                        || parsed.iter().copied().collect::<HashSet<_>>().len() != parsed.len()
                    {
                        return Err("--actors must contain unique integers in the range 1..=256"
                            .to_string());
                    }
                    actors = parsed;
                }
                "--warmup-ms" => {
                    warmup_ms = parse_bounded_ms(value, "--warmup-ms", 0)?;
                }
                "--measure-ms" => {
                    measure_ms = parse_bounded_ms(value, "--measure-ms", 1)?;
                }
                "--seed" => {
                    seed = value
                        .parse::<u64>()
                        .map_err(|_| "--seed must be an unsigned 64-bit integer".to_string())?;
                }
                _ => return Err(format!("unknown option: {flag}")),
            }
            index += 2;
        }
        Ok(Self {
            git_commit: git_commit.ok_or_else(|| "missing --git-commit".to_string())?,
            deck_id,
            actors,
            warmup: Duration::from_millis(warmup_ms),
            measure: Duration::from_millis(measure_ms),
            seed,
        })
    }
}

fn parse_bounded_ms(value: &str, flag: &str, minimum: u64) -> Result<u64, String> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| format!("{flag} must be an unsigned integer"))?;
    if !(minimum..=600_000).contains(&parsed) {
        return Err(format!("{flag} must be in {minimum}..=600000"));
    }
    Ok(parsed)
}

#[derive(Serialize)]
struct CeilingDeckV1<'a> {
    id: &'a str,
    runtime_deck_hash: u64,
    source_path: &'a str,
    source_sha256: &'a str,
    mainboard_count: usize,
}

#[derive(Serialize)]
struct CeilingBinaryV1<'a> {
    package: &'static str,
    package_version: &'static str,
    git_commit_claim: &'a str,
    source_verification: &'static str,
    build_profile: &'static str,
}

#[derive(Serialize)]
struct CeilingWorkloadV1<'a> {
    driver: &'static str,
    policy: &'static str,
    ordered_deck_ids: [&'a str; 2],
    includes: &'static str,
    excludes: &'static [&'static str],
    actor_counts: &'a [usize],
    warmup_ns: u64,
    measure_target_ns: u64,
    seed: u64,
    seed_schedule: &'static str,
    phase_actor_partition_stride: u64,
    warmup_partition_offset: u64,
    partition_count: u64,
    per_actor_game_seed_increment: u64,
    decision_safety_cap: u64,
}

#[derive(Serialize)]
struct CeilingTrialV1 {
    actors: usize,
    warmup_attempted_games: u64,
    warmup_natural_terminal_games: u64,
    warmup_safety_cap_truncations: u64,
    warmup_halted_games: u64,
    warmup_apply_errors: u64,
    warmup_seed_partition_exhausted: bool,
    attempted_games: u64,
    natural_terminal_games: u64,
    safety_cap_truncations: u64,
    halted_games: u64,
    apply_errors: u64,
    measured_seed_partition_exhausted: bool,
    outcomes_valid: bool,
    decisions: u64,
    actor_seed_starts: Vec<u64>,
    actor_warmup_seed_starts: Vec<u64>,
    actor_attempt_counts: Vec<u64>,
    measurement_wall_ns: u64,
    games_per_second: f64,
    decisions_per_second: f64,
    games_per_second_per_actor: f64,
}

#[derive(Serialize)]
struct CeilingRecordV1<'a> {
    schema: &'static str,
    deck: CeilingDeckV1<'a>,
    binary: CeilingBinaryV1<'a>,
    hardware: CeilingHardwareV1,
    workload: CeilingWorkloadV1<'a>,
    trials: Vec<CeilingTrialV1>,
}

#[derive(Serialize)]
struct CeilingHardwareV1 {
    available_parallelism: usize,
}

fn duration_ns_u64(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn measure_surface_ceiling_v1(
    deck: &'static RuntimeDeckDefinition,
    actors: usize,
    warmup: Duration,
    measure: Duration,
    base_seed: u64,
) -> CeilingTrialV1 {
    let barrier = Arc::new(Barrier::new(actors));
    let shared_start = Arc::new(OnceLock::new());
    let handles: Vec<_> = (0..actors)
        .map(|actor| {
            let barrier = Arc::clone(&barrier);
            let shared_start = Arc::clone(&shared_start);
            std::thread::spawn(move || {
                let lane_seed = ceiling_partition_seed(base_seed, false, actor, 0);
                let warmup_lane_seed = ceiling_partition_seed(base_seed, true, actor, 0);
                let warm_start = Instant::now();
                let mut warmup_attempted_games = 0u64;
                let mut warmup_natural_terminal_games = 0u64;
                let mut warmup_safety_cap_truncations = 0u64;
                let mut warmup_halted_games = 0u64;
                let mut warmup_apply_errors = 0u64;
                let mut warmup_seed_partition_exhausted = false;
                while warm_start.elapsed() < warmup {
                    if warmup_attempted_games >= CEILING_SEED_PARTITION_STRIDE {
                        warmup_seed_partition_exhausted = true;
                        break;
                    }
                    let warm_seed =
                        ceiling_partition_seed(base_seed, true, actor, warmup_attempted_games);
                    let result = play_one_game_surface_for_deck(warm_seed, deck);
                    warmup_attempted_games = warmup_attempted_games.saturating_add(1);
                    match result.outcome {
                        SurfaceGameOutcomeV1::NaturalTerminal => {
                            warmup_natural_terminal_games =
                                warmup_natural_terminal_games.saturating_add(1)
                        }
                        SurfaceGameOutcomeV1::SafetyCap => {
                            warmup_safety_cap_truncations =
                                warmup_safety_cap_truncations.saturating_add(1)
                        }
                        SurfaceGameOutcomeV1::Halted => {
                            warmup_halted_games = warmup_halted_games.saturating_add(1)
                        }
                        SurfaceGameOutcomeV1::ApplyError => {
                            warmup_apply_errors = warmup_apply_errors.saturating_add(1)
                        }
                    }
                }
                barrier.wait();
                if actor == 0 {
                    shared_start
                        .set(Instant::now())
                        .expect("shared measurement start is set once");
                }
                barrier.wait();
                let start = *shared_start.get().expect("actor zero set shared start");
                let deadline = start + measure;
                let mut attempted_games = 0u64;
                let mut natural_terminal_games = 0u64;
                let mut safety_cap_truncations = 0u64;
                let mut halted_games = 0u64;
                let mut apply_errors = 0u64;
                let mut decisions = 0u64;
                let mut measured_seed_partition_exhausted = false;
                while Instant::now() < deadline {
                    if attempted_games >= CEILING_SEED_PARTITION_STRIDE {
                        measured_seed_partition_exhausted = true;
                        break;
                    }
                    let seed = ceiling_partition_seed(base_seed, false, actor, attempted_games);
                    let result = play_one_game_surface_for_deck(seed, deck);
                    decisions = decisions.saturating_add(result.decisions);
                    attempted_games = attempted_games.saturating_add(1);
                    match result.outcome {
                        SurfaceGameOutcomeV1::NaturalTerminal => {
                            natural_terminal_games = natural_terminal_games.saturating_add(1)
                        }
                        SurfaceGameOutcomeV1::SafetyCap => {
                            safety_cap_truncations = safety_cap_truncations.saturating_add(1)
                        }
                        SurfaceGameOutcomeV1::Halted => {
                            halted_games = halted_games.saturating_add(1)
                        }
                        SurfaceGameOutcomeV1::ApplyError => {
                            apply_errors = apply_errors.saturating_add(1)
                        }
                    }
                }
                (
                    actor,
                    lane_seed,
                    warmup_lane_seed,
                    warmup_attempted_games,
                    warmup_natural_terminal_games,
                    warmup_safety_cap_truncations,
                    warmup_halted_games,
                    warmup_apply_errors,
                    warmup_seed_partition_exhausted,
                    attempted_games,
                    natural_terminal_games,
                    safety_cap_truncations,
                    halted_games,
                    apply_errors,
                    measured_seed_partition_exhausted,
                    decisions,
                )
            })
        })
        .collect();
    let mut warmup_attempted_games = 0u64;
    let mut warmup_natural_terminal_games = 0u64;
    let mut warmup_safety_cap_truncations = 0u64;
    let mut warmup_halted_games = 0u64;
    let mut warmup_apply_errors = 0u64;
    let mut warmup_seed_partition_exhausted = false;
    let mut attempted_games = 0u64;
    let mut natural_terminal_games = 0u64;
    let mut safety_cap_truncations = 0u64;
    let mut halted_games = 0u64;
    let mut apply_errors = 0u64;
    let mut measured_seed_partition_exhausted = false;
    let mut decisions = 0u64;
    let mut actor_seed_starts = vec![0u64; actors];
    let mut actor_warmup_seed_starts = vec![0u64; actors];
    let mut actor_attempt_counts = vec![0u64; actors];
    for handle in handles {
        let (
            actor,
            lane_seed,
            lane_warmup_seed,
            lane_warmup_attempted,
            lane_warmup_natural,
            lane_warmup_safety_cap,
            lane_warmup_halted,
            lane_warmup_apply_errors,
            lane_warmup_partition_exhausted,
            lane_attempted,
            lane_natural,
            lane_safety_cap,
            lane_halted,
            lane_apply_errors,
            lane_measured_partition_exhausted,
            lane_decisions,
        ) = handle.join().expect("ceiling worker panicked");
        actor_seed_starts[actor] = lane_seed;
        actor_warmup_seed_starts[actor] = lane_warmup_seed;
        actor_attempt_counts[actor] = lane_attempted;
        warmup_attempted_games = warmup_attempted_games.saturating_add(lane_warmup_attempted);
        warmup_natural_terminal_games =
            warmup_natural_terminal_games.saturating_add(lane_warmup_natural);
        warmup_safety_cap_truncations =
            warmup_safety_cap_truncations.saturating_add(lane_warmup_safety_cap);
        warmup_halted_games = warmup_halted_games.saturating_add(lane_warmup_halted);
        warmup_apply_errors = warmup_apply_errors.saturating_add(lane_warmup_apply_errors);
        warmup_seed_partition_exhausted |= lane_warmup_partition_exhausted;
        attempted_games = attempted_games.saturating_add(lane_attempted);
        natural_terminal_games = natural_terminal_games.saturating_add(lane_natural);
        safety_cap_truncations = safety_cap_truncations.saturating_add(lane_safety_cap);
        halted_games = halted_games.saturating_add(lane_halted);
        apply_errors = apply_errors.saturating_add(lane_apply_errors);
        measured_seed_partition_exhausted |= lane_measured_partition_exhausted;
        decisions = decisions.saturating_add(lane_decisions);
    }
    let wall = shared_start
        .get()
        .expect("measurement start is always set")
        .elapsed();
    assert_eq!(
        warmup_attempted_games,
        warmup_natural_terminal_games
            .saturating_add(warmup_safety_cap_truncations)
            .saturating_add(warmup_halted_games)
            .saturating_add(warmup_apply_errors),
        "every warmup game must have exactly one outcome"
    );
    assert_eq!(
        attempted_games,
        natural_terminal_games
            .saturating_add(safety_cap_truncations)
            .saturating_add(halted_games)
            .saturating_add(apply_errors),
        "every attempted game must have exactly one outcome"
    );
    let seconds = wall.as_secs_f64();
    let games_per_second = natural_terminal_games as f64 / seconds;
    let decisions_per_second = decisions as f64 / seconds;
    let games_per_second_per_actor = games_per_second / actors as f64;
    assert!(
        games_per_second.is_finite()
            && decisions_per_second.is_finite()
            && games_per_second_per_actor.is_finite(),
        "ceiling rates must be finite"
    );
    CeilingTrialV1 {
        actors,
        warmup_attempted_games,
        warmup_natural_terminal_games,
        warmup_safety_cap_truncations,
        warmup_halted_games,
        warmup_apply_errors,
        warmup_seed_partition_exhausted,
        attempted_games,
        natural_terminal_games,
        safety_cap_truncations,
        halted_games,
        apply_errors,
        measured_seed_partition_exhausted,
        outcomes_valid: warmup_safety_cap_truncations == 0
            && warmup_halted_games == 0
            && warmup_apply_errors == 0
            && !warmup_seed_partition_exhausted
            && safety_cap_truncations == 0
            && halted_games == 0
            && apply_errors == 0
            && !measured_seed_partition_exhausted,
        decisions,
        actor_seed_starts,
        actor_warmup_seed_starts,
        actor_attempt_counts,
        measurement_wall_ns: duration_ns_u64(wall),
        games_per_second,
        decisions_per_second,
        games_per_second_per_actor,
    }
}

fn run_ceiling_json_v1(config: CeilingConfigV1) {
    let deck = runtime_deck_by_id(&config.deck_id).expect("validated runtime deck");
    let trials = config
        .actors
        .iter()
        .copied()
        .map(|actors| {
            measure_surface_ceiling_v1(deck, actors, config.warmup, config.measure, config.seed)
        })
        .collect();
    let record = CeilingRecordV1 {
        schema: RAW_CEILING_SCHEMA_V1,
        deck: CeilingDeckV1 {
            id: deck.id,
            runtime_deck_hash: deck.runtime_deck_hash,
            source_path: deck.source_path,
            source_sha256: deck.source_sha256,
            mainboard_count: deck.mainboard_count,
        },
        binary: CeilingBinaryV1 {
            package: env!("CARGO_PKG_NAME"),
            package_version: env!("CARGO_PKG_VERSION"),
            git_commit_claim: &config.git_commit,
            source_verification: "user_supplied_unverified/v1",
            build_profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        },
        hardware: CeilingHardwareV1 {
            available_parallelism: std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1),
        },
        workload: CeilingWorkloadV1 {
            driver: "HarnessSurfaceV2/v2",
            policy: "seeded_random_legal/v1",
            ordered_deck_ids: [deck.id, deck.id],
            includes: "engine_plus_harness_surface_v2",
            excludes: &[
                "policy_surface_v5",
                "rl_session_v1",
                "observation_v5_feature_encoding",
                "legal_action_v5_encoding",
                "privileged_environment_integrity",
                "jsonl_protocol",
                "neural_inference",
                "neural_policy_action_sampling",
                "loss_backward_optimizer",
                "artifact_persistence",
                "python_ipc",
            ],
            actor_counts: &config.actors,
            warmup_ns: duration_ns_u64(config.warmup),
            measure_target_ns: duration_ns_u64(config.measure),
            seed: config.seed,
            seed_schedule: "wrapping_u64_phase_actor_partition_plus_game_index/v1",
            phase_actor_partition_stride: CEILING_SEED_PARTITION_STRIDE,
            warmup_partition_offset: CEILING_WARMUP_PARTITION_OFFSET,
            partition_count: 512,
            per_actor_game_seed_increment: 1,
            decision_safety_cap: DECISION_SAFETY_CAP,
        },
        trials,
    };
    println!(
        "{}",
        serde_json::to_string(&record).expect("ceiling record serializes")
    );
}

// ---------------------------------------------------------- measurement

fn measure_games<F: FnMut(u64) -> u64>(
    mut play_one: F,
    warmup: Duration,
    target: Duration,
) -> (u64, u64, Duration) {
    let mut seed = 0u64;
    let warm_start = Instant::now();
    while warm_start.elapsed() < warmup {
        play_one(seed);
        seed += 1;
    }
    let start = Instant::now();
    let mut games = 0u64;
    let mut decisions = 0u64;
    while start.elapsed() < target {
        decisions += play_one(seed);
        games += 1;
        seed += 1;
    }
    (games, decisions, start.elapsed())
}

// ---------------------------------------------------- section 1: snapshot

fn move_hand_to_graveyard(state: &mut GameState, player: PlayerId, id: ObjectId) {
    let ps = &mut state.players[player.index()];
    if let Some(pos) = ps.hand.iter().position(|&h| h == id) {
        ps.hand.remove(pos);
        ps.graveyard.push(id);
        state.objects.get_mut(id).zone = Zone::Graveyard;
    }
}

/// Builds a synthetic state with roughly `total_objects` objects, spread
/// plausibly across library/hand/battlefield/graveyard for both players,
/// plus a couple of fabricated token objects per player (standing in for
/// Voldaren Epicure-style ETB tokens). Object count is fixed at
/// construction (zone moves mutate in place; see `snapshot.rs`'s module
/// doc), so this is a representative shape for a mid-game state of that
/// size -- construction is synthetic (direct zone assignment, not played
/// out turn by turn); plausibility over realism, per the design brief.
fn synthetic_state_of_size(total_objects: u32, seed: u64) -> GameState {
    let cycle = burn_deck_ids();
    let token_card_def = card_id_by_name("Mountain").unwrap();
    let base = total_objects.saturating_sub(4); // reserve 4 slots for fabricated tokens below
    let per_p0 = base / 2;
    let per_p1 = base - per_p0;
    let lib0: Vec<u16> = (0..per_p0)
        .map(|i| cycle[i as usize % cycle.len()])
        .collect();
    let lib1: Vec<u16> = (0..per_p1)
        .map(|i| cycle[i as usize % cycle.len()])
        .collect();
    let mut state = GameState::new_from_libraries(&lib0, &lib1, debug_name, seed);

    for player in [PlayerId::P0, PlayerId::P1] {
        for _ in 0..7 {
            state.draw_card(player);
        }
        let hand_snapshot: Vec<ObjectId> = state.players[player.index()].hand.clone();
        for (i, &id) in hand_snapshot.iter().enumerate() {
            if i % 3 == 0 {
                state.move_hand_to_battlefield(player, id);
            }
        }
        for _ in 0..5 {
            if let Some(id) = state.draw_card(player) {
                move_hand_to_graveyard(&mut state, player, id);
            }
        }
        for _ in 0..2 {
            let id = state.objects.push(GameObject {
                card_def: token_card_def,
                name: "Food Token".to_string(),
                owner: player,
                controller: player,
                zone: Zone::Battlefield,
                tapped: false,
                summoning_sick: false,
                damage: 0,
                counters: Counters::default(),
                attachments: Vec::new(),
                v4: mtg_kernel::state::ObjectStateV4::from_card_def(token_card_def),
                spell_copy_origin: None,
                plotted_turn: None,
                zone_change_count: 0,
            });
            state.players[player.index()].battlefield.push(id);
        }
    }
    state
}

fn time_iters<F: FnMut()>(mut f: F, warmup: u64, timed: u64) -> Duration {
    for _ in 0..warmup {
        f();
    }
    let start = Instant::now();
    for _ in 0..timed {
        f();
    }
    start.elapsed()
}

fn section1_snapshot_scaling() {
    println!("--- Section 1: snapshot/restore scaling (PERFORMANCE-ONLY) ---");
    println!(
        "{:>10} {:>12} {:>14} {:>14}",
        "objects", "actual_objs", "snapshot ns/op", "restore ns/op"
    );
    for &target in &[80u32, 200, 350, 500] {
        let state = synthetic_state_of_size(target, 0xABCD_0000 + target as u64);
        let actual_objects = state.objects.len();

        let snap_elapsed = time_iters(
            || {
                std::hint::black_box(state.snapshot());
            },
            SNAPSHOT_WARMUP_ITERS,
            SNAPSHOT_TIMED_ITERS,
        );
        let snapshot_ns = snap_elapsed.as_nanos() as f64 / SNAPSHOT_TIMED_ITERS as f64;

        let snap = state.snapshot();
        let mut scratch = state.clone();
        let restore_elapsed = time_iters(
            || {
                scratch.restore(&snap);
                std::hint::black_box(&scratch);
            },
            SNAPSHOT_WARMUP_ITERS,
            SNAPSHOT_TIMED_ITERS,
        );
        let restore_ns = restore_elapsed.as_nanos() as f64 / SNAPSHOT_TIMED_ITERS as f64;

        println!("{target:>10} {actual_objects:>12} {snapshot_ns:>14.1} {restore_ns:>14.1}");
    }
    println!();
}

// -------------------------------------------------- section 2: step throughput

fn goldfish_library(names: &[&str]) -> Vec<u16> {
    names
        .iter()
        .map(|n| card_id_by_name(n).unwrap_or_else(|| panic!("card {n:?} not found")))
        .collect()
}

/// Deterministic scripted Mono-Red Burn goldfish (see `tests/burn_goldfish.rs`,
/// duplicated here per this file's own "no cross-target sharing" convention --
/// tests/ isn't a library target an example can import from anyway):
/// P0 plays lands then bolts P1 to death; P1 never casts anything.
fn play_goldfish_once(_seed: u64) -> u64 {
    let mut p0 = vec!["Mountain", "Mountain", "Mountain"];
    p0.extend(std::iter::repeat_n("Lightning Bolt", 14));
    p0.extend(std::iter::repeat_n("Mountain", 30));
    let p1: Vec<&str> = std::iter::repeat_n("Mountain", 50).collect();
    let p0_lib = goldfish_library(&p0);
    let p1_lib = goldfish_library(&p1);
    let mut state = GameState::new_from_libraries(&p0_lib, &p1_lib, debug_name, 0xC0FFEE);
    for _ in 0..7 {
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P0));
        event::propose_and_commit(&mut state, ProposedEvent::draw(PlayerId::P1));
    }

    let bolt_def = card_id_by_name("Lightning Bolt").unwrap();
    let mut last_cast_turn = 0u32;
    let mut decisions = 0u64;
    loop {
        let decision = engine::advance_until_decision(&mut state);
        decisions += 1;
        match &decision {
            Decision::GameOver { .. } => break,
            Decision::DeclareAttackers { .. } => {
                engine::step(&mut state, Action::DeclareAttackers(Vec::new())).unwrap();
            }
            Decision::ChooseTargets { .. } => {
                engine::step(&mut state, Action::ChooseTarget(Target::Player(PlayerId::P1))).unwrap();
            }
            Decision::CastSpellOrPass { player, land_drops, .. } => {
                let player = *player;
                if !land_drops.is_empty() {
                    let land = land_drops[0];
                    engine::step(&mut state, Action::PlayLand(land)).unwrap();
                } else if player == PlayerId::P0 && state.turn >= 2 && state.turn != last_cast_turn {
                    let bolt_in_hand = state.players[0].hand.iter().copied().find(|&id| state.objects.get(id).card_def == bolt_def);
                    if let Some(bolt) = bolt_in_hand {
                        last_cast_turn = state.turn;
                        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
                    } else {
                        engine::step(&mut state, Action::Pass).unwrap();
                    }
                } else {
                    engine::step(&mut state, Action::Pass).unwrap();
                }
            }
            _ => unreachable!("the burn goldfish's library (Mountain + Lightning Bolt only) cannot produce this decision"),
        }
        if decisions >= DECISION_SAFETY_CAP {
            warn_safety_cap_once();
            break;
        }
    }
    decisions
}

fn section2_step_throughput() {
    println!("--- Section 2: step throughput, single-threaded (PERFORMANCE-ONLY) ---");

    let (games, decisions, elapsed) = measure_games(
        play_goldfish_once,
        Duration::from_millis(200),
        GOLDFISH_DURATION,
    );
    let secs = elapsed.as_secs_f64();
    let games_per_sec = games as f64 / secs;
    let decisions_per_sec = decisions as f64 / secs;
    println!(
        "(a) Burn goldfish script, replayed:  {games_per_sec:>8.1} games/sec   {decisions_per_sec:>10.1} decisions/sec   ({games} games, {decisions} decisions, {secs:.2}s)"
    );

    let (games, decisions, elapsed) = measure_games(
        play_one_game_raw,
        Duration::from_millis(300),
        RANDOM_GAME_DURATION,
    );
    let secs = elapsed.as_secs_f64();
    let games_per_sec = games as f64 / secs;
    let decisions_per_sec = decisions as f64 / secs;
    println!(
        "(b) full Burn-mirror, random policy: {games_per_sec:>8.1} games/sec   {decisions_per_sec:>10.1} decisions/sec   ({games} games, {decisions} decisions, {secs:.2}s)"
    );
    println!();
}

// ------------------------------------------------- section 3: self-play threading

fn section3_selfplay_threading() {
    println!("--- Section 3: self-play throughput via HarnessSurfaceV2, N threads (PERFORMANCE-ONLY) ---");
    println!("Engine+surface only: excludes Python, NN inference, optimization, artifact persistence, and orchestration.");
    println!(
        "Do not compare these measurements directly with end-to-end XMage trainer throughput."
    );
    println!(
        "{:>8} {:>14} {:>18} {:>18}",
        "threads", "games/sec", "H-visible dec/sec", "games/sec/thread"
    );

    for &n in &SELFPLAY_THREAD_COUNTS {
        let start = Instant::now();
        let handles: Vec<_> = (0..n)
            .map(|t| {
                std::thread::spawn(move || {
                    let mut seed = 0x9E37_0000u64 + (t as u64) * 7_919;
                    let warm_deadline = Instant::now() + Duration::from_millis(150);
                    while Instant::now() < warm_deadline {
                        play_one_game_surface(seed);
                        seed = seed.wrapping_add(1);
                    }
                    let deadline = Instant::now() + SELFPLAY_DURATION_PER_CONFIG;
                    let mut games = 0u64;
                    let mut decisions = 0u64;
                    while Instant::now() < deadline {
                        decisions += play_one_game_surface(seed);
                        games += 1;
                        seed = seed.wrapping_add(1);
                    }
                    (games, decisions)
                })
            })
            .collect();

        let mut total_games = 0u64;
        let mut total_decisions = 0u64;
        for h in handles {
            let (g, d) = h.join().expect("worker thread panicked");
            total_games += g;
            total_decisions += d;
        }
        let wall = start.elapsed().as_secs_f64();
        let games_per_sec = total_games as f64 / wall;
        let decisions_per_sec = total_decisions as f64 / wall;
        println!(
            "{n:>8} {games_per_sec:>14.2} {decisions_per_sec:>18.1} {:>18.3}",
            games_per_sec / n as f64
        );
    }
    println!();
}

// -------------------------------------------------- section 4: legal actions

/// Plays a raw random-policy game up to `max_decisions`, remembering the
/// richest `CastSpellOrPass` window seen (castable spells + mana abilities
/// + a land drop all simultaneously available -- the brief's "cast windows
///   with several castable spells + mana abilities + land drop" shape) and
///   returns a snapshot of the state at that point.
fn capture_rich_cast_window(seed: u64, max_decisions: u64) -> GameState {
    let mut state = build_mirror_state(seed);
    let mut rng = SplitMix64::seed(seed ^ 0x5EED_1234_ABCD_0003);
    let mut best: Option<GameState> = None;
    let mut best_score = 0usize;
    for _ in 0..max_decisions {
        let decision = engine::advance_until_decision(&mut state);
        if let Decision::CastSpellOrPass {
            castable_spells,
            mana_abilities,
            land_drops,
            ..
        } = &decision
        {
            if !castable_spells.is_empty() && !mana_abilities.is_empty() && !land_drops.is_empty() {
                let score = castable_spells.len() + mana_abilities.len();
                if score > best_score {
                    best_score = score;
                    best = Some(state.clone());
                }
            }
        }
        if matches!(decision, Decision::GameOver { .. }) {
            break;
        }
        let action = random_action_for_decision(&decision, &state, &mut rng);
        if engine::step(&mut state, action).is_err() {
            break;
        }
    }
    best.unwrap_or(state)
}

fn section4_legal_actions_cost() {
    println!("--- Section 4: legal-actions enumeration cost (PERFORMANCE-ONLY) ---");
    let captured = capture_rich_cast_window(0x1357_9BDF, 2_000);
    let decision = engine::advance_until_decision(&mut captured.clone());
    let (castable, mana, land) = match &decision {
        Decision::CastSpellOrPass {
            castable_spells,
            mana_abilities,
            land_drops,
            ..
        } => (
            castable_spells.len(),
            mana_abilities.len(),
            land_drops.len(),
        ),
        _ => (0, 0, 0),
    };
    println!("captured window: {castable} castable spells, {mana} mana abilities, {land} land drops, {} total objects", captured.objects.len());

    let mut probe = captured.clone();
    let elapsed = time_iters(
        || {
            std::hint::black_box(engine::advance_until_decision(&mut probe));
        },
        1_000,
        LEGAL_ACTIONS_TIMED_ITERS,
    );
    let ns_per_call = elapsed.as_nanos() as f64 / LEGAL_ACTIONS_TIMED_ITERS as f64;
    assert_eq!(
        probe, captured,
        "advance_until_decision must not mutate an already-idle decision state"
    );
    println!("advance_until_decision (idle re-enumeration): {ns_per_call:.1} ns/op over {LEGAL_ACTIONS_TIMED_ITERS} iterations");

    // Section 3's HarnessSurfaceV2::next_decision computes `state.state_hash()`
    // unconditionally at the top of every internal loop iteration (surface_v2.rs:350),
    // even for iterations that end up surfaced (not suppressed) -- where the hash is
    // computed and then never used. Measured here directly to attribute section 3's
    // per-decision cost relative to section 2's hash-free raw driver.
    let hash_elapsed = time_iters(
        || {
            std::hint::black_box(captured.state_hash());
        },
        1_000,
        LEGAL_ACTIONS_TIMED_ITERS,
    );
    let hash_ns_per_call = hash_elapsed.as_nanos() as f64 / LEGAL_ACTIONS_TIMED_ITERS as f64;
    println!("state_hash() on the same captured state:      {hash_ns_per_call:.1} ns/op over {LEGAL_ACTIONS_TIMED_ITERS} iterations");
    println!();
}

// --------------------------------------------------- section 5: allocator

#[cfg(feature = "count_allocs")]
mod counting_alloc {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicU64, Ordering};

    pub static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
    pub static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

    pub struct CountingAlloc;

    unsafe impl GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
            ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
            unsafe { System.alloc(layout) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) }
        }
    }

    pub fn snapshot() -> (u64, u64) {
        (
            ALLOC_COUNT.load(Ordering::Relaxed),
            ALLOC_BYTES.load(Ordering::Relaxed),
        )
    }
}

#[cfg(feature = "count_allocs")]
#[global_allocator]
static GLOBAL: counting_alloc::CountingAlloc = counting_alloc::CountingAlloc;

#[cfg(feature = "count_allocs")]
fn section5_alloc_profile() {
    println!("--- Section 5: allocator profile (PERFORMANCE-ONLY, count_allocs feature) ---");
    let mut seed = 0x4110_C000u64;
    // warm up allocator/heap so the first game's one-time setup cost doesn't skew the average
    play_one_game_raw(seed);
    seed += 1;

    let (count_before, bytes_before) = counting_alloc::snapshot();
    let mut total_decisions = 0u64;
    for _ in 0..ALLOC_PROFILE_GAMES {
        total_decisions += play_one_game_raw(seed);
        seed += 1;
    }
    let (count_after, bytes_after) = counting_alloc::snapshot();

    let allocs = count_after - count_before;
    let bytes = bytes_after - bytes_before;
    println!(
        "{ALLOC_PROFILE_GAMES} games, {total_decisions} decisions: {:.0} allocations/game, {:.0} bytes/game, {:.1} allocations/decision",
        allocs as f64 / ALLOC_PROFILE_GAMES as f64,
        bytes as f64 / ALLOC_PROFILE_GAMES as f64,
        allocs as f64 / total_decisions as f64
    );
    println!();
}

#[cfg(test)]
mod ceiling_tests {
    use super::*;

    const COMMIT: &str = "7735d368117c20211c66e72cb5efc71e1bd4d74f";

    #[test]
    fn ceiling_cli_is_strict_and_defaults_remain_explicit() {
        let config = CeilingConfigV1::parse(&[
            "--git-commit".to_string(),
            COMMIT.to_string(),
            "--deck".to_string(),
            "Rally".to_string(),
            "--actors".to_string(),
            "1,4,8,16".to_string(),
            "--warmup-ms".to_string(),
            "0".to_string(),
            "--measure-ms".to_string(),
            "1".to_string(),
            "--seed".to_string(),
            "71501".to_string(),
        ])
        .unwrap();
        assert_eq!(config.deck_id, "Rally");
        assert_eq!(config.actors, [1, 4, 8, 16]);
        assert_eq!(config.warmup, Duration::ZERO);
        assert_eq!(config.measure, Duration::from_millis(1));
        assert_eq!(config.seed, 71501);

        assert!(CeilingConfigV1::parse(&[]).is_err());
        assert!(CeilingConfigV1::parse(&[
            "--git-commit".to_string(),
            COMMIT.to_string(),
            "--actors".to_string(),
            "1,1".to_string(),
        ])
        .is_err());
        assert!(CeilingConfigV1::parse(&[
            "--git-commit".to_string(),
            COMMIT.to_string(),
            "--deck".to_string(),
            "rally".to_string(),
        ])
        .is_err());
    }

    #[test]
    fn bounded_rally_ceiling_trial_reports_exact_counts_and_timing() {
        let deck = runtime_deck_by_id("Rally").unwrap();
        let trial =
            measure_surface_ceiling_v1(deck, 1, Duration::ZERO, Duration::from_millis(1), 71501);
        assert_eq!(trial.actors, 1);
        assert_eq!(trial.warmup_attempted_games, 0);
        assert_eq!(trial.warmup_natural_terminal_games, 0);
        assert_eq!(trial.warmup_safety_cap_truncations, 0);
        assert_eq!(trial.warmup_halted_games, 0);
        assert_eq!(trial.warmup_apply_errors, 0);
        assert!(trial.attempted_games >= 1);
        assert_eq!(
            trial.attempted_games,
            trial.natural_terminal_games
                + trial.safety_cap_truncations
                + trial.halted_games
                + trial.apply_errors
        );
        assert!(trial.decisions >= trial.attempted_games);
        assert_eq!(trial.actor_seed_starts, [71501]);
        assert_eq!(trial.actor_attempt_counts, [trial.attempted_games]);
        assert!(trial.measurement_wall_ns >= 1_000_000);
        assert!(trial.games_per_second.is_finite());
        assert!(trial.decisions_per_second.is_finite());
    }

    #[test]
    fn ceiling_seed_partitions_are_disjoint_across_phases_and_actors() {
        let base_seed = u64::MAX - 71_500;
        let mut seeds = HashSet::new();
        for warmup in [false, true] {
            for actor in 0..16 {
                for game_index in 0..10_000 {
                    assert!(
                        seeds.insert(ceiling_partition_seed(base_seed, warmup, actor, game_index,))
                    );
                }
            }
        }
        assert_eq!(seeds.len(), 2 * 16 * 10_000);

        let trial = measure_surface_ceiling_v1(
            runtime_deck_by_id("Rally").unwrap(),
            4,
            Duration::ZERO,
            Duration::from_millis(1),
            base_seed,
        );
        assert_eq!(
            trial.actor_seed_starts,
            (0..4)
                .map(|actor| ceiling_partition_seed(base_seed, false, actor, 0))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            trial.actor_warmup_seed_starts,
            (0..4)
                .map(|actor| ceiling_partition_seed(base_seed, true, actor, 0))
                .collect::<Vec<_>>()
        );
    }
}

#[cfg(not(feature = "count_allocs"))]
fn section5_alloc_profile() {
    println!("--- Section 5: allocator profile (SKIPPED) ---");
    println!("Run with: cargo run --release --example bench_kernel --features count_allocs");
    println!();
}
