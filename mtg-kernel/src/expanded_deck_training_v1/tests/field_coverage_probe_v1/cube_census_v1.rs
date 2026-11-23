//! Cube coverage census (measurement only, test only): uniform-random games
//! between dealt 40-card cube decks through the flat encoder path the trainer
//! uses (pass A of the field coverage probe in uniform mode: the FlatV7
//! transfer of the V4 fixture scores every decision through
//! `select_with_training_tensor_v4`, then a SplitMix64 stream of the episode
//! seed picks a legal action). One row per stopped game, keyed by the inner
//! error rather than the scorer's collapsed label.
//!
//! Pool: the cube list (`data/cube_v1/mtgo_vintage_cube_20261007.json`)
//! minus every card in the gap record (`..._gaps.json`: `partial`,
//! `fdn_only`, `missing`) minus a fence list (`MTG_KERNEL_CUBE_FENCE`, default
//! none: a JSON list of names, or an object whose `cards` is one). A card's
//! colours are its cube-list group (mono-colour, guild or `Colorless`); the
//! `Multicolor` group (three or more colours, and the rainbow lands) never
//! fits a two-colour seat and is left out. Whether a card is a land comes
//! from the build's card definition. Every remaining name must resolve in
//! this build (`card_id_by_name`), so the census runs under
//! `eternal-fixtures` and refuses to start anywhere it cannot.
//!
//! Deal rule (the 11-19 smoke's): per seat two colours, 23 nonland spells
//! whose colours are inside them (colourless allowed), 2 nonbasic lands under
//! the same rule, 15 basics split 8 and 7 (8 to the colour more of the
//! spells carry, the earlier of WUBRG on a tie), sideboard empty. Seat 0 is
//! dealt first; seat 1 draws from what is left, so no cube card is in both
//! decks (basics are not cube cards). A seat's colour pair is the first pair
//! of a seeded shuffle of the ten with enough cards left. Everything is a
//! function of the game seed (`DEAL_STREAM`), so one seed replays its deal.
//!
//! Game `i` uses seed `first_seed + i`; the starting player is the seed's
//! low bit. Runtime rules default to none (the trainer's default rules:
//! `resolution-boundary-v1` refuses any game with a Delve card, which the
//! cube has); London v1 with learned mulligans on both seats, as the probe.
//!
//! Stop rows: every game that does not end naturally is a stop. Its inner
//! error is, by kind: for an action-slice refusal, the input diagnosis
//! (`diagnose_current_decision_input_v4`), or, when the diagnosis names the
//! raw observation, that observation's own refusal text rebuilt on a fresh
//! session surface (the session maps it to `CorruptCurrentBinding`); for an
//! `ObservationContract` refusal, the scorer-refused fields present in the
//! scoring observation (`flat_policy_v2.rs` `build_globals`,
//! `add_public_card`, the pending-engine checks), else the V7 extension
//! error, else the diagnosis; for a non-natural terminal, its reason; for
//! every other stop, its error text. The census groups rows by kind, stage,
//! decision kind and the inner error with digits folded and `{..}` bodies
//! dropped, and gives each group its count and the seed of its first game.
//!
//! Environment (census test only):
//!   MTG_KERNEL_CUBE_CENSUS_GAMES         games (default 4; the coordinator
//!                                        runs at least 2,000)
//!   MTG_KERNEL_CUBE_CENSUS_FIRST_SEED    seed of game 0 (default `SEED_BASE_CUBE`)
//!   MTG_KERNEL_CUBE_CENSUS_REPLAY_SEED   run only this seed and print its full row
//!   MTG_KERNEL_CUBE_CENSUS_THREADS       worker threads (default 1)
//!   MTG_KERNEL_CUBE_CENSUS_RUNTIME_RULES an episode `runtime_rules` wire name
//!                                        (default none)
//!   MTG_KERNEL_CUBE_CENSUS_OUT           also write the census JSON here
//!   MTG_KERNEL_CUBE_FENCE                fence list (default none)

use super::*;
use crate::rl::{
    DiscardResumeSemanticV2, PendingEffectChoiceSemanticV4, PlayPermissionExpiryV2, TargetRefV1,
};
use crate::rl_session::FastActorDecisionV1;

/// THE T1 SWITCH. The episode's deck format (T1's `deck_format` field).
/// `None` writes no key, so today every dealt game stops at the 60-card
/// admission check as a `configuration_refusal`. Once T1 lands, set this to
/// `Some("limited40-v1")`, and remove the census test's `#[ignore]`.
const CUBE_DECK_FORMAT: Option<&str> = None;

#[cfg(feature = "eternal-fixtures")]
const CUBE_LIST: &str = "data/cube_v1/mtgo_vintage_cube_20261007.json";
#[cfg(feature = "eternal-fixtures")]
const CUBE_GAPS: &str = "data/cube_v1/mtgo_vintage_cube_20261007_gaps.json";
const GAP_KEYS: [&str; 3] = ["partial", "fdn_only", "missing"];
const SEED_BASE_CUBE: u64 = 0xc0be_ce25_0000_0000;
const DEAL_STREAM: u64 = 0x6375_6265_6465_616c;
const NONLAND_SPELLS: usize = 23;
const NONBASIC_LANDS: usize = 2;
const BASIC_LANDS: usize = 15;
const WUBRG: [char; 5] = ['W', 'U', 'B', 'R', 'G'];
const BASIC_NAMES: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];
/// Card names kept on one census group.
const GROUP_CARD_CAP: usize = 64;
/// Characters kept of a folded inner error.
const KEY_CHAR_CAP: usize = 300;

/// One pool card: its colours as a WUBRG bit set (bit 0 is W).
#[derive(Clone, Debug, PartialEq, Eq)]
struct CubeCardV1 {
    name: String,
    id: u16,
    colours: u8,
    land: bool,
}

#[derive(Debug)]
struct CubePoolV1 {
    cards: Vec<CubeCardV1>,
    /// Plains, Island, Swamp, Mountain, Forest.
    basics: [u16; 5],
    report: Value,
}

/// One seat's dealt deck; `spells` and `lands` are card ids, sorted.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DealtDeckV1 {
    colours: u8,
    spells: Vec<u16>,
    lands: Vec<u16>,
    /// Basic land count per colour, WUBRG order.
    basics: [u8; 5],
}

impl DealtDeckV1 {
    fn mainboard(&self, basic_ids: &[u16; 5]) -> Vec<u16> {
        let mut out: Vec<u16> = self.spells.iter().chain(&self.lands).copied().collect();
        for (colour, &count) in self.basics.iter().enumerate() {
            out.extend(std::iter::repeat_n(basic_ids[colour], usize::from(count)));
        }
        out.sort_unstable();
        out
    }

    fn label(&self, seed: u64, seat: usize) -> String {
        format!("cube-{seed:016x}-s{seat}-{}", colour_letters(self.colours))
    }
}

