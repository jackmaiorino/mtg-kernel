//! Typed Human projection from the authenticated Drakuseth consumer facts.
//! Historical identities use prompt-local roles; raw object epochs never leave.
use super::muldrotha_v71::{href, root, HumanCallRootV62};
use super::tinybones_v72::{HumanTinybonesRelationV72, Incarnations};
use super::visible::Handles;
use super::{HumanCardRefV1, HumanDecisionErrorV1, PlayerSeatV1};
use crate::fdn_family_source_v76 as p;
use serde::Serialize;
type Error = HumanDecisionErrorV1;
macro_rules! record {
    ($name:ident { $($field:ident:$ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $(pub $field:$ty),* }
    };
}
record!(HumanFactsV1 {
    meaning: crate::rules_meaning_drakuseth_v67::MeaningV67,
    authorized_printings: Vec<HumanCardRefV1>, battlefield: [Vec<HumanCurrentV1>;2],
    casts: Vec<HumanCastV1>, triggers: Vec<HumanTriggerV1>, publications: Vec<HumanPublicationV1>, pending_damage:Option<HumanDamageV1>
});
record!(HumanCurrentV1 {
    card: HumanCardRefV1,
    identity: HumanTinybonesRelationV72,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
    counters: crate::tinybones_public_v72::CountersV72,
    form: crate::optional_trigger_public_v1::FormV1,
    currently_attacking: bool
});
record!(HumanCastV1 {
    native_stack_position: u32,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    payment_pending: bool,
    is_uncast_copy: bool,
    genuine_cast_finalized: bool
});
record!(HumanRootV1 {
    source: HumanCallRootV62,
    original_identity: HumanTinybonesRelationV72
});
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanTargetV1 {
    Player { player: PlayerSeatV1 },
    Object { identity: HumanTinybonesRelationV72 },
}
record!(HumanSlotV1 {
    slot: u32,
    printed_damage: i32,
    original_target: HumanTargetV1
});
record!(HumanCurrentSlotV1 { original:HumanSlotV1, current_original_target:Option<HumanCardRefV1>, currently_legal:bool });
record!(HumanTriggerV1 {
    stage:p::StageV76, native_position:Option<u32>, publication:Option<u32>, source:HumanRootV1,
    current_or_original_departure_characteristics:p::DamageSourceV76,
    original_target_slots:Vec<HumanCurrentSlotV1>, mandatory_target_selected:bool, placement_complete:bool
});
record!(HumanPublicationV1 { occurrence:u32, source:HumanRootV1, original_target_slots:Vec<HumanSlotV1>, state:p::PublicationStateV76 });
record!(HumanPreventionV1 { menu_index:u16, kind:crate::chandra_public_v74::PreventionKindV74, source:Option<HumanCardRefV1>, shield_remaining:Option<i32> });
record!(HumanDamageV1 { publication:u32, chooser:PlayerSeatV1, active_original_slot:u32, original_amount:i32, remaining_amount:i32,
    rows:Vec<crate::drakuseth_consumer_v1::DamageRowV1>, remaining_choice_slots:Vec<u32>, options:Vec<HumanPreventionV1> });
fn relation(
    r: &crate::optional_trigger_public_v1::ReferenceV1,
    roles: &mut Incarnations,
) -> Result<HumanTinybonesRelationV72, Error> {
    roles.relation(&crate::tinybones_public_v72::IncarnationV72 {
        object: r.object,
        generation: r.generation,
        owner: r.owner,
        controller: r.controller,
        zone: r.zone,
    })
}
fn source(
    r: &crate::optional_trigger_public_v1::RootV1,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanRootV1, Error> {
    Ok(HumanRootV1 {
        source: root(r, h)?,
        original_identity: relation(&r.original_source, roles)?,
    })
}
fn slot(r: &p::OriginalSlotV76, roles: &mut Incarnations) -> Result<HumanSlotV1, Error> {
    Ok(HumanSlotV1 {
        slot: r.slot,
        printed_damage: r.printed_damage,
        original_target: match r.original_target {
            p::TargetIdentityV76::Player { player } => HumanTargetV1::Player { player },
            p::TargetIdentityV76::Object {
                object,
                generation,
                owner,
                controller_when_selected,
                original_zone,
            } => HumanTargetV1::Object {
                identity: roles.relation(&crate::tinybones_public_v72::IncarnationV72 {
                    object,
                    generation,
                    owner,
                    controller: controller_when_selected,
                    zone: original_zone,
                })?,
            },
        },
    })
}
pub(super) fn capture(
    v: &crate::drakuseth_consumer_v1::FactsV1,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let [p::LaterFamilyV76::Drakuseth(f)] = v.source.families.as_slice() else {
        return Err(Error::InvalidVisibleReference);
    };
    let mut battlefield = [Vec::new(), Vec::new()];
    for (out, rows) in battlefield.iter_mut().zip(&f.battlefield) {
        for r in rows {
            out.push(HumanCurrentV1 {
                card: href(&r.card, h)?,
                identity: relation(&r.card, roles)?,
                is_token: r.is_token,
                tapped: r.tapped,
                summoning_sick: r.summoning_sick,
                marked_damage: r.marked_damage,
                counters: r.counters.clone(),
                form: r.form.clone(),
                currently_attacking: r.currently_attacking,
            });
        }
    }
    Ok(HumanFactsV1 {
        meaning: v.meaning.clone(),
        pending_damage: v
            .pending_damage
            .as_ref()
            .map(|d| {
                Ok::<_, Error>(HumanDamageV1 {
                    publication: d.publication,
                    chooser: d.chooser,
                    active_original_slot: d.active_original_slot,
                    original_amount: d.original_amount,
                    remaining_amount: d.remaining_amount,
                    rows: d.rows.clone(),
                    remaining_choice_slots: d.remaining_choice_slots.clone(),
                    options: d
                        .options
                        .iter()
                        .map(|o| {
                            Ok(HumanPreventionV1 {
                                menu_index: o.menu_index,
                                kind: o.kind,
                                source: o.source.as_ref().map(|r| href(r, h)).transpose()?,
                                shield_remaining: o.shield_remaining,
                            })
                        })
                        .collect::<Result<_, Error>>()?,
                })
            })
            .transpose()?,
        authorized_printings: f
            .authorized_printings
            .iter()
            .map(|r| href(r, h))
            .collect::<Result<_, _>>()?,
        battlefield,
        casts: f
            .casts
            .iter()
            .map(|r| {
                Ok(HumanCastV1 {
                    native_stack_position: r.native_stack_position,
                    source: href(&r.source, h)?,
                    original_controller: r.original_controller,
                    payment_pending: r.payment_pending,
                    is_uncast_copy: r.is_uncast_copy,
                    genuine_cast_finalized: r.genuine_cast_finalized,
                })
            })
            .collect::<Result<_, Error>>()?,
        triggers: f
            .triggers
            .iter()
            .map(|r| {
                Ok(HumanTriggerV1 {
                    stage: r.stage,
                    native_position: r.native_position,
                    publication: r.publication,
                    source: source(&r.source, h, roles)?,
                    current_or_original_departure_characteristics: r
                        .current_or_original_departure_characteristics
                        .clone(),
                    original_target_slots: r
                        .original_target_slots
                        .iter()
                        .map(|s| {
                            Ok(HumanCurrentSlotV1 {
                                original: slot(&s.original, roles)?,
                                current_original_target: s
                                    .current_original_target
                                    .as_ref()
                                    .map(|r| href(r, h))
                                    .transpose()?,
                                currently_legal: s.currently_legal,
                            })
                        })
                        .collect::<Result<_, Error>>()?,
                    mandatory_target_selected: r.mandatory_target_selected,
                    placement_complete: r.placement_complete,
                })
            })
            .collect::<Result<_, Error>>()?,
        publications: f
            .publications
            .iter()
            .map(|r| {
                Ok(HumanPublicationV1 {
                    occurrence: r.occurrence,
                    source: source(&r.source, h, roles)?,
                    original_target_slots: r
                        .original_target_slots
                        .iter()
                        .map(|r| slot(r, roles))
                        .collect::<Result<_, _>>()?,
                    state: r.state.clone(),
                })
            })
            .collect::<Result<_, Error>>()?,
    })
}
