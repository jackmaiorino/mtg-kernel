//! Partial source for X damage followed by a batch of excess-damage tokens.
pub(crate) mod owned;

use crate::{
    card_def::{CardType, Keywords, CARD_DEFS},
    effect::{EffectOp, ExecCtx},
    event::{self, ProposedEvent},
    state::{GameState, StackItem, Target},
};

/// Called unconditionally by native resolution while the complete paid spell
/// remains on the stack, before any resolution publication or removal.
pub(crate) fn preflight_resolution(state: &GameState, item: &StackItem) -> Result<(), String> {
    let definition = &CARD_DEFS[usize::from(state.objects.get(item.source).card_def)];
    let root = item
        .inline_effect
        .clone()
        .or_else(|| (definition.spell_effect)());
    let Some(op @ EffectOp::DealXDamageThenCreateTokensForExcess { .. }) = root else {
        return Ok(());
    };
    let EffectOp::DealXDamageThenCreateTokensForExcess {
        target_index,
        token_def,
    } = &op
    else {
        unreachable!()
    };
    let mut ctx = ExecCtx::no_targets(item.source, item.controller);
    ctx.stack_item_id = Some(item.v4.stack_item_id);
    ctx.targets = item.targets.clone();
    ctx.target_contracts = item.v4.target_contracts.clone();
    ctx.x_value = item.v4.x_value;

    let index = usize::from(*target_index);
    if !ctx.target_incarnation_matches(index, state) {
        return Ok(());
    }
    let Some(Target::Object(target)) = ctx.targets.get(index).copied() else {
        return Err("excess damage lost creature target".into());
    };
    let lethal = lethal_threshold(state, ctx.source, target)?;
    let ProposedEvent::Damage(original_damage) =
        crate::event::damage_from_exec_ctx_v1(&ctx, Target::Object(target), i32::from(ctx.x_value))
    else {
        unreachable!()
    };
    let amount = if crate::effect::crystal_prevention_v1::needs_choice(state, &original_damage) {
        // Capacity bound only. This is never a damage result or replacement
        // answer: real ordered damage is committed by the private Crystal owner.
        crate::twinflame_tyrant_v1::sources(state, &original_damage)?
            .iter()
            .try_fold(i32::from(ctx.x_value), |bound, _| {
                bound
                    .checked_mul(2)
                    .ok_or("excess ordered damage capacity bound overflow")
            })?
    } else {
        // Reuse the actual commuting replacement pass on a disposable copy.
        let mut preview = state.clone();
        let result =
            event::apply_replacements(&mut preview, ProposedEvent::Damage(original_damage));
        if preview.engine.halted.is_some() {
            return Err("excess damage replacement preview refused".into());
        }
        match result {
            None => 0,
            Some(ProposedEvent::Damage(damage))
                if damage.source == ctx.source && damage.target == Target::Object(target) =>
            {
                damage.amount.max(0)
            }
            _ => return Err("excess damage preview changed unsupported recipient or event".into()),
        }
    };
    state
        .objects
        .get(target)
        .damage
        .checked_add(amount as u32)
        .ok_or("excess damage marked-damage overflow")?;
    let excess = i64::from(amount).saturating_sub(lethal).max(0);
    let original = usize::try_from(excess).map_err(|_| "excess token count overflow")?;
    if original == 0 {
        return Ok(());
    }
    preflight_tokens(state, ctx.controller, *token_def, original)
}

