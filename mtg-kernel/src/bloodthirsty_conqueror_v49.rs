//! Original loss operations for the prepared Bloodthirsty Conqueror successor.
//! Private input worlds are never actor observations or trigger-log events.
use crate::card_def::{CardType, Keywords, Subtype, TargetSpec, CARD_DEFS};
use crate::event::CommittedEvent;
use crate::ids::PlayerId;
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::{AbilitySourceContractV4, GameState, Zone};
use serde::{Deserialize, Serialize};
pub(crate) mod custody;
use crate::effect::EffectOp;
use crate::trigger::{PendingTrigger, TriggerCondition, TriggeredAbilityDef};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LossV49 {
    pub history_index: u64,
    pub player: PlayerId,
    pub amount: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchV49 {
    pub history_start: u64,
    pub history_end: u64,
    pub losses: Vec<LossV49>,
    pub observers: Vec<AbilitySourceContractV4>,
    input_history_index: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObserverInputV49 {
    history_start: u64,
    history_end: u64,
    losses: Vec<LossV49>,
    original_world: Vec<u8>,
    observer_world: Vec<u8>,
}
pub(crate) struct RecorderV49 {
    start: usize,
    original_world: Vec<u8>,
}
pub fn recipe() -> EffectOp {
    EffectOp::ConquerorGainFromCaptureV49 {
        capture_history_index: u64::MAX,
    }
}
pub const TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::OpponentLosesLifeBatchV49,
    home_zone: Zone::Battlefield,
    face_index: 0,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    effect: recipe,
}];
pub const PRINTED_COST: Cost = Cost {
    generic: 3,
    pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)],
    x_count: 0,
};
pub(crate) fn owns_definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
        d.has_full_support()
            && d.name == "Bloodthirsty Conqueror"
            && d.object_name == d.name
            && d.cost == PRINTED_COST
            && d.mana_value == 5
            && d.types == [CardType::Creature]
            && d.colors == [ManaColor::B]
            && d.subtypes == [Subtype::Vampire, Subtype::Knight]
            && d.supertypes.is_empty()
            && d.power == Some(5)
            && d.toughness == Some(5)
            && d.keywords == (Keywords::FLYING | Keywords::DEATHTOUCH)
            && d.target_spec == TargetSpec::None
            && !d.is_token
            && !d.is_land
            && d.activated_abilities.is_empty()
            && d.additional_cost.is_none()
            && d.optional_additional_cost.is_none()
            && d.alt_cost.is_none()
            && d.kicker_cost.is_none()
            && d.kicked_target_spec.is_none()
            && d.flashback.is_none()
            && d.escape.is_none()
            && d.plot_cost.is_none()
            && d.madness_cost.is_none()
            && d.mode2.is_none()
            && d.mode3.is_none()
            && d.additional_modes.is_empty()
            && d.omen.is_none()
            && d.adventure.is_none()
            && d.bestow.is_none()
            && d.saga.is_none()
            && d.transform_face.is_none()
            && d.ward_cost.is_none()
            && !d.changeling
            && d.animation.is_none()
            && d.controlled_creature_grant.is_none()
            && d.controlled_counter_keyword.is_none()
            && d.game_outcome_protection_v1.is_none()
            && d.damage_replacement_v1.is_none()
            && d.equipment.is_none()
            && d.attachment.is_none()
            && d.starting_loyalty.is_none()
            && (d.spell_effect)()
                == Some(EffectOp::MoveObject {
                    object: crate::effect::ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                })
            && d.enters_with_plus_one_counters.is_none()
            && d.generic_cost_reduction.is_none()
            && !d.spell_cannot_be_countered
            && !d.records_mana_spent
            && d.produces_mana.is_empty()
            && (d.mana_ability)().is_none()
            && d.mana_ability_choices.is_empty()
            && !d.enters_battlefield_tapped
            && d.mana_ability_def.is_none()
            && d.minimum_blockers == 1
            && !d.mana_ability_includes_chosen_color
            && d.as_enters_choose_color_other_than.is_none()
            && d.additional_mana_abilities.is_empty()
            && d.enters_battlefield_tapped_unless.is_none()
            && !d.delve
            && !d.cant_be_blocked_by_monarchs_creatures
            && d.conditional_tap_yield.is_none()
            && d.enters_tapped_unless_controller.is_none()
            && d.additional_mana_ability_conditions.is_empty()
            && d.restricted_mana_abilities.is_empty()
            && d.activated_ability_generic_reductions.is_empty()
            && d.combat_static_rules == crate::card_def::CombatStaticRulesDef::NONE
            && d.as_enters_color_choice_v1.is_none()
            && d.chosen_color_creature_boost_v1.is_none()
            && {
                let roots = crate::trigger::triggers_for(id);
                roots.len() == 1
                    && roots[0].condition == TriggerCondition::OpponentLosesLifeBatchV49
                    && roots[0].home_zone == Zone::Battlefield
                    && roots[0].face_index == 0
                    && !roots[0].intervening_if_kicked
                    && !roots[0].intervening_if_controls_another_source_card
                    && (roots[0].effect)() == recipe()
            }
    })
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
fn observers(state: &GameState) -> Vec<AbilitySourceContractV4> {
    state
        .objects
        .iter()
        .filter(|(id, object)| {
            object.zone == Zone::Battlefield
                && object.v4.face_index == 0
                && object.v4.face_down_v1.is_none()
                && owns_definition(object.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, *id)
        })
        .map(|(id, _)| AbilitySourceContractV4::capture(state, id))
        .collect()
}
fn losses(state: &GameState, start: usize, end: usize) -> Result<Vec<LossV49>, String> {
    let events = state
        .engine
        .event_history
        .get(start..end)
        .ok_or("Conqueror loss interval changed")?;
    Ok(events
        .iter()
        .enumerate()
        .filter_map(|(offset, event)| {
            let (player, amount) = match event {
                CommittedEvent::LifeLoss { player, amount } => (*player, *amount),
                CommittedEvent::Damage {
                    target: crate::state::Target::Player(player),
                    amount,
                    ..
                } => (*player, *amount),
                _ => return None,
            };
            (amount > 0).then_some(LossV49 {
                history_index: (start + offset) as u64,
                player,
                amount,
            })
        })
        .collect())
}
impl RecorderV49 {
    fn original(state: &GameState) -> Result<Self, String> {
        let mut original = state.clone();
        original.engine.event_history.clear();
        Ok(Self {
            start: state.engine.event_history.len(),
            original_world: serde_json::to_vec(&original).map_err(|e| e.to_string())?,
        })
    }
    pub(crate) fn capture(state: &GameState) -> Option<Self> {
        if !enabled() {
            return None;
        }
        // GameState consists solely of serializable typed engine state.
        Some(Self::original(state).expect("Conqueror original typed state serializes"))
    }
    pub(crate) fn finish(self, state: &mut GameState) {
        let end = state.engine.event_history.len();
        let actual = losses(state, self.start, end).expect("actual committed interval exists");
        if actual.is_empty() {
            return;
        }
        let observers = observers(state);
        let mut observer_state = state.clone();
        observer_state.engine.event_history.clear();
        let observer_world = serde_json::to_vec(&observer_state)
            .expect("Conqueror post-event typed state serializes");
        let input_history_index = state.engine.event_history.len() as u64;
        state
            .engine
            .event_history
            .push(CommittedEvent::ConquerorObserverInputV49 {
                input: Box::new(ObserverInputV49 {
                    history_start: self.start as u64,
                    history_end: end as u64,
                    losses: actual.clone(),
                    original_world: self.original_world,
                    observer_world,
                }),
            });
        let batch_index = state.engine.event_history.len() as u64;
        let captures = observers
            .iter()
            .enumerate()
            .filter_map(|(observer, source)| {
                actual
                    .iter()
                    .any(|loss| loss.player == source.controller.opponent())
                    .then_some(observer as u64)
            })
            .collect::<Vec<_>>();
        state
            .engine
            .event_history
            .push(CommittedEvent::ConquerorLossBatchV49 {
                batch: Box::new(BatchV49 {
                    history_start: self.start as u64,
                    history_end: end as u64,
                    losses: actual,
                    observers,
                    input_history_index,
                }),
            });
        for observer in captures {
            let capture = CommittedEvent::ConquerorCapturedV49 {
                batch_history_index: batch_index,
                observer_index: observer,
            };
            state.engine.event_history.push(capture.clone());
            state.engine.event_log.push(capture);
        }
    }
}
impl BatchV49 {
    pub(crate) fn validate(&self, state: &GameState, index: usize) -> Result<(), String> {
        let input_at = usize::try_from(self.input_history_index)
            .map_err(|_| "Conqueror observer input exceeds usize")?;
        let Some(CommittedEvent::ConquerorObserverInputV49 { input }) =
            state.engine.event_history.get(input_at)
        else {
            return Err("Conqueror original post-event input absent".into());
        };
        if input_at + 1 != index
            || self.history_start != input.history_start
            || self.history_end != input.history_end
            || self.losses != input.losses
        {
            return Err("Conqueror derived batch changed actual original input".into());
        }
        let observed = input.validate(state, input_at)?;
        if observers(&observed) != self.observers {
            return Err("Conqueror original post-event universe changed".into());
        }
        for source in &self.observers {
            crate::engine::validate_historical_ability_source_contract(state, *source)?;
        }
        for player in [PlayerId::P0, PlayerId::P1] {
            self.amount_for(player)?;
        }
        Ok(())
    }
    pub(crate) fn amount_for(&self, player: PlayerId) -> Result<i32, String> {
        self.losses
            .iter()
            .filter(|loss| loss.player == player)
            .try_fold(0i32, |amount, loss| {
                amount
                    .checked_add(loss.amount)
                    .ok_or_else(|| "Conqueror actual batch amount overflows".into())
            })
    }
}
impl ObserverInputV49 {
    fn validate(&self, state: &GameState, index: usize) -> Result<GameState, String> {
        let start =
            usize::try_from(self.history_start).map_err(|_| "Conqueror start exceeds usize")?;
        let end = usize::try_from(self.history_end).map_err(|_| "Conqueror end exceeds usize")?;
        if start >= end
            || end != index
            || self.losses.is_empty()
            || self.losses != losses(state, start, end)?
        {
            return Err("Conqueror actual loss operation changed".into());
        }
        let mut original: GameState =
            serde_json::from_slice(&self.original_world).map_err(|e| e.to_string())?;
        let mut observed: GameState =
            serde_json::from_slice(&self.observer_world).map_err(|e| e.to_string())?;
        if !original.engine.event_history.is_empty() || !observed.engine.event_history.is_empty() {
            return Err("Conqueror recursive original input history".into());
        }
        original.engine.event_history = state.engine.event_history[..start].to_vec();
        observed.engine.event_history = state.engine.event_history[..end].to_vec();
        if original.turn_key_v1() != observed.turn_key_v1()
            || original.active_player != observed.active_player
            || original.step != observed.step
        {
            return Err("Conqueror observer input crossed an engine step".into());
        }
        let mut life = [original.players[0].life, original.players[1].life];
        for event in &state.engine.event_history[start..end] {
            match event {
                CommittedEvent::LifeLoss { player, amount }
                | CommittedEvent::Damage {
                    target: crate::state::Target::Player(player),
                    amount,
                    ..
                } => {
                    life[player.index()] = life[player.index()]
                        .checked_sub(*amount)
                        .ok_or("Conqueror actual life total overflow")?;
                }
                CommittedEvent::LifeGain { player, amount } => {
                    life[player.index()] = life[player.index()]
                        .checked_add(*amount)
                        .ok_or("Conqueror actual life total overflow")?;
                }
                _ => {}
            }
        }
        if life != [observed.players[0].life, observed.players[1].life] {
            return Err("Conqueror observed life does not match actual operation events".into());
        }
        Ok(observed)
    }
}
pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::ConquerorLossBatchV49 { .. }
                | CommittedEvent::ConquerorObserverInputV49 { .. }
        )
    }) {
        return Err("Conqueror private loss world entered public trigger log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ConquerorObserverInputV49 { input } = event {
            input.validate(state, index)?;
            if !matches!(state.engine.event_history.get(index + 1), Some(CommittedEvent::ConquerorLossBatchV49 { batch }) if batch.input_history_index == index as u64)
            {
                return Err(
                    "Conqueror original observer input lost its unique derived batch".into(),
                );
            }
        }
        if let CommittedEvent::ConquerorLossBatchV49 { batch } = event {
            batch.validate(state, index)?;
        }
    }
    let batches = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| {
            if let CommittedEvent::ConquerorLossBatchV49 { batch } = event {
                Some(batch.as_ref())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if enabled() || !batches.is_empty() {
        for loss in losses(state, 0, state.engine.event_history.len())? {
            if batches
                .iter()
                .filter(|batch| batch.losses.iter().any(|member| *member == loss))
                .count()
                != 1
            {
                return Err(
                    "Conqueror actual positive loss lost its unique operation boundary".into(),
                );
            }
        }
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ConquerorLossBatchV49 { batch } = event {
            let expected = batch
                .observers
                .iter()
                .enumerate()
                .filter_map(|(observer, source)| {
                    batch
                        .losses
                        .iter()
                        .any(|loss| loss.player == source.controller.opponent())
                        .then_some(observer as u64)
                })
                .collect::<Vec<_>>();
            for (offset, observer) in expected.iter().enumerate() {
                if state.engine.event_history.get(index + 1 + offset)
                    != Some(&CommittedEvent::ConquerorCapturedV49 {
                        batch_history_index: index as u64,
                        observer_index: *observer,
                    })
                {
                    return Err("Conqueror mandatory original capture erased or rewritten".into());
                }
            }
        }
        if let CommittedEvent::ConquerorCapturedV49 { .. } = event {
            root_at(state, index as u64)?;
        }
        if let CommittedEvent::ConquerorCollectedV49 {
            capture_history_index,
        } = event
        {
            if *capture_history_index >= index as u64 {
                return Err("Conqueror collection precedes original occurrence".into());
            }
            root_at(state, *capture_history_index)?;
        }
    }
    custody::validate(state)
}
pub(crate) fn root_at(
    state: &GameState,
    capture: u64,
) -> Result<(AbilitySourceContractV4, i32), String> {
    let Some(CommittedEvent::ConquerorCapturedV49 {
        batch_history_index,
        observer_index,
    }) = state
        .engine
        .event_history
        .get(usize::try_from(capture).map_err(|_| "Conqueror capture exceeds usize")?)
    else {
        return Err("Conqueror original captured occurrence absent".into());
    };
    let Some(CommittedEvent::ConquerorLossBatchV49 { batch }) = state
        .engine
        .event_history
        .get(usize::try_from(*batch_history_index).map_err(|_| "Conqueror batch exceeds usize")?)
    else {
        return Err("Conqueror actual original loss operation absent".into());
    };
    batch.validate(state, *batch_history_index as usize)?;
    let source = *batch
        .observers
        .get(usize::try_from(*observer_index).map_err(|_| "Conqueror observer exceeds usize")?)
        .ok_or("Conqueror original observer absent")?;
    let amount = batch.amount_for(source.controller.opponent())?;
    let eligible = batch
        .observers
        .iter()
        .enumerate()
        .filter_map(|(observer, source)| {
            batch
                .losses
                .iter()
                .any(|loss| loss.player == source.controller.opponent())
                .then_some(observer as u64)
        })
        .collect::<Vec<_>>();
    let position = eligible
        .iter()
        .position(|observer| observer == observer_index)
        .ok_or("Conqueror source did not observe an opponent loss")?;
    if amount <= 0 || *batch_history_index + 1 + position as u64 != capture {
        return Err("Conqueror original occurrence order or positive amount changed".into());
    }
    Ok((source, amount))
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[CommittedEvent],
) -> Result<Vec<PendingTrigger>, crate::ids::ObjectId> {
    let mut pending = Vec::new();
    for event in events {
        let CommittedEvent::ConquerorCapturedV49 { .. } = event else {
            continue;
        };
        let indices = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(index, original)| (original == event).then_some(index as u64))
            .collect::<Vec<_>>();
        let [capture] = indices.as_slice() else {
            return Err(crate::ids::ObjectId(0));
        };
        let (source, _) = root_at(state, *capture).map_err(|_| crate::ids::ObjectId(0))?;
        if state.engine.event_history.iter().any(|original| matches!(original, CommittedEvent::ConquerorCollectedV49 { capture_history_index } if capture_history_index == capture)) { return Err(source.source); }
        pending.push(PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::ConquerorGainFromCaptureV49 {
                capture_history_index: *capture,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: Vec::new(),
            target_contracts: Vec::new(),
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: Vec::new(),
        });
        state
            .engine
            .event_history
            .push(CommittedEvent::ConquerorCollectedV49 {
                capture_history_index: *capture,
            });
    }
    Ok(pending)
}
pub(crate) fn validate_source(
    state: &GameState,
    effect: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let Some(capture) = custody::binding(effect) else {
        return Ok(());
    };
    if source != Some(root_at(state, capture)?.0) {
        return Err("Conqueror bound root original source changed".into());
    }
    Ok(())
}
pub(crate) fn require_legacy_consumer(state: &GameState) -> Result<(), String> {
    // The new catalog has an occurrence mechanic absent from every frozen
    // consumer contract. V49 will opt in through a distinct typed producer.
    if enabled()
        || state.objects.iter().any(|(_, object)| {
            object.zone == Zone::Battlefield
                && object.v4.face_down_v1.is_none()
                && owns_definition(object.card_def)
        })
        || state.engine.event_history.iter().any(|event| {
            matches!(
                event,
                CommittedEvent::ConquerorCapturedV49 { .. }
                    | CommittedEvent::ConquerorJournalV49 { .. }
            )
        })
    {
        return Err("Bloodthirsty Conqueror requires its original loss-batch V49 consumer".into());
    }
    Ok(())
}
#[cfg(test)]
mod tests;
