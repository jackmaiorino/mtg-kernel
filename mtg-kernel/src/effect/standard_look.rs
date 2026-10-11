//! Knight-Errant's private optional selection, followed by a whole-library shuffle.
use super::*;
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConvokeLookChoice {
    pub player: PlayerId,
    pub count: u8,
    pub max_taken: u16,
    pub max_mana_value: u16,
    pub prefix: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}
pub(super) fn candidates(
    state: &GameState,
    choice: &ConvokeLookChoice,
) -> Result<Vec<EffectObjectBinding>, String> {
    creature_prefix_mana_value_at_most(state, choice.max_mana_value, &choice.prefix)
}
pub(super) fn validate_choice(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &ConvokeLookChoice,
) -> Result<(), String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    if effect_op_at_path(&root, &choice.path)
        != Some(&EffectOp::LookTopTakeCreaturesManaValueAtMostThenShuffle {
            count: choice.count,
            max_taken: choice
                .max_taken
                .try_into()
                .map_err(|_| "convoke look max overflow")?,
            max_mana_value: choice.max_mana_value,
        })
        || pending.ctx.controller != choice.player
        || pending.frames != choice.remaining
    {
        return Err("convoke look changed root or continuation".into());
    }
    validate_bound_library_prefix(state, &choice.prefix)?;
    if bind_library_top(state, choice.player, choice.count) != choice.prefix {
        return Err("convoke look changed library prefix".into());
    }
    Ok(())
}
pub(super) fn stage(
    state: &mut GameState,
    pending: &mut EffectContinuation,
    count: u8,
    max_taken: u8,
    max_mana_value: u16,
    path: Vec<u16>,
) -> Result<bool, String> {
    let player = pending.ctx.controller;
    let choice = ConvokeLookChoice {
        player,
        count,
        max_taken: u16::from(max_taken),
        max_mana_value,
        prefix: bind_library_top(state, player, count),
        path: path.clone(),
        remaining: pending.frames.clone(),
    };
    validate_choice(state, pending, &choice)?;
    state.reveal_library_top(player, player, choice.prefix.len());
    let cards = candidates(state, &choice)?;
    if cards.is_empty() {
        finish(state, pending, &choice, &[])?;
        return Ok(false);
    }
    pending.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path,
        selected: Vec::new(),
        legal: cards
            .into_iter()
            .map(|binding| EffectTargetCandidate {
                target: Target::Object(binding.object),
                expected_object: Some(binding),
            })
            .collect(),
        min_targets: 0,
        max_targets: u16::from(max_taken),
        ordered: false,
        purpose: EffectTargetSelectionPurpose::ConvokeLook { choice },
    });
    Ok(true)
}
pub(super) fn finish(
    state: &mut GameState,
    pending: &EffectContinuation,
    choice: &ConvokeLookChoice,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    validate_choice(state, pending, choice)?;
    if selected.len() > usize::from(choice.max_taken) {
        return Err("too many convoke look selections".into());
    }
    let cards = canonicalize_binding_subset(&candidates(state, choice)?, selected)?;
    let token = state
        .preflight_library_shuffle(choice.player)
        .map_err(|e| e.to_string())?;
    drop(token);
    for card in cards {
        event::propose_and_commit(
            state,
            event::ProposedEvent::zone_change(card.object, Zone::Hand),
        );
        if state.objects.get(card.object).zone == Zone::Hand {
            for observer in [PlayerId::P0, PlayerId::P1] {
                state
                    .reveal_hand_card(observer, choice.player, card.object)
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    state
        .shuffle_library(choice.player)
        .map_err(|e| e.to_string())?;
    Ok(())
}
