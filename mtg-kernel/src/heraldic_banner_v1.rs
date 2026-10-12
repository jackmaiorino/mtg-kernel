//! Definition-owned pre-entry color choice and continuous chosen-color rules.
use crate::card_def::{
    AsEntersColorChoiceDefV1, CardType, ChosenColorCreatureBoostDefV1, CARD_DEFS,
};
use crate::event::{CommittedEvent, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::ManaColor;
use crate::state::{AbilitySourceContractV4, GameObject, GameState, Zone};
use serde::{Deserialize, Serialize};

pub const COLORS: [ManaColor; 5] = [
    ManaColor::W,
    ManaColor::U,
    ManaColor::B,
    ManaColor::R,
    ManaColor::G,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub ordinal: u32,
    pub proposal: ProposedEvent,
    pub controller: PlayerId,
    pub card_def: u16,
    pub original: Option<GameObject>,
}

pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(id as usize).is_some_and(|definition| {
        definition.is_executable()
            && definition.as_enters_color_choice_v1 == Some(AsEntersColorChoiceDefV1::AnyWubrg)
            && definition.chosen_color_creature_boost_v1
                == Some(ChosenColorCreatureBoostDefV1 {
                    power: 1,
                    toughness: 0,
                })
    })
}
pub(crate) fn potentially_enters(state: &GameState) -> bool {
    state
        .objects
        .iter()
        .any(|(_, object)| owns_definition(object.card_def))
}
pub(crate) fn choice(
    state: &GameState,
    proposal: &ProposedEvent,
    ordinal: u32,
) -> Result<Option<ChoiceV1>, String> {
    let (controller, card_def, original) = match proposal {
        ProposedEvent::ZoneChange(change) if change.to_zone == Zone::Battlefield => {
            let object = state
                .objects
                .try_get(change.object)
                .ok_or("color entry object is absent")?;
            if object.zone == Zone::Battlefield || !owns_definition(object.card_def) {
                return Ok(None);
            }
            if change.battlefield_face_index.is_some_and(|face| face != 0) {
                return Err("Banner color entry has a foreign face".into());
            }
            (
                change.battlefield_controller.unwrap_or(object.owner),
                object.card_def,
                Some(object.clone()),
            )
        }
        ProposedEvent::CreateToken(token) if owns_definition(token.token_def) => {
            if token.entry_face_index.is_some_and(|face| face != 0) {
                return Err("Banner copied entry has a foreign face".into());
            }
            (token.controller, token.token_def, None)
        }
        _ => return Ok(None),
    };
    Ok(Some(ChoiceV1 {
        ordinal,
        proposal: proposal.clone(),
        controller,
        card_def,
        original,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnteredV1 {
    pub operation: u32,
    pub capture: u32,
    pub answer: u32,
    pub entry: u32,
    pub object: AbilitySourceContractV4,
    pub color: ManaColor,
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if crate::linked_aura_operation_v1::locally_owned(state) {
        return Ok(()); // Only the actual nonserializable operation runner owns this interval.
    }
    if crate::linked_aura_operation_v1::has_banner_pending(state) {
        return crate::linked_aura_operation_v1::validate(state);
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::BannerEntryOperationStartedV1 { .. }
                | CommittedEvent::BannerEntryColorCapturedV1 { .. }
                | CommittedEvent::BannerEntryColorChosenV1 { .. }
                | CommittedEvent::BannerEntryColorCommittedV1 { .. }
                | CommittedEvent::BannerEntryOperationCompletedV1 { .. }
        )
    }) {
        return Err("private Banner authority moved into public event log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::BannerEntryOperationStartedV1 { .. } => {
                let closed = state.engine.event_history.iter().enumerate().filter(|(_, event)| matches!(event,
                    CommittedEvent::BannerEntryOperationCompletedV1 { operation, .. } if *operation as usize == index)).collect::<Vec<_>>();
                if closed.len() != 1 || closed[0].0 <= index {
                    return Err(
                        "Banner operation lost its mandatory pending/completed owner".into(),
                    );
                }
            }
            CommittedEvent::BannerEntryColorCapturedV1 { operation, choice } => {
                if *operation as usize >= index
                    || !matches!(
                        state.engine.event_history.get(*operation as usize),
                        Some(CommittedEvent::BannerEntryOperationStartedV1 { .. })
                    )
                    || !owns_definition(choice.card_def)
                {
                    return Err("Banner capture lost its genuine operation/definition".into());
                }
                let answers = state.engine.event_history.iter().enumerate().filter(|(_, e)| matches!(e, CommittedEvent::BannerEntryColorChosenV1 { capture, color, .. } if *capture as usize == index && COLORS.contains(color))).collect::<Vec<_>>();
                if answers.len() != 1 || answers[0].0 <= index {
                    return Err("Banner capture lost its unique actual color answer".into());
                }
                let entries = state.engine.event_history.iter().enumerate().filter(|(_, e)| matches!(e, CommittedEvent::BannerEntryColorCommittedV1 { entered } if entered.capture as usize == index && entered.answer as usize == answers[0].0 && entered.operation == *operation)).collect::<Vec<_>>();
                if entries.len() != 1 || entries[0].0 <= answers[0].0 {
                    return Err("Banner answer lost its exact committed entry".into());
                }
            }
            CommittedEvent::BannerEntryColorChosenV1 { capture, .. }
                if !matches!(
                    state.engine.event_history.get(*capture as usize),
                    Some(CommittedEvent::BannerEntryColorCapturedV1 { .. })
                ) =>
            {
                return Err("orphan Banner color answer".into())
            }
            CommittedEvent::BannerEntryColorCommittedV1 { entered } => {
                validate_entered(state, entered, index)?;
            }
            CommittedEvent::BannerEntryOperationCompletedV1 { operation, entries } => {
                if *operation as usize >= index
                    || !matches!(
                        state.engine.event_history.get(*operation as usize),
                        Some(CommittedEvent::BannerEntryOperationStartedV1 { .. })
                    )
                {
                    return Err("orphan Banner operation completion".into());
                }
                let expected = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(at, e)| match e {
                        CommittedEvent::BannerEntryColorCommittedV1 { entered }
                            if entered.operation == *operation && at < index =>
                        {
                            Some(at as u32)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if expected.is_empty() || *entries != expected {
                    return Err("Banner completion omitted/reordered actual entries".into());
                }
            }
            _ => {}
        }
    }
    for (id, object) in state.objects.iter() {
        if object.zone == Zone::Battlefield && owns_definition(object.card_def) {
            authenticated_color(state, id)?;
        }
    }
    Ok(())
}
fn validate_entered(state: &GameState, entered: &EnteredV1, at: usize) -> Result<(), String> {
    if !(entered.operation < entered.capture
        && entered.capture < entered.answer
        && entered.answer < entered.entry
        && (entered.entry as usize) < at)
        || !COLORS.contains(&entered.color)
    {
        return Err("Banner entry authority order changed".into());
    }
    let Some(CommittedEvent::BannerEntryColorCapturedV1 { operation, choice }) =
        state.engine.event_history.get(entered.capture as usize)
    else {
        return Err("Banner entry lacks original capture".into());
    };
    if *operation != entered.operation
        || choice.card_def != entered.object.card_def
        || choice.controller != entered.object.controller
        || !matches!(state.engine.event_history.get(entered.answer as usize), Some(CommittedEvent::BannerEntryColorChosenV1 { capture, color }) if *capture == entered.capture && *color == entered.color)
    {
        return Err("Banner entry/color/captured controller changed".into());
    }
    let actual = state
        .engine
        .event_history
        .get(entered.entry as usize)
        .ok_or("Banner entry occurrence missing")?;
    match (&choice.proposal, actual) {
        (
            ProposedEvent::ZoneChange(change),
            CommittedEvent::ZoneChange {
                object,
                from,
                to: Zone::Battlefield,
                ..
            },
        ) if *object == change.object
            && *object == entered.object.source
            && choice.original.as_ref().is_some_and(|before| {
                before.zone == *from
                    && before.owner == entered.object.owner
                    && before.zone_change_count.checked_add(1)
                        == Some(entered.object.zone_change_count)
            }) => {}
        (
            ProposedEvent::CreateToken(token),
            CommittedEvent::CreateToken {
                object,
                token_def,
                controller,
            },
        ) if *object == entered.object.source
            && *token_def == token.token_def
            && *controller == token.controller
            && entered.object.zone_change_count == 0
            && entered.object.owner == *controller => {}
        _ => return Err("Banner color has no actual corresponding battlefield entry".into()),
    }
    if entered.object.zone != Zone::Battlefield {
        return Err("Banner entry binding has foreign zone".into());
    }
    Ok(())
}
pub(crate) fn authenticated_color(
    state: &GameState,
    source: ObjectId,
) -> Result<ManaColor, String> {
    let object = state
        .objects
        .try_get(source)
        .ok_or("Banner source missing")?;
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| match event {
            CommittedEvent::BannerEntryColorCommittedV1 { entered }
                if entered.object.source == source
                    && entered.object.zone_change_count == object.zone_change_count =>
            {
                Some(entered)
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    if rows.len() != 1
        || rows[0].object.owner != object.owner
        || object.v4.chosen_color != Some(rows[0].color)
    {
        return Err("Banner current incarnation lost its original chosen color".into());
    }
    Ok(rows[0].color)
}
pub(crate) fn controlled_boost(state: &GameState, recipient: ObjectId) -> (i32, i32) {
    let Some(target) = state.objects.try_get(recipient) else {
        return (0, 0);
    };
    if target.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, recipient, CardType::Creature)
    {
        return (0, 0);
    }
    let mut power = 0;
    let mut toughness = 0;
    for (source, object) in state.objects.iter() {
        if object.zone != Zone::Battlefield
            || object.controller != target.controller
            || !owns_definition(object.card_def)
            || !crate::continuous_characteristics_v1::printed_abilities_active(state, source)
        {
            continue;
        }
        let Ok(color) = authenticated_color(state, source) else {
            continue;
        };
        if crate::engine::object_color_mask(state, recipient)
            & crate::card_def::mana_color_mask(color)
            == 0
        {
            continue;
        }
        let boost = CARD_DEFS[object.card_def as usize]
            .chosen_color_creature_boost_v1
            .unwrap();
        power += i32::from(boost.power);
        toughness += i32::from(boost.toughness);
    }
    (power, toughness)
}

impl std::hash::Hash for ChoiceV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        use std::hash::Hash;
        serde_json::to_vec(self)
            .expect("actual Banner entry capture serializes")
            .hash(state);
    }
}
