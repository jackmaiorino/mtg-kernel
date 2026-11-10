//! Rebuild the active, publicly revealed discard menu in a disposable world.
use super::{
    bind_hand, revealed_discard_eligible_hand, revealed_discard_excludes_creatures,
    stage_duress_discard_choice, validate_pending_effect_choice, EffectContinuation,
    EffectObjectBinding, EffectTargetSelectionPurpose, PendingEffectChoice,
};
use crate::{engine::Decision, ids::{ObjectId, PlayerId}, state::{GameState, StackItemKind}};

pub(crate) struct PreparedV1<'a> {
    original: &'a EffectContinuation,
    owner: PlayerId,
    exclude_creatures: bool,
    eligible_printings: Vec<u16>,
    eligible_objects: Vec<ObjectId>,
}

fn printings(state: &GameState, bindings: &[EffectObjectBinding]) -> Vec<u16> {
    let mut names: Vec<_> = bindings.iter()
        .map(|binding| state.objects.get(binding.object).card_def)
        .collect();
    names.sort_unstable();
    names
}

impl<'a> PreparedV1<'a> {
    pub(crate) fn capture(
        state: &'a GameState,
        actor: PlayerId,
        known_names: &[u16],
    ) -> Result<Option<Self>, String> {
        let Some(original) = &state.engine.pending_effect else {
            return Ok(None);
        };
        let Some(PendingEffectChoice::SelectTargets {
            player,
            purpose: EffectTargetSelectionPurpose::DuressDiscard {
                player: owner, original_hand, eligible, canonical_path,
            },
            ..
        }) = &original.choice else {
            return Ok(None);
        };
        if *player != actor || *owner != actor.opponent() {
            return Ok(None);
        }
        if original.resolving_item.kind != StackItemKind::Spell
            || !canonical_path.is_empty()
            || !original.frames.is_empty()
            || original.answered_choice_guard.is_some()
        {
            return Err("unsupported revealed discard sampling continuation".into());
        }
        validate_pending_effect_choice(state)?;
        // Read identities only from the exact roster the spell just revealed.
        // Complete name counts, including duplicates, must match that roster.
        let known = state.known_hand_cards(actor, *owner);
        if known.len() != original_hand.len()
            || original_hand.iter().any(|binding| !known.iter().any(|row| {
                row.object == binding.object && row.zone_change_count == binding.expected_zone_change_count
            }))
            || printings(state, original_hand).as_slice() != known_names
        {
            return Err("revealed discard sampling needs the complete public hand multiset".into());
        }
        Ok(Some(Self {
            original,
            owner: *owner,
            exclude_creatures: revealed_discard_excludes_creatures(state, original, *owner, canonical_path)?,
            eligible_printings: printings(state, eligible),
            eligible_objects: eligible.iter().map(|binding| binding.object).collect(),
        }))
    }

    pub(crate) fn releases_selection_pin(&self, object: ObjectId) -> bool {
        self.eligible_objects.contains(&object)
    }

    pub(crate) fn rebind(&self, sampled: &mut GameState, known_names: &[u16]) -> Result<Decision, String> {
        let mut pending = sampled.engine.pending_effect.take()
            .ok_or("revealed discard sampled continuation absent")?;
        if &pending != self.original {
            return Err("revealed discard sampled continuation changed before rebinding".into());
        }
        let hand = bind_hand(sampled, self.owner);
        if printings(sampled, &hand).as_slice() != known_names {
            return Err("revealed discard sampled hand names changed".into());
        }
        let eligible = revealed_discard_eligible_hand(sampled, &hand, self.exclude_creatures)?;
        if printings(sampled, &eligible) != self.eligible_printings {
            return Err("revealed discard sampled name actions changed".into());
        }
        stage_duress_discard_choice(&mut pending, self.owner, hand, eligible, Vec::new());
        let decision = crate::engine::pending_effect_targets_decision_v2(&pending)
            .ok_or("revealed discard sampled menu absent")?;
        sampled.engine.pending_effect = Some(pending);
        validate_pending_effect_choice(sampled)?;
        Ok(decision)
    }
}