fn colour_letters(colours: u8) -> String {
    WUBRG
        .iter()
        .enumerate()
        .filter(|(bit, _)| colours & (1 << bit) != 0)
        .map(|(_, letter)| *letter)
        .collect()
}

/// A cube-list group's colours; `None` for `Multicolor` (three or more).
fn group_colours(group: &str) -> Result<Option<u8>, String> {
    const W: u8 = 1;
    const U: u8 = 2;
    const B: u8 = 4;
    const R: u8 = 8;
    const G: u8 = 16;
    Ok(Some(match group {
        "White" => W,
        "Blue" => U,
        "Black" => B,
        "Red" => R,
        "Green" => G,
        "Colorless" => 0,
        "Azorius" => W | U,
        "Dimir" => U | B,
        "Rakdos" => B | R,
        "Gruul" => R | G,
        "Selesnya" => G | W,
        "Orzhov" => W | B,
        "Izzet" => U | R,
        "Golgari" => B | G,
        "Boros" => R | W,
        "Simic" => G | U,
        "Multicolor" => return Ok(None),
        other => return Err(format!("unknown cube group {other:?}")),
    }))
}

fn string_list(value: &Value, what: &str) -> Result<Vec<String>, String> {
    value
        .as_array()
        .ok_or_else(|| format!("{what} must be a list of card names"))?
        .iter()
        .map(|name| {
            name.as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("{what} must be a list of card names"))
        })
        .collect()
}

/// A fence file: a JSON list of names, or an object whose `cards` is one.
fn fence_names(fence: &Value) -> Result<BTreeSet<String>, String> {
    let list = if fence.is_array() {
        fence
    } else {
        &fence["cards"]
    };
    Ok(string_list(list, "the fence")?.into_iter().collect())
}

/// The deal pool: the cube list minus the gap record minus the fence, every
/// remaining card resolved by `lookup` to `(id, is_land)`. Refuses an unknown
/// group, an unresolved name, a gap name the cube does not list, a card
/// listed twice or two names on one id.
fn cube_pool_v1(
    cube: &Value,
    gaps: &Value,
    fence: &BTreeSet<String>,
    lookup: &dyn Fn(&str) -> Option<(u16, bool)>,
) -> Result<CubePoolV1, String> {
    if cube["schema"] != "cube_list/v1" {
        return Err("the cube list's schema is not cube_list/v1".into());
    }
    if gaps["schema"] != "cube_coverage_gaps/v1" {
        return Err("the gap record's schema is not cube_coverage_gaps/v1".into());
    }
    let mut gap_names = BTreeSet::new();
    for key in GAP_KEYS {
        gap_names.extend(string_list(&gaps[key], &format!("gap record `{key}`"))?);
    }
    let rows = cube["cards"]
        .as_array()
        .ok_or("the cube list has no cards")?;
    let mut names = BTreeSet::new();
    let mut cards = Vec::new();
    let mut unknown = Vec::new();
    let (mut removed_gap, mut removed_fence, mut multicolor) = (0usize, 0usize, 0usize);
    for row in rows {
        let name = row["name"].as_str().ok_or("a cube card has no name")?;
        let group = row["group"].as_str().ok_or("a cube card has no group")?;
        if !names.insert(name.to_owned()) {
            return Err(format!("the cube list names {name} twice"));
        }
        let colours = group_colours(group)?;
        if gap_names.contains(name) {
            removed_gap += 1;
        } else if fence.contains(name) {
            removed_fence += 1;
        } else if let Some(colours) = colours {
            match lookup(name) {
                Some((id, land)) => cards.push(CubeCardV1 {
                    name: name.to_owned(),
                    id,
                    colours,
                    land,
                }),
                None => unknown.push(name.to_owned()),
            }
        } else {
            multicolor += 1;
        }
    }
    let not_in_cube: Vec<&String> = gap_names.difference(&names).collect();
    if !not_in_cube.is_empty() {
        return Err(format!(
            "the gap record names cards the cube list does not: {not_in_cube:?}"
        ));
    }
    if !unknown.is_empty() {
        return Err(format!(
            "{} cube card(s) have no id in this build (run under eternal-fixtures): {unknown:?}",
            unknown.len()
        ));
    }
    let mut ids = BTreeSet::new();
    for card in &cards {
        if !ids.insert(card.id) {
            return Err(format!("two cube names resolve to card id {}", card.id));
        }
    }
    let mut basics = [0u16; 5];
    for (slot, name) in basics.iter_mut().zip(BASIC_NAMES) {
        match lookup(name) {
            Some((id, true)) => *slot = id,
            _ => return Err(format!("basic land {name} does not resolve to a land")),
        }
    }
    let fence_not_in_cube: Vec<&String> = fence.difference(&names).collect();
    let report = json!({
        "cube_cards": rows.len(),
        "removed_by_gap_record": removed_gap,
        "removed_by_fence": removed_fence,
        "fence_names_not_in_cube": fence_not_in_cube,
        "multicolor_group_left_out": multicolor,
        "pool_nonland_spells": cards.iter().filter(|c| !c.land).count(),
        "pool_nonbasic_lands": cards.iter().filter(|c| c.land).count(),
    });
    Ok(CubePoolV1 {
        cards,
        basics,
        report,
    })
}

/// Uniform in `[0, n)`, by rejection.
fn below(rng: &mut SplitMix64, n: usize) -> usize {
    let n = n as u64;
    let limit = u64::MAX - (u64::MAX % n + 1) % n;
    loop {
        let x = rng.next_u64();
        if x <= limit {
            return (x % n) as usize;
        }
    }
}

fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    for i in (1..items.len()).rev() {
        items.swap(i, below(rng, i + 1));
    }
}

