//! Exact printed Hunter recipes and event-time attack authority. Unregistered.
use crate::card_def::{CardType, CARD_DEFS};
use crate::effect::{
    EffectContinuation, EffectObjectBinding, EffectOp, ObjectSelectionActionV1,
    ObjectSelectionFilterV1, ObjectSelectionRuleV1, PlayerRef,
};
use crate::event::CommittedEvent;
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use crate::trigger::PendingTrigger;

pub(crate) fn attack_program() -> EffectOp {
    EffectOp::SelectObjectsV1 {
        rule: ObjectSelectionRuleV1 {
            player: PlayerRef::Controller,
            zone: Zone::Battlefield,
            any_player: false,
            filter: ObjectSelectionFilterV1::AnotherCreatureFromCapturedSourceV1,
            min: 0,
            max: 1,
            action: ObjectSelectionActionV1::SacrificeThenCounterCapturedSourceV1,
        },
    }
}
pub(crate) fn death_program() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}
pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && crate::rules_meaning_high_society_hunter_v1::exact_definition(id)
}
fn enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        CARD_DEFS
            .iter()
            .enumerate()
            .any(|(id, _)| owns_definition(id as u16))
    })
}
pub(crate) fn capture_attack(state: &mut GameState, declaration: usize) {
    if !enabled() {
        return;
    }
    let Some(CommittedEvent::DeclaredAttacker {
        source,
        source_zone_change_count,
        controller,
    }) = state.engine.event_history.get(declaration)
    else {
        return;
    };
    let (id, generation, actor) = (*source, *source_zone_change_count, *controller);
    let live = state.objects.get(id);
    if !owns_definition(live.card_def)
        || live.zone != Zone::Battlefield
        || live.zone_change_count != generation
        || live.controller != actor
        || live.v4.face_index != 0
        || live.v4.face_down_v1.is_some()
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    {
        return;
    }
    let event = CommittedEvent::HighSocietyHunterAttackCaptureV1 {
        declaration_history_index: declaration as u64,
        source: AbilitySourceContractV4::capture(state, id),
        turn_key: state.turn_key_v1(),
        active_player: state.active_player,
    };
    state.engine.event_history.push(event.clone());
    state.engine.event_log.push(event);
}
fn capture_at(state: &GameState, index: usize) -> Result<AbilitySourceContractV4, String> {
    let Some(CommittedEvent::HighSocietyHunterAttackCaptureV1 {
        declaration_history_index,
        source,
        turn_key,
        active_player,
    }) = state.engine.event_history.get(index)
    else {
        return Err("Hunter attack capture absent".into());
    };
    let declaration = usize::try_from(*declaration_history_index)
        .map_err(|_| "Hunter declaration index exceeds usize")?;
    if declaration>=index || !owns_definition(source.card_def) || source.zone!=Zone::Battlefield
        || !matches!(state.engine.event_history.get(declaration),Some(CommittedEvent::DeclaredAttacker{source:id,source_zone_change_count,controller}) if *id==source.source && *source_zone_change_count==source.zone_change_count && *controller==source.controller)
        || source.controller!=*active_player
        || state.engine.event_history[..index].iter().any(|e|matches!(e,CommittedEvent::HighSocietyHunterAttackCaptureV1{declaration_history_index:old,..} if old==declaration_history_index))
    {return Err("Hunter original attack authority changed".into())}
    crate::engine::validate_historical_ability_source_contract(state, *source)?;
    // Closed historical captures remain valid; only fresh collection needs the turn.
    let collected=state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::HighSocietyHunterAttackCollectedV1{capture_history_index} if *capture_history_index==index as u64));
    if !collected && (*turn_key != state.turn_key_v1() || *active_player != state.active_player) {
        return Err("Hunter uncollected attack crossed its actual turn".into());
    }
    Ok(*source)
}
pub(crate) fn collect_attacks(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, ObjectId> {
    if !enabled() {
        return Ok(Vec::new());
    }
    for event in &state.engine.event_history {
        if let CommittedEvent::HighSocietyHunterAttackCollectedV1 {
            capture_history_index,
        } = event
        {
            if usize::try_from(*capture_history_index)
                .ok()
                .and_then(|i| state.engine.event_history.get(i))
                .is_none_or(|e| {
                    !matches!(e, CommittedEvent::HighSocietyHunterAttackCaptureV1 { .. })
                })
            {
                return Err(ObjectId(0));
            }
        }
    }
    let indices = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            matches!(e, CommittedEvent::HighSocietyHunterAttackCaptureV1 { .. }).then_some(i)
        })
        .collect::<Vec<_>>();
    let mut pending = Vec::new();
    let mut consumed = Vec::new();
    for index in indices {
        let source = capture_at(state, index).map_err(|_| ObjectId(0))?;
        let capture = &state.engine.event_history[index];
        let prior=state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::HighSocietyHunterAttackCollectedV1{capture_history_index} if *capture_history_index==index as u64)).count();
        let current = events.iter().filter(|e| *e == capture).count();
        if prior > 1 || (prior == 0 && current != 1) || (prior == 1 && current != 0) {
            return Err(source.source);
        }
        if prior == 0 {
            pending.push(PendingTrigger {
                controller: source.controller,
                source: source.source,
                effect: attack_program(),
                is_madness_offer: false,
                kicked: false,
                target_spec: crate::card_def::TargetSpec::None,
                targets: Vec::new(),
                target_contracts: Vec::new(),
                placement_ordered: false,
                source_contract: Some(source),
                granted_by: None,
                optional_additional_cost_paid: None,
                paid_cost_refs: Vec::new(),
            });
            consumed.push(index as u64);
        }
    }
    for index in consumed {
        state
            .engine
            .event_history
            .push(CommittedEvent::HighSocietyHunterAttackCollectedV1 {
                capture_history_index: index,
            });
    }
    Ok(pending)
}
pub(crate) fn validate_source(
    state: &GameState,
    source: AbilitySourceContractV4,
) -> Result<(), String> {
    if !owns_definition(source.card_def) {
        return Err("Hunter consequence belongs to another printed source".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, source)?;
    let captures = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| {
            matches!(e,CommittedEvent::HighSocietyHunterAttackCaptureV1{source:s,..} if *s==source)
                .then_some(i)
        })
        .collect::<Vec<_>>();
    if captures.is_empty() {
        return Err("Hunter cost has no actual original attack".into());
    }
    for index in captures {
        capture_at(state, index)?;
    }
    Ok(())
}
pub(crate) fn pay_selection(
    state: &mut GameState,
    pending: &EffectContinuation,
    selected: &[EffectObjectBinding],
) -> Result<(), String> {
    let source = pending
        .ctx
        .ability_source_contract
        .ok_or("Hunter selection lost original source")?;
    validate_source(state, source)?;
    if pending.ctx.controller != source.controller
        || pending.resolving_item.kind != crate::state::StackItemKind::TriggeredAbility
        || pending.resolving_item.v4.granted_by.is_some()
        || pending.resolving_item.inline_effect.as_ref() != Some(&attack_program())
        || selected.len() > 1
    {
        return Err("Hunter consequence lost its exact printed attack root".into());
    }
    let Some(chosen) = selected.first() else {
        return Ok(());
    };
    let live = state
        .objects
        .try_get(chosen.object)
        .ok_or("Hunter selected object absent")?;
    if live.zone != Zone::Battlefield
        || chosen.expected_zone != Zone::Battlefield
        || live.zone_change_count != chosen.expected_zone_change_count
        || live.controller != source.controller
        || !crate::engine::object_has_type(state, chosen.object, CardType::Creature)
        || (chosen.object == source.source && live.zone_change_count == source.zone_change_count)
    {
        return Err("Hunter sacrifice requires another exact controlled creature".into());
    }
    if let Some(live) = state.objects.try_get(source.source) {
        if live.zone == Zone::Battlefield
            && live.owner == source.owner
            && live.zone_change_count == source.zone_change_count
        {
            live.counters
                .plus1_plus1
                .checked_add(1)
                .ok_or("Hunter +1/+1 counter overflow")?;
        }
    }
    crate::event::commit_sacrifice_death_group_v1(state, &[chosen.object]);
    if state.objects.get(chosen.object).zone == Zone::Battlefield
        && state.objects.get(chosen.object).zone_change_count == chosen.expected_zone_change_count
    {
        return Ok(());
    }
    counter_exact_source(state, source)
}
fn counter_exact_source(
    state: &mut GameState,
    source: AbilitySourceContractV4,
) -> Result<(), String> {
    // The source can change controller, characteristics or abilities after capture.
    // Counter only the original owned battlefield incarnation, never a returned one.
    if state.objects.try_get(source.source).is_some_and(|live| {
        live.zone == Zone::Battlefield
            && live.owner == source.owner
            && live.zone_change_count == source.zone_change_count
    }) {
        crate::event::add_plus_one_counters(state, source.source, source.controller, 1)?;
    }
    Ok(())
}
pub(crate) fn require_legacy_supported(
    state: &GameState,
    actor: Option<PlayerId>,
) -> Result<(), String> {
    if !enabled() {
        return Ok(());
    }
    let visible = state.objects.iter().any(|(_, o)| {
        owns_definition(o.card_def)
            && o.v4.face_down_v1.is_none()
            && (matches!(
                o.zone,
                Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
            ) || o.zone == Zone::Hand && actor == Some(o.owner))
    });
    let history = state.engine.event_history.iter().any(|e| match e {
        CommittedEvent::HighSocietyHunterAttackCaptureV1 { .. } => true,
        CommittedEvent::ControlledCreatureDeathCaptureV1 { source, .. } => {
            owns_definition(source.card_def)
        }
        _ => false,
    });
    if visible || history {
        return Err(
            "High-Society Hunter requires its distinct death/optional-sacrifice consumer successor"
                .into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
