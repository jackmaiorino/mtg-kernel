//! Embercleave actor facts with prompt-local handles and historical roles.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    tinybones_v72::{HumanTinybonesRelationV72, Incarnations},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1, PlayerSeatV1,
};
use crate::{
    embercleave_public_v65::{AbilityKindV65, FactsV65, StageV65},
    optional_trigger_public_v1::{FormV1, ReferenceV1},
};
use serde::Serialize;

type Error = HumanDecisionErrorV1;

macro_rules! record {
    ($name:ident { $($field:ident: $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $(pub $field: $ty),* }
    };
}

record!(HumanFactsV1 {
    meaning: crate::rules_meaning_embercleave_v65::MeaningV65,
    authorized: Vec<HumanCardRefV1>,
    battlefield: [Vec<HumanCurrentV1>; 2],
    current_cast_reduction: [HumanReductionV1; 2],
    casts: Vec<HumanCastV1>,
    abilities: Vec<HumanAbilityV1>,
});
record!(HumanCurrentV1 {
    card: HumanCardRefV1,
    form: FormV1,
    attached_to: Option<HumanCardRefV1>,
});
record!(HumanReductionV1 {
    controller: PlayerSeatV1,
    current_attackers: Vec<HumanCardRefV1>,
    count: u32,
    generic_due_now: u8,
    retained_red_pips: u8,
    printed_mana_value: u16,
});
record!(HumanCastV1 {
    position: u32,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_uncast_copy: bool,
    payment_pending: bool,
    genuine_cast_finalized: bool,
});
record!(HumanTargetV1 {
    original: HumanTinybonesRelationV72,
    current_original_target: Option<HumanCardRefV1>,
    currently_legal: bool,
});
record!(HumanAbilityV1 {
    kind: AbilityKindV65,
    stage: StageV65,
    position: u32,
    source: HumanCallRootV62,
    original_source_incarnation_present: bool,
    original_targets: Vec<HumanTargetV1>,
    current_candidate_pool: Vec<HumanCardRefV1>,
    selection_complete: bool,
    payment_complete: bool,
});

fn references(rows: &[ReferenceV1], handles: &mut Handles) -> Result<Vec<HumanCardRefV1>, Error> {
    rows.iter().map(|row| href(row, handles)).collect()
}

pub(super) fn project(
    facts: &FactsV65,
    handles: &mut Handles,
    roles: &mut Incarnations,
) -> Result<HumanFactsV1, Error> {
    let mut battlefield = [Vec::new(), Vec::new()];
    for (rows, native) in battlefield.iter_mut().zip(&facts.battlefield) {
        for card in native {
            rows.push(HumanCurrentV1 {
                card: href(&card.card, handles)?,
                form: card.form.clone(),
                attached_to: card.attached_to.as_ref().map(|r| href(r, handles)).transpose()?,
            });
        }
    }
    let quote = |native: &crate::embercleave_public_v65::ReductionV65, h: &mut Handles| {
        Ok::<_, Error>(HumanReductionV1 {
            controller: native.controller,
            current_attackers: references(&native.current_attackers, h)?,
            count: native.count,
            generic_due_now: native.generic_due_now,
            retained_red_pips: native.retained_red_pips,
            printed_mana_value: native.printed_mana_value,
        })
    };
    Ok(HumanFactsV1 {
        meaning: facts.meaning.clone(),
        authorized: references(&facts.authorized, handles)?,
        battlefield,
        current_cast_reduction: [
            quote(&facts.current_cast_reduction[0], handles)?,
            quote(&facts.current_cast_reduction[1], handles)?,
        ],
        casts: facts.casts.iter().map(|cast| Ok(HumanCastV1 {
            position: cast.position,
            source: href(&cast.source, handles)?,
            original_controller: cast.original_controller,
            is_uncast_copy: cast.is_uncast_copy,
            payment_pending: cast.payment_pending,
            genuine_cast_finalized: cast.genuine_cast_finalized,
        })).collect::<Result<_, Error>>()?,
        abilities: facts.abilities.iter().map(|ability| {
            let original_targets = ability.original_targets.iter().map(|target| {
                Ok(HumanTargetV1 {
                    original: roles.relation(&crate::tinybones_public_v72::IncarnationV72 {
                        object: target.object,
                        generation: target.generation,
                        owner: target.owner,
                        controller: target.controller_when_selected,
                        zone: target.original_zone,
                    })?,
                    current_original_target: target.current_original_target.as_ref()
                        .map(|r| href(r, handles)).transpose()?,
                    currently_legal: target.currently_legal,
                })
            }).collect::<Result<_, Error>>()?;
            Ok(HumanAbilityV1 {
                kind: ability.kind,
                stage: ability.stage,
                position: ability.position,
                source: root(&ability.source, handles)?,
                original_source_incarnation_present: ability.original_source_incarnation_present,
                original_targets,
                current_candidate_pool: references(&ability.current_candidate_pool, handles)?,
                selection_complete: ability.selection_complete,
                payment_complete: ability.payment_complete,
            })
        }).collect::<Result<_, Error>>()?,
    })
}
