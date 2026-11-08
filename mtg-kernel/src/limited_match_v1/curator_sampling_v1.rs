//! Full-pool sampling for the authenticated, unordered Curator pile prompt.
use super::{closed_list_v2, derive_seed, draw, LimitedBeliefV1, SplitMix64};
use crate::{
    card_def::CARD_DEFS,
    curator_receipts_v1::{self as receipts, JournalV1, ShownV1},
    effect::{
        curator_v1, EffectBooleanChoicePurpose, EffectObjectBinding, PendingEffectChoice,
    },
    event::CommittedEvent,
    ids::{ObjectId, PlayerId},
    state::{GameState, LibraryKnowledgeEntry, ObjectStateV4},
};

struct ActivePile {
    entry: u64,
    controller: PlayerId,
    prefix: Vec<EffectObjectBinding>,
    layout: Vec<Option<u16>>,
}

fn layouts(
    labels: &mut [Option<u16>],
    fixed: &[Option<u16>],
    index: usize,
    result: &mut Vec<Vec<Option<u16>>>,
) {
    if index == labels.len() {
        result.push(labels.to_vec());
        return;
    }
    for next in index..labels.len() {
        let label = labels[next];
        if labels[index..next].contains(&label)
            || matches!((fixed[index], label), (Some(known), Some(shown)) if known != shown)
        {
            continue;
        }
        labels.swap(index, next);
        layouts(labels, fixed, index + 1, result);
        labels.swap(index, next);
    }
}

fn active_pile(
    state: &GameState,
    actor: PlayerId,
    unknown: &[ObjectId],
    seed: u64,
) -> Result<Option<ActivePile>, String> {
    let Some(pending) = &state.engine.pending_effect else {
        return Ok(None);
    };
    let Some(PendingEffectChoice::ChooseBoolean {
        player,
        purpose: EffectBooleanChoicePurpose::CuratorPileV1 { face_down_count, face_up_count },
        ..
    }) = &pending.choice else {
        return Ok(None);
    };
    if *player != actor {
        return Err("Curator pile prompt does not belong to the sampled actor".into());
    }
    let choice = curator_v1::prepare_pile_answer(state, pending, *face_down_count, *face_up_count)?;
    let (_, shown) = receipts::validate_reveal(state, &choice, true)?;
    // The actual public names and hidden pile size are the only pile operands.
    // Independently known positions constrain the sample without using which
    // native physical copies the opponent had assigned to either pile.
    let mut labels: Vec<_> = shown.iter().map(|row| Some(row.printing)).collect();
    labels.extend(std::iter::repeat_n(None, usize::from(*face_down_count)));
    labels.sort_unstable();
    let fixed: Vec<_> = choice.prefix.iter().map(|binding| {
        (!unknown.contains(&binding.object))
            .then(|| state.objects.get(binding.object).card_def)
    }).collect();
    let mut possible = Vec::new();
    layouts(&mut labels, &fixed, 0, &mut possible);
    if possible.is_empty() {
        return Err("public Curator names and known positions have no pile layout".into());
    }
    let mut rng = SplitMix64::seed(derive_seed(b"mtg-kernel/curator-pile-sampling/v1\0", &[seed]));
    let layout = possible.swap_remove(draw(&mut rng, possible.len()));
    Ok(Some(ActivePile {
        entry: choice.entry,
        controller: choice.player,
        prefix: choice.prefix,
        layout,
    }))
}

impl ActivePile {
    fn place_shown(
        &self,
        state: &mut GameState,
        opponent_slots: &mut Vec<ObjectId>,
    ) -> Result<(), String> {
        let mut shown = Vec::new();
        let mut down = Vec::new();
        for (&binding, &label) in self.prefix.iter().zip(&self.layout) {
            let Some(printing) = label else {
                down.push(binding);
                continue;
            };
            let card = state.objects.get_mut(binding.object);
            if opponent_slots.contains(&binding.object) {
                card.card_def = printing;
                card.name = CARD_DEFS[usize::from(printing)].name.into();
                card.v4 = ObjectStateV4::from_card_def(printing);
            }
            if card.card_def != printing {
                return Err("sampled Curator pile disagrees with a known library position".into());
            }
            shown.push(ShownV1 { original: binding, printing });
        }
        // The ordinary corpus sampler accounts for these placements as known
        // deck cards and fills all other unknown slots, including down slots.
        opponent_slots.retain(|id| !shown.iter().any(|row| row.original.object == *id));
        let mut progress_count = 0;
        let mut reveal_count = 0;
        for event in state.engine.event_history.iter_mut() {
            match event {
                CommittedEvent::CuratorProgressV1 { entry_history_index, choice }
                    if *entry_history_index == self.entry && choice.phase == 1 =>
                {
                    choice.down.clone_from(&down);
                    progress_count += 1;
                }
                CommittedEvent::CuratorReceiptV1 { journal } => {
                    if let JournalV1::Revealed { entry, shown: actual, .. } = journal.as_mut() {
                        if *entry == self.entry {
                            actual.clone_from(&shown);
                            reveal_count += 1;
                        }
                    }
                }
                _ => {}
            }
        }
        if (progress_count, reveal_count) != (1, 1) {
            return Err("sampled Curator pile lost its native progress or reveal".into());
        }
        Ok(())
    }

    fn restore_controller_look(&self, state: &mut GameState) {
        // The normal clone clears the other actor's private memory. This one
        // prefix is a native operation operand, reconstructed from the sampled
        // cards to support the controller's subsequent graveyard-order prompt.
        state.library_knowledge[self.controller.index()][self.controller.index()] = self.prefix
            .iter().enumerate().map(|(position, binding)| LibraryKnowledgeEntry {
                position: position as u32,
                object: binding.object,
                zone_change_count: binding.expected_zone_change_count,
            }).collect();
    }
}

pub(crate) fn rewrite(
    state: &mut GameState,
    actor: PlayerId,
    mut slots: [Vec<ObjectId>; 2],
    belief: &LimitedBeliefV1,
    seed: u64,
    known_names: Vec<u16>,
) -> Result<(), String> {
    let pile = active_pile(state, actor, &slots[1], seed)?;
    receipts::mark_sampled_hand_arrivals(state, actor, &slots[1])?;
    if let Some(pile) = &pile {
        pile.place_shown(state, &mut slots[1])?;
    }
    closed_list_v2::rewrite_names(state, actor, slots, belief, seed, known_names)?;
    if let Some(pile) = pile {
        pile.restore_controller_look(state);
    }
    Ok(())
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
#[path = "curator_sampling_v1_tests.rs"]
mod tests;
