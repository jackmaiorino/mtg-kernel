//! Canonical JSON emitted directly into the caller's retained byte buffer.
//!
//! Members are written in serde_json::Map's lexicographic order. Only rows
//! whose canonical ordering depends on their encoded bytes need temporary
//! buffers. The Value builders remain the reference for malformed-input error
//! precedence and tests; successful writes never construct a Value tree.

use super::*;

type Result = std::result::Result<(), NativeFlatTensorErrorV2>;

macro_rules! object {
    ($out:ident; $first:literal => $value:expr $(, $key:literal => $rest:expr)* $(,)?) => {{
        $out.extend_from_slice(concat!("{\"", $first, "\":").as_bytes());
        $value?;
        $(
            $out.extend_from_slice(concat!(",\"", $key, "\":").as_bytes());
            $rest?;
        )*
        $out.push(b'}');
        Ok::<(), NativeFlatTensorErrorV2>(())
    }};
}

mod context_v3;
pub(super) use context_v3::{write_engine_context, write_policy_surface, write_surface_context};

fn array<T>(
    out: &mut Vec<u8>,
    rows: impl IntoIterator<Item = T>,
    mut write: impl FnMut(T, &mut Vec<u8>) -> Result,
) -> Result {
    out.push(b'[');
    for (index, row) in rows.into_iter().enumerate() {
        if index != 0 {
            out.push(b',');
        }
        write(row, out)?;
    }
    out.push(b']');
    Ok(())
}

fn optional<T>(
    out: &mut Vec<u8>,
    value: Option<T>,
    write: impl FnOnce(T, &mut Vec<u8>) -> Result,
) -> Result {
    match value {
        Some(value) => write(value, out),
        None => append_json_v2(out, &()),
    }
}

fn sorted<T>(
    out: &mut Vec<u8>,
    rows: impl IntoIterator<Item = T>,
    mut write: impl FnMut(T, &mut Vec<u8>) -> Result,
) -> Result {
    let mut encoded = Vec::new();
    for row in rows {
        let mut bytes = Vec::new();
        write(row, &mut bytes)?;
        encoded.push(bytes);
    }
    encoded.sort();
    array(out, encoded, |bytes, out| {
        out.extend_from_slice(&bytes);
        Ok(())
    })
}

fn raw(value: Option<u32>) -> std::result::Result<usize, NativeFlatTensorErrorV2> {
    usize::try_from(value.ok_or(NativeFlatTensorErrorV2::RelationShape)?)
        .map_err(|_| NativeFlatTensorErrorV2::CheckedIntegerRange)
}

fn stable(decision: FlatScoringDecisionViewV1<'_>, index: usize, out: &mut Vec<u8>) -> Result {
    write_canonical_stable_ref_v2(decision, index, None, out)
}

// Error paths may encounter fields in JSON order rather than construction
// order. Resolve an error through the original builder only on that cold path.
fn original_error(
    result: Result,
    reference: impl FnOnce() -> std::result::Result<Value, NativeFlatTensorErrorV2>,
) -> Result {
    result.map_err(|error| reference().err().unwrap_or(error))
}

