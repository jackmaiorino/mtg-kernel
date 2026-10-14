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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    pub fn replaced_token_count(&self, original: usize) -> Result<usize, String> {
        self.seasons.iter().try_fold(original, |count, _| {
            count
                .checked_mul(2)
                .ok_or_else(|| "token roster count overflow".into())
        })
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
        || state.engine.event_history.iter().any(|event| {
            matches!(
                event,
                crate::event::CommittedEvent::TokenCreationReplacementV48 { .. }
                    | crate::event::CommittedEvent::PlusCounterReplacementV48 { .. }
            )
        })
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenTemplateV48 {
    pub definition: u16,
    pub controller: PlayerId,
    pub face: Option<u8>,
    pub tapped: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRequestV48 {
    pub template: TokenTemplateV48,
    pub replacement: ReplacementInputV48,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenCreationV48 {
    pub index: u64,
    pub original: Vec<TokenRequestV48>,
    pub created: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlusCounterPlacementV48 {
    pub index: u64,
    pub recipient: crate::state::ObjectLinkV4,
    pub actor: PlayerId,
    pub replacement: ReplacementInputV48,
    pub original: i32,
    pub before: i32,
    pub after: i32,
    pub counter_event: u64,
}

pub(crate) fn expected_plus_at(
    state: &GameState,
    at: u64,
    recipient: ObjectId,
    actor: PlayerId,
    original: i32,
) -> Result<i32, String> {
    let matches = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            crate::event::CommittedEvent::PlusCounterReplacementV48 { placement: p }
                if p.counter_event == at =>
            {
                Some(p.as_ref())
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [] => Ok(original),
        [p] if p.recipient.object == recipient && p.actor == actor && p.original == original => {
            p.replacement.replaced_count(original)
        }
        _ => Err("counter replacement original operation changed".into()),
    }
}

/// Expand against one unchanged pre-event board. Entry of an additional
/// replacement source cannot change later requests in this roster.
pub(crate) fn expand_tokens(
    state: &GameState,
    events: Vec<crate::event::ProposedEvent>,
) -> Result<(Vec<crate::event::ProposedEvent>, Vec<TokenRequestV48>), String> {
    use crate::event::ProposedEvent;
    let mut result = Vec::new();
    let mut original = Vec::new();
    for event in events {
        if let ProposedEvent::CreateToken(t) = &event {
            let replacement = ReplacementInputV48::capture(state, t.controller);
            let count = replacement.replaced_token_count(1)?;
            original.push(TokenRequestV48 {
                template: TokenTemplateV48 {
                    definition: t.token_def,
                    controller: t.controller,
                    face: t.entry_face_index,
                    tapped: t.entry_tapped,
                },
                replacement,
            });
            result
                .try_reserve(count)
                .map_err(|_| "token roster allocation failed")?;
            for _ in 0..count {
                result.push(event.clone());
            }
        } else {
            result.push(event);
        }
    }
    if original
        .iter()
        .all(|request| request.replacement.seasons.is_empty())
    {
        original.clear();
    }
    Ok((result, original))
}

pub(crate) fn record_token_creation(
    state: &mut GameState,
    start: usize,
    original: Vec<TokenRequestV48>,
) -> Result<(), String> {
    if original.is_empty() {
        return Ok(());
    }
    let created = state.engine.event_history[start..]
        .iter()
        .enumerate()
        .filter_map(|(i, event)| {
            matches!(event, crate::event::CommittedEvent::CreateToken { .. })
                .then_some((start + i) as u64)
        })
        .collect();
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(crate::event::CommittedEvent::TokenCreationReplacementV48 {
            creation: Box::new(TokenCreationV48 {
                index,
                original,
                created,
            }),
        });
    validate_token_journals(state)
}

pub(crate) fn validate_token_journals(state: &GameState) -> Result<(), String> {
    use crate::event::CommittedEvent;
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::TokenCreationReplacementV48 { .. }
                | CommittedEvent::PlusCounterReplacementV48 { .. }
        )
    }) {
        return Err("private replacement replayed as trigger input".into());
    }
    let mut counter_events = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::PlusCounterReplacementV48 { placement: p } = event else {
            continue;
        };
        let count = p.replacement.replaced_count(p.original)?;
        if p.index != index as u64
            || p.original <= 0
            || p.replacement.seasons.is_empty()
            || p.counter_event >= p.index
            || p.before.checked_add(count) != Some(p.after)
            || counter_events.contains(&p.counter_event)
            || p.replacement.seasons.iter().enumerate().any(|(i, s)| {
                s.zone != Zone::Battlefield
                    || s.controller != p.replacement.controller
                    || !owns_definition(s.card_def)
                    || p.replacement.seasons[..i]
                        .iter()
                        .any(|other| other.source == s.source)
            })
            || !matches!(state.engine.event_history.get(p.counter_event as usize),Some(CommittedEvent::PlusOneCountersAdded {object,zone_change_count,player,count:actual}) if *object==p.recipient.object && *zone_change_count==p.recipient.zone_change_count && *player==p.actor && *actual==count)
        {
            return Err("actual effect counter replacement changed".into());
        }
        counter_events.push(p.counter_event);
    }
    let mut all_created = Vec::new();
    for (index, event) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::TokenCreationReplacementV48 { creation: c } = event else {
            continue;
        };
        if c.index != index as u64
            || c.original.is_empty()
            || c.original.iter().all(|r| r.replacement.seasons.is_empty())
        {
            return Err("token replacement original roster changed".into());
        }
        let mut cursor = 0;
        for request in &c.original {
            if request.replacement.controller != request.template.controller
                || request
                    .replacement
                    .seasons
                    .iter()
                    .enumerate()
                    .any(|(i, s)| {
                        s.zone != Zone::Battlefield
                            || s.controller != request.template.controller
                            || !owns_definition(s.card_def)
                            || request.replacement.seasons[..i]
                                .iter()
                                .any(|other| other.source == s.source)
                    })
            {
                return Err("token replacement captured source changed".into());
            }
            let count = request.replacement.replaced_token_count(1)?;
            for _ in 0..count {
                let at = *c
                    .created
                    .get(cursor)
                    .ok_or("token replacement omitted created member")?;
                if at >= c.index
                    || cursor > 0 && c.created[cursor - 1] >= at
                    || all_created.contains(&at)
                    || !matches!(state.engine.event_history.get(at as usize),Some(CommittedEvent::CreateToken {token_def,controller,..}) if *token_def==request.template.definition && *controller==request.template.controller)
                {
                    return Err("token replacement actual creation changed".into());
                }
                all_created.push(at);
                cursor += 1;
            }
        }
        if cursor != c.created.len() {
            return Err("token replacement added unrequested member".into());
        }
    }
    Ok(())
}

/// Operation owners use the original creation roster, not the later board.
pub(crate) fn expected_tokens(
    state: &GameState,
    start: usize,
    end: usize,
    definition: u16,
    controller: PlayerId,
    original: usize,
) -> Result<usize, String> {
    validate_token_journals(state)?;
    let mut expected = original;
    let mut seen = 0;
    for event in &state.engine.event_history[start..end] {
        let crate::event::CommittedEvent::TokenCreationReplacementV48 { creation: c } = event
        else {
            continue;
        };
        for request in &c.original {
            if request.template.definition == definition
                && request.template.controller == controller
            {
                seen += 1;
                expected = expected
                    .checked_add(request.replacement.replaced_token_count(1)? - 1)
                    .ok_or("token count overflow")?;
            }
        }
    }
    if seen > original {
        return Err("token replacement original request count changed".into());
    }
    Ok(expected)
}

#[cfg(test)]
mod tests;