pub(crate) fn preflight_tokens(
    state: &GameState,
    controller: crate::ids::PlayerId,
    token_def: u16,
    original: usize,
) -> Result<(), String> {
    if original == 0 {
        return Ok(());
    }
    let token = CARD_DEFS
        .get(usize::from(token_def))
        .ok_or("excess token definition absent")?;
    if !(token.is_token && token.is_executable() && token.has_full_support()) {
        return Err("excess token definition is not executable Full token".into());
    }
    let replacement = crate::doubling_season_v48::ReplacementInputV48::capture(state, controller);
    let expanded = replacement.replaced_token_count(original)?;
    let final_objects = state
        .objects
        .len()
        .checked_add(expanded)
        .ok_or("excess token arena count overflow")?;
    if u64::try_from(final_objects).map_err(|_| "excess token arena count overflow")?
        > u64::from(u32::MAX) + 1
    {
        return Err("excess token arena id overflow".into());
    }
    let bytes = expanded
        .checked_mul(std::mem::size_of::<ProposedEvent>())
        .ok_or("excess token roster byte overflow")?;
    if bytes > isize::MAX as usize {
        return Err("excess token roster capacity overflow".into());
    }
    // Every member has this same definition and unchanged pre-entry board.
    // Validate one template without allocating an exponential expanded roster.
    let template = [ProposedEvent::create_token(token_def, controller)];
    crate::doubling_season_v48::preflight_entry_roster(state, &template)?;
    crate::doubling_season_v48::preflight_token_initialization(state, &template)?;
    Ok(())
}

pub(crate) fn lethal_threshold(
    state: &GameState,
    source: crate::ids::ObjectId,
    target: crate::ids::ObjectId,
) -> Result<i64, String> {
    let creature = creature_lethal_threshold(state, source, target);
    Ok(intersection(state, target, creature)?
        .map_or(creature, |input| input.effective_lethal_damage))
}

pub(crate) fn creature_lethal_threshold(
    state: &GameState,
    source: crate::ids::ObjectId,
    target: crate::ids::ObjectId,
) -> i64 {
    let live = state.objects.get(target);
    let remaining = (i64::from(crate::engine::effective_toughness(state, target))
        - i64::from(live.damage))
    .max(0);
    if crate::engine::has_effective_keyword(state, source, Keywords::DEATHTOUCH) {
        remaining.min(1)
    } else {
        remaining
    }
}

/// CR 120.10: greatest per-type excess means the smaller lethal threshold.
/// Loyalty is the unsigned incarnation-bound ledger before actual damage.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntersectionV1 {
    pub schema: String,
    pub creature_lethal_damage: i64,
    pub predamage_loyalty: u32,
    pub effective_lethal_damage: i64,
}
pub(crate) fn intersection(
    state: &GameState,
    target: crate::ids::ObjectId,
    creature: i64,
) -> Result<Option<IntersectionV1>, String> {
    if !crate::engine::object_has_type(state, target, CardType::Planeswalker) {
        return Ok(None);
    }
    if !crate::engine::object_has_type(state, target, CardType::Creature) {
        return Err("excess intersection lost creature type".into());
    }
    let loyalty = crate::planeswalker_v1::loyalty(state, target)
        .ok_or("excess intersection lost original loyalty ledger")?;
    Ok(Some(IntersectionV1 {
        schema: "mtg-kernel/goblin-excess-intersection/v1".into(),
        creature_lethal_damage: creature,
        predamage_loyalty: loyalty,
        effective_lethal_damage: creature.min(i64::from(loyalty)),
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicOrderingStageV1 {
    pub source: crate::rl::CardStableRefV1,
    pub target: crate::rl::CardStableRefV1,
    pub controller: crate::rl::PlayerSeatV1,
    pub chooser: crate::rl::PlayerSeatV1,
    pub original_x: u16,
    pub lethal_damage: i64,
    pub remaining_damage: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intersection: Option<IntersectionV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCompletedV1 {
    pub source: crate::rl::CardStableRefV1,
    pub target: crate::rl::CardStableRefV1,
    pub controller: crate::rl::PlayerSeatV1,
    pub original_x: u16,
    pub lethal_damage: i64,
    pub actual_damage: i32,
    pub excess: u32,
    pub tokens_created: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intersection: Option<IntersectionV1>,
}
pub fn public_ordering_stage(
    state: &GameState,
) -> Result<Option<PublicOrderingStageV1>, crate::rl::RlContractError> {
    owned::public_ordering_stage(state)
}
pub fn public_completed(
    state: &GameState,
) -> Result<Vec<PublicCompletedV1>, crate::rl::RlContractError> {
    owned::public_completed(state)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
