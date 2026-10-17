//! Genuine Partial Giada source. Entry replacements use the original prebatch
//! battlefield; restricted white mana is authorized by selected spell subtypes.
use crate::{
    card_def::{Subtype, TokenCounterReplacementDefV48, CARD_DEFS},
    ids::{ObjectId, PlayerId},
    state::{AbilitySourceContractV4, GameState, Zone},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryReplacementV1 {
    pub source: AbilitySourceContractV4,
    pub subtype: Subtype,
    pub already_controlled: u32,
}

pub(crate) fn capture_entry_replacements(
    state: &GameState,
    controller: PlayerId,
    entering: Option<ObjectId>,
) -> Vec<EntryReplacementV1> {
    state
        .objects
        .iter()
        .filter_map(|(id, object)| {
            if object.zone != Zone::Battlefield
                || object.controller != controller
                || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
            {
                return None;
            }
            let definition = &CARD_DEFS[usize::from(object.card_def)];
            let TokenCounterReplacementDefV48::OtherControlledSubtypeEntry(subtype) =
                definition.token_counter_replacement_v48?
            else {
                return None;
            };
            if !definition.is_executable() || object.v4.face_index != 0 {
                return None;
            }
            let already_controlled = state
                .objects
                .iter()
                .filter(|(candidate, permanent)| {
                    permanent.zone == Zone::Battlefield
                        && Some(*candidate) != entering
                        && permanent.controller == controller
                        && crate::engine::has_effective_subtype(state, *candidate, subtype)
                })
                .count();
            Some(EntryReplacementV1 {
                source: AbilitySourceContractV4::capture(state, id),
                subtype,
                already_controlled: u32::try_from(already_controlled)
                    .expect("arena ids bound object count"),
            })
        })
        .collect()
}

pub(crate) fn apply_entry_counters(
    state: &mut GameState,
    entrant: ObjectId,
    input: Option<&crate::doubling_season_v48::ReplacementInputV48>,
) -> Result<(), String> {
    let Some(input) = input else {
        return Ok(());
    };
    if input.subtype_entries_v1.is_empty() {
        return Ok(());
    }
    let live = state.objects.get(entrant);
    if live.zone != Zone::Battlefield || live.controller != input.controller {
        return Ok(());
    }
    let mut additional = 0i32;
    for replacement in &input.subtype_entries_v1 {
        // Only sources already active before this simultaneous event apply.
        // This permanent never applies its own Giada ability to itself.
        if replacement.source.source == entrant
            || !crate::engine::has_effective_subtype(state, entrant, replacement.subtype)
        {
            continue;
        }
        let count = i32::try_from(replacement.already_controlled)
            .map_err(|_| "entry subtype count overflow")?;
        let placed =
            crate::doubling_season_v48::entry_amount(state, Some(input), input.controller, count)?;
        additional = additional
            .checked_add(placed)
            .ok_or("additional entry counter overflow")?;
    }
    let total = state
        .objects
        .get(entrant)
        .counters
        .plus1_plus1
        .checked_add(additional)
        .ok_or("combined entry counter overflow")?;
    state.objects.get_mut(entrant).counters.plus1_plus1 = total;
    Ok(())
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
