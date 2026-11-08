//! A rules-only closed-list world. The opponent's unknown definitions are
//! supplied by the corpus, and name facts in hand never pin a physical copy.
use super::*;
use crate::state::HandKnowledgeEntry;

pub(crate) fn resampled_slots(
    state: &GameState,
    actor: PlayerId,
    pinned: &[ObjectId],
) -> [Vec<ObjectId>; 2] {
    let opponent = actor.opponent();
    let mut opponent_slots = state.players[opponent.index()].hand.clone();
    opponent_slots.extend(unknown_library_slots_v1(state, actor, opponent, pinned));
    [
        unknown_slots_v1(state, actor, actor, pinned),
        opponent_slots,
    ]
}

fn shuffle<T>(items: &mut [T], rng: &mut SplitMix64) {
    for i in (1..items.len()).rev() {
        items.swap(i, draw(rng, i + 1));
    }
}

pub(crate) fn rewrite(
    state: &mut GameState,
    actor: PlayerId,
    slots: [Vec<ObjectId>; 2],
    belief: &LimitedBeliefV1,
    seed: u64,
) -> Result<(), String> {
    let opponent = actor.opponent();
    let known_hand = state
        .known_hand_cards(actor, opponent)
        .iter()
        .filter(|entry| {
            let card = state.objects.get(entry.object);
            card.owner == opponent
                && card.zone == Zone::Hand
                && card.zone_change_count == entry.zone_change_count
        })
        .map(|entry| state.objects.get(entry.object).card_def)
        .collect();
    rewrite_names(state, actor, slots, belief, seed, known_hand)
}

pub(crate) fn rewrite_names(
    state: &mut GameState,
    actor: PlayerId,
    slots: [Vec<ObjectId>; 2],
    belief: &LimitedBeliefV1,
    seed: u64,
    mut known_hand: Vec<u16>,
) -> Result<(), String> {
    let opponent = actor.opponent();
    let [own_library, opponent_slots] = slots;
    let hand_len = state.players[opponent.index()].hand.len();
    let unknown: HashSet<_> = opponent_slots.iter().copied().collect();
    known_hand.sort_unstable();
    if known_hand.len() > hand_len {
        return Err("invalid hand knowledge".into());
    }

    let mut observed = observed_cards_v1(state, opponent, &unknown);
    observed.extend(known_hand.iter().copied());
    // Face-down identities only reach here if this actor is allowed to look.
    observed.extend(
        state
            .objects
            .iter()
            .filter(|(id, card)| {
                !unknown.contains(id)
                    && card.owner == opponent
                    && !card.v4.is_token
                    && card.spell_copy_origin.is_none()
                    && card.v4.face_down_v1.is_some()
            })
            .map(|(_, card)| card.card_def),
    );
    observed.sort_unstable();

    let unknown_count = opponent_slots.len() - known_hand.len();
    let mut opponent_definitions = known_hand.clone();
    if unknown_count > 0 {
        let (rest, _) = belief_remainder(belief, &observed, observed.len() + unknown_count, seed)?;
        opponent_definitions.extend(rest.into_iter().cycle().take(unknown_count));
    }
    // Shuffle name facts among hand slots, then recreate the name ledger at
    // the sampled placement. Neither the live slot nor its rank is a fact.
    let mut hand_order: Vec<_> = (0..hand_len).collect();
    let mut rng = SplitMix64::seed(derive_seed(b"mtg-kernel/limited-hand/v2\0", &[seed]));
    shuffle(&mut hand_order, &mut rng);
    let sampled_known_hand = hand_order[..known_hand.len()]
        .iter()
        .map(|&index| {
            let object = opponent_slots[index];
            HandKnowledgeEntry {
                object,
                zone_change_count: state.objects.get(object).zone_change_count,
            }
        })
        .collect::<Vec<_>>();
    let opponent_order = hand_order
        .into_iter()
        .map(|index| opponent_slots[index])
        .chain(opponent_slots[hand_len..].iter().copied());

    // The actor's own unknown library multiset follows from their deck and
    // known cards. Canonicalize it before shuffling so the true order cannot
    // influence a world.
    let mut own_definitions: Vec<_> = own_library
        .iter()
        .map(|&id| state.objects.get(id).card_def)
        .collect();
    own_definitions.sort_unstable();
    let mut rng = SplitMix64::seed(derive_seed(b"mtg-kernel/limited-own-library/v2\0", &[seed]));
    shuffle(&mut own_definitions, &mut rng);

    for (id, definition) in own_library
        .into_iter()
        .zip(own_definitions)
        .chain(opponent_order.zip(opponent_definitions))
    {
        let card = state.objects.get_mut(id);
        card.card_def = definition;
        card.name = CARD_DEFS[usize::from(definition)].name.into();
        card.v4 = ObjectStateV4::from_card_def(definition);
    }

    // A random rollout doesn't consult another player's private memory. Drop
    // that ledger rather than retaining a binding to a replaced definition.
    state.hand_knowledge[opponent.index()] = Default::default();
    state.library_knowledge[opponent.index()] = Default::default();
    state.library_knowledge[opponent.index()][actor.index()] =
        crate::effect::curator_v1::public_order_library_receipts(state, actor);
    let facts = &mut state.hand_knowledge[actor.index()][opponent.index()];
    *facts = sampled_known_hand;
    facts.sort_unstable_by_key(|entry| entry.object);
    Ok(())
}
