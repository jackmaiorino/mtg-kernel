//! The Stage 4a lawful sampler (RUNNER.md section 1).
//!
//! One sample of the acting player's information set:
//! 1. the V4 whole-object determinization in `FutureChanceV3` mode: own and
//!    opponent unknown hand/library objects are shuffled within their owner,
//!    a pending library search keeps its offered cards (`plan_future_v3`),
//!    hidden objects referenced by triggers, stack items or other engine
//!    records make the sample fail (it is rejected, never pinned), and the
//!    future engine randomness is replaced by a fresh stream;
//! 2. the public deck prior: the opponent's unknown cards are replaced by a
//!    draw from one registered experiment deck, chosen uniformly among the
//!    decks consistent with the opponent's publicly observed card
//!    multiplicities. The true opponent deck is never read. Own registration
//!    is known, so the actor's own unknown cards keep the true remaining
//!    multiset (step 1).
//!
//! The V4 sampler then rebuilds the live candidates and fails the sample if
//! anything the actor observes changed. Every failure is a declared
//! rejection: the caller never retries the seed.
//!
//! This is an explicitly approximate sampling convention, not a posterior.

use crate::card_def::CARD_DEFS;
use crate::ids::{ObjectId, PlayerId};
use crate::rl_session::{FastActorSessionV1, V4SearchSampleMode, V4SearchStateErrorV1};
use crate::runtime_decks::RUNTIME_DECKS;
use crate::state::{GameState, ObjectStateV4, SplitMix64, Zone};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

pub(crate) const SAMPLER_VERSION: &str =
    "stage4a-sampler-v1: V4 FutureChanceV3 whole-object determinization + uniform public deck prior";

/// The registered experiment decks the prior ranges over.
#[derive(Clone, Debug)]
pub(crate) struct DeckPrior {
    /// Sorted card-definition multiset per deck.
    decks: Vec<Vec<u16>>,
    ids: Vec<&'static str>,
}

impl DeckPrior {
    pub(crate) fn new(deck_indices: &[usize]) -> Self {
        let mut decks = Vec::new();
        let mut ids = Vec::new();
        for &i in deck_indices {
            let mut cards = RUNTIME_DECKS[i].card_ids.to_vec();
            cards.sort_unstable();
            decks.push(cards);
            ids.push(RUNTIME_DECKS[i].id);
        }
        Self { decks, ids }
    }

    pub(crate) fn ids(&self) -> &[&'static str] {
        &self.ids
    }

    /// Decks whose multiset contains `observed` (sorted) and whose size is
    /// `total`.
    fn consistent(&self, observed: &[u16], total: usize) -> Vec<usize> {
        (0..self.decks.len())
            .filter(|&d| self.decks[d].len() == total && is_submultiset(observed, &self.decks[d]))
            .collect()
    }
}

/// Both sorted.
fn is_submultiset(small: &[u16], big: &[u16]) -> bool {
    let mut j = 0;
    for &x in small {
        while j < big.len() && big[j] < x {
            j += 1;
        }
        if j == big.len() || big[j] != x {
            return false;
        }
        j += 1;
    }
    true
}

/// `big` minus `small` (both sorted; `small` must be a submultiset).
fn difference(big: &[u16], small: &[u16]) -> Vec<u16> {
    let mut out = Vec::with_capacity(big.len().saturating_sub(small.len()));
    let mut j = 0;
    for &x in big {
        if j < small.len() && small[j] == x {
            j += 1;
        } else {
            out.push(x);
        }
    }
    out
}

/// Unbiased draw in [0, bound), as in the V4 sampler.
fn draw(rng: &mut SplitMix64, bound: u64) -> usize {
    let threshold = bound.wrapping_neg() % bound;
    loop {
        let value = rng.next_u64();
        if value >= threshold {
            return (value % bound) as usize;
        }
    }
}

/// The prior's own stream, separate from the hidden-pool shuffle and the
/// future-chance stream that the V4 sampler derives from the same seed.
fn prior_seed(seed: u64) -> u64 {
    let mut hash = Sha256::new();
    hash.update(b"mtg-kernel/stage4a-deck-prior/v1\0");
    hash.update(seed.to_le_bytes());
    let bytes = hash.finalize();
    u64::from_le_bytes(bytes[..8].try_into().expect("eight digest bytes"))
}

/// The opponent's unknown slots from `actor`'s view, in the V4 sampler's
/// canonical order (hand indices, then library indices).
fn unknown_slots(state: &GameState, actor: PlayerId, owner: PlayerId) -> Vec<ObjectId> {
    let p = &state.players[owner.index()];
    let mut slots = Vec::new();
    for &id in &p.hand {
        let generation = state.objects.get(id).zone_change_count;
        if !state
            .known_hand_cards(actor, owner)
            .iter()
            .any(|k| k.object == id && k.zone_change_count == generation)
        {
            slots.push(id);
        }
    }
    for (i, &id) in p.library.iter().enumerate() {
        let generation = state.objects.get(id).zone_change_count;
        if !state.known_library_cards(actor, owner).iter().any(|k| {
            k.position as usize == i && k.object == id && k.zone_change_count == generation
        }) {
            slots.push(id);
        }
    }
    slots
}

