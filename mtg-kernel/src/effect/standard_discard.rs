//! Optional hand selection followed by an exact draw count, and random impulse exile.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DiscardDrawChoice {
    pub player: PlayerId,
    pub player_ref: PlayerRef,
    pub maximum: u8,
    pub hand: Vec<EffectObjectBinding>,
    pub path: Vec<u16>,
    pub remaining: Vec<EffectFrame>,
}
pub(super) fn validate(
    state: &GameState,
    pending: &EffectContinuation,
    choice: &DiscardDrawChoice,
) -> Result<(), String> {
    let root = validated_definition_owned_root_effect(state, pending)?;
    if effect_op_at_path(&root, &choice.path)
        != Some(&EffectOp::DiscardUpToThenDraw {
            player: choice.player_ref,
            maximum: choice.maximum,
        })
        || pending.ctx.resolve_player(choice.player_ref, state) != choice.player
        || pending.frames != choice.remaining
    {
        return Err("discard draw continuation changed".into());
    }
    validate_bound_hand_exact(state, choice.player, &choice.hand)
}
pub(super) fn stage(
    state: &GameState,
    pending: &mut EffectContinuation,
    player_ref: PlayerRef,
    maximum: u8,
    path: Vec<u16>,
) -> Result<bool, String> {
    let player = pending.ctx.resolve_player(player_ref, state);
    let hand = bind_hand(state, player);
    let choice = DiscardDrawChoice {
        player,
        player_ref,
        maximum,
        hand,
        path: path.clone(),
        remaining: pending.frames.clone(),
    };
    validate(state, pending, &choice)?;
    if choice.hand.is_empty() || maximum == 0 {
        return Ok(false);
    }
    pending.choice = Some(PendingEffectChoice::SelectTargets {
        player,
        path,
        selected: vec![],
        legal: choice
            .hand
            .iter()
            .map(|b| EffectTargetCandidate {
                target: Target::Object(b.object),
                expected_object: Some(*b),
            })
            .collect(),
        min_targets: 0,
        max_targets: u16::from(maximum).min(choice.hand.len() as u16),
        ordered: true,
        purpose: EffectTargetSelectionPurpose::DiscardDraw { choice },
    });
    Ok(true)
}
pub(super) fn finish(
    state: &mut GameState,
    pending: &EffectContinuation,
    choice: &DiscardDrawChoice,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    validate(state, pending, choice)?;
    if selected.len() > usize::from(choice.maximum)
        || selected
            .iter()
            .enumerate()
            .any(|(i, b)| !choice.hand.contains(b) || selected[..i].contains(b))
    {
        return Err("discard draw selected cards changed".into());
    }
    // Selection order also specifies graveyard order when it matters. Madness
    // is the same replacement/trigger path used by ordinary discard costs.
    for card in selected {
        crate::engine::commit_discarded_card(state, card.object);
    }
    for _ in selected {
        event::propose_and_commit(state, event::ProposedEvent::draw(choice.player));
    }
    Ok(())
}
pub(super) fn random_exile(
    state: &mut GameState,
    player: PlayerId,
    minimum_cards: u8,
) -> Result<(), String> {
    if controller_graveyard_card_count(state, player) < usize::from(minimum_cards) {
        return Ok(());
    }
    let mut staged = state.clone();
    let Some(card) = staged
        .random_graveyard_card_v1(player)
        .map_err(|e| e.to_string())?
    else {
        return Ok(());
    };
    event::propose_and_commit(
        &mut staged,
        event::ProposedEvent::zone_change(card, Zone::Exile),
    );
    let live = staged.objects.get(card);
    if live.zone == Zone::Exile {
        let definition = &crate::card_def::CARD_DEFS[live.card_def as usize];
        let play_or_cast = if definition.is_playable_land() {
            Some(crate::engine::PlayOrCast::Play)
        } else if definition.is_castable() {
            Some(crate::engine::PlayOrCast::Cast)
        } else {
            None
        };
        if let Some(play_or_cast) = play_or_cast {
            staged
                .engine
                .exile_play_permissions
                .push(crate::engine::PlayPermission {
                    object: card,
                    without_mana_cost: Default::default(),
                    holder: player,
                    zone_change_generation: live.zone_change_count,
                    play_or_cast,
                    expiry: crate::engine::PlayPermissionExpiry::EndOfTurn,
                });
        }
    }
    *state = staged;
    Ok(())
}
