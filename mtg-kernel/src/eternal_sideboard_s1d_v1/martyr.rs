//! Definition-owned private activation payment for Martyr of Sands.
use crate::{
    card_def::{ActivatedAbilityDef, CostComponent, TargetSpec, CARD_DEFS},
    effect::{EffectObjectBinding, ExecCtx},
    engine::{Decision, PendingActivation, UnsupportedMechanic},
    event::{self, CommittedEvent, ProposedEvent},
    event_history_v1::EventKindV1,
    ids::{ObjectId, PlayerId, StackItemId},
    mana::Cost,
    state::{AbilitySourceContractV4, GameState, StackItem, Target, Zone},
};
use std::collections::{BTreeMap, BTreeSet};

pub(crate) fn ability(ability: &ActivatedAbilityDef) -> bool {
    (ability.effect)() == super::martyr_of_sands()
        && ability.cost == [
            CostComponent::Mana(Cost { generic: 1, pips: &[], x_count: 0 }),
            CostComponent::SacrificeSelf,
        ]
        && ability.target_spec == TargetSpec::None
        && ability.activation_zone == Zone::Battlefield
        && !ability.sorcery_speed_only
}

fn eligible(state: &GameState, player: PlayerId, card: ObjectId) -> Option<EffectObjectBinding> {
    let live = state.objects.try_get(card)?;
    (live.owner == player && live.zone == Zone::Hand
        && crate::engine::object_color_mask(state, card) & super::white_mask() != 0)
        .then_some(EffectObjectBinding {
            object: card,
            expected_zone: Zone::Hand,
            expected_zone_change_count: live.zone_change_count,
        })
}

pub(crate) fn validate(state: &GameState, pending: &PendingActivation) -> Result<(), String> {
    let hand = &state.players[pending.controller.index()].hand;
    let mut distinct = BTreeSet::new();
    if pending.controller != state.priority_player
        || pending.object_cost_chosen.iter().any(|&binding| {
            !distinct.insert(binding.object)
                || !hand.contains(&binding.object)
                || eligible(state, pending.controller, binding.object) != Some(binding)
        })
    {
        return Err("Martyr reveal selection changed its original white hand cards".into());
    }
    Ok(())
}

pub(crate) fn decision(state: &GameState, pending: &PendingActivation) -> Result<Decision, String> {
    let chosen: BTreeSet<_> = pending.object_cost_chosen.iter().map(|row| row.object).collect();
    let legal_targets: Vec<_> = state.players[pending.controller.index()].hand.iter().copied()
        .filter(|&card| !chosen.contains(&card) && eligible(state, pending.controller, card).is_some())
        .map(Target::Object)
        .collect();
    let max_targets = u16::try_from(chosen.len() + legal_targets.len())
        .map_err(|_| "Martyr reveal selection exceeds native menu width")?;
    Ok(Decision::ChooseEffectTargets {
        player: pending.controller,
        source: pending.source,
        selected_count: u16::try_from(chosen.len()).map_err(|_| "Martyr selection exceeds native menu width")?,
        min_targets: 0,
        max_targets,
        legal_targets,
        can_finish: true,
    })
}

pub(crate) fn choose(state: &mut GameState, target: Target) -> Result<(), String> {
    let pending = state.engine.pending_activation.as_ref().ok_or("Martyr activation absent")?;
    let Target::Object(card) = target else {
        return Err("Martyr must reveal a white card in its controller's hand".into());
    };
    if !state.players[pending.controller.index()].hand.contains(&card)
        || pending.object_cost_chosen.iter().any(|row| row.object == card)
    {
        return Err("Martyr reveal pick is not an unused card in its controller's hand".into());
    }
    let selected = eligible(state, pending.controller, card).ok_or("Martyr reveal pick is not white")?;
    state.engine.pending_activation.as_mut().unwrap().object_cost_chosen.push(selected);
    Ok(())
}

/// Runs inside the same native transaction as mana and sacrifice, before the
/// source leaves. The actual public event contains names without hand slots.
pub(crate) fn reveal(state: &mut GameState, pending: &PendingActivation) -> Result<Vec<u16>, String> {
    validate(state, pending)?;
    u16::try_from(pending.object_cost_chosen.len()).map_err(|_| "Martyr reveal count exceeds native width")?;
    let mut counts = BTreeMap::<u16, usize>::new();
    for row in &pending.object_cost_chosen {
        *counts.entry(state.objects.get(row.object).card_def).or_default() += 1;
    }
    for row in &pending.object_cost_chosen {
        let printing = state.objects.get(row.object).card_def;
        // spellbench-public-reveal: Martyr actually reveals its selected white cards.
        state.record_hand_reveal(pending.controller.opponent(), pending.controller, row.object, Some(counts[&printing]))?;
    }
    Ok(counts.into_iter().flat_map(|(printing, count)| std::iter::repeat_n(printing, count)).collect())
}

fn paid(
    state: &GameState,
    stack_item_id: StackItemId,
) -> Result<(AbilitySourceContractV4, u16), String> {
    if stack_item_id == StackItemId::default() {
        return Err("Martyr reveal has no published stack item".into());
    }
    let mut receipts = state.engine.event_history.scan_for_v1(&[EventKindV1::MartyrRevealPaidV1])
        .iter().filter_map(|event| match event {
            CommittedEvent::MartyrRevealPaidV1 { item, source, revealed } if *item == stack_item_id =>
                Some((*source, revealed)),
            _ => None,
        });
    let (source, revealed) = receipts.next().ok_or("Martyr activation has no committed reveal")?;
    if receipts.next().is_some()
        || source.zone != Zone::Battlefield
        || revealed.windows(2).any(|pair| pair[0] > pair[1])
        || revealed.iter().any(|&id| CARD_DEFS.get(usize::from(id)).is_none())
    {
        return Err("Martyr activation has a malformed committed reveal".into());
    }
    let count = u16::try_from(revealed.len()).map_err(|_| "Martyr reveal count exceeds native width")?;
    Ok((source, count))
}

pub(crate) fn paid_count(state: &GameState, item: &StackItem) -> Result<u16, String> {
    let (source, count) = paid(state, item.v4.stack_item_id)?;
    if item.v4.ability_source_contract != Some(source) || item.source != source.source
        || item.controller != source.controller || item.is_copy {
        return Err("Martyr reveal does not bind this original paid ability".into());
    }
    Ok(count)
}

pub(crate) fn resolve(state: &mut GameState, ctx: &ExecCtx) {
    let count = ctx.stack_item_id.and_then(|item| paid(state, item).ok()).and_then(|(source, count)| {
        (source.source == ctx.source && source.controller == ctx.controller).then_some(count)
    });
    let Some(count) = count else {
        state.engine.halted = Some((UnsupportedMechanic::InvalidEffectContinuation, ctx.source));
        return;
    };
    if count != 0 {
        event::propose_and_commit(state, ProposedEvent::life_gain(ctx.controller, 3 * i32::from(count)));
    }
}
