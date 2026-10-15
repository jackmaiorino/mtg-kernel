//! Earlier typed programs carried by the V40 union, without a frozen V8/V9 artifact.
use crate::{
    ids::PlayerId,
    rl::{PublicObservationProjectionV5, RlContractError},
    state::GameState,
};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EarlyFactsV40 {
    pub zombie_roots: Vec<crate::rules_meaning_soul_shackled_zombie_v1::ZombieRootMeaningV1>,
    pub zombie_contexts: Vec<ZombieContextV40>,
    pub keep_one: Option<crate::raid_entry_public_v1::KeepOneContextV1>,
    pub gorehorn_sources: Vec<crate::gorehorn_damage_v1::GorehornSourceV1>,
    pub raid_roots: Vec<crate::rules_meaning_raid_entry_v1::MeaningV1>,
    pub ooze_roots: Vec<crate::rules_meaning_ooze_v1::OozeRootMeaningV1>,
    pub heartfire_roots: Vec<crate::rules_meaning_heartfire_v1::HeartfireRootMeaningV1>,
    pub saved_heartfire_damage: Vec<crate::heartfire_public_v1::HeartfireSavedDamageV1>,
    pub mana_bundles: Vec<crate::bloom_tender_public_v1::BloomTenderManaBundleV1>,
}
#[derive(Clone, PartialEq)]
pub(crate) struct EarlyProjectionV40 {
    pub projection: PublicObservationProjectionV5,
    pub facts: EarlyFactsV40,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ZombieContextV40 {
    pub stage: crate::soul_shackled_zombie_public_v1::TriggerStageV1,
    pub source: crate::rl::CardStableRefV1,
    pub original_controller: crate::rl::PlayerSeatV1,
    pub captured_definition_colors: Vec<crate::mana::ManaColor>,
    pub selected: Vec<crate::rl::CardStableRefV1>,
    pub selected_graveyard_owner: Option<crate::rl::PlayerSeatV1>,
    pub selected_live_legal: Vec<bool>,
}
fn zombie_contexts(state: &GameState) -> Result<Vec<ZombieContextV40>, RlContractError> {
    let mut rows = Vec::new();
    for (ordinal, pending) in state.engine.pending_triggers.iter().enumerate() {
        if pending.effect != crate::effect::EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1 {
            continue;
        }
        crate::engine::validate_single_graveyard_public_pending(state, pending)
            .map_err(RlContractError)?;
        let source = pending
            .source_contract
            .ok_or_else(|| RlContractError("missing V40 Zombie source".into()))?;
        rows.push(crate::rl_session::akroma_zombie_context_v40(
            state,
            crate::soul_shackled_zombie_public_v1::TriggerStageV1::Pending {
                ordinal: ordinal as u64,
            },
            source,
            &pending.targets,
            &pending.target_contracts,
        )?);
    }
    for (ordinal, item) in state.stack.iter().enumerate() {
        if item.inline_effect.as_ref()
            != Some(&crate::effect::EffectOp::ExileSingleGraveyardTargetsThenDrainTwoV1)
        {
            continue;
        }
        crate::engine::validated_stack_item_target_spec(item, state).map_err(RlContractError)?;
        let source = item
            .v4
            .ability_source_contract
            .ok_or_else(|| RlContractError("missing V40 Zombie stack source".into()))?;
        rows.push(crate::rl_session::akroma_zombie_context_v40(
            state,
            crate::soul_shackled_zombie_public_v1::TriggerStageV1::Stack {
                ordinal: ordinal as u64,
            },
            source,
            &item.targets,
            &item.v4.target_contracts,
        )?);
    }
    Ok(rows
        .into_iter()
        .map(|row| ZombieContextV40 {
            stage: row.stage,
            source: row.source,
            original_controller: row.original_controller,
            captured_definition_colors: row.captured_definition_colors,
            selected: row.selected,
            selected_graveyard_owner: row.selected_graveyard_owner,
            selected_live_legal: row.selected_live_legal,
        })
        .collect())
}
pub(crate) fn build(
    state: &GameState,
    projection: PublicObservationProjectionV5,
    actor: PlayerId,
) -> Result<EarlyProjectionV40, RlContractError> {
    crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
    let keep_one = if state
        .engine
        .pending_effect
        .as_ref()
        .is_some_and(|pending| pending.ctx.controller == actor)
    {
        crate::effect::selection_shape(state)
            .map_err(RlContractError)?
            .map(
                |(minimum, maximum)| crate::raid_entry_public_v1::KeepOneContextV1 {
                    looked_count: maximum as u8,
                    graveyard_min: minimum,
                    graveyard_max: maximum,
                    retained_max: 1,
                    graveyard_ordered: true,
                },
            )
    } else {
        None
    };
    let mut mana_bundles = Vec::new();
    for row in &projection.surface.battlefield[actor.index()] {
        let def = &crate::card_def::CARD_DEFS[usize::from(row.stable.card_db_id)];
        let Some(ability) = def.mana_ability_def else {
            continue;
        };
        if ability.amount != crate::card_def::ManaAbilityAmountDef::EachControlledPermanentColor {
            continue;
        }
        if ability.cost != crate::card_def::ManaAbilityCostDef::TapSelf
            || ability.controller_damage != 0
            || ability.max_activations_per_turn.is_some()
            || def.mana_ability_choices != [crate::mana::ManaColor::G]
        {
            return Err(RlContractError("unsupported V40 bundle mana recipe".into()));
        }
        let id = crate::ids::ObjectId(row.stable.arena_id);
        if !crate::engine::available_mana_ability_choices(actor, id, state)
            .contains(&crate::mana::ManaColor::G)
        {
            continue;
        }
        let live = state.objects.get(id);
        if live.zone != crate::state::Zone::Battlefield
            || live.zone_change_count != row.stable.zone_change_count
            || live.card_def != row.stable.card_db_id
            || live.v4.face_index != row.face_index
            || crate::rl::PlayerSeatV1::from(live.controller) != row.stable.controller
        {
            return Err(RlContractError("changed V40 bundle source".into()));
        }
        let colors = crate::bloom_tender_mana_v1::colors(state, actor);
        let mask = projection.surface.battlefield[actor.index()]
            .iter()
            .fold(0, |m, card| m | card.characteristics.effective_color_mask);
        let visible_colors = [
            crate::mana::ManaColor::W,
            crate::mana::ManaColor::U,
            crate::mana::ManaColor::B,
            crate::mana::ManaColor::R,
            crate::mana::ManaColor::G,
        ]
        .into_iter()
        .filter(|color| mask & (1 << color.pool_index()) != 0)
        .collect::<Vec<_>>();
        if colors != visible_colors {
            return Err(RlContractError(
                "private bundle colors require a visibility successor".into(),
            ));
        }
        mana_bundles.push(crate::bloom_tender_public_v1::BloomTenderManaBundleV1 {
            source: row.stable.clone(),
            colors,
        });
    }
    let facts = EarlyFactsV40 {
        zombie_roots: crate::rules_meaning_soul_shackled_zombie_v1::catalog_roots(),
        zombie_contexts: zombie_contexts(state)?,
        keep_one,
        gorehorn_sources: crate::gorehorn_damage_v1::public_sources(state)
            .map_err(RlContractError)?,
        raid_roots: crate::raid_entry_public_v1::roots().map_err(RlContractError)?,
        ooze_roots: crate::rules_meaning_ooze_v1::catalog_roots(),
        heartfire_roots: crate::rules_meaning_heartfire_v1::catalog_roots(),
        saved_heartfire_damage: crate::heartfire_public_v1::saved_for_akroma_v40(
            state,
            &projection,
        )?,
        mana_bundles,
    };
    Ok(EarlyProjectionV40 { projection, facts })
}
