//! Unregistered Self-Reflection root using the accepted token-copy allocator.
use crate::card_def::{CardType, TargetSpec, CARD_DEFS};
use crate::effect::{EffectOp, ExecCtx};
use crate::state::{GameState, StackItem, StackItemKind, StackTargetContractV4, Target};

/// Copy-layer values are fixed before proposing entry. The anonymous case
/// deliberately carries neither a physical identity nor its hidden definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CopyTokenValuesV1 {
    AnonymousFaceDown,
    Definition { card_def: u16, face_index: u8 },
}
fn copiable_values(state: &GameState, object: crate::ids::ObjectId) -> CopyTokenValuesV1 {
    let live = state.objects.get(object);
    if live.v4.face_down_v1.is_some() {
        CopyTokenValuesV1::AnonymousFaceDown
    } else {
        CopyTokenValuesV1::Definition {
            card_def: live.card_def,
            face_index: live.v4.face_index,
        }
    }
}

pub(crate) const ANONYMOUS_BLUEPRINT: &str = "Nameless Creature Copy Token";
pub(crate) fn anonymous_blueprint_is_exact(card_def: u16) -> bool {
    let def = &CARD_DEFS[card_def as usize];
    def.name == ANONYMOUS_BLUEPRINT
        && def.object_name.is_empty()
        && def.has_full_support()
        && def.is_token
        && def.types == [CardType::Creature]
        && def.subtypes.is_empty()
        && def.supertypes.is_empty()
        && def.colors.is_empty()
        && def.cost.generic == 0
        && def.cost.pips.is_empty()
        && def.cost.x_count == 0
        && def.mana_value == 0
        && def.power == Some(2)
        && def.toughness == Some(2)
        && def.keywords == crate::card_def::Keywords::NONE
        && def.target_spec == TargetSpec::None
        && (def.spell_effect)().is_none()
        && (def.mana_ability)().is_none()
        && crate::trigger::triggers_for(card_def).is_empty()
        && !def.is_land
        && !def.changeling
        && !def.enters_battlefield_tapped
        && !def.spell_cannot_be_countered
        && !def.records_mana_spent
        && !def.mana_ability_includes_chosen_color
        && !def.delve
        && !def.cant_be_blocked_by_monarchs_creatures
        && def.minimum_blockers == 1
        && def.produces_mana.is_empty()
        && def.mana_ability_choices.is_empty()
        && def.activated_abilities.is_empty()
        && def.additional_modes.is_empty()
        && def.additional_mana_abilities.is_empty()
        && def.additional_mana_ability_conditions.is_empty()
        && def.restricted_mana_abilities.is_empty()
        && def.activated_ability_generic_reductions.is_empty()
        && def.generic_cost_reduction.is_none()
        && def.ward_cost.is_none()
        && def.equipment.is_none()
        && def.starting_loyalty.is_none()
        && def.enters_with_plus_one_counters.is_none()
        && def.controlled_counter_keyword.is_none()
        && def.alt_cost.is_none()
        && def.kicker_cost.is_none()
        && def.additional_cost.is_none()
        && def.flashback.is_none()
        && def.plot_cost.is_none()
        && def.madness_cost.is_none()
        && def.mode2.is_none()
        && def.mode3.is_none()
        && def.kicked_target_spec.is_none()
        && def.escape.is_none()
        && def.mana_ability_def.is_none()
        && def.omen.is_none()
        && def.as_enters_choose_color_other_than.is_none()
        && def.enters_battlefield_tapped_unless.is_none()
        && def.attachment.is_none()
        && def.transform_face.is_none()
        && def.saga.is_none()
        && def.optional_additional_cost.is_none()
        && def.bestow.is_none()
        && def.adventure.is_none()
        && def.conditional_tap_yield.is_none()
        && def.enters_tapped_unless_controller.is_none()
        && def.animation.is_none()
}

