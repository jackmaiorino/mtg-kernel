//! Explicit operands for the finite Eternal selection families. This adds to
//! the ordinary chooser-visible legal targets; it never serializes a saved
//! continuation or another player's simultaneous private answer.
use crate::{
    effect::{EffectObjectBinding, EffectTargetSelectionPurpose, PendingEffectChoice},
    eternal_cards_v1::e3::{E3ChoiceKindV1, E3ChoiceV1},
    ids::PlayerId,
    rl::{CardStableRefV1, RlContractError},
    state::{GameState, Zone},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SelectionV1 {
    RetargetSpell {
        stack_item_id: u64,
    },
    DoomsdayOrderedFive,
    Oracle {
        devotion: u32,
        looked: Vec<CardStableRefV1>,
    },
    ShowAndTell {
        later_players: Vec<PlayerId>,
    },
    Atraxa {
        revealed: Vec<CardStableRefV1>,
        picked: Vec<CardStableRefV1>,
    },
    Scry {
        looked: Vec<CardStableRefV1>,
        top: Option<Vec<CardStableRefV1>>,
    },
    ArchonOpponentSacrificeCreatureOrPlaneswalker,
}

pub fn capture_v1(
    state: &GameState,
    viewer: PlayerId,
) -> Result<Option<SelectionV1>, RlContractError> {
    let Some(pending) = &state.engine.pending_effect else {
        return Ok(None);
    };
    let Some(PendingEffectChoice::SelectTargets {
        player,
        purpose: EffectTargetSelectionPurpose::EternalV1 { choice },
        ..
    }) = &pending.choice
    else {
        return Ok(None);
    };
    if *player != choice.player {
        return Err(RlContractError("Eternal chooser binding mismatch".into()));
    }
    project(state, viewer, choice)
}

fn own_looked(
    state: &GameState,
    viewer: PlayerId,
    bindings: &[EffectObjectBinding],
) -> Result<Vec<CardStableRefV1>, RlContractError> {
    bindings
        .iter()
        .map(|binding| {
            // The trusted choice variant grants this chooser a look at these exact
            // library incarnations. Reject stale ownership/zone before card_ref.
            let object = state
                .objects
                .try_get(binding.object)
                .ok_or_else(|| RlContractError("missing Eternal looked card".into()))?;
            if binding.expected_zone != Zone::Library
                || object.zone != Zone::Library
                || object.owner != viewer
                || object.zone_change_count != binding.expected_zone_change_count
                || object.v4.face_down_v1.is_some()
            {
                return Err(RlContractError("stale Eternal looked card".into()));
            }
            crate::rl::card_ref(state, binding.object)
        })
        .collect()
}

fn project(
    state: &GameState,
    viewer: PlayerId,
    choice: &E3ChoiceV1,
) -> Result<Option<SelectionV1>, RlContractError> {
    // The other observer never gets a choice's private operands, including
    // empty/nonempty distinctions, regardless of the hidden card identities.
    if choice.player != viewer {
        return Ok(None);
    }
    Ok(Some(match &choice.kind {
        E3ChoiceKindV1::RetargetSpellV1 { spell } => {
            if !state.stack.iter().any(|item| item.id == *spell) {
                return Err(RlContractError("missing retargeted public spell".into()));
            }
            SelectionV1::RetargetSpell {
                stack_item_id: u64::from(spell.0),
            }
        }
        E3ChoiceKindV1::DoomsdayV1 => SelectionV1::DoomsdayOrderedFive,
        E3ChoiceKindV1::OracleV1 { devotion, prefix } => SelectionV1::Oracle {
            devotion: *devotion,
            looked: own_looked(state, viewer, prefix)?,
        },
        E3ChoiceKindV1::ShowAndTellV1 { chosen: _, later } => SelectionV1::ShowAndTell {
            // Earlier choices have not been revealed. Do not inspect their
            // bindings, lengths, card definitions, or saved effect frames.
            later_players: later.clone(),
        },
        E3ChoiceKindV1::AtraxaV1 { revealed, picked } => SelectionV1::Atraxa {
            revealed: own_looked(state, viewer, revealed)?,
            picked: own_looked(state, viewer, picked)?,
        },
        E3ChoiceKindV1::ScryV1 { prefix, top } => SelectionV1::Scry {
            looked: own_looked(state, viewer, prefix)?,
            top: top
                .as_ref()
                .map(|cards| own_looked(state, viewer, cards))
                .transpose()?,
        },
        E3ChoiceKindV1::E7aV1(crate::eternal_legacy_e7a_v1::E7aChoiceV1::ArchonSacrificeV1) => {
            SelectionV1::ArchonOpponentSacrificeCreatureOrPlaneswalker
        }
        _ => {
            return Err(RlContractError(
                "unrepresented Eternal selection operands".into(),
            ))
        }
    }))
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state_for};

    #[test]
    fn issue454_show_and_tell_hides_prior_private_answers_and_saved_frames() {
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(viewer);
            let hidden = put(&mut state, viewer.opponent(), "Island", Zone::Hand);
            let mut choice = E3ChoiceV1 {
                kind: E3ChoiceKindV1::ShowAndTellV1 {
                    chosen: Vec::new(),
                    later: Vec::new(),
                },
                player: viewer,
                candidates: Vec::new(),
                bindings: Vec::new(),
                min: 0,
                max: 1,
                ordered: false,
                path: Vec::new(),
                remaining: Vec::new(),
            };
            let baseline = project(&state, viewer, &choice).unwrap();
            for name in ["Island", "Atraxa, Grand Unifier"] {
                state.objects.get_mut(hidden).card_def =
                    crate::card_def::card_id_by_name(name).unwrap();
                choice.kind = E3ChoiceKindV1::ShowAndTellV1 {
                    chosen: vec![EffectObjectBinding {
                        object: hidden,
                        expected_zone: Zone::Hand,
                        expected_zone_change_count: 0,
                    }],
                    later: Vec::new(),
                };
                assert_eq!(project(&state, viewer, &choice).unwrap(), baseline);
                assert_eq!(project(&state, viewer.opponent(), &choice).unwrap(), None);
            }
        }
    }
}
