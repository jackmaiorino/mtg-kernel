//! Exact wide life operations and distinct Conqueror capture successor.
//! Frozen V49 records, journals, DTOs and ordinary events remain unchanged.
use crate::bloodthirsty_conqueror_v49::{observers, owns_definition};
use crate::{
    card_def::TargetSpec,
    effect::EffectOp,
    event::CommittedEvent,
    ids::PlayerId,
    state::{AbilitySourceContractV4, GameState},
    trigger::PendingTrigger,
};
use serde::{Deserialize, Serialize};
pub(crate) mod custody;
fn enabled() -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LossV59 {
    pub history_index: u64,
    pub player: PlayerId,
    pub amount: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BatchV59 {
    pub history_start: u64,
    pub history_end: u64,
    pub losses: Vec<LossV59>,
    pub observers: Vec<AbilitySourceContractV4>,
    input_history_index: u64,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ObserverInputV59 {
    history_start: u64,
    history_end: u64,
    losses: Vec<LossV59>,
    original_world: Vec<u8>,
    observer_world: Vec<u8>,
}
pub(crate) struct RecorderV59 {
    start: usize,
    original_world: Vec<u8>,
}
fn losses(state: &GameState, start: usize, end: usize) -> Result<Vec<LossV59>, String> {
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
                CommittedEvent::LifeLossWideV59 { player, amount } => (*player, *amount),
                _ => return None,
            };
            (amount > 0).then_some(LossV59 {
                history_index: (start + offset) as u64,
                player,
                amount,
            })
        })
        .collect())
}
impl RecorderV59 {
    fn original(state: &GameState) -> Result<Self, String> {
        let mut original = state.clone();
        original.engine.event_history.clear();
        Ok(Self {
            start: state.engine.event_history.len(),
            original_world: serde_json::to_vec(&original).map_err(|e| e.to_string())?,
        })
    }
    pub(crate) fn capture(state: &GameState) -> Option<Self> {
        if !enabled()
            || !state
                .objects
                .iter()
                .any(|(_, object)| owns_definition(object.card_def))
        {
            return None;
        }
        // Include every zone: a genuine return/entry in this same batch can
        // establish an ordinary post-event observer. A card copy likewise
        // has an actual original source in the preceding world.
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
        if observers.is_empty() {
            return;
        }
        let mut observer_state = state.clone();
        observer_state.engine.event_history.clear();
        let observer_world = serde_json::to_vec(&observer_state)
            .expect("Conqueror post-event typed state serializes");
        let input_history_index = state.engine.event_history.len() as u64;
        state
            .engine
            .event_history
            .push(CommittedEvent::ConquerorObserverInputV59 {
                input: Box::new(ObserverInputV59 {
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
            .push(CommittedEvent::ConquerorLossBatchV59 {
                batch: Box::new(BatchV59 {
                    history_start: self.start as u64,
                    history_end: end as u64,
                    losses: actual,
                    observers,
                    input_history_index,
                }),
            });
        for observer in captures {
            let capture = CommittedEvent::ConquerorCapturedV59 {
                batch_history_index: batch_index,
                observer_index: observer,
            };
            state.engine.event_history.push(capture.clone());
            state.engine.event_log.push(capture);
        }
    }
}
impl BatchV59 {
    pub(crate) fn validate(&self, state: &GameState, index: usize) -> Result<(), String> {
        let input_at = usize::try_from(self.input_history_index)
            .map_err(|_| "Conqueror observer input exceeds usize")?;
        let Some(CommittedEvent::ConquerorObserverInputV59 { input }) =
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
    pub(crate) fn amount_for(&self, player: PlayerId) -> Result<u32, String> {
        self.losses
            .iter()
            .filter(|loss| loss.player == player)
            .try_fold(0u32, |amount, loss| {
                amount
                    .checked_add(loss.amount)
                    .ok_or_else(|| "Conqueror actual batch amount overflows".into())
            })
    }
}
impl ObserverInputV59 {
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
        original.engine.event_history = state.engine.event_history[..start].to_vec().into();
        observed.engine.event_history = state.engine.event_history[..end].to_vec().into();
        if original.turn_key_v1() != observed.turn_key_v1()
            || original.active_player != observed.active_player
            || original.step != observed.step
        {
            return Err("Conqueror observer input crossed an engine step".into());
        }
        let mut life = [original.players[0].life, original.players[1].life];
        for event in &state.engine.event_history[start..end] {
            match event {
                CommittedEvent::LifeLossWideV59 { player, amount } => {
                    life[player.index()] = checked_life(life[player.index()], *amount, false)?;
                }
                CommittedEvent::LifeGainWideV59 { player, amount } => {
                    life[player.index()] = checked_life(life[player.index()], *amount, true)?;
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
    if !has_wide_history(state) {
        return custody::validate(state);
    }
    if !enabled() {
        return Err("wide life history belongs to unsupported feature pool".into());
    }
    for event in &state.engine.event_history {
        if matches!(event, CommittedEvent::LifeLossWideV59 { amount, .. } | CommittedEvent::LifeGainWideV59 { amount, .. } if *amount <= i32::MAX.unsigned_abs())
        {
            return Err("wide life event erased its exact wide amount".into());
        }
    }
    for event in &state.engine.event_log {
        if matches!(
            event,
            CommittedEvent::ConquerorCapturedV59 { .. }
                | CommittedEvent::FiendishPandaWideGainCaptureV59 { .. }
        ) && state
            .engine
            .event_history
            .iter()
            .filter(|original| *original == event)
            .count()
            != 1
        {
            return Err(
                "wide life current capture lost its actual unique history occurrence".into(),
            );
        }
    }
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            CommittedEvent::ConquerorLossBatchV59 { .. }
                | CommittedEvent::ConquerorObserverInputV59 { .. }
        )
    }) {
        return Err("Conqueror private loss world entered public trigger log".into());
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ConquerorObserverInputV59 { input } = event {
            input.validate(state, index)?;
            if !matches!(state.engine.event_history.get(index + 1), Some(CommittedEvent::ConquerorLossBatchV59 { batch }) if batch.input_history_index == index as u64)
            {
                return Err(
                    "Conqueror original observer input lost its unique derived batch".into(),
                );
            }
        }
        if let CommittedEvent::ConquerorLossBatchV59 { batch } = event {
            batch.validate(state, index)?;
        }
    }
    let batches = state
        .engine
        .event_history
        .iter()
        .filter_map(|event| {
            if let CommittedEvent::ConquerorLossBatchV59 { batch } = event {
                Some(batch.as_ref())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    if batches
        .windows(2)
        .any(|pair| pair[0].history_end > pair[1].history_start)
    {
        return Err("Conqueror original operation intervals overlap".into());
    }
    // Original inputs above dictate every owned operation's complete actual
    // loss roster. Ordinary losses without a post-event observer need no
    // Conqueror records, including losses before entry or after departure.
    // Two different owned operations may never claim the same actual loss.
    for loss in losses(state, 0, state.engine.event_history.len())? {
        if batches
            .iter()
            .filter(|batch| batch.losses.contains(&loss))
            .count()
            > 1
        {
            return Err(
                "Conqueror actual positive loss has overlapping operation boundaries".into(),
            );
        }
    }
    for (index, event) in state.engine.event_history.iter().enumerate() {
        if let CommittedEvent::ConquerorLossBatchV59 { batch } = event {
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
                    != Some(&CommittedEvent::ConquerorCapturedV59 {
                        batch_history_index: index as u64,
                        observer_index: *observer,
                    })
                {
                    return Err("Conqueror mandatory original capture erased or rewritten".into());
                }
            }
        }
        if let CommittedEvent::ConquerorCapturedV59 { .. } = event {
            root_at(state, index as u64)?;
        }
        if let CommittedEvent::ConquerorCollectedV59 {
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
) -> Result<(AbilitySourceContractV4, u32), String> {
    let Some(CommittedEvent::ConquerorCapturedV59 {
        batch_history_index,
        observer_index,
    }) = state
        .engine
        .event_history
        .get(usize::try_from(capture).map_err(|_| "Conqueror capture exceeds usize")?)
    else {
        return Err("Conqueror original captured occurrence absent".into());
    };
    let Some(CommittedEvent::ConquerorLossBatchV59 { batch }) = state
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
    if amount == 0 || *batch_history_index + 1 + position as u64 != capture {
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
        let CommittedEvent::ConquerorCapturedV59 { .. } = event else {
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
        if state.engine.event_history.iter().any(|original| matches!(original, CommittedEvent::ConquerorCollectedV59 { capture_history_index } if capture_history_index == capture)) { return Err(source.source); }
        pending.push(PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::ConquerorGainFromWideCaptureV59 {
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
            .push(CommittedEvent::ConquerorCollectedV59 {
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

/// A single positive operation whose before/after totals are native i32.
/// Their greatest possible difference is u32::MAX, never an arbitrary envelope.
pub(crate) fn checked_life(before: i32, amount: u32, gain: bool) -> Result<i32, String> {
    if !enabled() || amount <= i32::MAX.unsigned_abs() {
        return Err("wide life requires the Limited successor and an actual wide amount".into());
    }
    let before = i64::from(before);
    let after = if gain {
        before + i64::from(amount)
    } else {
        before - i64::from(amount)
    };
    i32::try_from(after).map_err(|_| "wide life result exceeds native i32".into())
}
pub(crate) fn positive_gain(event: &CommittedEvent) -> Option<(PlayerId, u32)> {
    match event {
        CommittedEvent::LifeGain { player, amount } if *amount > 0 => {
            Some((*player, amount.unsigned_abs()))
        }
        CommittedEvent::LifeGainWideV59 { player, amount }
            if enabled() && *amount > i32::MAX.unsigned_abs() =>
        {
            Some((*player, *amount))
        }
        _ => None,
    }
}
pub(crate) fn has_wide_history(state: &GameState) -> bool {
    let selected = |event: &CommittedEvent| {
        matches!(
            event,
            CommittedEvent::LifeLossWideV59 { .. }
                | CommittedEvent::LifeGainWideV59 { .. }
                | CommittedEvent::ConquerorObserverInputV59 { .. }
                | CommittedEvent::ConquerorLossBatchV59 { .. }
                | CommittedEvent::ConquerorCapturedV59 { .. }
                | CommittedEvent::ConquerorCollectedV59 { .. }
                | CommittedEvent::ConquerorJournalV59 { .. }
                | CommittedEvent::FiendishPandaWideGainCaptureV59 { .. }
        )
    };
    state.engine.event_history.iter().any(selected)
        || state.engine.event_log.iter().any(selected)
        || state.stack.iter().any(|item| {
            item.inline_effect
                .as_ref()
                .and_then(custody::binding)
                .is_some()
        })
        || state
            .engine
            .pending_triggers
            .iter()
            .any(|pending| custody::binding(&pending.effect).is_some())
}

pub(crate) fn require_legacy(state: &GameState) -> Result<(), String> {
    if has_wide_history(state) {
        Err("wide life requires complete NextFive successor facts".into())
    } else {
        Ok(())
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
pub(crate) mod tests {
    use super::*;
    use crate::next_five_public_v59::tests as native;
    use crate::{
        event::{self, ProposedEvent},
        policy_observation_v6::tests::put,
        state::Zone,
    };
    pub(crate) fn scene(
        actor: PlayerId,
        conq: bool,
    ) -> (
        GameState,
        crate::ids::ObjectId,
        Option<crate::ids::ObjectId>,
    ) {
        let mut state = native::ready(actor);
        // Explicit protection operands keep both players alive at negative
        // representable life. They are not claimed as paid Herald casts.
        for player in [actor, actor.opponent()] {
            put(
                &mut state,
                player,
                "Herald of Eternal Dawn",
                Zone::Battlefield,
            );
        }
        let snack = native::cast(&mut state, actor, "Midnight Snack", None, None);
        native::settle(&mut state);
        let panda = if conq {
            native::cast(&mut state, actor, "Bloodthirsty Conqueror", None, None);
            native::settle(&mut state);
            let panda = native::cast(&mut state, actor, "Fiendish Panda", None, None);
            native::settle(&mut state);
            Some(panda)
        } else {
            None
        };
        (state, snack, panda)
    }
    pub(crate) fn gain_input(state: &mut GameState, actor: PlayerId) {
        for _ in 0..2 {
            event::propose_and_commit(state, ProposedEvent::life_gain(actor, 1_100_000_000));
            event::propose_and_commit(state, ProposedEvent::life_loss(actor, 1_100_000_000));
            native::settle(state);
        }
        event::propose_and_commit(
            state,
            ProposedEvent::life_gain(actor.opponent(), i32::MAX - 20),
        );
        event::propose_and_commit(state, ProposedEvent::life_loss(actor, 100_000_000));
        native::settle(state);
        assert_eq!(state.players[actor.index()].life, -99_999_980);
        assert_eq!(state.players[actor.opponent().index()].life, i32::MAX);
    }
    #[test]
    fn paid_snack_commits_one_exact_wide_loss_and_conqueror_panda_gain_with_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, snack, panda) = scene(actor, true);
            gain_input(&mut state, actor);
            let panda = panda.unwrap();
            let before_counters = state.objects.get(panda).counters.plus1_plus1;
            crate::midnight_snack_v1::tests::activate_paid(&mut state, actor, snack);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                native::settle(game);
                validate(game).unwrap();
                crate::fiendish_panda_v1::validate_state(game).unwrap();
                assert!(game.engine.halted.is_none());
                assert_eq!(game.players[actor.opponent().index()].life, -52_516_353);
                assert_eq!(game.players[actor.index()].life, 2_100_000_020);
                assert_eq!(
                    game.objects.get(panda).counters.plus1_plus1,
                    before_counters + 1
                );
                assert_eq!(
                    crate::midnight_snack_v1::gained_this_turn(game, actor).unwrap(),
                    4_400_000_000
                );
                assert_eq!(game.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::LifeLossWideV59 { player, amount: 2_200_000_000 } if *player == actor.opponent())).count(), 1);
                assert_eq!(game.engine.event_history.iter().filter(|e| matches!(e, CommittedEvent::LifeGainWideV59 { player, amount: 2_200_000_000 } if *player == actor)).count(), 1);
                assert_eq!(
                    game.engine
                        .event_history
                        .iter()
                        .filter(|e| matches!(e, CommittedEvent::ConquerorCapturedV59 { .. }))
                        .count(),
                    1
                );
                assert_eq!(
                    game.engine
                        .event_history
                        .iter()
                        .filter(|e| matches!(
                            e,
                            CommittedEvent::FiendishPandaWideGainCaptureV59 { .. }
                        ))
                        .count(),
                    1
                );
                assert!(crate::life_loss_turn_v1::flags(game).unwrap()[actor.opponent().index()]);
                assert!(crate::conqueror_public_v49::capture(game).is_err());
                assert!(crate::panda_public_v47::capture(game).is_err());
            }
            assert_eq!(state, restored);
        }
    }
    #[test]
    fn paid_snack_wide_loss_without_observer_keeps_exact_target_and_final_life_restore_both_seats()
    {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, snack, _) = scene(actor, false);
            gain_input(&mut state, actor);
            crate::midnight_snack_v1::tests::activate_paid(&mut state, actor, snack);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                native::settle(game);
                validate(game).unwrap();
                assert_eq!(game.players[actor.opponent().index()].life, -52_516_353);
                assert_eq!(game.players[actor.index()].life, -99_999_980);
                assert!(game.engine.event_history.iter().all(|e| !matches!(
                    e,
                    CommittedEvent::ConquerorCapturedV59 { .. }
                        | CommittedEvent::LifeGainWideV59 { .. }
                )));
            }
            assert_eq!(state, restored);
        }
    }
    #[test]
    fn paid_wide_conqueror_range_preflight_does_not_block_actual_counter_and_refuses_forged_rows_both_seats(
    ) {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, snack, _) = scene(actor, true);
            gain_input(&mut state, actor);
            crate::midnight_snack_v1::tests::activate_paid(&mut state, actor, snack);
            for _ in 0..40 {
                let decision = native::next(&mut state);
                if state.stack.iter().any(|item| {
                    item.inline_effect
                        .as_ref()
                        .and_then(custody::binding)
                        .is_some()
                }) {
                    break;
                }
                native::quiet(&mut state, decision);
            }
            let item = state
                .stack
                .iter()
                .find(|item| {
                    item.inline_effect
                        .as_ref()
                        .and_then(custody::binding)
                        .is_some()
                })
                .unwrap()
                .clone();
            // Actual event input changes the prospective gain range while the
            // genuine paid source's trigger waits. Countering is still legal.
            event::propose_and_commit(&mut state, ProposedEvent::life_gain(actor, 100_000_000));
            let before = serde_json::to_vec(&state).unwrap();
            assert!(custody::acquire_for_resolution(&state, &item).is_err());
            assert!(custody::acquire(&state, &item).unwrap().is_some());
            let mut forged = item.clone();
            forged.controller = actor.opponent();
            assert!(custody::acquire(&state, &forged).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            for variant in 0..3 {
                let mut changed = state.clone();
                match variant {
                    0 => {
                        let event = changed
                            .engine
                            .event_history
                            .iter_mut()
                            .find(|event| {
                                matches!(event, CommittedEvent::ConquerorCapturedV59 { .. })
                            })
                            .unwrap();
                        if let CommittedEvent::ConquerorCapturedV59 { observer_index, .. } = event {
                            *observer_index += 1;
                        }
                    }
                    1 => {
                        let event = changed
                            .engine
                            .event_history
                            .iter_mut()
                            .find(|event| matches!(event, CommittedEvent::LifeLossWideV59 { .. }))
                            .unwrap();
                        if let CommittedEvent::LifeLossWideV59 { amount, .. } = event {
                            *amount -= 1;
                        }
                    }
                    _ => {
                        let event = changed
                            .engine
                            .event_history
                            .iter()
                            .find(|event| {
                                matches!(event, CommittedEvent::ConquerorCapturedV59 { .. })
                            })
                            .unwrap()
                            .clone();
                        changed.engine.event_log.push(event.clone());
                        changed.engine.event_log.push(event);
                    }
                }
                let bytes = serde_json::to_vec(&changed).unwrap();
                assert!(validate(&changed).is_err());
                assert_eq!(serde_json::to_vec(&changed).unwrap(), bytes);
            }
            let mut restored: GameState = serde_json::from_slice(&before).unwrap();
            for game in [&mut state, &mut restored] {
                // Existing native counter primitive on an actual published row,
                // without claiming a fictional printed counter-ability spell.
                assert!(
                    crate::engine::counter_stack_item_by_id(game, item.v4.stack_item_id)
                        .unwrap()
                        .is_some()
                );
                native::settle(game);
                validate(game).unwrap();
                assert!(game
                    .engine
                    .event_history
                    .iter()
                    .all(|event| !matches!(event, CommittedEvent::LifeGainWideV59 { .. })));
                assert_eq!(game.players[actor.index()].life, 20); // actual response gain; no wide gain
            }
            assert_eq!(state, restored);
        }
    }
    #[test]
    fn paid_history_wide_gain_primitive_counts_once_for_first_gain_and_never_retries_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, _, _) = scene(actor, false);
            // Explicit source/ledger setup operand. This group tests the native
            // wide event primitive, not a claim that Snack itself gains life.
            put(&mut state, actor, "Cat Collector", Zone::Battlefield);
            crate::life_gain_turn_v1::initialize_for_pool(&mut state);
            event::propose_and_commit(&mut state, ProposedEvent::life_loss(actor, 100_000_000));
            event::propose_and_commit(
                &mut state,
                ProposedEvent::life_gain_wide_v59(actor, 2_200_000_000),
            );
            assert_eq!(state.life_gain_turn_v1.as_ref().unwrap().captures.len(), 1);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                native::settle(game);
                let cats = game
                    .objects
                    .iter()
                    .filter(|(_, object)| {
                        object.zone == Zone::Battlefield
                            && crate::card_def::CARD_DEFS[usize::from(object.card_def)].name
                                == "Cat Token"
                            && object.controller == actor
                    })
                    .count();
                assert_eq!(cats, 1);
                event::propose_and_commit(game, ProposedEvent::life_gain(actor, 1));
                native::settle(game);
                assert_eq!(
                    game.objects
                        .iter()
                        .filter(|(_, object)| object.zone == Zone::Battlefield
                            && crate::card_def::CARD_DEFS[usize::from(object.card_def)].name
                                == "Cat Token"
                            && object.controller == actor)
                        .count(),
                    cats
                );
                assert_eq!(
                    crate::midnight_snack_v1::gained_this_turn(game, actor).unwrap(),
                    2_200_000_001
                );
            }
            assert_eq!(state, restored);
        }
    }
    #[test]
    fn paid_hatcher_and_wide_snack_conqueror_panda_keep_original_enclosure_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, snack, panda) = scene(actor, true);
            let hatcher = native::cast(&mut state, actor, "Drake Hatcher", None, None);
            native::settle(&mut state);
            assert_eq!(state.objects.get(hatcher).zone, Zone::Battlefield);
            gain_input(&mut state, actor);
            crate::midnight_snack_v1::tests::activate_paid(&mut state, actor, snack);
            let mut restored: GameState =
                serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            for game in [&mut state, &mut restored] {
                native::settle(game);
                validate(game).unwrap();
                crate::drake_hatcher_v1::custody::validate(game).unwrap();
                crate::fiendish_panda_v1::validate_state(game).unwrap();
                assert_eq!(game.players[actor.index()].life, 2_100_000_020);
                assert_eq!(game.players[actor.opponent().index()].life, -52_516_353);
                assert_eq!(game.objects.get(panda.unwrap()).counters.plus1_plus1, 3);
                assert_eq!(game.engine.event_history.iter().filter(|event| matches!(event,
                    CommittedEvent::LifeGainWideV59 { player, amount: 2_200_000_000 } if *player == actor)).count(), 1);
                assert!(game
                    .engine
                    .event_history
                    .iter()
                    .any(|event| matches!(event, CommittedEvent::DrakeJournalV1 { .. })));
                assert!(game.engine.halted.is_none());
            }
            assert_eq!(state, restored);
        }
    }
}
