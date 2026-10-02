//! Clone-only reconstruction of an already validated own-library choice.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Error {
    InvalidChoice,
    ChangedContract,
    ChangedMembership,
}

#[derive(Clone)]
pub(crate) struct Plan {
    choice: PendingEffectChoice,
    owner: PlayerId,
    filter: Option<LibraryCardFilter>,
    identities: Vec<(EffectObjectBinding, u16)>,
}

fn parts(
    p: &EffectTargetSelectionPurpose,
) -> Option<(PlayerId, LibraryCardFilter, &[EffectObjectBinding])> {
    match p {
        EffectTargetSelectionPurpose::SearchLibraryToHand {
            player,
            filter,
            original_library,
            ..
        }
        | EffectTargetSelectionPurpose::SearchLibraryToHandMany {
            player,
            filter,
            original_library,
            ..
        }
        | EffectTargetSelectionPurpose::SearchLibraryToBattlefieldTapped {
            player,
            filter,
            original_library,
            ..
        } => Some((*player, *filter, original_library)),
        _ => None,
    }
}

pub(crate) fn plan(state: &GameState, actor: PlayerId) -> Result<Option<Plan>, Error> {
    plan_inner(state, actor, false)
}

// Additive FutureChanceV3 support. The original LibraryChoiceV2 contract
// deliberately retains its original admission set.
pub(crate) fn plan_future_v3(state: &GameState, actor: PlayerId) -> Result<Option<Plan>, Error> {
    plan_inner(state, actor, true)
}

fn plan_inner(state: &GameState, actor: PlayerId, throne: bool) -> Result<Option<Plan>, Error> {
    let Some(choice) = state
        .engine
        .pending_effect
        .as_ref()
        .and_then(|p| p.choice.as_ref())
    else {
        return Ok(None);
    };
    let PendingEffectChoice::SelectTargets {
        player, purpose, ..
    } = choice
    else {
        return Ok(None);
    };
    let (owner, filter, library) = if let Some((owner, filter, library)) = parts(purpose) {
        (owner, Some(filter), library)
    } else if let EffectTargetSelectionPurpose::UndercityThroneCreature {
        binding,
        original_library,
        ..
    } = purpose
    {
        if !throne {
            return Ok(None);
        }
        (binding.player, None, original_library.as_slice())
    } else {
        return Ok(None);
    };
    validate_pending_effect_choice(state).map_err(|_| Error::InvalidChoice)?;
    if *player != actor || owner != actor {
        return Err(Error::InvalidChoice);
    }
    let mut identities: Vec<_> = library
        .iter()
        .map(|b| (*b, state.objects.get(b.object).card_def))
        .collect();
    identities.sort_by_key(|(b, _)| b.object);
    Ok(Some(Plan {
        choice: choice.clone(),
        owner,
        filter,
        identities,
    }))
}

impl Plan {
    /// Only this exact pending choice is exempted. Frames and guards never are.
    pub(crate) fn matches(&self, choice: &PendingEffectChoice) -> bool {
        &self.choice == choice
    }

    pub(crate) fn rebuild(&self, state: &mut GameState) -> Result<(), Error> {
        let choice = state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|p| p.choice.as_ref())
            .ok_or(Error::ChangedContract)?;
        if !self.matches(choice) {
            return Err(Error::ChangedContract);
        }
        let PendingEffectChoice::SelectTargets {
            selected, legal, ..
        } = choice
        else {
            return Err(Error::ChangedContract);
        };
        let library = bind_library_exact(state, self.owner);
        let mut identities: Vec<_> = library
            .iter()
            .map(|b| (*b, state.objects.get(b.object).card_def))
            .collect();
        identities.sort_by_key(|(b, _)| b.object);
        if identities != self.identities {
            return Err(Error::ChangedMembership);
        }
        let rebuilt: Vec<_> = if let Some(filter) = self.filter {
            let mut bindings = library_search_candidates(state, self.owner, filter, &library)
                .map_err(|_| Error::InvalidChoice)?;
            bindings.retain(|b| !selected.iter().any(|s| s.expected_object == Some(*b)));
            bindings
                .into_iter()
                .map(|b| EffectTargetCandidate {
                    target: Target::Object(b.object),
                    expected_object: Some(b),
                })
                .collect()
        } else {
            let PendingEffectChoice::SelectTargets {
                purpose:
                    EffectTargetSelectionPurpose::UndercityThroneCreature {
                        revealed_prefix, ..
                    },
                ..
            } = choice
            else {
                return Err(Error::ChangedContract);
            };
            if library.get(..revealed_prefix.len()) != Some(revealed_prefix.as_slice()) {
                return Err(Error::ChangedMembership);
            }
            // The revealed prefix and its exact candidate order remain fixed.
            // Only the interpreter's private full-library snapshot is rebound.
            legal.clone()
        };
        let mut old = legal.clone();
        let mut new = rebuilt.clone();
        let key = |c: &EffectTargetCandidate| c.expected_object.map(|b| b.object);
        old.sort_by_key(key);
        new.sort_by_key(key);
        if old != new {
            return Err(Error::ChangedMembership);
        }
        let choice = state
            .engine
            .pending_effect
            .as_mut()
            .and_then(|p| p.choice.as_mut())
            .ok_or(Error::ChangedContract)?;
        let PendingEffectChoice::SelectTargets { legal, purpose, .. } = choice else {
            return Err(Error::ChangedContract);
        };
        match purpose {
            EffectTargetSelectionPurpose::SearchLibraryToHand {
                original_library, ..
            }
            | EffectTargetSelectionPurpose::SearchLibraryToHandMany {
                original_library, ..
            }
            | EffectTargetSelectionPurpose::SearchLibraryToBattlefieldTapped {
                original_library,
                ..
            } => *original_library = library,
            EffectTargetSelectionPurpose::UndercityThroneCreature {
                original_library, ..
            } if self.filter.is_none() => *original_library = library,
            _ => return Err(Error::ChangedContract),
        }
        *legal = rebuilt;
        validate_pending_effect_choice(state).map_err(|_| Error::InvalidChoice)
    }
}
