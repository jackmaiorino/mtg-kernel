//! Human projection of chosen-type facts with observer-local card handles.
use super::{
    muldrotha_v71::{href, root, HumanCallRootV62},
    visible::Handles,
    HumanCardRefV1, HumanDecisionErrorV1,
};
use crate::{
    chosen_type_consumer_v1::FactsV1,
    chosen_type_public_v66::FellowshipEntryV66,
    creature_types_v60::CreatureTypeV60,
    mana::ManaColor,
    rl::PlayerSeatV1,
    rules_meaning_chosen_type_v66::{FamilyV66, MeaningV66},
};
use serde::Serialize;

type Error = HumanDecisionErrorV1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanChosenTypeFactsV1 {
    pub meanings: Vec<MeaningV66>,
    pub visible_printings: Vec<HumanCardRefV1>,
    pub active_entry: Option<HumanActiveEntryV1>,
    pub entries: Vec<HumanEntryV1>,
    pub permanents: Vec<HumanChosenPermanentV1>,
    pub floating_mana: Vec<HumanFloatingManaV1>,
    pub casts: Vec<HumanChosenCastV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanActiveEntryV1 {
    pub printing: FamilyV66,
    pub choice: crate::chosen_type_public_v66::ChoiceV66,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEntryV1 {
    pub occurrence: u32,
    pub batch_entry_ordinal: u32,
    pub printing: FamilyV66,
    pub source: HumanCallRootV62,
    pub chosen: CreatureTypeV60,
    pub created_by_token_event: bool,
    pub fellowship: Option<HumanFellowshipV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFellowshipV1 {
    pub preentry_chosen_creature_count: i32,
    pub entry_fellowship_after_replacements: i32,
    pub captured_talent_doublings: u32,
    pub captured_seasons: Vec<HumanCallRootV62>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanChosenPermanentV1 {
    pub card: HumanCardRefV1,
    pub printing: FamilyV66,
    pub entry_occurrence: u32,
    pub chosen: CreatureTypeV60,
    pub printed_abilities_active: bool,
    pub current_fellowship: i32,
    pub current_controlled_creature_bonuses: Vec<HumanChosenBoostV1>,
    pub currently_payable_mana_colors: Vec<ManaColor>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanChosenBoostV1 {
    pub recipient: HumanCardRefV1,
    pub power: i32,
    pub toughness: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFloatingManaV1 {
    pub holder: PlayerSeatV1,
    pub pool_ordinal: u32,
    pub color: ManaColor,
    pub chosen: CreatureTypeV60,
    pub original_entry_occurrence: u32,
    pub original_entry_controller: PlayerSeatV1,
    pub current_original_source: Option<HumanCardRefV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanChosenCastV1 {
    pub native_stack_position: u32,
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub payment_pending: bool,
    pub is_uncast_copy: bool,
    pub genuine_cast_finalized: bool,
}

fn fellowship(row: &FellowshipEntryV66, handles: &mut Handles) -> Result<HumanFellowshipV1, Error> {
    Ok(HumanFellowshipV1 {
        preentry_chosen_creature_count: row.preentry_chosen_creature_count,
        entry_fellowship_after_replacements: row.entry_fellowship_after_replacements,
        captured_talent_doublings: row.captured_talent_doublings,
        captured_seasons: row
            .captured_seasons
            .iter()
            .map(|source| root(source, handles))
            .collect::<Result<_, _>>()?,
    })
}

pub(super) fn project(
    facts: &FactsV1,
    handles: &mut Handles,
) -> Result<HumanChosenTypeFactsV1, Error> {
    let native = &facts.native;
    Ok(HumanChosenTypeFactsV1 {
        meanings: native.meanings.clone(),
        visible_printings: native
            .visible_printings
            .iter()
            .map(|card| href(card, handles))
            .collect::<Result<_, _>>()?,
        active_entry: facts
            .active_printing
            .zip(native.active_entry_choice.clone())
            .map(|(printing, choice)| HumanActiveEntryV1 { printing, choice }),
        entries: native
            .completed_entries
            .iter()
            .map(|row| {
                Ok(HumanEntryV1 {
                    occurrence: row.occurrence,
                    batch_entry_ordinal: row.batch_entry_ordinal,
                    printing: row.family,
                    source: root(&row.source, handles)?,
                    chosen: row.chosen,
                    created_by_token_event: row.created_by_token_event,
                    fellowship: row
                        .fellowship
                        .as_ref()
                        .map(|row| fellowship(row, handles))
                        .transpose()?,
                })
            })
            .collect::<Result<_, Error>>()?,
        permanents: native
            .permanents
            .iter()
            .map(|row| {
                Ok(HumanChosenPermanentV1 {
                    card: href(&row.card, handles)?,
                    printing: row.family,
                    entry_occurrence: row.entry_occurrence,
                    chosen: row.chosen,
                    printed_abilities_active: row.printed_abilities_active,
                    current_fellowship: row.current_fellowship,
                    current_controlled_creature_bonuses: row
                        .current_controlled_creature_bonuses
                        .iter()
                        .map(|boost| {
                            Ok(HumanChosenBoostV1 {
                                recipient: href(&boost.recipient, handles)?,
                                power: boost.power,
                                toughness: boost.toughness,
                            })
                        })
                        .collect::<Result<_, Error>>()?,
                    currently_payable_mana_colors: row.currently_payable_mana_colors.clone(),
                })
            })
            .collect::<Result<_, Error>>()?,
        floating_mana: native
            .floating_chosen_type_mana
            .iter()
            .map(|row| {
                Ok(HumanFloatingManaV1 {
                    holder: row.holder,
                    pool_ordinal: row.pool_ordinal,
                    color: row.color,
                    chosen: row.chosen,
                    original_entry_occurrence: row.original_entry_occurrence,
                    original_entry_controller: row.original_entry_controller,
                    current_original_source: row
                        .current_original_source
                        .as_ref()
                        .map(|source| href(source, handles))
                        .transpose()?,
                })
            })
            .collect::<Result<_, Error>>()?,
        casts: native
            .casts
            .iter()
            .map(|row| {
                Ok(HumanChosenCastV1 {
                    native_stack_position: row.native_stack_position,
                    source: href(&row.source, handles)?,
                    controller: row.controller,
                    payment_pending: row.payment_pending,
                    is_uncast_copy: row.is_uncast_copy,
                    genuine_cast_finalized: row.genuine_cast_finalized,
                })
            })
            .collect::<Result<_, Error>>()?,
    })
}
