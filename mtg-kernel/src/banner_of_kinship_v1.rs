//! Unregistered Banner of Kinship prerequisites, not an entry driver.
//!
//! These read-only calculations neither install an ability nor commit an entry.
//! Registration still needs a definition-owned pre-entry operation stage and
//! an incarnation-bound chosen-type record. In particular, an EntryChoice is
//! not an engine pending choice or a permission to mutate a restored world.
use crate::{
    card_def::{CardType, CARD_DEFS},
    creature_types_v60::{self, CreatureTypeV60},
    event::ProposedEvent,
    ids::{ObjectId, PlayerId},
    state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Zone},
};

pub const MAGE_COMMIT: &str = "a5c90fe180021e70e2a644ade00eeab07f857a40";
pub const MAGE_PATH: &str = "Mage.Sets/src/mage/cards/b/BannerOfKinship.java";
pub const PRINTING_SET: &str = "FDN";
pub const PRINTING_COLLECTOR_NUMBER: &str = "127";
pub const PRINTING_SCRYFALL_ID: &str = "a14c16c0-4053-46b0-8fa6-be8b4a7a1c8a";
pub const MANA_COST: &str = "{5}";

/// A read-only pre-entry input. Fields stay private so a caller cannot change
/// the captured controller, proposal or scene and claim it is the same input.
/// There is deliberately no engine installer, serialization or owner permit.
pub struct EntryChoiceV1 {
    original: GameState,
    proposal: ProposedEvent,
    controller: PlayerId,
    original_source: Option<AbilitySourceContractV4>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryAmountsV1 {
    pub chosen: CreatureTypeV60,
    pub before_replacements: i32,
    pub after_replacements: i32,
}

impl EntryChoiceV1 {
    /// Capture one genuine proposed entry before the entry roster commits.
    /// This generic prerequisite does not assert the object has Banner's rule.
    pub fn capture(state: &GameState, proposal: &ProposedEvent) -> Result<Self, String> {
        let (controller, original_source) = match proposal {
            ProposedEvent::ZoneChange(change) if change.to_zone == Zone::Battlefield => {
                let object = state
                    .objects
                    .try_get(change.object)
                    .ok_or("entry source absent")?;
                if object.zone == Zone::Battlefield {
                    return Err("entry source is already on the battlefield".into());
                }
                if CARD_DEFS.get(usize::from(object.card_def)).is_none() {
                    return Err("entry source definition absent".into());
                }
                (
                    change.battlefield_controller.unwrap_or_else(|| {
                        if object.zone == Zone::Stack {
                            object.controller
                        } else {
                            object.owner
                        }
                    }),
                    Some(AbilitySourceContractV4::capture(state, change.object)),
                )
            }
            ProposedEvent::CreateToken(token) => {
                if CARD_DEFS.get(usize::from(token.token_def)).is_none() {
                    return Err("entry token definition absent".into());
                }
                (token.controller, None)
            }
            _ => return Err("creature type input is not a battlefield entry".into()),
        };
        Ok(Self {
            original: state.clone(),
            proposal: proposal.clone(),
            controller,
            original_source,
        })
    }

    pub fn controller(&self) -> PlayerId {
        self.controller
    }

    /// Every legal type is offered, independently of visible or hidden cards.
    pub fn choices(&self) -> &'static [CreatureTypeV60] {
        CreatureTypeV60::ALL
    }

    /// Sample creatures in the captured pre-entry scene, then apply the
    /// existing native entry-counter replacements to that fixed amount.
    /// The caller still needs a real definition-owned entry stage to commit it.
    pub fn answer(&self, state: &GameState, index: u16) -> Result<EntryAmountsV1, String> {
        if state != &self.original {
            return Err("creature type entry input changed before its answer".into());
        }
        let chosen =
            CreatureTypeV60::from_choice_index(index).ok_or("illegal creature type index")?;
        if let Some(source) = self.original_source {
            if AbilitySourceContractV4::capture(state, source.source) != source {
                return Err("entry source incarnation changed".into());
            }
        }
        let mut amount = 0i32;
        for (id, object) in state.objects.iter() {
            if object.zone != Zone::Battlefield || object.controller != self.controller {
                continue;
            }
            // The checked V60 query rejects unknown subtype and animation
            // provenance instead of approximating an all-types creature.
            if creature_types_v60::matches_effective_creature(state, id, chosen)
                .map_err(|e| format!("unsupported entry creature type: {e:?}"))?
            {
                amount = amount.checked_add(1).ok_or("fellowship count overflow")?;
            }
        }
        // These counters come from Banner's replacement effect even when the
        // underlying move has a different origin. A rule-origin marker for
        // other counters must not suppress Season for fellowship counters.
        let mut counter_proposal = self.proposal.clone();
        if let ProposedEvent::ZoneChange(change) = &mut counter_proposal {
            change.counter_origin_v48 = Some(crate::doubling_season_v48::CounterOriginV48::Effect);
        }
        let input = crate::doubling_season_v48::entry_input(state, &counter_proposal)
            .ok_or("entry counter replacement input missing")?;
        let after =
            crate::doubling_season_v48::entry_amount(state, Some(&input), self.controller, amount)?;
        Ok(EntryAmountsV1 {
            chosen,
            before_replacements: amount,
            after_replacements: after,
        })
    }
}

/// Checked anthem arithmetic for a caller that already owns the actual
/// definition and chosen-type binding. It neither grants the ability nor adds
/// a native continuous effect. Current counters, controller and incarnation
/// are read afresh; a departed or reentered source cannot supply the bonus.
pub fn quote_controlled_boost(
    state: &GameState,
    source: ObjectLinkV4,
    chosen: CreatureTypeV60,
    recipient: ObjectId,
) -> Result<(i128, i128), String> {
    let object = state
        .objects
        .try_get(source.object)
        .ok_or("anthem source absent")?;
    if object.zone != Zone::Battlefield || object.zone_change_count != source.zone_change_count {
        return Err("anthem source incarnation is no longer on the battlefield".into());
    }
    if object.counters.fellowship < 0 {
        return Err("negative fellowship counters".into());
    }
    let target = state
        .objects
        .try_get(recipient)
        .ok_or("anthem recipient absent")?;
    if target.zone != Zone::Battlefield
        || target.controller != object.controller
        || !crate::engine::object_has_type(state, recipient, CardType::Creature)
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, source.object)
    {
        return Ok((0, 0));
    }
    if !creature_types_v60::matches_effective_creature(state, recipient, chosen)
        .map_err(|e| format!("unsupported anthem creature type: {e:?}"))?
    {
        return Ok((0, 0));
    }
    let amount = i128::from(object.counters.fellowship);
    Ok((amount, amount))
}

#[cfg(test)]
mod tests;