/// Both seats' decks for one seed (module doc: deal rule).
fn deal_pair_v1(pool: &CubePoolV1, seed: u64) -> Result<[DealtDeckV1; 2], String> {
    let mut rng = SplitMix64::seed(seed ^ DEAL_STREAM);
    let mut used = BTreeSet::new();
    let mut decks = Vec::with_capacity(2);
    for seat in 0..2 {
        let mut pairs: Vec<u8> = (0..5)
            .flat_map(|a| (a + 1..5).map(move |b| (1u8 << a) | (1u8 << b)))
            .collect();
        shuffle(&mut pairs, &mut rng);
        let mut dealt = None;
        for colours in pairs {
            let open = |land: bool| -> Vec<usize> {
                (0..pool.cards.len())
                    .filter(|&i| {
                        let card = &pool.cards[i];
                        !used.contains(&i) && card.land == land && card.colours & !colours == 0
                    })
                    .collect()
            };
            let (mut spells, mut lands) = (open(false), open(true));
            if spells.len() < NONLAND_SPELLS || lands.len() < NONBASIC_LANDS {
                continue;
            }
            shuffle(&mut spells, &mut rng);
            shuffle(&mut lands, &mut rng);
            spells.truncate(NONLAND_SPELLS);
            lands.truncate(NONBASIC_LANDS);
            dealt = Some((colours, spells, lands));
            break;
        }
        let (colours, spells, lands) = dealt.ok_or_else(|| {
            format!("seed {seed}: no colour pair has enough cards left for seat {seat}")
        })?;
        used.extend(spells.iter().chain(&lands).copied());
        let [first, second] = [0, 1].map(|k| {
            (0..5usize)
                .filter(|bit| colours & (1 << bit) != 0)
                .nth(k)
                .unwrap()
        });
        let carrying = |bit: usize| {
            spells
                .iter()
                .filter(|&&i| pool.cards[i].colours & (1 << bit) != 0)
                .count()
        };
        let mut basics = [0u8; 5];
        let (more, fewer) = if carrying(second) > carrying(first) {
            (second, first)
        } else {
            (first, second)
        };
        basics[more] = BASIC_LANDS.div_ceil(2) as u8;
        basics[fewer] = (BASIC_LANDS / 2) as u8;
        let ids = |indices: &[usize]| -> Vec<u16> {
            let mut ids: Vec<u16> = indices.iter().map(|&i| pool.cards[i].id).collect();
            ids.sort_unstable();
            ids
        };
        decks.push(DealtDeckV1 {
            colours,
            spells: ids(&spells),
            lands: ids(&lands),
            basics,
        });
    }
    Ok([decks.remove(0), decks.remove(0)])
}

/// Every clause of the deal rule, checked against the pool.
fn check_deal_v1(pool: &CubePoolV1, decks: &[DealtDeckV1; 2]) -> Result<(), String> {
    let by_id: BTreeMap<u16, &CubeCardV1> = pool.cards.iter().map(|c| (c.id, c)).collect();
    let mut seen: BTreeSet<u16> = BTreeSet::new();
    for (seat, deck) in decks.iter().enumerate() {
        let fail = |why: &str| Err(format!("seat {seat}: {why}"));
        if deck.colours.count_ones() != 2 || deck.colours >= 1 << 5 {
            return fail("not exactly two colours");
        }
        if deck.spells.len() != NONLAND_SPELLS || deck.lands.len() != NONBASIC_LANDS {
            return fail("wrong number of spells or nonbasic lands");
        }
        for (ids, land) in [(&deck.spells, false), (&deck.lands, true)] {
            for id in ids {
                let Some(card) = by_id.get(id) else {
                    return fail(&format!("card id {id} is not in the pool"));
                };
                if card.land != land || card.colours & !deck.colours != 0 {
                    return fail(&format!("{} is outside the slot or colours", card.name));
                }
                if !seen.insert(*id) {
                    return fail(&format!("{} is dealt twice", card.name));
                }
            }
        }
        // 8 basics of the colour more spells carry (the earlier of WUBRG on
        // a tie), 7 of the other, none of any other colour.
        let colours: Vec<usize> = (0..5)
            .filter(|bit| deck.colours & (1 << bit) != 0)
            .collect();
        let carrying = |bit: usize| {
            deck.spells
                .iter()
                .filter(|id| by_id.get(*id).is_some_and(|c| c.colours & (1 << bit) != 0))
                .count()
        };
        let more = if carrying(colours[1]) > carrying(colours[0]) {
            colours[1]
        } else {
            colours[0]
        };
        let mut expected = [0u8; 5];
        for &bit in &colours {
            expected[bit] = if bit == more {
                BASIC_LANDS.div_ceil(2) as u8
            } else {
                (BASIC_LANDS / 2) as u8
            };
        }
        if deck.basics != expected {
            return fail("basics are not the 8 and 7 split of the seat's colours");
        }
        if deck.mainboard(&pool.basics).len() != 40 {
            return fail("mainboard is not 40 cards");
        }
    }
    Ok(())
}

/// The census episode in the trainer's wire form. Built from JSON so that
/// T1's new field is the one line `CUBE_DECK_FORMAT` controls.
fn census_episode_json_v1(
    seed: u64,
    decks: [&ExpandedDeckListV1; 2],
    runtime_rules: Option<&str>,
) -> Value {
    let mut episode = json!({
        "id": format!("cube-census-{seed:016x}"),
        "seed": seed,
        "starting_player": seed & 1,
        "learner_seat": 0,
        "registered": decks,
        "selected": decks,
        "postboard": false,
        "max_physical_decisions": MAX_PHYSICAL_DECISIONS,
        "max_policy_steps": MAX_POLICY_STEPS,
        "opening": "london-v1",
        "mulligan_modes": ["learned", "learned"],
    });
    if let Some(rules) = runtime_rules {
        episode["runtime_rules"] = json!(rules);
    }
    if let Some(format) = CUBE_DECK_FORMAT {
        episode["deck_format"] = json!(format);
    }
    episode
}

fn deck_list(deck: &DealtDeckV1, pool: &CubePoolV1, seed: u64, seat: usize) -> ExpandedDeckListV1 {
    ExpandedDeckListV1 {
        label: deck.label(seed, seat),
        mainboard: deck.mainboard(&pool.basics),
        sideboard: Vec::new(),
    }
}