macro_rules! extension_writer {
    ($name:ident, $view:ty, $reference:ident) => {
        pub(super) fn $name(view: $view, out: &mut Vec<u8>) -> Result {
            let decision = view.common();
            let ext = view.extensions();
            let result = (|| {
                object!(out;
                    "decision_local_library" => optional(out, ext.decision_local_library.as_ref(), |library, out| {
                        object!(out;
                            "cards" => array(out, &library.object_indices, |&i, out| object!(out; "stable" => stable(decision, i as usize, out))),
                            "chooser" => write_relative_player_json_v2(out, library.chooser, false),
                            "library_owner" => write_relative_player_json_v2(out, library.library_owner, false)
                        )
                    }),
                    "finalized_chosen_creature_costs" => array(out, &ext.finalized_chosen_creature_costs, |cost, out| {
                        object!(out;
                            "chosen" => stable(decision, cost.chosen_object as usize, out),
                            "power_lki" => append_json_v2(out, &cost.power_lki),
                            "source" => stable(decision, cost.source_object as usize, out),
                            "stack_index" => append_json_v2(out, &cost.stack_index)
                        )
                    }),
                    "historical_public_sources" => array(out, &ext.historical_public_sources, |source, out| {
                        object!(out;
                            "context" => append_json_v2(out, &source.context),
                            "source" => stable(decision, source.model_object_index as usize, out),
                            "stack_item_kind" => append_json_v2(out, &source.stack_item_kind)
                        )
                    }),
                    "pending_cast_object_cost" => optional(out, ext.pending_cast_object_cost.as_ref(), |cost, out| {
                        object!(out;
                            "cast_method" => append_json_v2(out, &cost.cast_method),
                            "controller" => write_relative_player_json_v2(out, cost.controller, false),
                            "cost_kind" => append_json_v2(out, &cost.cost_kind),
                            "remaining_count" => append_json_v2(out, &cost.remaining_count),
                            "required_count" => append_json_v2(out, &cost.required_count),
                            "selected" => array(out, &cost.selected_objects, |&i, out| stable(decision, i as usize, out)),
                            "source" => stable(decision, cost.source_object as usize, out)
                        )
                    }),
                    "pending_chosen_creature_cost" => optional(out, ext.pending_chosen_creature_cost.as_ref(), |cost, out| {
                        object!(out;
                            "controller" => write_relative_player_json_v2(out, cost.controller, false),
                            "selected_zone" => append_json_v2(out, &cost.selected_zone),
                            "source" => stable(decision, cost.source_object as usize, out)
                        )
                    })
                )?;
                // Absent Ward members must be omitted, not emitted as null.
                out.pop();
                if let Some(payment) = &ext.pending_ward_payment {
                    out.extend_from_slice(br#","pending_ward_payment":"#);
                    ward(payment, out)?;
                }
                if !ext.queued_ward_payments.is_empty() {
                    out.extend_from_slice(br#","queued_ward_payments":"#);
                    array(out, &ext.queued_ward_payments, |queued, out| {
                        object!(out;
                            "payment" => ward(&queued.payment, out),
                            "stack_index" => append_json_v2(out, &queued.stack_index)
                        )
                    })?;
                }
                out.push(b'}');
                Ok(())
            })();
            original_error(result, || $reference(view))
        }
    };
}

extension_writer!(
    write_extensions_v3,
    crate::flat_policy_v3::FlatScoringDecisionViewV3<'_>,
    canonical_extensions_v3
);
extension_writer!(
    write_extensions_v4,
    crate::flat_policy_v4::FlatScoringDecisionViewV4<'_>,
    canonical_extensions_v4
);

fn ward(payment: &crate::flat_policy_v3::FlatWardPaymentV3, out: &mut Vec<u8>) -> Result {
    object!(out;
        "generic" => append_json_v2(out, &payment.generic),
        "payer" => write_relative_player_json_v2(out, payment.payer, false),
        "targeting_stack_index" => append_json_v2(out, &payment.targeting_stack_index)
    )
}

pub(super) fn player_status(decision: FlatScoringDecisionViewV1<'_>, out: &mut Vec<u8>) -> Result {
    let result = array(
        out,
        decision.globals().players.iter().enumerate(),
        |(index, player), out| {
            let rows = checked_table_slice_v2(
                decision.completed_dungeons(),
                player.completed_dungeon_start,
                player.completed_dungeon_count,
            )?;
            if rows.iter().any(|row| row.player as usize != index) {
                return Err(NativeFlatTensorErrorV2::ChildTableShape);
            }
            object!(out;
                "draws_this_turn" => append_json_v2(out, &player.draws_this_turn),
                "drew_from_empty" => append_json_v2(out, &player.drew_from_empty),
                "dungeon" => object!(out;
                    "completed_dungeons" => sorted(out, rows, |row, out| append_json_v2(out, &row.dungeon_id)),
                    "dungeon_id" => append_json_v2(out, &player.dungeon_id),
                    "room_id" => append_json_v2(out, &player.room_id)
                ),
                "has_lost" => append_json_v2(out, &player.has_lost),
                "lands_played_this_turn" => append_json_v2(out, &player.lands_played_this_turn),
                "spells_cast_this_turn" => append_json_v2(out, &player.spells_cast_this_turn)
            )
        },
    );
    original_error(result, || canonical_player_status_v2(decision))
}

pub(super) fn object_relations(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let result = sorted(
        out,
        decision.relations().iter().filter(|row| {
            matches!(
                row.role,
                FlatRelationRoleV2::AttachedTo | FlatRelationRoleV2::ExiledBy
            )
        }),
        |row, out| {
            let source = raw(row.source_object)?;
            let target = raw(row.target_object)?;
            projected_required_node_v2(row.source_object, projection)?;
            projected_required_node_v2(row.target_object, projection)?;
            let name = if row.role == FlatRelationRoleV2::AttachedTo {
                "attached_to"
            } else {
                "exiled_by"
            };
            out.push(b'{');
            append_json_v2(out, name)?;
            out.push(b':');
            stable(decision, target, out)?;
            out.extend_from_slice(br#","object":"#);
            stable(decision, source, out)?;
            out.extend_from_slice(br#","relation_kind":"#);
            append_json_v2(out, name)?;
            out.push(b'}');
            Ok(())
        },
    );
    original_error(result, || {
        canonical_object_relations_v2(decision, projection)
    })
}

pub(super) fn permissions(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let result = sorted(
        out,
        decision
            .relations()
            .iter()
            .filter(|row| row.role == FlatRelationRoleV2::Permission),
        |row, out| {
            let payload = match row.payload {
                FlatRelationPayloadV2::Permission(payload) => payload,
                _ => return Err(NativeFlatTensorErrorV2::RelationShape),
            };
            let index = raw(row.target_object)?;
            projected_required_node_v2(row.target_object, projection)?;
            object!(out;
                "expiry" => match payload.expiry {
                    0 => object!(out; "expiry_kind" => append_json_v2(out, "end_of_turn")),
                    1 => object!(out;
                        "expiry_kind" => append_json_v2(out, "until_holders_next_turn"),
                        "holder_turn_started" => append_json_v2(out, &payload.holder_turn_started)
                    ),
                    _ => Err(NativeFlatTensorErrorV2::EnumRange),
                },
                "holder" => write_relative_player_json_v2(out, payload.holder, false),
                "object" => stable(decision, index, out),
                "play_or_cast" => write_enum_json_v2(out, usize::from(payload.play_or_cast), &["play", "cast"])
            )
        },
    );
    original_error(result, || canonical_permissions_v2(decision, projection))
}

pub(super) fn stack(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let result = array(
        out,
        decision
            .relations()
            .iter()
            .filter(|row| row.role == FlatRelationRoleV2::StackTarget && row.secondary_order == 0)
            .enumerate(),
        |(index, baseline), out| {
            if usize::try_from(baseline.primary_order).ok() != Some(index) {
                return Err(NativeFlatTensorErrorV2::RelationOrder);
            }
            let payload = match baseline.payload {
                FlatRelationPayloadV2::Stack(payload) => payload,
                _ => return Err(NativeFlatTensorErrorV2::RelationShape),
            };
            let source = raw(baseline.source_object)?;
            projected_required_node_v2(baseline.source_object, projection)?;
            let mut targets = decision
                .relations()
                .iter()
                .filter(|row| {
                    row.role == FlatRelationRoleV2::StackTarget
                        && row.primary_order == baseline.primary_order
                        && row.secondary_order > 0
                })
                .collect::<Vec<_>>();
            targets.sort_by_key(|row| row.secondary_order);
            let mut paid = decision
                .relations()
                .iter()
                .filter(|row| {
                    row.role == FlatRelationRoleV2::PaidCost
                        && row.primary_order == baseline.primary_order
                })
                .collect::<Vec<_>>();
            paid.sort_by_key(|row| row.secondary_order);
            object!(out;
                "cast_method" => if payload.cast_method == 0 { append_json_v2(out, &()) } else { write_enum_json_v2(out, usize::from(payload.cast_method - 1), &CAST_METHOD_NAMES_V2) },
                "controller" => write_relative_player_json_v2(out, payload.controller, false),
                "face_index" => append_json_v2(out, &payload.face_index),
                "is_copy" => append_json_v2(out, &payload.is_copy),
                "is_flashback" => append_json_v2(out, &payload.is_flashback),
                "kicked" => append_json_v2(out, &payload.kicked),
                "madness_offer" => append_json_v2(out, &payload.madness_offer),
                "mode_chosen" => append_json_v2(out, &payload.mode_chosen),
                "paid_cost_refs" => array(out, paid.iter().enumerate(), |(index, row), out| {
                    if usize::try_from(row.secondary_order).ok() != Some(index) { return Err(NativeFlatTensorErrorV2::RelationOrder); }
                    stable(decision, raw(row.target_object)?, out)
                }),
                "source" => stable(decision, source, out),
                "stack_item_kind" => write_enum_json_v2(out, usize::from(payload.stack_item_kind), &STACK_KIND_NAMES_V2),
                "targets" => array(out, targets.iter().enumerate(), |(index, row), out| {
                    if usize::try_from(row.secondary_order).ok() != Some(index + 1) { return Err(NativeFlatTensorErrorV2::RelationOrder); }
                    let target = match row.payload {
                        FlatRelationPayloadV2::Stack(target) => target,
                        _ => return Err(NativeFlatTensorErrorV2::RelationShape),
                    };
                    if target.controller != payload.controller || target.stack_item_kind != payload.stack_item_kind
                        || target.is_copy != payload.is_copy || target.is_flashback != payload.is_flashback
                        || target.mode_chosen != payload.mode_chosen || target.madness_offer != payload.madness_offer
                        || target.kicked != payload.kicked || target.cast_method != payload.cast_method
                        || target.face_index != payload.face_index || target.x_value != payload.x_value {
                        return Err(NativeFlatTensorErrorV2::RelationShape);
                    }
                    match target.target_kind {
                        FlatTargetKindV2::Player => object!(out;
                            "player" => write_relative_player_json_v2(out, target.target_player, false),
                            "target_kind" => append_json_v2(out, "player")
                        ),
                        FlatTargetKindV2::Object => object!(out;
                            "object" => write_canonical_stable_ref_v2(decision, raw(row.target_object)?, Some(target.target_object_controller), out),
                            "target_kind" => append_json_v2(out, "object")
                        ),
                        FlatTargetKindV2::None => Err(NativeFlatTensorErrorV2::RelationShape),
                    }
                }),
                "x_value" => append_json_v2(out, &payload.x_value)
            )
        },
    );
    original_error(result, || canonical_stack_v2(decision, projection))
}

pub(super) fn combat(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let result = (|| {
        let mut attackers = decision
            .relations()
            .iter()
            .filter(|row| row.role == FlatRelationRoleV2::CombatAttacker)
            .collect::<Vec<_>>();
        attackers.sort_by_key(|row| row.primary_order);
        let mut blocked = Vec::new();
        for (index, row) in attackers.iter().enumerate() {
            if usize::try_from(row.primary_order).ok() != Some(index) {
                return Err(NativeFlatTensorErrorV2::RelationOrder);
            }
            let index = raw(row.target_object)?;
            projected_required_node_v2(row.target_object, projection)?;
            match row.payload {
                FlatRelationPayloadV2::CombatAttacker {
                    blocked_order: Some(order),
                } => blocked.push((order, index)),
                FlatRelationPayloadV2::CombatAttacker {
                    blocked_order: None,
                } => {}
                _ => return Err(NativeFlatTensorErrorV2::RelationShape),
            }
        }
        blocked.sort_by_key(|(order, _)| *order);
        let pairs = relation_pairs_v2(decision.relations(), FlatRelationRoleV2::CombatBlocker)?;
        object!(out;
            "attacker_to_ordered_blockers" => array(out, blocked.iter().enumerate(), |(mapping_order, &(stored_order, attacker_raw)), out| {
                if usize::try_from(stored_order).ok() != Some(mapping_order) { return Err(NativeFlatTensorErrorV2::RelationOrder); }
                let mut blockers = pairs.iter().filter_map(|pair| {
                    let attacker = pair.iter().find(|row| row.associated_order == 0)?;
                    (usize::try_from(attacker.primary_order).ok() == Some(mapping_order)).then_some(*pair)
                }).collect::<Vec<_>>();
                blockers.sort_by_key(|pair| pair[0].secondary_order);
                out.push(b'[');
                stable(decision, attacker_raw, out)?;
                out.push(b',');
                array(out, blockers.iter().enumerate(), |(index, pair), out| {
                    let attacker = pair.iter().find(|row| row.associated_order == 0).ok_or(NativeFlatTensorErrorV2::RelationShape)?;
                    let blocker = pair.iter().find(|row| row.associated_order == 1).ok_or(NativeFlatTensorErrorV2::RelationShape)?;
                    if usize::try_from(attacker.secondary_order).ok() != Some(index) || raw(attacker.target_object)? != attacker_raw {
                        return Err(NativeFlatTensorErrorV2::RelationOrder);
                    }
                    stable(decision, raw(blocker.target_object)?, out)
                })?;
                out.push(b']');
                Ok(())
            }),
            "attackers_declared" => append_json_v2(out, &decision.globals().attackers_declared),
            "blockers_declared" => append_json_v2(out, &decision.globals().blockers_declared),
            "ordered_attackers" => array(out, attackers, |row, out| stable(decision, raw(row.target_object)?, out))
        )
    })();
    original_error(result, || canonical_combat_v2(decision, projection))
}

pub(super) fn effects(
    decision: FlatScoringDecisionViewV1<'_>,
    projection: &ObjectProjectionV2,
    out: &mut Vec<u8>,
) -> Result {
    let result = (|| {
        let mut baselines = decision
            .relations()
            .iter()
            .filter(|row| row.role == FlatRelationRoleV2::EffectSource)
            .collect::<Vec<_>>();
        baselines.sort_by_key(|row| row.primary_order);
        array(
            out,
            baselines.iter().enumerate(),
            |(effect_order, baseline), out| {
                if usize::try_from(baseline.primary_order).ok() != Some(effect_order) {
                    return Err(NativeFlatTensorErrorV2::RelationOrder);
                }
                let payload = effect_payload_v2(baseline)?;
                let mut objects = Vec::new();
                let mut players = Vec::new();
                for row in decision.relations().iter().filter(|row| {
                    row.role == FlatRelationRoleV2::EffectAffected
                        && row.primary_order == baseline.primary_order
                }) {
                    let data = effect_payload_v2(row)?;
                    match row.associated_order {
                        0 => objects.push(raw(row.target_object)?),
                        1 => players.push(data.affected_player),
                        _ => return Err(NativeFlatTensorErrorV2::RelationShape),
                    }
                }
                let subtypes = |kind| {
                    decision.effect_subtype_changes().iter().filter(move |row| {
                        usize::try_from(row.effect_order).ok() == Some(effect_order)
                            && row.kind == kind
                    })
                };
                object!(out;
                    "add_color_mask" => append_json_v2(out, &payload.add_color_mask),
                    "add_keyword_mask" => append_json_v2(out, &payload.add_keyword_mask),
                    "add_landwalk_mask" => append_json_v2(out, &payload.add_landwalk_mask),
                    "add_subtype_ids" => sorted(out, subtypes(FlatEffectSubtypeChangeKindV2::Add), |row, out| append_json_v2(out, &row.subtype_id)),
                    "affected_objects" => sorted(out, objects, |index, out| stable(decision, index, out)),
                    "affected_players" => sorted(out, players, |player, out| write_relative_player_json_v2(out, player, false)),
                    "controller" => write_relative_player_json_v2(out, payload.controller, true),
                    "damage_cannot_be_prevented" => append_json_v2(out, &payload.damage_cannot_be_prevented),
                    "duration" => write_enum_json_v2(out, usize::from(payload.duration), &EFFECT_DURATION_NAMES_V2),
                    "global" => append_json_v2(out, &payload.global),
                    "grants_haste" => append_json_v2(out, &payload.grants_haste),
                    "layers" => append_json_v2(out, &payload.layers),
                    "minimum_blockers" => append_json_v2(out, &payload.minimum_blockers),
                    "power_delta" => append_json_v2(out, &payload.power_delta),
                    "prevent_damage_from_color_mask" => append_json_v2(out, &payload.prevent_damage_from_color_mask),
                    "remove_color_mask" => append_json_v2(out, &payload.remove_color_mask),
                    "remove_keyword_mask" => append_json_v2(out, &payload.remove_keyword_mask),
                    "remove_landwalk_mask" => append_json_v2(out, &payload.remove_landwalk_mask),
                    "remove_subtype_ids" => sorted(out, subtypes(FlatEffectSubtypeChangeKindV2::Remove), |row, out| append_json_v2(out, &row.subtype_id)),
                    "set_power" => append_json_v2(out, &payload.set_power),
                    "set_toughness" => append_json_v2(out, &payload.set_toughness),
                    "source" => optional(out, baseline.target_object, |index, out| {
                        projected_required_node_v2(Some(index), projection)?;
                        stable(decision, raw(Some(index))?, out)
                    }),
                    "toughness_delta" => append_json_v2(out, &payload.toughness_delta),
                    "ward_generic_delta" => append_json_v2(out, &payload.ward_generic_delta)
                )
            },
        )
    })();
    original_error(result, || canonical_effects_v2(decision, projection))
}
