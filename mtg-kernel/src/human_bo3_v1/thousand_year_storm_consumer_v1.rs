//! Local Human handles for original Storm spells and independently copied slots.
use super::{
    muldrotha_v71::href,
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::{Handles, HumanTargetV1, Project},
    HumanCardRefV1, HumanDecisionErrorV1 as Error,
};
use crate::{
    optional_trigger_public_v1::{self as common, IdentityV1},
    rl::{ActionSemanticV1 as Action, PlayerSeatV1},
    thousand_year_storm_consumer_v1::ActionV1,
    thousand_year_storm_public_v1 as public,
    tinybones_public_v72::IncarnationV72,
};
use serde::Serialize;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFactsV1 {
    meaning: public::MeaningV1,
    sources: Vec<HumanSourceV1>,
    current_turn_casts: [u16; 2],
    casts: Vec<HumanCastV1>,
    triggers: Vec<HumanTriggerV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanSourceV1 {
    card: HumanCardRefV1,
    form: common::FormV1,
    counters: common::CountersV1,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
enum HumanTargetSnapshotV1 {
    Player {
        player: PlayerSeatV1,
    },
    Object {
        original: HumanTinybonesRelationV72,
        current_original: Option<HumanCardRefV1>,
    },
    Stack {
        relation: String,
        original_source: HumanTinybonesRelationV72,
        kind: crate::state::StackItemKind,
        current_position: Option<u32>,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanPaidV1 {
    original: HumanTinybonesRelationV72,
    power_lki: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanSnapshotV1 {
    original_spell: HumanTinybonesRelationV72,
    printed_name: String,
    is_copy: bool,
    cast_method: crate::state::CastMethodV4,
    mode: u8,
    x_value: u16,
    kicked: bool,
    optional_additional_cost_paid: Option<crate::card_def::OptionalAdditionalCostDef>,
    authorized_paid_costs: Vec<HumanPaidV1>,
    mana_spent: u16,
    targets: Vec<HumanTargetSnapshotV1>,
    current_stack_position: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanRootV1 {
    original_source: HumanTinybonesRelationV72,
    current_original_source: Option<HumanCardRefV1>,
    captured_controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCastV1 {
    spell: HumanSnapshotV1,
    current_turn: bool,
    prior_own_casts: u16,
    observers: Vec<HumanRootV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanCopyV1 {
    ordinal: u16,
    spell: HumanSnapshotV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanChoiceV1 {
    chooser: PlayerSeatV1,
    copy_ordinal: u16,
    target_slot: u16,
    selected_prefix: Vec<HumanTargetSnapshotV1>,
    original_target: HumanTargetSnapshotV1,
    legal_new_targets: Vec<HumanTargetV1>,
    can_keep_original: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct HumanTriggerV1 {
    cast: u32,
    source: HumanRootV1,
    expected_copies: u16,
    stage: Option<public::StageV1>,
    position: Option<u32>,
    created_copies: Vec<HumanCopyV1>,
    choice: Option<HumanChoiceV1>,
}
fn relation(
    card: &common::ReferenceV1,
    roles: &mut Incarnations,
) -> Result<HumanTinybonesRelationV72, Error> {
    roles.relation(&IncarnationV72 {
        object: card.object,
        generation: card.generation,
        owner: card.owner,
        controller: card.controller,
        zone: card.zone,
    })
}
fn target(
    value: &public::TargetV1,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanTargetSnapshotV1, Error> {
    Ok(match value {
        public::TargetV1::Player { player } => HumanTargetSnapshotV1::Player { player: *player },
        public::TargetV1::Object {
            original,
            current_original,
        } => HumanTargetSnapshotV1::Object {
            original: roles.relation(original)?,
            current_original: current_original.as_ref().map(|r| href(r, h)).transpose()?,
        },
        public::TargetV1::Stack {
            relation,
            original_source,
            kind,
            current_position,
        } => HumanTargetSnapshotV1::Stack {
            relation: format!("storm-target-{}", relation + 1),
            original_source: roles.relation(original_source)?,
            kind: *kind,
            current_position: *current_position,
        },
    })
}
fn snapshot(
    value: &public::SnapshotV1,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanSnapshotV1, Error> {
    let IdentityV1::Known { card_id } = value.original_spell.identity else {
        return Err(Error::InvalidVisibleReference);
    };
    let printed_name = crate::card_def::CARD_DEFS
        .get(card_id as usize)
        .ok_or(Error::InvalidVisibleReference)?
        .name
        .to_string();
    Ok(HumanSnapshotV1 {
        original_spell: relation(&value.original_spell, roles)?,
        printed_name,
        is_copy: value.is_copy,
        cast_method: value.cast_method,
        mode: value.mode,
        x_value: value.x_value,
        kicked: value.kicked,
        optional_additional_cost_paid: value.optional_additional_cost_paid,
        authorized_paid_costs: value
            .authorized_paid_costs
            .iter()
            .map(|p| {
                Ok(HumanPaidV1 {
                    original: roles.relation(&p.original)?,
                    power_lki: p.power_lki,
                })
            })
            .collect::<Result<_, Error>>()?,
        mana_spent: value.mana_spent,
        targets: value
            .targets
            .iter()
            .map(|t| target(t, h, roles))
            .collect::<Result<_, _>>()?,
        current_stack_position: value.current_stack_position,
    })
}
fn root(
    value: &common::RootV1,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanRootV1, Error> {
    Ok(HumanRootV1 {
        original_source: relation(&value.original_source, roles)?,
        current_original_source: value
            .current_original_source
            .as_ref()
            .map(|r| href(r, h))
            .transpose()?,
        captured_controller: value.captured_controller,
    })
}
pub(super) fn project(
    value: &public::FactsV1,
    h: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let sources = value
        .sources
        .iter()
        .map(|r| {
            Ok(HumanSourceV1 {
                card: href(&r.source, h)?,
                form: r.form.clone(),
                counters: r.counters.clone(),
                is_token: r.is_token,
                tapped: r.tapped,
                summoning_sick: r.summoning_sick,
                marked_damage: r.marked_damage,
            })
        })
        .collect::<Result<_, Error>>()?;
    let casts = value
        .casts
        .iter()
        .map(|r| {
            Ok(HumanCastV1 {
                spell: snapshot(&r.spell, h, roles)?,
                current_turn: r.current_turn,
                prior_own_casts: r.prior_own_casts,
                observers: r
                    .observers
                    .iter()
                    .map(|o| root(o, h, roles))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let triggers = value
        .triggers
        .iter()
        .map(|r| {
            Ok(HumanTriggerV1 {
                cast: r.cast,
                source: root(&r.source, h, roles)?,
                expected_copies: r.expected_copies,
                stage: r.stage,
                position: r.position,
                created_copies: r
                    .created_copies
                    .iter()
                    .map(|c| {
                        Ok(HumanCopyV1 {
                            ordinal: c.ordinal,
                            spell: snapshot(&c.spell, h, roles)?,
                        })
                    })
                    .collect::<Result<_, Error>>()?,
                choice: r
                    .choice
                    .as_ref()
                    .map(|c| {
                        Ok::<_, Error>(HumanChoiceV1 {
                            chooser: c.chooser,
                            copy_ordinal: c.copy_ordinal,
                            target_slot: c.target_slot,
                            selected_prefix: c
                                .selected_prefix
                                .iter()
                                .map(|t| target(t, h, roles))
                                .collect::<Result<_, _>>()?,
                            original_target: target(&c.original_target, h, roles)?,
                            legal_new_targets: c.legal_new_targets.project(h)?,
                            can_keep_original: c.can_keep_original,
                        })
                    })
                    .transpose()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanFactsV1 {
        meaning: value.meaning.clone(),
        sources,
        current_turn_casts: value.current_turn_casts,
        casts,
        triggers,
    })
}
pub(super) fn label(
    facts: Option<&public::FactsV1>,
    action: &Action,
    _: &Handles,
) -> Result<Option<String>, Error> {
    Ok(
        crate::thousand_year_storm_consumer_v1::action(facts, action)
            .map_err(|_| Error::InvalidVisibleReference)?
            .map(|a| match a {
                ActionV1::ChooseNewTarget { copy, slot } => format!(
                    "Choose a new target for copy {copy}, target slot {}",
                    usize::from(slot) + 1
                ),
                ActionV1::KeepOriginalTarget { copy, slot } => format!(
                    "Keep the original target for copy {copy}, target slot {}",
                    usize::from(slot) + 1
                ),
            }),
    )
}
