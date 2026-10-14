//! Shared replacement inputs. No registry entry or consumer admission.
use crate::{
    card_def::{
        CardCapability, CardType, Keywords, TargetSpec, TokenCounterReplacementDefV48, CARD_DEFS,
    },
    effect::{EffectOp, ObjectRef},
    ids::{ObjectId, PlayerId},
    mana::{ManaColor, Pip},
    state::{AbilitySourceContractV4, GameState, Zone},
};
use serde::{Deserialize, Serialize};

pub const MAGE_COMMIT: &str = "a5c90fe180021e70e2a644ade00eeab07f857a40";
pub const MAGE_PATH: &str = "Mage.Sets/src/mage/cards/d/DoublingSeason.java";

pub fn owns_definition(id: u16) -> bool {
    let Some(d) = CARD_DEFS.get(usize::from(id)) else {
        return false;
    };
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && d.name == "Doubling Season"
        && d.capability == CardCapability::Full
        && d.types == [CardType::Enchantment]
        && d.subtypes.is_empty()
        && d.supertypes.is_empty()
        && d.cost.generic == 4
        && d.cost.x_count == 0
        && d.cost.pips == [Pip::Colored(ManaColor::G)]
        && d.colors == [ManaColor::G]
        && d.mana_value == 5
        && d.power.is_none()
        && d.toughness.is_none()
        && !d.is_land
        && !d.is_token
        && d.keywords == Keywords::NONE
        && d.target_spec == TargetSpec::None
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.ward_cost.is_none()
        && d.starting_loyalty.is_none()
        && d.activated_abilities.is_empty()
        && crate::trigger::triggers_for(id).is_empty()
        && d.token_counter_replacement_v48
            == Some(TokenCounterReplacementDefV48::DoubleEffectTokensAndControlledPermanentCounters)
        && (d.spell_effect)()
            == Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
}

/// Every source is sampled before the event, in physical battlefield order.
/// The recipient controller and placing actor remain distinct.
pub(crate) fn sources(state: &GameState, controller: PlayerId) -> Vec<AbilitySourceContractV4> {
    state
        .objects
        .iter()
        .filter(|(id, o)| {
            o.zone == Zone::Battlefield
                && o.controller == controller
                && o.v4.face_index == 0
                && owns_definition(o.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .map(|(id, _)| AbilitySourceContractV4::capture(state, id))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CounterOriginV48 {
    Effect,
    Cost,
    /// Turn-based counters and played lands do not become effects merely
    /// because they use the same placement primitive.
    Rule,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReplacementInputV48 {
    pub controller: PlayerId,
    pub seasons: Vec<AbilitySourceContractV4>,
}
impl ReplacementInputV48 {
    pub(crate) fn capture(state: &GameState, controller: PlayerId) -> Self {
        Self {
            controller,
            seasons: sources(state, controller),
        }
    }
    /// The same nonconsuming replacements all commute; each applies once.
    pub fn replaced_count(&self, original: i32) -> Result<i32, String> {
        if original < 0 {
            return Err("negative replacement count".into());
        }
        let mut result = original;
        for _ in &self.seasons {
            result = result
                .checked_mul(2)
                .ok_or("token/counter replacement overflow")?;
        }
        Ok(result)
    }
}

/// Counter effects use the live recipient's controller. Talent's distinct
/// actor-relative modifier is deliberately applied separately, as before.
pub(crate) fn counter_amount(
    state: &GameState,
    recipient: ObjectId,
    actor: PlayerId,
    origin: CounterOriginV48,
    amount: i32,
) -> Result<i32, String> {
    if amount < 0 {
        return Err("negative counter placement".into());
    }
    let live = state
        .objects
        .try_get(recipient)
        .ok_or("missing counter recipient")?;
    if live.zone != Zone::Battlefield {
        return Err("counter recipient is not a permanent".into());
    }
    let scaled = crate::standard_cards_v1::scale_counters(state, actor, amount);
    if origin != CounterOriginV48::Effect {
        return Ok(scaled);
    }
    ReplacementInputV48::capture(state, live.controller).replaced_count(scaled)
}

pub(crate) fn requires_successor(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(_, o)| o.zone == Zone::Battlefield && owns_definition(o.card_def))
        || state.stack.iter().any(|item| {
            item.v4
                .source_contract
                .is_some_and(|s| owns_definition(s.card_def))
                || state
                    .objects
                    .try_get(item.source)
                    .is_some_and(|o| owns_definition(o.card_def))
        })
}

#[cfg(test)]
mod tests;
