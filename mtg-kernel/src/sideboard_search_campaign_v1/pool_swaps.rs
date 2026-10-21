//! B0 pool swaps: a W5 extension that enumerates 1 to 4 card mainboard swaps
//! drawn from the supported pool rather than only the registered 75
//! (collab `LANES/brewing-b0-20261020/DESIGN.md`, section 4). Enumerates and
//! registers only; it never scores.
//!
//! Registration rule: an in-card copy still in the registered sideboard moves
//! to the mainboard and one out-card copy takes its sideboard place, so the 75
//! is conserved (W5 plan semantics). Any other in-card copy is new to the 75
//! and its out-card copy leaves the 75. Every result must pass
//! `RegisteredDeckV1::new_executable_v1` (60 + 15, four copies across the
//! whole 75, basics exempt); illegal swaps are skipped, never repaired.

use crate::sideboard::{mainboard_slice_sha256_v1, CardCountV1, RegisteredDeckV1};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const POOL_SWAP_CANDIDATES_SCHEMA_V1: &str = "kernel-b0-pool-swap-candidates/v1";
/// Duplicate decks are merged by this mainboard hash, computed over the
/// ascending-sorted mainboard (the W5 trace hash convention).
pub const POOL_SWAP_HASH_CONVENTION_V1: &str = "deck_configuration_mainboard_sha256_v1-sorted";