/// The scorer-refused fields present in a V6 scoring observation: the
/// predicates of `flat_policy_v2.rs` `build_globals` (globals, stack items,
/// public cards, pending engine context), `add_public_card` (counters) and
/// `discard_resume_id`, one name each, in that order.
fn refused_observation_fields_v1(
    observation: &crate::policy_observation_v6::ObservationV6,
) -> Vec<&'static str> {
    let p = &observation.projection;
    let s = &p.surface;
    let mut fields = Vec::new();
    let mut flag = |present: bool, name: &'static str| {
        if present {
            fields.push(name);
        }
    };
    flag(
        p.creatures_attacked_this_turn.is_some(),
        "creatures_attacked_this_turn",
    );
    flag(p.poison_counters.is_some(), "poison_counters");
    flag(p.ninja_emblems.is_some(), "ninja_emblems");
    flag(p.poison_prevention.is_some(), "poison_prevention");
    flag(
        s.exile_play_permissions
            .iter()
            .any(|permission| permission.without_mana_cost.0),
        "exile_play_permission.without_mana_cost",
    );
    flag(
        s.exile_play_permissions.iter().any(|permission| {
            matches!(
                permission.expiry,
                PlayPermissionExpiryV2::UntilHoldersNextEndStep
            )
        }),
        "exile_play_permission.until_holders_next_end_step",
    );
    flag(p.restricted_mana.is_some(), "restricted_mana");
    flag(
        s.stack.iter().any(|item| item.granted_ability.is_some()),
        "stack.granted_ability",
    );
    flag(
        s.stack
            .iter()
            .any(|item| item.counter_distribution.is_some()),
        "stack.counter_distribution",
    );
    flag(
        s.stack.iter().any(|item| item.counter_transfer.is_some()),
        "stack.counter_transfer",
    );
    let public = || {
        s.battlefield
            .iter()
            .flatten()
            .chain(s.graveyards.iter().flatten())
            .chain(s.exile.iter())
    };
    flag(
        public().any(|card| card.creature_upgrade.is_some()),
        "card.creature_upgrade",
    );
    flag(
        public().any(|card| card.characteristics.effective_identity.is_some()),
        "card.effective_identity",
    );
    flag(
        public().any(|card| card.characteristics.base_pt_until_end_of_turn.is_some()),
        "card.base_pt_until_end_of_turn",
    );
    flag(
        public().any(|card| card.characteristics.legend_rules.is_some()),
        "card.legend_rules",
    );
    flag(
        public().any(|card| card.characteristics.legend_return_sources.is_some()),
        "card.legend_return_sources",
    );
    flag(
        public().any(|card| card.counters.oil != 0),
        "card.oil_counters",
    );
    flag(
        public().any(|card| card.counters.charge != 0),
        "card.charge_counters",
    );
    flag(
        public().any(|card| card.counters.net != 0),
        "card.net_counters",
    );
    let engine = &s.engine_context;
    let activation = engine.pending_activation.as_ref();
    flag(
        activation.is_some_and(|pending| pending.crew_finished),
        "pending_activation.crew_finished",
    );
    flag(
        activation.is_some_and(|pending| pending.loyalty_x.is_some()),
        "pending_activation.loyalty_x",
    );
    flag(
        activation.is_some_and(|pending| pending.granted_ability.is_some()),
        "pending_activation.granted_ability",
    );
    flag(
        engine
            .pending_triggers
            .iter()
            .any(|pending| pending.counter_distribution.is_some()),
        "pending_trigger.counter_distribution",
    );
    flag(
        engine
            .pending_triggers
            .iter()
            .any(|pending| pending.counter_transfer.is_some()),
        "pending_trigger.counter_transfer",
    );
    flag(
        engine.pending_spell_copy.as_ref().is_some_and(|pending| {
            matches!(pending.inherited_target, TargetRefV1::StackItem { .. })
        }),
        "pending_spell_copy.stack_item_target",
    );
    flag(
        engine.pending_effect.as_ref().is_some_and(|pending| {
            matches!(
                &pending.choice,
                Some(PendingEffectChoiceSemanticV4::Options {
                    creature_options: Some(_),
                    ..
                })
            )
        }),
        "pending_effect.creature_options",
    );
    flag(
        engine.pending_discard.as_ref().is_some_and(|pending| {
            matches!(
                pending.resume_stage,
                DiscardResumeSemanticV2::TimeStopCleanupV68
            )
        }),
        "pending_discard.time_stop_cleanup",
    );
    // `build_relations` and `push_context_target`: a target that is a stack
    // item (a counterspell's target, for one) has no reference kind.
    let stack_item = |targets: &[TargetRefV1]| {
        targets
            .iter()
            .any(|target| matches!(target, TargetRefV1::StackItem { .. }))
    };
    flag(
        s.stack.iter().any(|item| stack_item(&item.targets)),
        "stack.stack_item_target",
    );
    flag(
        engine
            .pending_cast
            .as_ref()
            .is_some_and(|pending| stack_item(&pending.chosen_targets)),
        "pending_cast.stack_item_target",
    );
    flag(
        activation.is_some_and(|pending| stack_item(&pending.chosen_targets)),
        "pending_activation.stack_item_target",
    );
    flag(
        engine.pending_effect.as_ref().is_some_and(|pending| {
            matches!(
                &pending.choice,
                Some(PendingEffectChoiceSemanticV4::Targets {
                    selected_targets,
                    legal_targets,
                    ..
                }) if stack_item(selected_targets) || stack_item(legal_targets)
            )
        }),
        "pending_effect.stack_item_target",
    );
    fields
}

/// Census detail for a selection error, added to pass A's row: the refused
/// observation fields and V7 extension error for an `ObservationContract`
/// stop, and, when the diagnosis names the raw observation (which the
/// session reports as `CorruptCurrentBinding`), that observation's own
/// refusal, rebuilt on a fresh session surface. Read only; a panic here is
/// recorded on the row, not on the game.
pub(super) fn census_stop_detail_v1(
    session: &FastActorSessionV1,
    d: FastActorDecisionV1,
    stage: &str,
    diagnosis: &str,
    row: &mut Value,
) {
    let player = crate::kernel_native_search_opponent_v1::player_id_v1(d.acting_player);
    let detail = catch_unwind(AssertUnwindSafe(|| {
        let mut detail = json!({});
        if stage == "observation_contract" {
            match session.flat_policy_observation_v4(d) {
                Ok(observation) => {
                    detail["observation_fields"] =
                        json!(refused_observation_fields_v1(&observation));
                    detail["v7_extensions_error"] =
                        match crate::policy_observation_v7::policy_observation_extensions_v7(
                            session.game_state(),
                            player,
                        ) {
                            Ok(_) => Value::Null,
                            Err(error) => json!(error.0),
                        };
                }
                Err(error) => detail["observation_getter_error"] = json!(format!("{error:?}")),
            }
        }
        if diagnosis.starts_with("observation ") {
            let surface = crate::policy_surface_v5::PolicySurfaceV5::new_for_session();
            detail["observation_raw_error"] =
                match crate::rl::observe_policy_v6_unhashed_for_flat_policy(
                    session.game_state(),
                    &surface,
                    player,
                    d.step,
                    d.physical_decision_id,
                    d.substep_index,
                    d.substep_count,
                ) {
                    Ok(_) => json!("unlocated: a fresh surface builds the observation"),
                    Err(error) => json!(error.0),
                };
        }
        detail
    }))
    .unwrap_or_else(|payload| json!({"census_detail_panic": panic_message(&*payload)}));
    if let (Some(row), Value::Object(detail)) = (row.as_object_mut(), detail) {
        row.extend(detail);
    }
}

/// The inner error of a stopped pass A result (module doc: stop rows).
fn inner_error_v1(pass: &Value) -> String {
    let text = |key: &str| pass[key].as_str().unwrap_or("").to_owned();
    let error = text("error");
    let diagnosis = text("diagnosis");
    match pass["class"].as_str().unwrap_or("") {
        "encoder_refusal" => match pass["stage"].as_str().unwrap_or("") {
            "observation_contract" => {
                let fields: Vec<&str> = pass["observation_fields"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect();
                if !fields.is_empty() {
                    format!("observation_contract fields: {}", fields.join(", "))
                } else if let Some(extension) = pass["v7_extensions_error"].as_str() {
                    format!("observation_contract v7_extensions: {extension}")
                } else if let Some(getter) = pass["observation_getter_error"].as_str() {
                    format!("observation_contract getter: {getter}")
                } else {
                    format!(
                        "observation_contract unlocated (none of the mirrored predicates;                          object registration, extensions or relations): {diagnosis}"
                    )
                }
            }
            "tensorization" => error,
            _ => match pass["observation_raw_error"].as_str() {
                Some(raw) => format!("observation_raw: {raw}"),
                None if diagnosis.starts_with("unlocated") => format!("{error} ({diagnosis})"),
                None => diagnosis,
            },
        },
        "non_natural" => pass["terminal"]["reason"].as_str().unwrap_or("").to_owned(),
        "panic" => text("message"),
        _ => error,
    }
}

/// The grouping form of an inner error: `{..}` bodies dropped, each run of
/// digits folded to one `#` (so index 9 and index 10 group together),
/// whitespace collapsed, at most `KEY_CHAR_CAP` characters.
fn fold_inner_error_v1(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut depth = 0usize;
    for c in text.chars() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            _ if c.is_ascii_digit() => {
                if !out.ends_with('#') {
                    out.push('#');
                }
            }
            _ => out.push(c),
        }
    }
    out.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(KEY_CHAR_CAP)
        .collect()
}

