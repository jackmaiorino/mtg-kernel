//! Paired deals, seat layouts, league draws and seat streams (design
//! Sections 2.2 and 3.3), and the recorded Gate 2 pair rule (the research
//! lead's ruling 1 on the sizing note's Section 5).
use super::derive_seed_v1;
use crate::ids::PlayerId;
use crate::state::SplitMix64;
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, String>;

pub const TRAINING_SCHEDULE_SEED_V1: u64 = 657_000;
pub const QUALIFICATION_SEED_V1: u64 = 657_001;
pub const DEV_SEED_V1: u64 = 657_050;
pub const LEAGUE_TAG_V1: &[u8] = b"mtg-kernel/limited-campaign-league-opp/v1\0";
pub const ACTOR_RNG_TAG_V1: &[u8] = b"mtg-kernel/limited-campaign-actor-rng/v1\0";
/// 16 pairs (32 games) per update: pairs with `p mod 16` in `0..12` are
/// self-play, `12..16` league.
pub const PAIRS_PER_UPDATE_V1: u64 = 16;
pub const SELF_PLAY_PAIRS_PER_UPDATE_V1: u64 = 12;
/// League snapshots are `step_0` and every tenth step before the current one.
pub const LEAGUE_INTERVAL_V1: u64 = 10;
/// The dev panel: pairs `0..32` at `DEV_SEED_V1` on `train` decks against
/// `step_0`, after every 20th update.
pub const DEV_PAIRS_V1: u64 = 32;
pub const DEV_INTERVAL_V1: u64 = 20;
/// Gate 2: eight `train` pairs at `QUALIFICATION_SEED_V1` and two dev pairs.
pub const QUALIFICATION_PAIRS_V1: usize = 8;
pub const QUALIFICATION_DEV_PAIRS_V1: u64 = 2;
pub const GATE2_REQUIRED_DECK_V1: &str = "FDN_top_09446_URpG";

/// The pair's deal from `limited_match_v1::deal_pair_v1` over the split's
/// decks in pool order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DealV1 {
    pub pair: u64,
    pub decks: [usize; 2],
    pub env_seed: u64,
    pub starting: PlayerId,
}

pub fn deal_v1(seed: u64, pair: u64, deck_count: usize) -> Result<DealV1> {
    let (decks, env_seed, starting) =
        crate::limited_match_v1::deal_pair_v1(seed, pair, deck_count)?;
    Ok(DealV1 {
        pair,
        decks,
        env_seed,
        starting,
    })
}

/// Which policy plays a seat: `A` is the policy under training (or the
/// candidate), `B` its opponent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SlotV1 {
    A,
    B,
}

/// Seat policies and the starting player of one leg. Seat 0 always plays
/// `decks[0]` and seat 1 `decks[1]`. A symmetric pair (both seats the same
/// policy) flips the starting player on leg 1; an asymmetric pair keeps the
/// dealt starting player and swaps the policies between the seats.
pub fn leg_layout_v1(symmetric: bool, deal: &DealV1, leg: u8) -> ([SlotV1; 2], PlayerId) {
    match (symmetric, leg) {
        (true, 0) => ([SlotV1::A, SlotV1::A], deal.starting),
        (true, _) => ([SlotV1::A, SlotV1::A], deal.starting.opponent()),
        (false, 0) => ([SlotV1::A, SlotV1::B], deal.starting),
        (false, _) => ([SlotV1::B, SlotV1::A], deal.starting),
    }
}

/// The seed of one seat's policy stream:
/// `derive_seed(ACTOR_RNG_TAG_V1, [schedule_seed, pair, leg, seat])`.
pub fn actor_seed_v1(schedule_seed: u64, pair: u64, leg: u8, seat: u8) -> u64 {
    derive_seed_v1(
        ACTOR_RNG_TAG_V1,
        &[schedule_seed, pair, u64::from(leg), u64::from(seat)],
    )
}

/// The global pair indices of the batch that update `step + 1` trains on,
/// collected with the policy at `step`.
pub fn update_pairs_v1(step: u64) -> std::ops::Range<u64> {
    let first = step * PAIRS_PER_UPDATE_V1;
    first..first + PAIRS_PER_UPDATE_V1
}

pub fn is_league_pair_v1(pair: u64) -> bool {
    pair % PAIRS_PER_UPDATE_V1 >= SELF_PLAY_PAIRS_PER_UPDATE_V1
}