/// Opponent-owned physical cards outside the unknown slots: public zones and
/// cards the actor knows in the opponent's hand or library (sorted).
fn observed_cards(state: &GameState, owner: PlayerId, unknown: &HashSet<ObjectId>) -> Vec<u16> {
    let mut observed: Vec<u16> = state
        .objects
        .iter()
        .filter(|(id, o)| {
            o.owner == owner
                && !o.v4.is_token
                && o.spell_copy_origin.is_none()
                && !unknown.contains(id)
                && matches!(
                    o.zone,
                    Zone::Hand
                        | Zone::Library
                        | Zone::Battlefield
                        | Zone::Graveyard
                        | Zone::Exile
                        | Zone::Stack
                        | Zone::Command
                )
        })
        .map(|(_, o)| o.card_def)
        .collect();
    observed.sort_unstable();
    observed
}

/// The prior step on a sampled state. Returns the chosen deck's index into
/// the prior (diagnostic only; nothing downstream reads it).
pub(crate) fn apply_deck_prior(
    state: &mut GameState,
    actor: PlayerId,
    prior: &DeckPrior,
    seed: u64,
) -> Result<usize, String> {
    let opp = actor.opponent();
    let slots = unknown_slots(state, actor, opp);
    let unknown: HashSet<ObjectId> = slots.iter().copied().collect();
    if unknown.len() != slots.len() {
        return Err("duplicate unknown slot".into());
    }
    let observed = observed_cards(state, opp, &unknown);
    let total = observed.len() + slots.len();
    let consistent = prior.consistent(&observed, total);
    if consistent.is_empty() {
        return Err(format!(
            "no registered deck is consistent with {} observed of {total} cards",
            observed.len()
        ));
    }
    let mut rng = SplitMix64::seed(prior_seed(seed));
    let chosen = consistent[draw(&mut rng, consistent.len() as u64)];
    let mut remaining = difference(&prior.decks[chosen], &observed);
    if remaining.len() != slots.len() {
        return Err("prior remainder differs from the unknown slot count".into());
    }
    for i in (1..remaining.len()).rev() {
        let j = draw(&mut rng, (i + 1) as u64);
        remaining.swap(i, j);
    }
    for (&id, &def) in slots.iter().zip(&remaining) {
        let obj = state.objects.get_mut(id);
        obj.card_def = def;
        obj.name = CARD_DEFS[def as usize].name.to_string();
        obj.v4 = ObjectStateV4::from_card_def(def);
    }
    Ok(chosen)
}

/// A successful sample: the world and the prior's deck choice.
pub(crate) struct Sample {
    pub(crate) world: FastActorSessionV1,
    pub(crate) prior_deck: usize,
}

/// One lawful sample of the current actor's information set. `Err` is a
/// declared rejection with its kind; never retry the seed.
pub(crate) fn sample(
    session: &FastActorSessionV1,
    seed: u64,
    prior: &DeckPrior,
) -> Result<Sample, String> {
    let mut chosen = None;
    let mut hook_error = None;
    let world = session
        .kernel_search_redeterminized_clone_hooked_v4(
            seed,
            V4SearchSampleMode::FutureChanceV3,
            |state, actor| match apply_deck_prior(state, actor, prior, seed) {
                Ok(d) => {
                    chosen = Some(d);
                    Ok(())
                }
                Err(e) => {
                    hook_error = Some(e);
                    Err(V4SearchStateErrorV1::SampleHookRejected)
                }
            },
        )
        .map_err(|e| match &hook_error {
            Some(h) => format!("{e:?}: {h}"),
            None => format!("{e:?}"),
        })?;
    Ok(Sample {
        world,
        prior_deck: chosen.ok_or("deck prior hook did not run")?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multiset_helpers() {
        assert!(is_submultiset(&[1, 2, 2], &[1, 2, 2, 3]));
        assert!(!is_submultiset(&[1, 2, 2, 2], &[1, 2, 2, 3]));
        assert!(!is_submultiset(&[4], &[1, 2, 3]));
        assert!(is_submultiset(&[], &[1]));
        assert_eq!(difference(&[1, 2, 2, 3, 5], &[2, 5]), vec![1, 2, 3]);
    }

    #[test]
    fn every_registered_experiment_deck_is_a_prior_member() {
        let prior = DeckPrior::new(&(0..9).collect::<Vec<_>>());
        assert_eq!(prior.ids().len(), 9);
        for d in 0..9 {
            // An empty observation is consistent with every deck of its size.
            let total = prior.decks[d].len();
            assert!(prior.consistent(&[], total).contains(&d));
            // A deck's own full list is consistent with itself.
            assert!(prior.consistent(&prior.decks[d], total).contains(&d));
        }
    }
}