/// One census row for a game's pass A result, `None` for a natural game.
fn census_stop_row_v1(index: usize, seed: u64, decks: [&str; 2], pass: &Value) -> Option<Value> {
    let kind = pass["class"].as_str().unwrap_or("unknown");
    if kind == "natural" {
        return None;
    }
    let inner = inner_error_v1(pass);
    Some(json!({
        "probe": "cube_census_stop",
        "index": index,
        "seed": seed,
        "decks": decks,
        "kind": kind,
        "stage": pass["stage"].as_str().unwrap_or(""),
        "decision_kind": pass["decision_kind"].as_str().unwrap_or(""),
        "stop_decision_index": pass["stop_decision_index"],
        "inner_error_key": fold_inner_error_v1(&inner),
        "inner_error": inner,
        "error": pass["error"],
        "diagnosis": pass["diagnosis"],
        "cards": pass["cards"].as_array().cloned().unwrap_or_default(),
    }))
}

/// The census: outcome counts over every game and the stop rows grouped by
/// kind, stage, decision kind and folded inner error, largest group first,
/// each with the seed and index of its first game (rows must be in game
/// order) and the digest of the rows.
fn census_json_v1(
    header: Value,
    outcomes: &BTreeMap<String, u64>,
    games: usize,
    rows: &[Value],
) -> Value {
    struct Group<'a> {
        games: u64,
        first: &'a Value,
        cards: BTreeSet<String>,
    }
    let mut groups: BTreeMap<[String; 4], Group> = BTreeMap::new();
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    let mut digest = Sha256::new();
    for (position, row) in rows.iter().enumerate() {
        if position > 0 {
            digest.update(b"\n");
        }
        digest.update(row.to_string().as_bytes());
        let field = |key: &str| row[key].as_str().unwrap_or("").to_owned();
        *kinds.entry(field("kind")).or_default() += 1;
        let group = groups
            .entry([
                field("kind"),
                field("stage"),
                field("decision_kind"),
                field("inner_error_key"),
            ])
            .or_insert(Group {
                games: 0,
                first: row,
                cards: BTreeSet::new(),
            });
        group.games += 1;
        for card in row["cards"].as_array().into_iter().flatten() {
            if group.cards.len() < GROUP_CARD_CAP {
                group.cards.insert(card.as_str().unwrap_or("").to_owned());
            }
        }
    }
    let mut stops: Vec<Value> = groups
        .into_iter()
        .map(|([kind, stage, decision_kind, key], group)| {
            json!({
                "kind": kind,
                "stage": stage,
                "decision_kind": decision_kind,
                "inner_error": key,
                "games": group.games,
                "replay_seed": group.first["seed"],
                "replay_index": group.first["index"],
                "example_inner_error": group.first["inner_error"],
                "example_error": group.first["error"],
                "example_diagnosis": group.first["diagnosis"],
                "cards": group.cards,
            })
        })
        .collect();
    stops.sort_by(|a, b| b["games"].as_u64().cmp(&a["games"].as_u64()));
    let mut census = json!({
        "schema": "cube_coverage_census/v1",
        "games": games,
        "outcomes": outcomes,
        "stopped_games": rows.len(),
        "stop_kinds": kinds,
        "stops": stops,
        "stop_rows_sha256": hex(&digest.finalize()),
    });
    if let (Some(census), Value::Object(header)) = (census.as_object_mut(), header) {
        census.extend(header);
    }
    census
}

/// Writes the census as pretty JSON and a newline, through a temporary
/// sibling file renamed into place.
fn write_census_v1(path: &std::path::Path, census: &Value) -> std::io::Result<()> {
    let mut bytes = serde_json::to_vec_pretty(census).map_err(std::io::Error::other)?;
    bytes.push(b'\n');
    let temporary = path.with_extension("json.partial");
    std::fs::write(&temporary, &bytes)?;
    std::fs::rename(&temporary, path)
}

#[cfg(feature = "eternal-fixtures")]
fn build_lookup(name: &str) -> Option<(u16, bool)> {
    let id = crate::card_def::card_id_by_name(name)?;
    let land = crate::card_def::CARD_DEFS.get(usize::from(id))?.is_land;
    Some((id, land))
}

#[cfg(feature = "eternal-fixtures")]
fn repo_file(relative: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative);
    std::fs::read(&path).unwrap_or_else(|error| panic!("read {relative}: {error}"))
}

/// The real pool and its input identities (fence from the environment).
#[cfg(feature = "eternal-fixtures")]
fn load_cube_pool_v1() -> (CubePoolV1, Value) {
    let digest = |bytes: &[u8]| hex(&Sha256::digest(bytes));
    let cube = repo_file(CUBE_LIST);
    let gaps = repo_file(CUBE_GAPS);
    let (fence, fence_identity) = match std::env::var("MTG_KERNEL_CUBE_FENCE") {
        Ok(path) => {
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|error| panic!("read MTG_KERNEL_CUBE_FENCE: {error}"));
            let names = fence_names(&serde_json::from_slice(&bytes).unwrap())
                .unwrap_or_else(|error| panic!("MTG_KERNEL_CUBE_FENCE: {error}"));
            let identity = json!({"sha256": digest(&bytes), "names": names.len()});
            (names, identity)
        }
        Err(_) => (BTreeSet::new(), Value::Null),
    };
    let pool = cube_pool_v1(
        &serde_json::from_slice(&cube).unwrap(),
        &serde_json::from_slice(&gaps).unwrap(),
        &fence,
        &build_lookup,
    )
    .unwrap_or_else(|error| panic!("cube pool: {error}"));
    let inputs = json!({
        "cube_list": {"path": CUBE_LIST, "sha256": digest(&cube)},
        "gap_record": {"path": CUBE_GAPS, "sha256": digest(&gaps)},
        "fence": fence_identity,
    });
    (pool, inputs)
}

#[cfg(feature = "eternal-fixtures")]
fn card_names(ids: &[u16]) -> Vec<&'static str> {
    ids.iter()
        .map(|&id| crate::card_def::CARD_DEFS[usize::from(id)].name)
        .collect()
}