/// The league snapshots available to a batch collected at `step`:
/// `{step_0} U {step_10, ..., step_(10 floor((step - 1) / 10))}`, only
/// `step_0` at step 0.
pub fn league_snapshot_steps_v1(step: u64) -> Vec<u64> {
    let newest = if step == 0 {
        0
    } else {
        (step - 1) / LEAGUE_INTERVAL_V1
    };
    (0..=newest).map(|k| k * LEAGUE_INTERVAL_V1).collect()
}

/// Uniform over `bound` with rejection, as the match driver draws.
fn draw(rng: &mut SplitMix64, bound: u64) -> u64 {
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return value % bound;
        }
    }
}

/// The league opponent of pair `pair` in the batch collected at `step`: one
/// uniform draw over [`league_snapshot_steps_v1`] from a `SplitMix64` seeded
/// with `derive_seed(LEAGUE_TAG_V1, [schedule_seed, step, pair])`.
pub fn league_opponent_step_v1(schedule_seed: u64, step: u64, pair: u64) -> u64 {
    let choices = league_snapshot_steps_v1(step);
    let mut rng = SplitMix64::seed(derive_seed_v1(LEAGUE_TAG_V1, &[schedule_seed, step, pair]));
    choices[draw(&mut rng, choices.len() as u64) as usize]
}

/// The recorded Gate 2 rule, as the qualification manifest states it.
pub const GATE2_PAIR_RULE_V1: &str = "Take pair indices 0 to 7 at seed 657_001 over the train \
split in pool.tsv order. If the set lacks FDN_top_09446_URpG or lacks any Drake Hatcher train \
deck, replace index 7 (then 6) by the lowest index of 8 or more whose pair contains the missing \
deck, FDN_top_09446_URpG first. (Research lead ruling 1 on issue 657.)";

/// The Gate 2 pairs: indices in ascending order and each replacement the
/// rule made, as `(replaced index, new index)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Gate2PairsV1 {
    pub rule: String,
    pub seed: u64,
    pub indices: Vec<u64>,
    pub replaced: Vec<(u64, u64)>,
}

/// Applies [`GATE2_PAIR_RULE_V1`] to the `train` deck names in pool order.
pub fn gate2_pairs_v1(train_names: &[String], drake_decks: &[&str]) -> Result<Gate2PairsV1> {
    let count = train_names.len();
    let names_of = |pair: u64| -> Result<[&str; 2]> {
        let deal = deal_v1(QUALIFICATION_SEED_V1, pair, count)?;
        Ok(deal.decks.map(|index| train_names[index].as_str()))
    };
    let has_curator =
        |pair: u64| -> Result<bool> { Ok(names_of(pair)?.contains(&GATE2_REQUIRED_DECK_V1)) };
    let has_drake =
        |pair: u64| -> Result<bool> { Ok(names_of(pair)?.iter().any(|n| drake_decks.contains(n))) };
    let mut chosen: Vec<u64> = (0..QUALIFICATION_PAIRS_V1 as u64).collect();
    let mut slots = [7usize, 6].into_iter();
    let mut replaced = Vec::new();
    let tests: [&dyn Fn(u64) -> Result<bool>; 2] = [&has_curator, &has_drake];
    for test in tests {
        let mut present = false;
        for &pair in &chosen {
            present |= test(pair)?;
        }
        if present {
            continue;
        }
        let slot = slots
            .next()
            .ok_or("Gate 2 rule ran out of replaceable indices")?;
        let mut candidate = QUALIFICATION_PAIRS_V1 as u64;
        while chosen.contains(&candidate) || !test(candidate)? {
            candidate += 1;
            if candidate > 1_000_000 {
                return Err("Gate 2 rule found no pair with the missing deck".into());
            }
        }
        replaced.push((chosen[slot], candidate));
        chosen[slot] = candidate;
    }
    let mut both = (false, false);
    for &pair in &chosen {
        both.0 |= has_curator(pair)?;
        both.1 |= has_drake(pair)?;
    }
    if both != (true, true) {
        return Err("Gate 2 pairs do not hold both required decks".into());
    }
    chosen.sort_unstable();
    Ok(Gate2PairsV1 {
        rule: GATE2_PAIR_RULE_V1.into(),
        seed: QUALIFICATION_SEED_V1,
        indices: chosen,
        replaced,
    })
}
