//! Rebuild the active, publicly revealed discard menu in a disposable world.
use super::{
    bind_hand, revealed_discard_eligible_hand, revealed_discard_excludes_creatures,
    revealed_discard_names_v1, stage_duress_discard_choice, validate_pending_effect_choice,
    EffectContinuation, EffectObjectBinding, EffectTargetSelectionPurpose, PendingEffectChoice,
};
use crate::{
    engine::Decision,
    ids::{ObjectId, PlayerId},
    state::{GameState, StackItemKind},
};

pub(crate) struct PreparedV1<'a> {
    original: &'a EffectContinuation,
    owner: PlayerId,
    exclude_creatures: bool,
    names: Vec<u16>,
    eligible_printings: Vec<u16>,
    eligible_objects: Vec<ObjectId>,
}

fn printings(state: &GameState, bindings: &[EffectObjectBinding]) -> Vec<u16> {
    let mut names: Vec<_> = bindings
        .iter()
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
            purpose:
                EffectTargetSelectionPurpose::DuressDiscard {
                    player: owner,
                    eligible,
                    canonical_path,
                    ..
                },
            ..
        }) = &original.choice
        else {
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
        let (_, names) = revealed_discard_names_v1(state, actor)?
            .ok_or("revealed discard sampling lacks native public names")?;
        let mut disclosed = names.iter();
        if !known_names
            .iter()
            .all(|known| disclosed.any(|name| name == known))
        {
            return Err("revealed discard names contradict retained minima".into());
        }
        Ok(Some(Self {
            original,
            owner: *owner,
            exclude_creatures: revealed_discard_excludes_creatures(
                state,
                original,
                *owner,
                canonical_path,
            )?,
            names,
            eligible_printings: printings(state, eligible),
            eligible_objects: eligible.iter().map(|binding| binding.object).collect(),
        }))
    }

    pub(crate) fn releases_selection_pin(&self, object: ObjectId) -> bool {
        self.eligible_objects.contains(&object)
    }

    pub(crate) fn names(&self) -> &[u16] {
        &self.names
    }

    pub(crate) fn rebind(&self, sampled: &mut GameState) -> Result<Decision, String> {
        let mut pending = sampled
            .engine
            .pending_effect
            .take()
            .ok_or("revealed discard sampled continuation absent")?;
        if &pending != self.original {
            return Err("revealed discard sampled continuation changed before rebinding".into());
        }
        let hand = bind_hand(sampled, self.owner);
        if printings(sampled, &hand) != self.names {
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