/// One eligible in-card and its exposure label. `primary` means a retained
/// embedding update in every principal pilot (the caller derives it from the
/// checkpoint-bound audit; this module never infers exposure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolInCardV1 {
    pub card_id: u16,
    pub primary: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolSwapV1 {
    pub cards_out: Vec<CardCountV1>,
    pub cards_in: Vec<CardCountV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolSwapCandidateV1 {
    pub index: u32,
    pub label: String,
    pub swap: PoolSwapV1,
    pub primary: bool,
    pub mainboard: Vec<u16>,
    pub sideboard: Vec<u16>,
    pub mainboard_sha256: String,
}

fn hex_v1(digest: [u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn sorted_mainboard_sha256_v1(mainboard: &[u16]) -> String {
    let mut sorted = mainboard.to_vec();
    sorted.sort_unstable();
    hex_v1(mainboard_slice_sha256_v1(&sorted))
}

pub fn candidate_label_v1(index: u32) -> String {
    format!("B0/{index:04}")
}

fn expand_v1(rows: &[CardCountV1]) -> Vec<u16> {
    rows.iter()
        .flat_map(|row| std::iter::repeat_n(row.card_id, usize::from(row.count)))
        .collect()
}

/// Applies one swap to the registered 75 under the module's registration
/// rule and returns the executable registration, or the reason it is illegal.
pub fn apply_pool_swap_v1(
    registered: &RegisteredDeckV1,
    swap: &PoolSwapV1,
    label: &str,
) -> Result<RegisteredDeckV1, String> {
    let outs = expand_v1(&swap.cards_out);
    let ins = expand_v1(&swap.cards_in);
    if outs.is_empty() || outs.len() != ins.len() || outs.len() > 4 {
        return Err("a swap moves 1 to 4 copies out and the same number in".to_owned());
    }
    if swap
        .cards_out
        .iter()
        .any(|out| swap.cards_in.iter().any(|row| row.card_id == out.card_id))
    {
        return Err("a card cannot be swapped both out and in".to_owned());
    }
    let configuration = registered.registered_configuration();
    let mut mainboard = configuration.mainboard().to_vec();
    let mut sideboard = configuration.sideboard().to_vec();
    for &card_id in &outs {
        let position = mainboard
            .iter()
            .position(|&id| id == card_id)
            .ok_or_else(|| format!("card {card_id} has too few mainboard copies to remove"))?;
        mainboard.remove(position);
    }
    let mut moved_from_sideboard = 0usize;
    for &card_id in &ins {
        if let Some(position) = sideboard.iter().position(|&id| id == card_id) {
            sideboard.remove(position);
            moved_from_sideboard += 1;
        }
        mainboard.push(card_id);
    }
    sideboard.extend(outs.iter().take(moved_from_sideboard));
    mainboard.sort_unstable();
    sideboard.sort_unstable();
    RegisteredDeckV1::new_executable_v1(label.to_owned(), mainboard, sideboard)
        .map_err(|error| format!("{error:?}"))
}

fn candidate_v1(
    registered: &RegisteredDeckV1,
    swap: PoolSwapV1,
    index: u32,
    primary: bool,
) -> Option<PoolSwapCandidateV1> {
    let label = candidate_label_v1(index);
    let deck = apply_pool_swap_v1(registered, &swap, &label).ok()?;
    let configuration = deck.registered_configuration();
    Some(PoolSwapCandidateV1 {
        index,
        label,
        swap,
        primary,
        mainboard: configuration.mainboard().to_vec(),
        sideboard: configuration.sideboard().to_vec(),
        mainboard_sha256: sorted_mainboard_sha256_v1(configuration.mainboard()),
    })
}

/// M1: every legal "-1 X, +1 Y" with X a distinct registered mainboard card
/// and Y an eligible in-card other than X, in ascending (X id, Y id) order.
/// Indices run 0..N1 with no gaps; a duplicate mainboard keeps its earliest
/// index.
pub fn generate_pool_swap_singles_v1(
    registered: &RegisteredDeckV1,
    in_cards: &[PoolInCardV1],
) -> Vec<PoolSwapCandidateV1> {
    let mut outs: Vec<u16> = registered.registered_configuration().mainboard().to_vec();
    outs.sort_unstable();
    outs.dedup();
    let mut eligible: Vec<PoolInCardV1> = in_cards.to_vec();
    eligible.sort_unstable_by_key(|card| card.card_id);
    eligible.dedup_by_key(|card| card.card_id);
    let mut seen = BTreeSet::from([sorted_mainboard_sha256_v1(
        registered.registered_configuration().mainboard(),
    )]);
    let mut candidates = Vec::new();
    for &out in &outs {
        for card in &eligible {
            if card.card_id == out {
                continue;
            }
            let swap = PoolSwapV1 {
                cards_out: vec![CardCountV1 {
                    card_id: out,
                    count: 1,
                }],
                cards_in: vec![CardCountV1 {
                    card_id: card.card_id,
                    count: 1,
                }],
            };
            let index = candidates.len() as u32;
            if let Some(candidate) = candidate_v1(registered, swap, index, card.primary) {
                if seen.insert(candidate.mainboard_sha256.clone()) {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates
}

fn single_parts_v1(parent: &PoolSwapCandidateV1) -> (u16, u16) {
    assert!(
        parent.swap.cards_out.len() == 1
            && parent.swap.cards_in.len() == 1
            && parent.swap.cards_out[0].count == 1
            && parent.swap.cards_in[0].count == 1,
        "M2 parents must be M1 singles"
    );
    (
        parent.swap.cards_out[0].card_id,
        parent.swap.cards_in[0].card_id,
    )
}

fn union_swap_v1(parents: &[&PoolSwapCandidateV1]) -> PoolSwapV1 {
    let mut outs = BTreeMap::<u16, u8>::new();
    let mut ins = BTreeMap::<u16, u8>::new();
    for parent in parents {
        let (out, card_in) = single_parts_v1(parent);
        *outs.entry(out).or_default() += 1;
        *ins.entry(card_in).or_default() += 1;
    }
    let rows = |map: BTreeMap<u16, u8>| {
        map.into_iter()
            .map(|(card_id, count)| CardCountV1 { card_id, count })
            .collect()
    };
    PoolSwapV1 {
        cards_out: rows(outs),
        cards_in: rows(ins),
    }
}

/// M2: from up to 8 ranked primary M1 singles, in this order: k-copy versions
/// by parent rank then k = 2, 3, 4; pairwise unions in lexicographic order of
/// parent-rank pairs; triples of the top 4 in lexicographic order; then the
/// quadruple of the top 4. A union with a card both out and in, or any
/// illegal registration, is skipped. A mainboard equal to anything in
/// `seen_hashes` (all M1 singles and the incumbent) or to an earlier M2 entry
/// is dropped. Indices continue from `first_index`.
pub fn build_multi_swap_neighborhood_v1(
    registered: &RegisteredDeckV1,
    ranked_parents: &[PoolSwapCandidateV1],
    first_index: u32,
    seen_hashes: &BTreeSet<String>,
) -> Vec<PoolSwapCandidateV1> {
    let parents: Vec<&PoolSwapCandidateV1> = ranked_parents.iter().take(8).collect();
    assert!(
        parents.iter().all(|parent| parent.primary),
        "M2 parents must be primary"
    );
    let mut swaps: Vec<PoolSwapV1> = Vec::new();
    for parent in &parents {
        let (out, card_in) = single_parts_v1(parent);
        for k in 2..=4u8 {
            swaps.push(PoolSwapV1 {
                cards_out: vec![CardCountV1 {
                    card_id: out,
                    count: k,
                }],
                cards_in: vec![CardCountV1 {
                    card_id: card_in,
                    count: k,
                }],
            });
        }
    }
    for i in 0..parents.len() {
        for j in (i + 1)..parents.len() {
            swaps.push(union_swap_v1(&[parents[i], parents[j]]));
        }
    }
    let top = parents.len().min(4);
    for i in 0..top {
        for j in (i + 1)..top {
            for l in (j + 1)..top {
                swaps.push(union_swap_v1(&[parents[i], parents[j], parents[l]]));
            }
        }
    }
    if top == 4 {
        swaps.push(union_swap_v1(&parents[..4]));
    }
    let mut seen = seen_hashes.clone();
    let mut candidates = Vec::new();
    for swap in swaps {
        let index = first_index + candidates.len() as u32;
        if let Some(candidate) = candidate_v1(registered, swap, index, true) {
            if seen.insert(candidate.mainboard_sha256.clone()) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sideboard::checked_in_pauper_registered_deck_by_id_v1;

    fn id(name: &str) -> u16 {
        crate::card_def::CARD_DEFS
            .iter()
            .position(|card| card.name == name)
            .unwrap_or_else(|| panic!("{name} is not registered")) as u16
    }

    fn burn() -> RegisteredDeckV1 {
        checked_in_pauper_registered_deck_by_id_v1("Burn").unwrap()
    }

    fn count(cards: &[u16], card_id: u16) -> usize {
        cards.iter().filter(|&&id| id == card_id).count()
    }

    fn one(out: &str, card_in: &str) -> PoolSwapV1 {
        PoolSwapV1 {
            cards_out: vec![CardCountV1 {
                card_id: id(out),
                count: 1,
            }],
            cards_in: vec![CardCountV1 {
                card_id: id(card_in),
                count: 1,
            }],
        }
    }

    #[test]
    fn a_sideboard_in_card_moves_main_side_and_conserves_the_75() {
        let deck = apply_pool_swap_v1(&burn(), &one("Mountain", "Pyroblast"), "t").unwrap();
        let configuration = deck.registered_configuration();
        assert_eq!(configuration.mainboard().len(), 60);
        assert_eq!(configuration.sideboard().len(), 15);
        assert_eq!(count(configuration.mainboard(), id("Pyroblast")), 1);
        assert_eq!(count(configuration.sideboard(), id("Pyroblast")), 3);
        assert_eq!(count(configuration.mainboard(), id("Mountain")), 17);
        assert_eq!(count(configuration.sideboard(), id("Mountain")), 1);
        let mut before = burn().registered_configuration().combined_card_counts_v1();
        let mut after = configuration.combined_card_counts_v1();
        before.sort();
        after.sort();
        assert_eq!(before, after, "the registered 75 is conserved");
    }

    #[test]
    fn a_new_in_card_enters_the_mainboard_and_the_out_card_leaves_the_75() {
        let deck = apply_pool_swap_v1(&burn(), &one("Mountain", "Chain Lightning"), "t").unwrap();
        let configuration = deck.registered_configuration();
        assert_eq!(count(configuration.mainboard(), id("Chain Lightning")), 1);
        assert_eq!(count(configuration.mainboard(), id("Mountain")), 17);
        assert_eq!(
            configuration.sideboard(),
            burn().registered_configuration().sideboard()
        );
    }

    #[test]
    fn the_four_copy_limit_counts_the_whole_75() {
        // Bolt is already a mainboard 4-of; a fifth copy is illegal.
        assert!(apply_pool_swap_v1(&burn(), &one("Mountain", "Lightning Bolt"), "t").is_err());
        // Four new Searing Blaze copies would make eight across the 75 if the
        // sideboard copies did not move; the move rule keeps it legal.
        let swap = PoolSwapV1 {
            cards_out: vec![CardCountV1 {
                card_id: id("Mountain"),
                count: 4,
            }],
            cards_in: vec![CardCountV1 {
                card_id: id("Searing Blaze"),
                count: 4,
            }],
        };
        let deck = apply_pool_swap_v1(&burn(), &swap, "t").unwrap();
        assert_eq!(
            count(
                deck.registered_configuration().mainboard(),
                id("Searing Blaze")
            ),
            4
        );
        assert_eq!(
            count(
                deck.registered_configuration().sideboard(),
                id("Searing Blaze")
            ),
            0
        );
        assert_eq!(
            count(deck.registered_configuration().sideboard(), id("Mountain")),
            4
        );
        assert!(
            apply_pool_swap_v1(&burn(), &one("Lightning Bolt", "Lightning Bolt"), "t").is_err()
        );
    }

    #[test]
    fn singles_are_legal_ordered_unique_and_carry_their_exposure_label() {
        let in_cards = [
            PoolInCardV1 {
                card_id: id("Chain Lightning"),
                primary: true,
            },
            PoolInCardV1 {
                card_id: id("Pyroblast"),
                primary: false,
            },
            PoolInCardV1 {
                card_id: id("Lightning Bolt"),
                primary: true,
            },
            PoolInCardV1 {
                card_id: id("Faithless Looting"),
                primary: true,
            },
        ];
        let singles = generate_pool_swap_singles_v1(&burn(), &in_cards);
        let hashes: BTreeSet<_> = singles.iter().map(|c| c.mainboard_sha256.clone()).collect();
        assert_eq!(hashes.len(), singles.len());
        let mut last = (0u16, 0u16);
        for (index, candidate) in singles.iter().enumerate() {
            assert_eq!(candidate.index as usize, index);
            assert_eq!(candidate.label, candidate_label_v1(candidate.index));
            let key = single_parts_v1(candidate);
            assert!(index == 0 || key > last, "ascending (X id, Y id)");
            last = key;
            assert_ne!(
                key.1,
                id("Lightning Bolt"),
                "a fifth Bolt is never generated"
            );
            assert_eq!(candidate.primary, key.1 != id("Pyroblast"));
            RegisteredDeckV1::new_executable_v1(
                candidate.label.clone(),
                candidate.mainboard.clone(),
                candidate.sideboard.clone(),
            )
            .unwrap();
        }
        // 12 distinct mainboard cards; Looting (2 copies) can be added except
        // in place of itself; Chain Lightning and Pyroblast in place of any.
        assert_eq!(singles.len(), 11 + 12 + 12);
    }

    /// Cross-check with the Python mirror (python/tests/test_deck_variant_analysis_v1.py uses the same
    /// fixture and expects the same bounds) at B0's alpha levels.
    #[test]
    fn paired_bootstrap_matches_the_python_mirror_at_b0_alpha_levels() {
        use crate::paired_bo1_harness_v1::{
            paired_bootstrap_ci_with_alpha_v1, BootstrapSidednessV1,
        };
        let deltas: [i8; 20] = [
            1, 0, -1, 1, 1, 0, 0, -1, 1, 0, 1, 0, 0, 1, -1, 0, 1, 1, 0, 0,
        ];
        let lower = |alpha: f64| {
            paired_bootstrap_ci_with_alpha_v1(
                &deltas,
                1000,
                0x5EED,
                BootstrapSidednessV1::OneSidedLower,
                alpha,
            )
        };
        assert_eq!(lower(0.05 / 3.0).lower, -0.1);
        assert_eq!(lower(0.05).lower, 0.0);
        assert_eq!(lower(0.05).mean, 0.25);
        let two = paired_bootstrap_ci_with_alpha_v1(
            &deltas,
            1000,
            0x5EED,
            BootstrapSidednessV1::TwoSided,
            0.05,
        );
        assert_eq!((two.lower, two.upper), (-0.05, 0.55));
    }

    #[test]
    fn multi_swaps_follow_the_traversal_skip_conflicts_and_drop_duplicates() {
        let in_cards = [
            PoolInCardV1 {
                card_id: id("Chain Lightning"),
                primary: true,
            },
            PoolInCardV1 {
                card_id: id("Faithless Looting"),
                primary: true,
            },
        ];
        let singles = generate_pool_swap_singles_v1(&burn(), &in_cards);
        let pick = |out: &str, card_in: &str| {
            singles
                .iter()
                .find(|c| single_parts_v1(c) == (id(out), id(card_in)))
                .unwrap()
                .clone()
        };
        let parents = vec![
            pick("Mountain", "Chain Lightning"),
            pick("Guttersnipe", "Faithless Looting"),
            pick("Mountain", "Faithless Looting"),
        ];
        let seen: BTreeSet<String> = singles.iter().map(|c| c.mainboard_sha256.clone()).collect();
        let first = singles.len() as u32;
        let multi = build_multi_swap_neighborhood_v1(&burn(), &parents, first, &seen);
        for (offset, candidate) in multi.iter().enumerate() {
            assert_eq!(candidate.index, first + offset as u32);
            assert!(!seen.contains(&candidate.mainboard_sha256));
        }
        // The first entry is "-2 Mountain, +2 Chain Lightning".
        assert_eq!(
            multi[0].swap.cards_out,
            vec![CardCountV1 {
                card_id: id("Mountain"),
                count: 2
            }]
        );
        // Looting goes 2 -> 4 at most: "-3/-4 X, +3/+4 Looting" are illegal and skipped.
        assert!(!multi.iter().any(|c| c
            .swap
            .cards_in
            .iter()
            .any(|row| row.card_id == id("Faithless Looting") && row.count >= 3)));
        // Parents 2 and 3 together add two Lootings (legal, 2 -> 4).
        assert!(multi.iter().any(|c| c.swap.cards_in
            == vec![CardCountV1 {
                card_id: id("Faithless Looting"),
                count: 2
            }]
            && c.swap.cards_out.len() == 2));
        let hashes: BTreeSet<_> = multi.iter().map(|c| c.mainboard_sha256.clone()).collect();
        assert_eq!(hashes.len(), multi.len());
    }
}