pub(crate) fn owns_spell(state: &GameState, item: &StackItem) -> bool {
    item.kind == StackItemKind::Spell
        && item.inline_effect.is_none()
        && state.objects.try_get(item.source).is_some_and(|source| {
            CARD_DEFS[source.card_def as usize].name == "Self-Reflection"
                && (CARD_DEFS[source.card_def as usize].spell_effect)()
                    == Some(EffectOp::CreateControlledCreatureTargetCopyV1)
        })
}
/// Reject corrupted finalized metadata before an ordinary priority action.
/// Pending placeholders use the same existing validator and retain its staged
/// cast semantics; departed target incarnations remain valid stale targets.
pub(crate) fn validate_live_stack(state: &GameState) -> Result<(), String> {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if !*ENABLED.get_or_init(|| CARD_DEFS.iter().any(|def| def.name == "Self-Reflection")) {
        return Ok(());
    }
    for item in &state.stack {
        // Kind and inline-effect compatibility are mutable fields to validate,
        // not gates which may hide a physically definition-owned spell.
        let owned = |card_def: u16| {
            CARD_DEFS.get(usize::from(card_def)).is_some_and(|def| {
                def.name == "Self-Reflection"
                    && (def.spell_effect)() == Some(EffectOp::CreateControlledCreatureTargetCopyV1)
            })
        };
        if state
            .objects
            .try_get(item.source)
            .is_some_and(|source| owned(source.card_def))
            || item
                .v4
                .source_contract
                .is_some_and(|source| owned(source.card_def))
        {
            crate::engine::validated_stack_item_target_spec(item, state)?;
        }
    }
    Ok(())
}
/// Copy layers change definition, not physical incarnation. Only this new
/// root accepts the existing exact-generation historical-definition authority.
/// Owner, zone, generation and virtual-copy origin remain independently checked.
pub(crate) fn normalize_historical_target(
    state: &GameState,
    mut contract: StackTargetContractV4,
) -> StackTargetContractV4 {
    if let StackTargetContractV4::Object {
        object,
        card_def,
        zone_change_count,
        ..
    } = &mut contract
    {
        if state
            .objects
            .try_get(*object)
            .is_some_and(|live| live.card_def != *card_def)
            && crate::standard_cards_v1::had_card_def(state, *object, *zone_change_count, *card_def)
        {
            *card_def = state.objects.get(*object).card_def;
        }
    }
    contract
}
pub(crate) fn target_matches_live(
    state: &GameState,
    target: Target,
    contract: StackTargetContractV4,
) -> bool {
    crate::engine::target_contract_matches_live(
        state,
        target,
        normalize_historical_target(state, contract),
    )
}
pub(crate) fn execute(state: &mut GameState, ctx: &ExecCtx) {
    let ([target @ Target::Object(object)], [contract]) =
        (ctx.targets.as_slice(), ctx.target_contracts.as_slice())
    else {
        return;
    };
    if !target_matches_live(state, *target, *contract)
        || !crate::engine::effect_target_is_legal(
            state,
            ctx.source,
            ctx.controller,
            TargetSpec::ControlledCreature,
            &ctx.targets,
            0,
        )
        || !crate::engine::object_has_type(state, *object, CardType::Creature)
    {
        return;
    }
    match copiable_values(state, *object) {
        CopyTokenValuesV1::AnonymousFaceDown => {
            let blueprint = crate::card_def::card_id_by_name(ANONYMOUS_BLUEPRINT).expect(
                "Self-Reflection admission requires its anonymous internal token blueprint",
            );
            assert!(anonymous_blueprint_is_exact(blueprint));
            // Initialization, replacements and ETB observers see only the
            // neutral definition. This token enters face up; no hidden card
            // identity or face-up permission is copied into its object state.
            crate::event::propose_and_commit(
                state,
                crate::event::ProposedEvent::create_token(blueprint, ctx.controller),
            );
        }
        CopyTokenValuesV1::Definition { .. } => {
            crate::standard_cards_v1::create_token_copy(state, *object, ctx.controller);
        }
    }
}

#[cfg(test)]
mod tests;