#[test]
#[cfg(feature = "eternal-fixtures")]
#[ignore = "needs T1's episode deck_format: set CUBE_DECK_FORMAT, then run with --ignored"]
fn cube_coverage_census_v1() {
    let env_u64 = |name: &str| {
        std::env::var(name).ok().map(|v| {
            v.parse::<u64>()
                .unwrap_or_else(|_| panic!("{name} must be an unsigned integer"))
        })
    };
    let (pool, inputs) = load_cube_pool_v1();
    let games = env_usize("MTG_KERNEL_CUBE_CENSUS_GAMES", 4);
    assert!(games > 0, "MTG_KERNEL_CUBE_CENSUS_GAMES must be at least 1");
    let first_seed = env_u64("MTG_KERNEL_CUBE_CENSUS_FIRST_SEED").unwrap_or(SEED_BASE_CUBE);
    let replay = env_u64("MTG_KERNEL_CUBE_CENSUS_REPLAY_SEED");
    let threads = env_usize("MTG_KERNEL_CUBE_CENSUS_THREADS", 1).max(1);
    let runtime_rules = std::env::var("MTG_KERNEL_CUBE_CENSUS_RUNTIME_RULES")
        .ok()
        .filter(|rules| rules != "none");
    let seeds: Vec<u64> = match replay {
        Some(seed) => vec![seed],
        None => (0..games as u64)
            .map(|i| first_seed.wrapping_add(i))
            .collect(),
    };
    // Deal every game before playing any: a deal the rule refuses stops the
    // census here, not halfway through.
    let mut dealt = Vec::with_capacity(seeds.len());
    for &seed in &seeds {
        let decks = deal_pair_v1(&pool, seed).unwrap_or_else(|error| panic!("deal: {error}"));
        check_deal_v1(&pool, &decks).unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        let lists = [0, 1].map(|seat| deck_list(&decks[seat], &pool, seed, seat));
        let episode: ExpandedEpisodeV1 = serde_json::from_value(census_episode_json_v1(
            seed,
            [&lists[0], &lists[1]],
            runtime_rules.as_deref(),
        ))
        .unwrap_or_else(|error| panic!("census episode does not deserialize: {error}"));
        dealt.push((seed, decks, episode));
    }
    if CUBE_DECK_FORMAT.is_none() {
        println!(
            "{}",
            json!({"probe": "cube_census_warning",
                "warning": "CUBE_DECK_FORMAT is unset: every game stops at 60-card admission"})
        );
    }
    let base = FrozenPlayPolicyV1::training_fixture_v4()
        .into_flat_v7_transfer_v1()
        .unwrap();
    let started = Instant::now();
    let mut results: Vec<(usize, Value)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                let mut policy = base.fork_for_collection_v3().unwrap();
                let spare = base.fork_for_collection_v3().unwrap();
                let dealt = &dealt;
                scope.spawn(move || {
                    let mut out = Vec::new();
                    for index in (t..dealt.len()).step_by(threads) {
                        let episode = &dealt[index].2;
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            pass_a(&mut policy, episode, Select::Uniform, true)
                        }));
                        let pass = result.unwrap_or_else(|payload| {
                            policy = spare.fork_for_collection_v3().unwrap();
                            json!({"class": "panic", "pass": "a",
                                "message": panic_message(&*payload)})
                        });
                        out.push((index, pass));
                    }
                    out
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect()
    });
    results.sort_by_key(|(index, _)| *index);
    let mut outcomes: BTreeMap<String, u64> = BTreeMap::new();
    let mut rows = Vec::new();
    let mut panicked = Vec::new();
    for (index, pass) in &results {
        let (seed, decks, episode) = &dealt[*index];
        let class = pass["class"].as_str().unwrap_or("").to_owned();
        if class == "panic" {
            panicked.push(*seed);
        }
        *outcomes.entry(class).or_default() += 1;
        let labels = [
            episode.selected[0].label.as_str(),
            episode.selected[1].label.as_str(),
        ];
        if replay.is_some() {
            let deck_json = |deck: &DealtDeckV1| {
                json!({"colours": colour_letters(deck.colours),
                    "spells": card_names(&deck.spells),
                    "nonbasic_lands": card_names(&deck.lands),
                    "basics": deck.basics})
            };
            println!(
                "{}",
                json!({"probe": "cube_census_replay", "seed": seed,
                    "starting_player": episode.starting_player,
                    "decks": [deck_json(&decks[0]), deck_json(&decks[1])], "pass_a": pass})
            );
        }
        if let Some(row) = census_stop_row_v1(*index, *seed, labels, pass) {
            println!("{row}");
            rows.push(row);
        }
    }
    let header = json!({
        "inputs": inputs,
        "pool": pool.report,
        "deal_rule": {
            "colours_per_seat": 2,
            "colour_source": "cube list group; Multicolor left out",
            "nonland_spells": NONLAND_SPELLS,
            "nonbasic_lands": NONBASIC_LANDS,
            "basic_lands": BASIC_LANDS,
            "colourless_allowed": true,
            "no_cube_card_in_both_decks": true,
            "sideboard": "empty",
            "deal_stream": format!("{DEAL_STREAM:016x}"),
        },
        "deck_format": CUBE_DECK_FORMAT,
        "first_seed": seeds[0],
        "replay_seed": replay,
        "threads": threads,
        "select": "uniform",
        "runtime_rules": runtime_rules,
        "opening": "london-v1",
        "feature_contract_digest": base.feature_identity_v1().feature_contract_digest,
        "card_db_hash": format!("{KERNEL_CARDDB_HASH:016x}"),
        "panicked_seeds": panicked,
        "seconds": (started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0,
    });
    let census = census_json_v1(header, &outcomes, results.len(), &rows);
    println!("{census}");
    if let Ok(path) = std::env::var("MTG_KERNEL_CUBE_CENSUS_OUT") {
        write_census_v1(std::path::Path::new(&path), &census).expect("write the census");
    }
    assert_eq!(results.len(), seeds.len(), "every game reports");
    assert!(
        panicked.is_empty(),
        "{} game(s) panicked: {panicked:?}",
        panicked.len()
    );
}

/// The real cube list and gap record give a pool every name of which
/// resolves in this build, and 200 seeds deal decks that keep the rule.
#[test]
#[cfg(feature = "eternal-fixtures")]
fn cube_list_deals_under_the_rule_v1() {
    let (pool, _) = load_cube_pool_v1();
    assert!(pool.report["pool_nonland_spells"].as_u64().unwrap() > 300);
    for i in 0..200 {
        let seed = SEED_BASE_CUBE.wrapping_add(i);
        let decks = deal_pair_v1(&pool, seed).unwrap();
        check_deal_v1(&pool, &decks).unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        for id in decks.iter().flat_map(|d| d.spells.iter().chain(&d.lands)) {
            let def = &crate::card_def::CARD_DEFS[usize::from(*id)];
            assert_eq!(
                def.capability,
                crate::card_def::CardCapability::Full,
                "{}",
                def.name
            );
        }
    }
}

