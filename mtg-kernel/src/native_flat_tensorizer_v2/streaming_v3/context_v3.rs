//! Streaming equivalents of the engine and private policy context builders.
use super::*;
use FlatContextKindV2 as C;
use FlatContextSubroleV2 as S;
use FlatRelationRoleV2 as R;

fn enumeration(out: &mut Vec<u8>, index: usize, names: &[&str]) -> Result {
    append_json_v2(
        out,
        names.get(index).ok_or(NativeFlatTensorErrorV2::EnumRange)?,
    )
}

fn context_raw(value: Option<u32>) -> std::result::Result<usize, NativeFlatTensorErrorV2> {
    usize::try_from(value.ok_or(NativeFlatTensorErrorV2::ContextShape)?)
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)
}

fn reference_row(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    order: usize,
    relation: &FlatRelationV2,
    payload: FlatContextRelationDataV2,
    out: &mut Vec<u8>,
) -> Result {
    if usize::try_from(relation.primary_order).ok() != Some(order)
        || payload.target_kind != FlatTargetKindV2::None
        || payload.target_player != FlatRelativePlayerV1::None
    {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    let raw = context_raw(relation.target_object)?;
    projected_required_node_v2(relation.target_object, projection)?;
    stable(decision, raw, out)
}

#[allow(clippy::too_many_arguments)]
fn references(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    role: R,
    context: C,
    subrole: S,
    count: u32,
    out: &mut Vec<u8>,
) -> Result {
    let rows = context_rows_v2(decision, role, context, subrole)?;
    if usize::try_from(count).ok() != Some(rows.len()) {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    array(
        out,
        rows.into_iter().enumerate(),
        |(order, (row, payload)), out| {
            reference_row(decision, projection, order, row, payload, out)
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn optional_reference(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    role: R,
    context: C,
    subrole: S,
    present: bool,
    out: &mut Vec<u8>,
) -> Result {
    let rows = context_rows_v2(decision, role, context, subrole)?;
    match (present, rows.as_slice()) {
        (false, []) => append_json_v2(out, &()),
        (true, [(row, payload)]) => reference_row(decision, projection, 0, row, *payload, out),
        _ => Err(NativeFlatTensorErrorV2::ContextShape),
    }
}

fn target(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    row: &FlatRelationV2,
    payload: FlatContextRelationDataV2,
    out: &mut Vec<u8>,
) -> Result {
    match payload.target_kind {
        FlatTargetKindV2::Player => {
            if row.target_object.is_some() {
                return Err(NativeFlatTensorErrorV2::ContextShape);
            }
            object!(out;
                "player" => write_relative_player_json_v2(out, payload.target_player, false),
                "target_kind" => append_json_v2(out, "player")
            )
        }
        FlatTargetKindV2::Object => {
            let raw = context_raw(row.target_object)?;
            projected_required_node_v2(row.target_object, projection)?;
            object!(out;
                "object" => stable(decision, raw, out),
                "target_kind" => append_json_v2(out, "object")
            )
        }
        FlatTargetKindV2::None => Err(NativeFlatTensorErrorV2::ContextShape),
    }
}

fn targets(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    context: C,
    subrole: S,
    count: u32,
    out: &mut Vec<u8>,
) -> Result {
    let rows = context_rows_v2(decision, R::PendingContext, context, subrole)?;
    if usize::try_from(count).ok() != Some(rows.len()) {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    array(
        out,
        rows.into_iter().enumerate(),
        |(order, (row, payload)), out| {
            if usize::try_from(row.primary_order).ok() != Some(order) {
                return Err(NativeFlatTensorErrorV2::ContextShape);
            }
            target(decision, projection, row, payload, out)
        },
    )
}

fn elements(
    decision: FlatScoringDecisionViewV1<'_>,
    start: u32,
    count: u32,
    kind: FlatContextElementKindV2,
    out: &mut Vec<u8>,
) -> Result {
    let rows = checked_table_slice_v2(decision.context_path_elements(), start, count)?;
    array(out, rows.iter().enumerate(), |(order, row), out| {
        if row.context != C::PendingEffect
            || row.context_order != 0
            || row.kind != kind
            || usize::try_from(row.order).ok() != Some(order)
        {
            return Err(NativeFlatTensorErrorV2::ChildTableShape);
        }
        match kind {
            FlatContextElementKindV2::StructuralPath => append_json_v2(out, &row.value),
            FlatContextElementKindV2::LegalColor => {
                enumeration(out, usize::from(row.value), &MANA_COLORS_V1)
            }
        }
    })
}

pub(in crate::native_flat_tensorizer_v2) fn write_engine_context(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let engine = decision.globals().engine;
    let result = (|| {
        object!(out;
            "current_stage" => enumeration(out, usize::from(engine.current_stage), &ENGINE_STAGE_NAMES_V2),
            "last_mana_ability_activator_since_priority_boundary" => write_relative_player_json_v2(out, engine.last_mana_ability_activator, true),
            "mana_activity_since_priority_boundary" => append_json_v2(out, &engine.mana_activity_since_priority_boundary),
            "pending_activation" => optional(out, engine.pending_activation, |pending, out| object!(out;
                "ability_index" => append_json_v2(out, &pending.ability_index),
                "chosen_targets" => targets(decision, projection, C::PendingActivation, S::PendingActivationChosenTarget, pending.chosen_target_count, out),
                "controller" => write_relative_player_json_v2(out, pending.controller, false),
                "cost_discard_paid" => {
                    if pending.discard_paid_present {
                        references(decision, projection, R::PendingContext, C::PendingActivation, S::PendingActivationDiscarded, pending.discard_paid_count, out)
                    } else if pending.discard_paid_count != 0 {
                        Err(NativeFlatTensorErrorV2::ContextShape)
                    } else { append_json_v2(out, &()) }
                },
                "source" => optional_reference(decision, projection, R::PendingContext, C::PendingActivation, S::PendingActivationSource, pending.source_present, out)
            )),
            "pending_cast" => optional(out, engine.pending_cast, |pending, out| object!(out;
                "additional_cost_discarded" => {
                    if pending.discarded_present {
                        references(decision, projection, R::PendingContext, C::PendingCast, S::PendingCastDiscarded, pending.discarded_count, out)
                    } else if pending.discarded_count != 0 {
                        Err(NativeFlatTensorErrorV2::ContextShape)
                    } else { append_json_v2(out, &()) }
                },
                "cast_mode" => if pending.cast_mode == 0 { append_json_v2(out, &()) }
                    else { enumeration(out, usize::from(pending.cast_mode - 1), &CAST_MODES_V1) },
                "chosen_targets" => targets(decision, projection, C::PendingCast, S::PendingCastChosenTarget, pending.chosen_target_count, out),
                "controller" => write_relative_player_json_v2(out, pending.controller, false),
                "is_flashback" => append_json_v2(out, &pending.is_flashback),
                "kicked" => append_json_v2(out, &pending.kicked),
                "mode_chosen" => append_json_v2(out, &pending.mode_chosen),
                "origin_zone" => append_json_v2(out, zone_name_v1(pending.origin_zone)),
                "sacrifice_chosen" => references(decision, projection, R::PendingContext, C::PendingCast, S::PendingCastSacrificed, pending.sacrificed_count, out),
                "source" => optional_reference(decision, projection, R::PendingContext, C::PendingCast, S::PendingCastSource, pending.source_present, out)
            )),
            "pending_discard" => optional(out, engine.pending_discard, |pending, out| object!(out;
                "count" => append_json_v2(out, &pending.count),
                "player" => write_relative_player_json_v2(out, pending.player, false),
                "resume_source" => optional_reference(decision, projection, R::PendingContext, C::PendingDiscard, S::PendingDiscardResumeSource, pending.resume_source_present, out),
                "resume_stage" => enumeration(out, usize::from(pending.resume_stage), &DISCARD_RESUME_NAMES_V2)
            )),
            "pending_effect" => pending_effect(decision, projection, out),
            "pending_optional_cost" => optional(out, engine.pending_optional_cost, |pending, out| object!(out;
                "discard_cards" => append_json_v2(out, &pending.discard_cards),
                "discard_payable" => append_json_v2(out, &pending.discard_payable),
                "player" => write_relative_player_json_v2(out, pending.player, false),
                "sacrifice_lands" => append_json_v2(out, &pending.sacrifice_lands),
                "sacrifice_payable" => append_json_v2(out, &pending.sacrifice_payable),
                "source" => optional_reference(decision, projection, R::PendingContext, C::PendingOptionalCost, S::PendingOptionalCostSource, pending.source_present, out),
                "spell_resume_source" => optional_reference(decision, projection, R::PendingContext, C::PendingOptionalCost, S::PendingOptionalCostSpellResumeSource, pending.spell_resume_source_present, out),
                "spell_resume_zone" => optional(out, pending.spell_resume_zone, |zone, out| append_json_v2(out, zone_name_v1(zone)))
            )),
            "pending_optional_cost_sacrifice" => optional(out, engine.pending_optional_sacrifice, |pending, out| object!(out;
                "chosen" => references(decision, projection, R::PendingContext, C::PendingOptionalCostSacrifice, S::PendingOptionalSacrificeChosen, pending.chosen_count, out),
                "player" => write_relative_player_json_v2(out, pending.player, false),
                "remaining" => append_json_v2(out, &pending.remaining),
                "source" => optional_reference(decision, projection, R::PendingContext, C::PendingOptionalCostSacrifice, S::PendingOptionalSacrificeSource, pending.source_present, out),
                "spell_resume_source" => optional_reference(decision, projection, R::PendingContext, C::PendingOptionalCostSacrifice, S::PendingOptionalSacrificeSpellResumeSource, pending.spell_resume_source_present, out),
                "spell_resume_zone" => optional(out, pending.spell_resume_zone, |zone, out| append_json_v2(out, zone_name_v1(zone)))
            )),
            "pending_spell_copy" => pending_spell_copy(decision, projection, out),
            "pending_triggers" => pending_triggers(decision, projection, out),
            "priority_passes" => append_json_v2(out, &engine.priority_passes),
            "stack_activity_since_priority_boundary" => append_json_v2(out, &engine.stack_activity_since_priority_boundary),
            "stack_nonempty" => append_json_v2(out, &engine.stack_nonempty)
        )
    })();
    original_error(result, || canonical_engine_context_v2(decision, projection))
}

fn pending_spell_copy(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let Some(pending) = decision.globals().engine.pending_spell_copy else {
        return append_json_v2(out, &());
    };
    let inherited = context_rows_v2(
        decision,
        R::PendingContext,
        C::PendingSpellCopy,
        S::PendingSpellCopyInheritedTarget,
    )?;
    if inherited.len() != 1
        || inherited[0].0.primary_order != 0
        || inherited[0].1.target_kind != pending.inherited_target_kind
    {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    object!(out;
        "copy" => optional_reference(decision, projection, R::PendingContext, C::PendingSpellCopy, S::PendingSpellCopyCopy, pending.copy_present, out),
        "inherited_target" => target(decision, projection, inherited[0].0, inherited[0].1, out),
        "parent" => optional_reference(decision, projection, R::PendingContext, C::PendingSpellCopy, S::PendingSpellCopyParent, pending.parent_present, out),
        "player" => write_relative_player_json_v2(out, pending.player, false),
        "stage" => enumeration(out, usize::from(pending.stage), &SPELL_COPY_STAGE_NAMES_V2)
    )
}

fn pending_effect(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let Some(pending) = decision.globals().engine.pending_effect else {
        return append_json_v2(out, &());
    };
    object!(out;
        "choice" => optional(out, pending.choice, |choice, out| {
            use FlatContextElementKindV2::{LegalColor, StructuralPath};
            match choice {
                FlatPendingEffectChoiceV2::Options { player, path_start, path_count, option_count } => object!(out;
                    "choice_kind" => append_json_v2(out, "options"),
                    "option_count" => append_json_v2(out, &option_count),
                    "player" => write_relative_player_json_v2(out, player, false),
                    "structural_path" => elements(decision, path_start, path_count, StructuralPath, out)
                ),
                FlatPendingEffectChoiceV2::Targets { player, path_start, path_count, selected_count, legal_count, min_targets, max_targets, can_finish, ordered, purpose } => object!(out;
                    "can_finish" => append_json_v2(out, &can_finish),
                    "choice_kind" => append_json_v2(out, "targets"),
                    "legal_targets" => targets(decision, projection, C::PendingEffect, S::PendingEffectLegalTarget, legal_count, out),
                    "max_targets" => append_json_v2(out, &max_targets),
                    "min_targets" => append_json_v2(out, &min_targets),
                    "ordered" => append_json_v2(out, &ordered),
                    "player" => write_relative_player_json_v2(out, player, false),
                    "purpose" => enumeration(out, usize::from(purpose), &TARGET_PURPOSE_NAMES_V2),
                    "selected_targets" => targets(decision, projection, C::PendingEffect, S::PendingEffectSelectedTarget, selected_count, out),
                    "structural_path" => elements(decision, path_start, path_count, StructuralPath, out)
                ),
                FlatPendingEffectChoiceV2::Color { player, path_start, path_count, legal_color_start, legal_color_count } => object!(out;
                    "choice_kind" => append_json_v2(out, "color"),
                    "legal_colors" => elements(decision, legal_color_start, legal_color_count, LegalColor, out),
                    "player" => write_relative_player_json_v2(out, player, false),
                    "structural_path" => elements(decision, path_start, path_count, StructuralPath, out)
                ),
                FlatPendingEffectChoiceV2::Number { player, path_start, path_count, minimum, maximum } => object!(out;
                    "choice_kind" => append_json_v2(out, "number"),
                    "maximum" => append_json_v2(out, &maximum),
                    "minimum" => append_json_v2(out, &minimum),
                    "player" => write_relative_player_json_v2(out, player, false),
                    "structural_path" => elements(decision, path_start, path_count, StructuralPath, out)
                ),
                FlatPendingEffectChoiceV2::Boolean { player, path_start, path_count, default, purpose } => object!(out;
                    "choice_kind" => append_json_v2(out, "boolean"),
                    "default" => append_json_v2(out, &default),
                    "player" => write_relative_player_json_v2(out, player, false),
                    "purpose" => enumeration(out, usize::from(purpose), &BOOLEAN_PURPOSE_NAMES_V2),
                    "structural_path" => elements(decision, path_start, path_count, StructuralPath, out)
                ),
            }
        }),
        "controller" => write_relative_player_json_v2(out, pending.controller, false),
        "source" => optional_reference(decision, projection, R::PendingContext, C::PendingEffect, S::PendingEffectSource, pending.source_present, out)
    )
}

fn pending_triggers(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let mut rows = context_rows_v2(
        decision,
        R::PendingContext,
        C::PendingTrigger,
        S::PendingTriggerSource,
    )?;
    rows.sort_by_key(|(row, _)| row.primary_order);
    if usize::try_from(decision.globals().engine.pending_trigger_count).ok() != Some(rows.len()) {
        return Err(NativeFlatTensorErrorV2::ContextShape);
    }
    array(
        out,
        rows.into_iter().enumerate(),
        |(order, (row, payload)), out| {
            if usize::try_from(row.primary_order).ok() != Some(order) {
                return Err(NativeFlatTensorErrorV2::RelationOrder);
            }
            let trigger = match payload.trigger_kind {
                1 => "triggered_ability",
                2 => "madness_offer",
                _ => return Err(NativeFlatTensorErrorV2::EnumRange),
            };
            object!(out;
                "controller" => write_relative_player_json_v2(out, payload.controller, false),
                "kicked" => append_json_v2(out, &payload.kicked),
                "source" => optional(out, row.target_object, |raw, out| {
                    let raw = usize::try_from(raw).map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)?;
                    projected_required_node_v2(row.target_object, projection)?;
                    stable(decision, raw, out)
                }),
                "trigger_kind" => append_json_v2(out, trigger)
            )
        },
    )
}

pub(in crate::native_flat_tensorizer_v2) fn write_surface_context(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let surface = decision.globals().surface;
    let result = (|| {
        object!(out;
            "combat_priority_rearmed_by_mana_activity" => append_json_v2(out, &surface.combat_priority_rearmed_by_mana_activity),
            "combat_priority_rearmed_by_stack_activity" => append_json_v2(out, &surface.combat_priority_rearmed_by_stack_activity),
            "combat_priority_spent" => append_json_v2(out, &surface.combat_priority_spent),
            "current_stage" => enumeration(out, usize::from(surface.current_stage), &SURFACE_STAGE_NAMES_V2),
            "madness_cast_reprompt_source" => optional_reference(decision, projection, R::PrivateContext, C::MadnessCastReprompt, S::MadnessCastRepromptSource, surface.madness_cast_reprompt_source_present, out),
            "mana_activity_since_last_stack_change" => append_json_v2(out, &surface.mana_activity_since_last_stack_change),
            "mana_activity_since_round_open" => append_json_v2(out, &surface.mana_activity_since_round_open),
            "private_blockers" => private_blockers(decision, projection, out),
            "private_discard" => {
                match surface.private_discard_remaining_needed {
                    None => {
                        if surface.private_discard_chosen_count != 0 || surface.private_discard_remaining_count != 0 {
                            Err(NativeFlatTensorErrorV2::ContextShape)
                        } else { append_json_v2(out, &()) }
                    },
                    Some(remaining) => object!(out;
                        "chosen" => references(decision, projection, R::PrivateContext, C::PrivateDiscard, S::PrivateDiscardChosen, surface.private_discard_chosen_count, out),
                        "remaining_choices" => references(decision, projection, R::PrivateContext, C::PrivateDiscard, S::PrivateDiscardRemainingChoice, surface.private_discard_remaining_count, out),
                        "remaining_needed" => append_json_v2(out, &remaining)
                    ),
                }
            },
            "private_optional_cost" => {
                match (surface.private_optional_discard_payable, surface.private_optional_sacrifice_payable, surface.private_optional_stage) {
                    (None, None, None) => append_json_v2(out, &()),
                    (Some(discard), Some(sacrifice), Some(stage)) => object!(out;
                        "discard_payable" => append_json_v2(out, &discard),
                        "sacrifice_payable" => append_json_v2(out, &sacrifice),
                        "stage" => enumeration(out, usize::from(stage), &SURFACE_STAGE_NAMES_V2)
                    ),
                    _ => Err(NativeFlatTensorErrorV2::ContextShape),
                }
            },
            "stack_grew_since_round_open" => append_json_v2(out, &surface.stack_grew_since_round_open),
            "stack_length_changed_since_observed" => append_json_v2(out, &surface.stack_length_changed_since_observed)
        )
    })();
    original_error(result, || {
        canonical_surface_context_v2(decision, projection)
    })
}

fn private_blockers(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    if !decision.globals().surface.private_blockers_present {
        if decision.relations().iter().any(|row| {
            matches!(
                row.payload,
                FlatRelationPayloadV2::Context(FlatContextRelationDataV2 {
                    context: C::PrivateBlockers,
                    ..
                })
            )
        }) {
            return Err(NativeFlatTensorErrorV2::ContextShape);
        }
        return append_json_v2(out, &());
    }
    object!(out;
        "accumulated" => {
            let attackers = context_rows_v2(decision, R::PrivateContext, C::PrivateBlockers, S::PrivateBlockersAccumulatedAttacker)?;
            let blockers = context_rows_v2(decision, R::PrivateContext, C::PrivateBlockers, S::PrivateBlockersAccumulatedBlocker)?;
            if attackers.len() != blockers.len() { return Err(NativeFlatTensorErrorV2::ContextShape); }
            array(out, attackers.iter().zip(&blockers).enumerate(), |(order, ((attacker, _), (blocker, _))), out| {
                if usize::try_from(attacker.primary_order).ok() != Some(order)
                    || usize::try_from(blocker.primary_order).ok() != Some(order)
                    || attacker.associated_order != 0 || blocker.associated_order != 1
                { return Err(NativeFlatTensorErrorV2::RelationOrder); }
                out.push(b'[');
                stable(decision, context_raw(attacker.target_object)?, out)?;
                out.push(b',');
                stable(decision, context_raw(blocker.target_object)?, out)?;
                out.push(b']');
                Ok(())
            })
        },
        "current_attacker" => {
            let current = context_rows_v2(decision, R::PrivateContext, C::PrivateBlockers, S::PrivateBlockersCurrentAttacker)?;
            match current.as_slice() {
                [] => append_json_v2(out, &()),
                [(row, _)] => {
                    let raw = context_raw(row.target_object)?;
                    projected_required_node_v2(row.target_object, projection)?;
                    stable(decision, raw, out)
                },
                _ => Err(NativeFlatTensorErrorV2::ContextShape),
            }
        },
        "remaining" => {
            let attackers = context_rows_v2(decision, R::PrivateContext, C::PrivateBlockers, S::PrivateBlockersRemainingAttacker)?;
            let blockers = context_rows_v2(decision, R::PrivateContext, C::PrivateBlockers, S::PrivateBlockersRemainingBlocker)?;
            array(out, attackers.iter().enumerate(), |(order, (attacker, _)), out| {
                if usize::try_from(attacker.primary_order).ok() != Some(order) { return Err(NativeFlatTensorErrorV2::RelationOrder); }
                let mut matching = blockers.iter().filter(|(row, _)| usize::try_from(row.primary_order).ok() == Some(order)).collect::<Vec<_>>();
                matching.sort_by_key(|(row, _)| row.secondary_order);
                out.push(b'[');
                stable(decision, context_raw(attacker.target_object)?, out)?;
                out.push(b',');
                array(out, matching.into_iter().enumerate(), |(index, (row, _)), out| {
                    if usize::try_from(row.secondary_order).ok() != Some(index) { return Err(NativeFlatTensorErrorV2::RelationOrder); }
                    stable(decision, context_raw(row.target_object)?, out)
                })?;
                out.push(b']');
                Ok(())
            })
        }
    )
}

pub(in crate::native_flat_tensorizer_v2) fn write_policy_surface(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let policy = decision.globals().policy_surface;
    let result = (|| {
        object!(out;
            "current_stage" => enumeration(out, usize::from(policy.current_stage), &POLICY_STAGE_NAMES_V2),
            "private_combat_selection" => {
                if !policy.private_combat_present {
                    if policy.private_combat_attacker_present || policy.candidate_index != 0
                        || policy.candidate_count != 0 || policy.selected_count != 0 || policy.remaining_count != 0
                    { return Err(NativeFlatTensorErrorV2::ContextShape); }
                    append_json_v2(out, &())
                } else {
                    let current = context_rows_v2(decision, R::PrivateContext, C::PrivateCombatSelection, S::PrivateCombatCurrentCandidate)?;
                    if current.len() != 1 || current[0].0.primary_order != policy.candidate_index {
                        return Err(NativeFlatTensorErrorV2::ContextShape);
                    }
                    let current_raw = context_raw(current[0].0.target_object)?;
                    object!(out;
                        "attacker" => optional_reference(decision, projection, R::PrivateContext, C::PrivateCombatSelection, S::PrivateCombatAttacker, policy.private_combat_attacker_present, out),
                        "candidate_count" => append_json_v2(out, &policy.candidate_count),
                        "candidate_index" => append_json_v2(out, &policy.candidate_index),
                        "current_candidate" => stable(decision, current_raw, out),
                        "remaining_after_current" => references(decision, projection, R::PrivateContext, C::PrivateCombatSelection, S::PrivateCombatRemainingCandidate, policy.remaining_count, out),
                        "selected" => references(decision, projection, R::PrivateContext, C::PrivateCombatSelection, S::PrivateCombatSelected, policy.selected_count, out)
                    )
                }
            }
        )
    })();
    original_error(result, || canonical_policy_surface_v2(decision, projection))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flat_policy_v2::*;

    #[derive(Clone)]
    struct Fixture {
        globals: FlatGlobalsV2,
        objects: Vec<FlatObjectCoreV2>,
        relations: Vec<FlatRelationV2>,
        paths: Vec<FlatContextPathElementV2>,
        projection: ObjectProjectionV2,
    }

    impl Fixture {
        fn view(&self) -> FlatScoringDecisionViewV1<'_> {
            FlatScoringDecisionViewV1::new(
                &self.globals,
                &self.objects,
                &self.relations,
                &[],
                &[],
                &[],
                &[],
                &[],
                &self.paths,
                &[],
                &[],
            )
        }

        fn push(
            &mut self,
            role: R,
            context: C,
            subrole: S,
            object: Option<u32>,
            order: u32,
            associated: u32,
        ) {
            self.relations.push(FlatRelationV2 {
                role,
                target_object: object,
                primary_order: order,
                associated_order: associated,
                payload: FlatRelationPayloadV2::Context(FlatContextRelationDataV2 {
                    context,
                    subrole,
                    target_player: FlatRelativePlayerV1::None,
                    ..FlatContextRelationDataV2::default()
                }),
                ..FlatRelationV2::default()
            });
        }

        fn push_target(&mut self, context: C, subrole: S, object: Option<u32>, order: u32) {
            self.push(R::PendingContext, context, subrole, object, order, 0);
            let FlatRelationPayloadV2::Context(payload) =
                &mut self.relations.last_mut().unwrap().payload
            else {
                unreachable!()
            };
            payload.target_kind = if object.is_some() {
                FlatTargetKindV2::Object
            } else {
                FlatTargetKindV2::Player
            };
            payload.target_player = if object.is_some() {
                FlatRelativePlayerV1::None
            } else {
                FlatRelativePlayerV1::Opponent
            };
        }

        fn rich() -> Self {
            let mut f = Self {
                globals: FlatGlobalsV2::default(),
                objects: (0..3)
                    .map(|i| FlatObjectCoreV2 {
                        card_token: i + 1,
                        visible_ordinal: i,
                        owner: FlatRelativePlayerV1::SelfPlayer,
                        controller: FlatRelativePlayerV1::SelfPlayer,
                        zone: Some(FlatZoneV2::Battlefield),
                        ..FlatObjectCoreV2::default()
                    })
                    .collect(),
                relations: Vec::new(),
                paths: vec![
                    FlatContextPathElementV2 {
                        context: C::PendingEffect,
                        kind: FlatContextElementKindV2::StructuralPath,
                        value: 3,
                        ..FlatContextPathElementV2::default()
                    },
                    FlatContextPathElementV2 {
                        context: C::PendingEffect,
                        kind: FlatContextElementKindV2::StructuralPath,
                        order: 1,
                        value: 7,
                        ..FlatContextPathElementV2::default()
                    },
                    FlatContextPathElementV2 {
                        context: C::PendingEffect,
                        kind: FlatContextElementKindV2::LegalColor,
                        value: 2,
                        ..FlatContextPathElementV2::default()
                    },
                    FlatContextPathElementV2 {
                        context: C::PendingEffect,
                        kind: FlatContextElementKindV2::LegalColor,
                        order: 1,
                        value: 5,
                        ..FlatContextPathElementV2::default()
                    },
                ],
                projection: ObjectProjectionV2 {
                    raw_to_node: vec![Some(0), Some(1), Some(2)],
                    node_to_raw: vec![0, 1, 2],
                },
            };
            f.globals.engine = FlatEngineGlobalsV2 {
                priority_passes: [true, false],
                stack_nonempty: true,
                stack_activity_since_priority_boundary: true,
                mana_activity_since_priority_boundary: true,
                last_mana_ability_activator: FlatRelativePlayerV1::Opponent,
                current_stage: 1,
                pending_cast: Some(FlatPendingCastGlobalsV2 {
                    source_present: true,
                    chosen_target_count: 1,
                    is_flashback: true,
                    cast_mode: 1,
                    discarded_present: true,
                    discarded_count: 1,
                    mode_chosen: Some(2),
                    origin_zone: FlatZoneV2::Graveyard,
                    sacrificed_count: 1,
                    kicked: Some(false),
                    ..FlatPendingCastGlobalsV2::default()
                }),
                pending_activation: Some(FlatPendingActivationGlobalsV2 {
                    source_present: true,
                    ability_index: 2,
                    chosen_target_count: 1,
                    discard_paid_present: true,
                    discard_paid_count: 1,
                    ..FlatPendingActivationGlobalsV2::default()
                }),
                pending_discard: Some(FlatPendingDiscardGlobalsV2 {
                    count: 2,
                    resume_source_present: true,
                    ..FlatPendingDiscardGlobalsV2::default()
                }),
                pending_optional_cost: Some(FlatPendingOptionalCostGlobalsV2 {
                    source_present: true,
                    discard_cards: 1,
                    sacrifice_lands: 2,
                    discard_payable: true,
                    sacrifice_payable: false,
                    spell_resume_source_present: true,
                    spell_resume_zone: Some(FlatZoneV2::Hand),
                    ..FlatPendingOptionalCostGlobalsV2::default()
                }),
                pending_optional_sacrifice: Some(FlatPendingOptionalSacrificeGlobalsV2 {
                    source_present: true,
                    remaining: 2,
                    chosen_count: 1,
                    spell_resume_source_present: true,
                    spell_resume_zone: Some(FlatZoneV2::Hand),
                    ..FlatPendingOptionalSacrificeGlobalsV2::default()
                }),
                pending_spell_copy: Some(FlatPendingSpellCopyGlobalsV2 {
                    parent_present: true,
                    inherited_target_kind: FlatTargetKindV2::Object,
                    copy_present: true,
                    ..FlatPendingSpellCopyGlobalsV2::default()
                }),
                pending_effect: Some(FlatPendingEffectGlobalsV2 {
                    source_present: true,
                    ..FlatPendingEffectGlobalsV2::default()
                }),
                pending_trigger_count: 2,
            };
            for (context, subrole) in [
                (C::PendingCast, S::PendingCastSource),
                (C::PendingCast, S::PendingCastDiscarded),
                (C::PendingCast, S::PendingCastSacrificed),
                (C::PendingActivation, S::PendingActivationSource),
                (C::PendingActivation, S::PendingActivationDiscarded),
                (C::PendingDiscard, S::PendingDiscardResumeSource),
                (C::PendingOptionalCost, S::PendingOptionalCostSource),
                (
                    C::PendingOptionalCost,
                    S::PendingOptionalCostSpellResumeSource,
                ),
                (
                    C::PendingOptionalCostSacrifice,
                    S::PendingOptionalSacrificeSource,
                ),
                (
                    C::PendingOptionalCostSacrifice,
                    S::PendingOptionalSacrificeChosen,
                ),
                (
                    C::PendingOptionalCostSacrifice,
                    S::PendingOptionalSacrificeSpellResumeSource,
                ),
                (C::PendingSpellCopy, S::PendingSpellCopyParent),
                (C::PendingSpellCopy, S::PendingSpellCopyCopy),
                (C::PendingEffect, S::PendingEffectSource),
            ] {
                f.push(R::PendingContext, context, subrole, Some(0), 0, 0);
            }
            f.push_target(C::PendingCast, S::PendingCastChosenTarget, Some(1), 0);
            f.push_target(
                C::PendingActivation,
                S::PendingActivationChosenTarget,
                None,
                0,
            );
            f.push_target(
                C::PendingSpellCopy,
                S::PendingSpellCopyInheritedTarget,
                Some(1),
                0,
            );
            f.push_target(C::PendingEffect, S::PendingEffectSelectedTarget, Some(1), 0);
            f.push_target(C::PendingEffect, S::PendingEffectLegalTarget, None, 0);
            f.push_target(C::PendingEffect, S::PendingEffectLegalTarget, Some(2), 1);
            for index in 0..2 {
                f.push(
                    R::PendingContext,
                    C::PendingTrigger,
                    S::PendingTriggerSource,
                    (index == 0).then_some(0),
                    index,
                    0,
                );
                let FlatRelationPayloadV2::Context(p) =
                    &mut f.relations.last_mut().unwrap().payload
                else {
                    unreachable!()
                };
                p.trigger_kind = index as u8 + 1;
                p.kicked = index == 0;
            }
            f.globals.surface = FlatSurfaceGlobalsV2 {
                current_stage: 1,
                combat_priority_spent: [false, true],
                combat_priority_rearmed_by_stack_activity: true,
                combat_priority_rearmed_by_mana_activity: true,
                stack_grew_since_round_open: true,
                mana_activity_since_round_open: true,
                stack_length_changed_since_observed: Some(false),
                mana_activity_since_last_stack_change: true,
                madness_cast_reprompt_source_present: true,
                private_blockers_present: true,
                private_discard_remaining_needed: Some(2),
                private_discard_chosen_count: 1,
                private_discard_remaining_count: 2,
                private_optional_discard_payable: Some(true),
                private_optional_sacrifice_payable: Some(false),
                private_optional_stage: Some(2),
            };
            for (context, subrole, raw, primary, associated) in [
                (
                    C::MadnessCastReprompt,
                    S::MadnessCastRepromptSource,
                    0,
                    0,
                    0,
                ),
                (
                    C::PrivateBlockers,
                    S::PrivateBlockersCurrentAttacker,
                    0,
                    0,
                    0,
                ),
                (
                    C::PrivateBlockers,
                    S::PrivateBlockersAccumulatedAttacker,
                    0,
                    0,
                    0,
                ),
                (
                    C::PrivateBlockers,
                    S::PrivateBlockersAccumulatedBlocker,
                    1,
                    0,
                    1,
                ),
                (
                    C::PrivateBlockers,
                    S::PrivateBlockersRemainingAttacker,
                    0,
                    0,
                    0,
                ),
                (
                    C::PrivateBlockers,
                    S::PrivateBlockersRemainingBlocker,
                    1,
                    0,
                    0,
                ),
                (C::PrivateDiscard, S::PrivateDiscardChosen, 0, 0, 0),
                (C::PrivateDiscard, S::PrivateDiscardRemainingChoice, 1, 0, 0),
                (C::PrivateDiscard, S::PrivateDiscardRemainingChoice, 2, 1, 0),
                (C::PrivateCombatSelection, S::PrivateCombatAttacker, 0, 0, 0),
                (
                    C::PrivateCombatSelection,
                    S::PrivateCombatCurrentCandidate,
                    1,
                    1,
                    0,
                ),
                (C::PrivateCombatSelection, S::PrivateCombatSelected, 0, 0, 0),
                (
                    C::PrivateCombatSelection,
                    S::PrivateCombatRemainingCandidate,
                    2,
                    0,
                    0,
                ),
            ] {
                f.push(
                    R::PrivateContext,
                    context,
                    subrole,
                    Some(raw),
                    primary,
                    associated,
                );
            }
            f.push(
                R::PrivateContext,
                C::PrivateBlockers,
                S::PrivateBlockersRemainingBlocker,
                Some(2),
                0,
                0,
            );
            f.relations.last_mut().unwrap().secondary_order = 1;
            f.globals.policy_surface = FlatPolicySurfaceGlobalsV2 {
                current_stage: 1,
                private_combat_present: true,
                private_combat_attacker_present: true,
                candidate_index: 1,
                candidate_count: 3,
                selected_count: 1,
                remaining_count: 1,
            };
            f
        }
    }

    type Writer =
        for<'a> fn(FlatScoringDecisionViewV1<'a>, &ObjectProjectionV2, &mut Vec<u8>) -> Result;
    type Reference = for<'a> fn(
        FlatScoringDecisionViewV1<'a>,
        &ObjectProjectionV2,
    ) -> std::result::Result<Value, NativeFlatTensorErrorV2>;

    fn check(f: &Fixture, expect_success: bool) {
        let cases: [(&str, Writer, Reference); 3] = [
            ("engine", write_engine_context, canonical_engine_context_v2),
            (
                "surface",
                write_surface_context,
                canonical_surface_context_v2,
            ),
            ("policy", write_policy_surface, canonical_policy_surface_v2),
        ];
        for (name, write, reference) in cases {
            let expected = reference(f.view(), &f.projection);
            if expect_success {
                assert!(expected.is_ok(), "{name}: {expected:?}");
            }
            let mut output = b"retained-prefix:".to_vec();
            let actual = write(f.view(), &f.projection, &mut output);
            match expected {
                Ok(value) => {
                    actual.unwrap();
                    assert_eq!(&output[..16], b"retained-prefix:");
                    assert_eq!(&output[16..], serde_json::to_vec(&value).unwrap(), "{name}");
                }
                Err(error) => assert_eq!(actual, Err(error), "{name}"),
            }
        }
    }

    #[test]
    fn streamed_contexts_match_nonempty_nested_reference_bytes() {
        let mut f = Fixture::rich();
        let player = FlatRelativePlayerV1::SelfPlayer;
        for choice in [
            FlatPendingEffectChoiceV2::Options {
                player,
                path_start: 0,
                path_count: 2,
                option_count: 3,
            },
            FlatPendingEffectChoiceV2::Targets {
                player,
                path_start: 0,
                path_count: 2,
                selected_count: 1,
                legal_count: 2,
                min_targets: 1,
                max_targets: 2,
                can_finish: true,
                ordered: true,
                purpose: 0,
            },
            FlatPendingEffectChoiceV2::Color {
                player,
                path_start: 0,
                path_count: 2,
                legal_color_start: 2,
                legal_color_count: 2,
            },
            FlatPendingEffectChoiceV2::Number {
                player,
                path_start: 0,
                path_count: 2,
                minimum: -3,
                maximum: 17,
            },
            FlatPendingEffectChoiceV2::Boolean {
                player,
                path_start: 0,
                path_count: 2,
                default: Some(false),
                purpose: 0,
            },
        ] {
            f.globals.engine.pending_effect.as_mut().unwrap().choice = Some(choice);
            check(&f, true);
        }
        // The copy target's player variant has a different canonical shape.
        f.globals
            .engine
            .pending_spell_copy
            .as_mut()
            .unwrap()
            .inherited_target_kind = FlatTargetKindV2::Player;
        for row in &mut f.relations {
            if let FlatRelationPayloadV2::Context(payload) = &mut row.payload {
                if payload.subrole == S::PendingSpellCopyInheritedTarget {
                    row.target_object = None;
                    payload.target_kind = FlatTargetKindV2::Player;
                    payload.target_player = FlatRelativePlayerV1::Opponent;
                }
            }
        }
        check(&f, true);
    }

    #[test]
    fn streamed_contexts_preserve_competing_error_precedence() {
        let mut f = Fixture::rich();
        f.globals.engine.current_stage = u8::MAX;
        f.globals
            .engine
            .pending_cast
            .as_mut()
            .unwrap()
            .discarded_count = 2;
        f.globals.surface.current_stage = u8::MAX;
        f.globals.surface.private_blockers_present = false;
        f.globals.policy_surface.current_stage = u8::MAX;
        f.globals.policy_surface.private_combat_present = false;
        check(&f, false);
        assert_eq!(
            canonical_engine_context_v2(f.view(), &f.projection),
            Err(NativeFlatTensorErrorV2::ContextShape)
        );
        assert_eq!(
            canonical_surface_context_v2(f.view(), &f.projection),
            Err(NativeFlatTensorErrorV2::ContextShape)
        );
        assert_eq!(
            canonical_policy_surface_v2(f.view(), &f.projection),
            Err(NativeFlatTensorErrorV2::ContextShape)
        );
    }
}