/// A small synthetic cube: `per` cards in each of the 17 groups, every
/// `land_every`-th of them a land, and the five basics on ids 1 to 5.
fn synthetic_cube(per: usize, land_every: usize) -> (Value, BTreeMap<String, (u16, bool)>) {
    let groups = [
        "White",
        "Blue",
        "Black",
        "Red",
        "Green",
        "Colorless",
        "Azorius",
        "Dimir",
        "Rakdos",
        "Gruul",
        "Selesnya",
        "Orzhov",
        "Izzet",
        "Golgari",
        "Boros",
        "Simic",
        "Multicolor",
    ];
    let mut cards = Vec::new();
    let mut ids = BTreeMap::new();
    let mut next = 1000u16;
    for group in groups {
        for k in 0..per {
            let name = format!("{group} {k}");
            ids.insert(name.clone(), (next, k % land_every == land_every - 1));
            next += 1;
            cards.push(json!({"name": name, "group": group}));
        }
    }
    for (offset, basic) in BASIC_NAMES.iter().enumerate() {
        ids.insert((*basic).to_owned(), (offset as u16 + 1, true));
    }
    (json!({"schema": "cube_list/v1", "cards": cards}), ids)
}

fn empty_gaps() -> Value {
    json!({"schema": "cube_coverage_gaps/v1", "partial": [], "fdn_only": [], "missing": []})
}

#[test]
fn cube_pool_applies_gaps_fence_and_refusals_v1() {
    let (cube, ids) = synthetic_cube(4, 4);
    let lookup = |name: &str| ids.get(name).copied();
    let gaps = json!({"schema": "cube_coverage_gaps/v1",
        "partial": ["White 0"], "fdn_only": [], "missing": ["Blue 1"]});
    let fence: BTreeSet<String> = ["Red 2".to_owned(), "Not A Cube Card".to_owned()].into();
    let pool = cube_pool_v1(&cube, &gaps, &fence, &lookup).unwrap();
    let names: BTreeSet<&str> = pool.cards.iter().map(|c| c.name.as_str()).collect();
    for gone in ["White 0", "Blue 1", "Red 2", "Multicolor 0"] {
        assert!(!names.contains(gone), "{gone} must not be in the pool");
    }
    assert_eq!(pool.cards.len(), 16 * 4 - 3);
    assert_eq!(pool.report["removed_by_gap_record"], 2);
    assert_eq!(pool.report["removed_by_fence"], 1);
    assert_eq!(pool.report["multicolor_group_left_out"], 4);
    assert_eq!(
        pool.report["fence_names_not_in_cube"],
        json!(["Not A Cube Card"])
    );
    let card = |name: &str| pool.cards.iter().find(|c| c.name == name).unwrap();
    assert_eq!(card("Izzet 0").colours, 0b01010);
    assert_eq!(card("Colorless 0").colours, 0);
    assert!(card("Green 3").land && !card("Green 2").land);
    assert_eq!(pool.basics, [1, 2, 3, 4, 5]);

    // An unresolved name, an unknown group, a gap name the cube lacks, a
    // card listed twice, two names on one id, and a fence that is not a list
    // are each refused.
    let missing_one = |name: &str| {
        if name == "Gruul 1" {
            None
        } else {
            lookup(name)
        }
    };
    let error = cube_pool_v1(&cube, &empty_gaps(), &BTreeSet::new(), &missing_one).unwrap_err();
    assert!(error.contains("Gruul 1"), "{error}");
    let mut odd = cube.clone();
    odd["cards"][0]["group"] = json!("Purple");
    assert!(cube_pool_v1(&odd, &empty_gaps(), &BTreeSet::new(), &lookup)
        .unwrap_err()
        .contains("Purple"));
    let stray = json!({"schema": "cube_coverage_gaps/v1",
        "partial": ["Elsewhere"], "fdn_only": [], "missing": []});
    assert!(cube_pool_v1(&cube, &stray, &BTreeSet::new(), &lookup)
        .unwrap_err()
        .contains("Elsewhere"));
    let mut twice = cube.clone();
    twice["cards"][1]["name"] = twice["cards"][0]["name"].clone();
    assert!(
        cube_pool_v1(&twice, &empty_gaps(), &BTreeSet::new(), &lookup)
            .unwrap_err()
            .contains("twice")
    );
    let shared = |name: &str| lookup(name).map(|(id, land)| (id.min(1001), land));
    assert!(
        cube_pool_v1(&cube, &empty_gaps(), &BTreeSet::new(), &shared)
            .unwrap_err()
            .contains("card id")
    );
    assert!(cube_pool_v1(&cube, &json!({"schema": "x"}), &BTreeSet::new(), &lookup).is_err());
    assert_eq!(
        fence_names(&json!({"cards": ["A", "B"]})).unwrap(),
        fence_names(&json!(["B", "A"])).unwrap()
    );
    assert!(fence_names(&json!({"cards": [1]})).is_err());
}

#[test]
fn cube_deal_keeps_the_rule_and_replays_from_its_seed_v1() {
    let (cube, ids) = synthetic_cube(12, 3);
    let lookup = |name: &str| ids.get(name).copied();
    let pool = cube_pool_v1(&cube, &empty_gaps(), &BTreeSet::new(), &lookup).unwrap();
    let mut pairs_seen = BTreeSet::new();
    for i in 0..64 {
        let seed = SEED_BASE_CUBE.wrapping_add(i);
        let decks = deal_pair_v1(&pool, seed).unwrap();
        check_deal_v1(&pool, &decks).unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        assert_eq!(
            decks,
            deal_pair_v1(&pool, seed).unwrap(),
            "same seed, same deal"
        );
        for (seat, deck) in decks.iter().enumerate() {
            pairs_seen.insert(deck.colours);
            let mainboard = deck.mainboard(&pool.basics);
            assert_eq!(mainboard.len(), 40);
            assert!(mainboard.windows(2).all(|w| w[0] <= w[1]), "sorted");
            let basics = mainboard.iter().filter(|&&id| id <= 5).count();
            assert_eq!(basics, BASIC_LANDS);
            let mut split: Vec<u8> = deck.basics.iter().copied().filter(|&n| n > 0).collect();
            split.sort_unstable();
            assert_eq!(split, [7, 8]);
            let label = deck.label(seed, seat);
            assert!(label.ends_with(&colour_letters(deck.colours)) && label.len() <= 64);
        }
    }
    assert!(pairs_seen.len() >= 8, "colour pairs vary with the seed");
    assert_ne!(
        deal_pair_v1(&pool, 1).unwrap(),
        deal_pair_v1(&pool, 2).unwrap()
    );

    // The checker refuses each broken clause.
    let decks = deal_pair_v1(&pool, 7).unwrap();
    let mut shared = decks.clone();
    shared[1].spells[0] = shared[0].spells[0];
    assert!(check_deal_v1(&pool, &shared).is_err());
    let mut off_colour = decks.clone();
    let outside = pool
        .cards
        .iter()
        .find(|c| !c.land && c.colours & !decks[0].colours != 0)
        .unwrap();
    off_colour[0].spells[0] = outside.id;
    assert!(check_deal_v1(&pool, &off_colour).is_err());
    let mut short = decks.clone();
    short[0].basics = [0; 5];
    assert!(check_deal_v1(&pool, &short).is_err());
    let mut swapped = decks.clone();
    for count in swapped[0].basics.iter_mut().filter(|n| **n > 0) {
        *count = 15 - *count;
    }
    assert!(
        check_deal_v1(&pool, &swapped).is_err(),
        "8 on the wrong colour"
    );
    let mut lopsided = decks.clone();
    let colour = (0..5)
        .find(|bit| decks[0].colours & (1 << bit) != 0)
        .unwrap();
    lopsided[0].basics = [0; 5];
    lopsided[0].basics[colour] = 15;
    assert!(check_deal_v1(&pool, &lopsided).is_err(), "15 of one colour");

    // Too few lands for any pair: refused, not short-dealt.
    let (thin, thin_ids) = synthetic_cube(3, 100);
    let thin_lookup = |name: &str| thin_ids.get(name).copied();
    let thin_pool = cube_pool_v1(&thin, &empty_gaps(), &BTreeSet::new(), &thin_lookup).unwrap();
    assert!(deal_pair_v1(&thin_pool, 1).is_err());

    // The census episode is the trainer's wire form under the switch as set.
    let lists = [0, 1].map(|seat| deck_list(&decks[seat], &pool, 7, seat));
    let json = census_episode_json_v1(7, [&lists[0], &lists[1]], Some("loss-terminal-v1"));
    assert_eq!(
        json.get("deck_format").is_some(),
        CUBE_DECK_FORMAT.is_some()
    );
    let episode: ExpandedEpisodeV1 = serde_json::from_value(json).unwrap();
    assert_eq!(episode.selected, lists);
    assert_eq!(episode.registered, lists);
    assert_eq!(episode.starting_player, 1);
    assert_eq!(episode.opening, Some(PauperOpeningV1::LondonV1));
    assert_eq!(
        episode.runtime_rules,
        Some(EpisodeRuntimeRulesV1::LossTerminalV1)
    );
    assert!(episode.selected[0].sideboard.is_empty());
}

#[test]
fn cube_census_groups_inner_errors_and_writes_v1() {
    let refusal = |diagnosis: &str, card: &str| {
        json!({"class": "encoder_refusal", "stage": "action_slice",
            "decision_kind": "ChooseAction", "stop_decision_index": 40,
            "error": "V4 actor-visible encoding: Action(UnsupportedActionSemantic)",
            "diagnosis": diagnosis, "cards": [card]})
    };
    let passes = [
        json!({"class": "natural", "terminal": {"reason": "life"}}),
        refusal(
            "action_core UnsupportedActionSemantic at 3: ChooseAttackTarget { object: 17 }",
            "Karn",
        ),
        json!({"class": "non_natural", "terminal": {"reason": "fail_closed:extra turns 2"}}),
        refusal(
            "action_core UnsupportedActionSemantic at 11: ChooseAttackTarget { object: 4 }",
            "Jace",
        ),
        json!({"class": "encoder_refusal", "stage": "observation_contract",
            "decision_kind": "ChooseTargets", "error": "V4 actor-visible encoding: ObservationContract",
            "diagnosis": "unlocated (the per-reference checks pass; a cross-action check failed)",
            "observation_fields": ["card.charge_counters"]}),
        json!({"class": "encoder_refusal", "stage": "action_slice",
            "decision_kind": "ChooseAction",
            "error": "V4 actor-visible encoding: Action(CorruptCurrentBinding)",
            "diagnosis": "observation CorruptCurrentBinding",
            "observation_raw_error": "life-payment Ward requires its successor"}),
        json!({"class": "configuration_refusal", "stop_decision_index": null,
            "error": "registered deck seat 0 must contain exactly 60 cards"}),
        json!({"class": "panic", "pass": "a", "message": "index 9 out of range"}),
    ];
    let mut outcomes: BTreeMap<String, u64> = BTreeMap::new();
    let mut rows = Vec::new();
    for (index, pass) in passes.iter().enumerate() {
        *outcomes
            .entry(pass["class"].as_str().unwrap().to_owned())
            .or_default() += 1;
        if let Some(row) = census_stop_row_v1(index, 500 + index as u64, ["a", "b"], pass) {
            rows.push(row);
        }
    }
    assert_eq!(rows.len(), passes.len() - 1, "a natural game is not a stop");
    let inner: Vec<&str> = rows
        .iter()
        .map(|r| r["inner_error"].as_str().unwrap())
        .collect();
    assert_eq!(
        inner[3],
        "observation_contract fields: card.charge_counters"
    );
    assert_eq!(
        inner[4],
        "observation_raw: life-payment Ward requires its successor"
    );
    assert_eq!(inner[1], "fail_closed:extra turns 2");
    assert_eq!(inner[6], "index 9 out of range");
    assert_eq!(
        fold_inner_error_v1("a 10 at 9: X { y: 3 } z"),
        fold_inner_error_v1("a 7 at 123: X { q } z")
    );
    assert_eq!(
        fold_inner_error_v1("a 10 at 9: X { y: 3 } z"),
        "a # at #: X z"
    );
    let header = json!({"deck_format": CUBE_DECK_FORMAT, "first_seed": 500});
    let census = census_json_v1(header, &outcomes, passes.len(), &rows);
    assert_eq!(census["games"], 8);
    assert_eq!(census["stopped_games"], 7);
    assert_eq!(census["outcomes"]["natural"], 1);
    assert_eq!(census["first_seed"], 500);
    let stops = census["stops"].as_array().unwrap();
    assert_eq!(stops.len(), 6, "the two attack-target rows fold together");
    assert_eq!(stops[0]["games"], 2);
    assert_eq!(
        stops[0]["inner_error"],
        "action_core UnsupportedActionSemantic at #: ChooseAttackTarget"
    );
    assert_eq!(stops[0]["replay_seed"], 501, "the group's first game");
    assert_eq!(stops[0]["cards"], json!(["Jace", "Karn"]));
    assert_eq!(
        stops
            .iter()
            .map(|s| s["games"].as_u64().unwrap())
            .sum::<u64>(),
        7
    );
    assert_eq!(
        census_json_v1(json!({}), &outcomes, passes.len(), &rows)["stop_rows_sha256"],
        census["stop_rows_sha256"]
    );

    let path = std::env::temp_dir().join(format!(
        "cube-census-writer-{}-{:x}.json",
        std::process::id(),
        Sha256::digest(census.to_string().as_bytes())[0]
    ));
    write_census_v1(&path, &census).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(bytes.last(), Some(&b'\n'));
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), census);
    assert!(!path.with_extension("json.partial").exists());
}
