//! Stackless combat replacement ordering. Only the actual engine wave can publish.
use crate::effect::crystal_prevention_v1::{BatchDamageV2, BatchOutcomeV2, ChoiceV1, PreventionV1};
use crate::engine::{CombatState, Decision};
use crate::event::{
    ActiveReplacement, CommittedEvent as E, DamageProposed, ProposedEvent, ReplacementEffectKind,
};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameObject, GameState, Step, Target};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateV38(pub Option<PendingV38>);
impl StateV38 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}
impl std::hash::Hash for StateV38 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        if let Some(p) = &self.0 {
            std::hash::Hash::hash(&"twinflame-combat/v38", h);
            std::hash::Hash::hash(p, h);
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct PublicationV38 {
    pub index: u32,
    pub turn: u32,
    pub active: PlayerId,
    pub combat: CombatState,
    pub packets: Vec<ChoiceV1>,
    pub sources: Vec<GameObject>,
    pub recipients: Vec<Option<GameObject>>,
    pub lifelink: Vec<Option<PlayerId>>,
    pub powers: Vec<i32>,
    pub keyword_masks: Vec<u32>,
    pub color_masks: Vec<u8>,
    pub legacy_normal_after: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct PendingV38 {
    pub publication: u32,
    pub latest: u32,
    pub remaining: Vec<usize>,
    pub selected: Option<usize>,
    pub packets: Vec<ChoiceV1>,
    pub outcomes: Vec<BatchOutcomeV2>,
    pub shields: Vec<ActiveReplacement>,
}
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub enum InputKindV38 {
    SelectPacket(usize),
    Replacement(u16),
    FinishPacket,
}
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct InputV38 {
    pub index: u32,
    pub before: PendingV38,
    pub kind: InputKindV38,
    /// True only for an actual returned affected-controller menu answer.
    pub chosen: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, std::hash::Hash, Serialize, Deserialize)]
pub struct OperationV38 {
    pub index: u32,
    pub input: u32,
    pub after: Option<PendingV38>,
    pub damage: Vec<u32>,
    pub gains: Vec<u32>,
    pub combat_player_damage: Vec<u32>,
}
// Serialized state cannot mint this atomic engine commit owner.
pub(crate) struct OwnerV38 {
    before: PendingV38,
    publication: PublicationV38,
}
impl OwnerV38 {
    pub(crate) fn permits(&self, state: &GameState, events: &[ProposedEvent]) -> bool {
        state.engine.twinflame_combat_v38.0.as_ref() == Some(&self.before)
            && matches!(state.engine.event_history.get(self.publication.index as usize), Some(E::TwinflameCombatPublishedV38 { publication }) if publication.as_ref()==&self.publication)
            && events
                == survivors(
                    &transition(
                        &self.publication,
                        &self.before,
                        &InputKindV38::FinishPacket,
                        false,
                    )
                    .ok()
                    .map(|p| p.outcomes)
                    .unwrap_or_default(),
                )
                .unwrap_or_default()
    }
}
fn proposed(choice: &ChoiceV1) -> DamageProposed {
    DamageProposed {
        source: choice.source,
        target: choice.target,
        amount: choice.remaining_amount,
        touched_by: choice.touched.clone(),
        is_combat: true,
        historical_source: None,
    }
}
fn publication(state: &GameState, index: u32) -> Result<&PublicationV38, String> {
    match state.engine.event_history.get(index as usize) {
        Some(E::TwinflameCombatPublishedV38 { publication }) if publication.index == index => {
            Ok(publication)
        }
        _ => Err("combat ordering lost its actual original wave".into()),
    }
}
fn initial(p: &PublicationV38) -> Result<PendingV38, String> {
    if p.packets.is_empty()
        || p.packets.len() != p.sources.len()
        || p.packets.len() != p.recipients.len()
        || p.packets.len() != p.lifelink.len()
        || p.packets.len() != p.powers.len()
        || p.packets.len() != p.keyword_masks.len()
        || p.packets.len() != p.color_masks.len()
    {
        return Err("combat publication changed complete packet roster".into());
    }
    let mut shields = Vec::<ActiveReplacement>::new();
    for packet in &p.packets {
        for option in &packet.options {
            if let PreventionV1::Replacement(r) = option {
                if let Some(original) = shields.iter().find(|o| o.id == r.id) {
                    if original != r {
                        return Err("shared combat shield has conflicting original amounts".into());
                    }
                } else {
                    shields.push(r.clone());
                }
            }
        }
    }
    let mut remaining = (0..p.packets.len()).collect::<Vec<_>>();
    remaining.sort_by_key(|at| usize::from(p.packets[*at].chooser != p.active));
    Ok(PendingV38 {
        publication: p.index,
        latest: p.index,
        remaining,
        selected: None,
        packets: p.packets.clone(),
        outcomes: vec![BatchOutcomeV2::Waiting; p.packets.len()],
        shields,
    })
}
fn group(p: &PendingV38) -> Vec<usize> {
    p.remaining.first().map_or(Vec::new(), |at| {
        let chooser = p.packets[*at].chooser;
        p.remaining
            .iter()
            .copied()
            .take_while(|at| p.packets[*at].chooser == chooser)
            .collect()
    })
}
fn rebased(p: &PendingV38, at: usize) -> ChoiceV1 {
    let mut packet = p.packets[at].clone();
    packet.options = packet
        .options
        .into_iter()
        .filter_map(|option| match option {
            PreventionV1::Replacement(r) => p
                .shields
                .iter()
                .find(|now| now.id == r.id)
                .cloned()
                .map(PreventionV1::Replacement),
            other => Some(other),
        })
        .collect();
    packet
}
fn allocation_choices(p: &PendingV38) -> Vec<usize> {
    let group = group(p);
    let shared = group.iter().any(|a| {
        group.iter().any(|b| {
            a != b
                && rebased(p, *a).options.iter().any(|option| match option {
                    PreventionV1::Replacement(r)
                        if matches!(r.kind, ReplacementEffectKind::PreventNextDamage { .. }) =>
                    {
                        rebased(p, *b)
                            .options
                            .iter()
                            .any(|o| matches!(o,PreventionV1::Replacement(other) if other.id==r.id))
                    }
                    _ => false,
                })
        })
    });
    if shared {
        group
    } else {
        Vec::new()
    }
}
fn needs_order(packet: &ChoiceV1) -> bool {
    packet.remaining_amount>0 && packet.options.len()>1
        && packet.options.iter().any(|o|matches!(o,PreventionV1::Replacement(r) if matches!(r.kind,ReplacementEffectKind::PreventNextDamage {..})))
        && packet.options.iter().any(|o|matches!(o,PreventionV1::Multiplier(_) | PreventionV1::Protection(_) | PreventionV1::Barricade(_)))
}
fn decision_for(p: &PendingV38) -> Result<Option<Decision>, String> {
    if let Some(at) = p.selected {
        let packet = &p.packets[at];
        if needs_order(packet) {
            return Ok(Some(Decision::ChooseEffectOption {
                player: packet.chooser,
                source: packet.source,
                option_count: u16::try_from(packet.options.len())
                    .map_err(|_| "combat replacement menu exceeds u16")?,
            }));
        }
    } else {
        let choices = allocation_choices(p);
        if let Some(at) = choices.first() {
            return Ok(Some(Decision::ChooseEffectOption {
                player: p.packets[*at].chooser,
                source: p.packets[*at].source,
                option_count: u16::try_from(choices.len())
                    .map_err(|_| "combat allocation menu exceeds u16")?,
            }));
        }
    }
    Ok(None)
}
fn transition(
    original: &PublicationV38,
    before: &PendingV38,
    kind: &InputKindV38,
    chosen: bool,
) -> Result<PendingV38, String> {
    let mut after = before.clone();
    match *kind {
        InputKindV38::SelectPacket(at) => {
            if before.selected.is_some() {
                return Err("combat packet already selected".into());
            }
            let offered = allocation_choices(before);
            if if chosen {
                !offered.contains(&at)
            } else {
                !offered.is_empty() || before.remaining.first() != Some(&at)
            } {
                return Err(
                    "combat allocation input differs from actual affected-player choices".into(),
                );
            }
            after.packets[at] = rebased(before, at);
            after.selected = Some(at);
        }
        InputKindV38::Replacement(option) => {
            let at = before
                .selected
                .ok_or("combat replacement has no selected packet")?;
            let packet = &before.packets[at];
            if chosen != needs_order(packet)
                || packet.remaining_amount <= 0
                || packet.options.is_empty()
                || (!chosen && option != 0)
            {
                return Err(
                    "combat replacement input differs from actual offered/forced action".into(),
                );
            }
            let next = super::ordering::expected_after(packet, option)?;
            if let Some(PreventionV1::Replacement(r)) = packet.options.get(option as usize) {
                if let ReplacementEffectKind::PreventNextDamage { remaining, .. } = r.kind {
                    let left = remaining - remaining.min(packet.remaining_amount);
                    if left > 0 {
                        let slot = after
                            .shields
                            .iter_mut()
                            .find(|o| o.id == r.id)
                            .ok_or("combat shared shield disappeared")?;
                        if let ReplacementEffectKind::PreventNextDamage { remaining, .. } =
                            &mut slot.kind
                        {
                            *remaining = left;
                        }
                    } else {
                        after.shields.retain(|o| o.id != r.id);
                    }
                }
            }
            after.packets[at] = next;
        }
        InputKindV38::FinishPacket => {
            let at = before
                .selected
                .ok_or("combat finish has no selected packet")?;
            let packet = &before.packets[at];
            if chosen || (packet.remaining_amount > 0 && !packet.options.is_empty()) {
                return Err("combat packet skipped applicable replacements".into());
            }
            after.outcomes[at] = if packet.remaining_amount > 0 {
                BatchOutcomeV2::Damage(BatchDamageV2 {
                    source: packet.source,
                    target: packet.target,
                    amount: packet.remaining_amount,
                    touched: packet.touched.clone(),
                })
            } else {
                BatchOutcomeV2::Prevented
            };
            after.remaining.retain(|slot| *slot != at);
            after.selected = None;
        }
    }
    if after.packets.len() != original.packets.len() {
        return Err("combat operation changed original roster width".into());
    }
    Ok(after)
}
fn survivors(outcomes: &[BatchOutcomeV2]) -> Result<Vec<ProposedEvent>, String> {
    outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            BatchOutcomeV2::Waiting => {
                Some(Err("combat committed before every affected answer".into()))
            }
            BatchOutcomeV2::Prevented => None,
            BatchOutcomeV2::Damage(d) => Some(Ok(ProposedEvent::Damage(DamageProposed {
                source: d.source,
                target: d.target,
                amount: d.amount,
                touched_by: d.touched.clone(),
                is_combat: true,
                historical_source: None,
            }))),
        })
        .collect()
}
pub(crate) fn has_pending(state: &GameState) -> bool {
    state.engine.twinflame_combat_v38.0.is_some()
}
pub(crate) fn shield_is_owned(state: &GameState, replacement: &ActiveReplacement) -> bool {
    state
        .engine
        .twinflame_combat_v38
        .0
        .as_ref()
        .is_some_and(|pending| {
            pending.selected.is_some_and(|at| {
                pending.packets[at]
                    .options
                    .contains(&PreventionV1::Replacement(replacement.clone()))
            })
        })
}
fn keyword_mask(state: &GameState, source: ObjectId) -> u32 {
    use crate::card_def::Keywords as K;
    [
        K::LIFELINK,
        K::DEATHTOUCH,
        K::FIRST_STRIKE,
        K::DOUBLE_STRIKE,
        K::TRAMPLE,
    ]
    .into_iter()
    .fold(0, |mask, keyword| {
        if crate::engine::has_effective_keyword(state, source, keyword) {
            mask | keyword.0
        } else {
            mask
        }
    })
}
pub(crate) fn begin(
    state: &mut GameState,
    events: &[ProposedEvent],
    legacy_normal_after: bool,
) -> Result<bool, String> {
    if !super::has_active_source(state) {
        return Ok(false);
    }
    let mut eligible = false;
    for event in events {
        if let ProposedEvent::Damage(d) = event {
            eligible |= !super::sources(state, d)?.is_empty();
        }
    }
    if !eligible {
        return Ok(false);
    }
    validate(state)?;
    if state.step != Step::CombatDamage
        || has_pending(state)
        || state.engine.pending_effect.is_some()
        || !state.stack.is_empty()
    {
        return Err("combat replacement publication is outside actual wave entry".into());
    }
    let mut packets = Vec::new();
    let mut sources = Vec::new();
    let mut recipients = Vec::new();
    let mut lifelink = Vec::new();
    let mut powers = Vec::new();
    let mut keyword_masks = Vec::new();
    let mut color_masks = Vec::new();
    for event in events {
        let ProposedEvent::Damage(d) = event else {
            return Err("combat wave contains foreign event".into());
        };
        let source = state
            .objects
            .try_get(d.source)
            .ok_or("combat source disappeared before publication")?;
        let chooser = match d.target {
            Target::Player(player) => player,
            Target::Object(id) => {
                state
                    .objects
                    .try_get(id)
                    .ok_or("combat recipient disappeared")?
                    .controller
            }
            Target::StackItem(_) => return Err("combat cannot damage stack item".into()),
        };
        packets.push(ChoiceV1 {
            source: d.source,
            target: d.target,
            historical_source: None,
            batch: None,
            creature_power: None,
            original_amount: d.amount,
            remaining_amount: d.amount,
            touched: d.touched_by.clone(),
            chooser,
            path: vec![
                u16::try_from(packets.len()).map_err(|_| "combat packet ordinal exceeds u16")?
            ],
            options: crate::effect::crystal_prevention_v1::applicable(state, d),
            remaining_frames: Vec::new(),
            applied_multipliers: Default::default(),
        });
        sources.push(source.clone());
        powers.push(crate::engine::effective_power(state, d.source));
        keyword_masks.push(keyword_mask(state, d.source));
        color_masks.push(crate::engine::object_color_mask(state, d.source));
        recipients.push(match d.target {
            Target::Object(id) => Some(state.objects.get(id).clone()),
            _ => None,
        });
        lifelink.push(crate::event::twinflame_packet_lifelink_player(
            state,
            d.clone(),
        ));
    }
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "combat publication index exceeds u32")?;
    let original = PublicationV38 {
        index,
        turn: state.turn_key_v1(),
        active: state.active_player,
        combat: state.engine.combat.clone(),
        packets,
        sources,
        recipients,
        lifelink,
        powers,
        keyword_masks,
        color_masks,
        legacy_normal_after,
    };
    let pending = initial(&original)?;
    state
        .engine
        .event_history
        .push(E::TwinflameCombatPublishedV38 {
            publication: Box::new(original),
        });
    state.engine.twinflame_combat_v38.0 = Some(pending);
    drive(state)?;
    Ok(true)
}
pub(crate) fn decision(state: &GameState) -> Result<Option<Decision>, String> {
    validate(state)?;
    if state.engine.halted.is_some() {
        return Ok(None);
    }
    state
        .engine
        .twinflame_combat_v38
        .0
        .as_ref()
        .map_or(Ok(None), decision_for)
}
fn perform(state: &mut GameState, kind: InputKindV38, chosen: bool) -> Result<(), String> {
    let before = state
        .engine
        .twinflame_combat_v38
        .0
        .clone()
        .ok_or("combat operation has no owner")?;
    let original = publication(state, before.publication)?.clone();
    let mut after = transition(&original, &before, &kind, chosen)?;
    let input = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "combat input index exceeds u32")?;
    state.engine.event_history.push(E::TwinflameCombatInputV38 {
        input: Box::new(InputV38 {
            index: input,
            before: before.clone(),
            kind: kind.clone(),
            chosen,
        }),
    });
    if let InputKindV38::Replacement(option) = kind {
        let at = before.selected.unwrap();
        if let PreventionV1::Replacement(r) = &before.packets[at].options[option as usize] {
            let result = crate::event::replacement_apply(
                r,
                ProposedEvent::Damage(proposed(&before.packets[at])),
                state,
            );
            let amount = match result {
                Some(ProposedEvent::Damage(d)) => d.amount,
                None => 0,
                _ => return Err("combat replacement changed event kind".into()),
            };
            if state.engine.halted.is_some() || amount != after.packets[at].remaining_amount {
                return Err("combat actual shield operation changed captured arithmetic".into());
            }
        }
    }
    let start = state.engine.event_history.len();
    let complete = after.remaining.is_empty();
    if complete {
        let owner = OwnerV38 {
            before: before.clone(),
            publication: original.clone(),
        };
        let log_start = state.engine.event_log.len();
        crate::event::commit_twinflame_prepared_combat(state, survivors(&after.outcomes)?, &owner)?;
        crate::engine::finish_combat_damage_events_v38(state, log_start);
    }
    let index = u32::try_from(state.engine.event_history.len())
        .map_err(|_| "combat operation index exceeds u32")?;
    after.latest = index;
    let saved = (!complete).then_some(after);
    let indices = |kind: u8| {
        state.engine.event_history[start..]
            .iter()
            .enumerate()
            .filter_map(|(offset, event)| {
                let found = match kind {
                    0 => matches!(event, E::Damage { .. }),
                    1 => matches!(event, E::LifeGain { .. }),
                    _ => matches!(event, E::CombatDamageToPlayer { .. }),
                };
                found.then_some(
                    u32::try_from(start + offset).map_err(|_| "combat outcome index exceeds u32"),
                )
            })
            .collect::<Result<Vec<_>, _>>()
    };
    let operation = OperationV38 {
        index,
        input,
        after: saved.clone(),
        damage: indices(0)?,
        gains: indices(1)?,
        combat_player_damage: indices(2)?,
    };
    state
        .engine
        .event_history
        .push(E::TwinflameCombatOperationV38 {
            operation: Box::new(operation),
        });
    state.engine.twinflame_combat_v38.0 = saved;
    if complete && original.legacy_normal_after {
        crate::engine::resume_legacy_combat_normal_v38(state);
    }
    Ok(())
}
fn drive(state: &mut GameState) -> Result<(), String> {
    while let Some(pending) = state.engine.twinflame_combat_v38.0.as_ref() {
        if decision_for(pending)?.is_some() {
            break;
        }
        let kind = if let Some(at) = pending.selected {
            let packet = &pending.packets[at];
            if packet.remaining_amount <= 0 || packet.options.is_empty() {
                InputKindV38::FinishPacket
            } else {
                InputKindV38::Replacement(0)
            }
        } else {
            InputKindV38::SelectPacket(
                *pending
                    .remaining
                    .first()
                    .ok_or("open combat owner has no packets")?,
            )
        };
        perform(state, kind, false)?;
    }
    Ok(())
}
pub(crate) fn answer(state: &mut GameState, index: u16) -> Result<(), String> {
    validate(state)?;
    let pending = state
        .engine
        .twinflame_combat_v38
        .0
        .as_ref()
        .ok_or("combat menu absent")?;
    if decision_for(pending)?.is_none() {
        return Err("combat answer has no actual returned menu".into());
    }
    let kind = if pending.selected.is_some() {
        InputKindV38::Replacement(index)
    } else {
        InputKindV38::SelectPacket(
            *allocation_choices(pending)
                .get(index as usize)
                .ok_or("combat allocation answer outside menu")?,
        )
    };
    transition(
        publication(state, pending.publication)?,
        pending,
        &kind,
        true,
    )?;
    perform(state, kind, true)?;
    drive(state)
}

pub(crate) fn validate(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|event| {
        matches!(
            event,
            E::TwinflameCombatPublishedV38 { .. }
                | E::TwinflameCombatInputV38 { .. }
                | E::TwinflameCombatOperationV38 { .. }
        )
    }) {
        return Err("combat private operation moved into public mutable log".into());
    }
    let mut open = None;
    for (at, event) in state.engine.event_history.iter().enumerate() {
        let E::TwinflameCombatPublishedV38 {
            publication: original,
        } = event
        else {
            continue;
        };
        if original.index as usize != at {
            return Err("combat publication occurrence changed".into());
        }
        let mut expected = initial(original)?;
        let mut closed = false;
        for (input_at, event) in state.engine.event_history.iter().enumerate().skip(at + 1) {
            let E::TwinflameCombatInputV38 { input } = event else {
                continue;
            };
            if input.before.publication != original.index {
                continue;
            }
            if closed || input.index as usize != input_at || input.before != expected {
                return Err("combat input skipped or replayed actual prior operation".into());
            }
            let mut next = transition(original, &expected, &input.kind, input.chosen)?;
            let mut operations = state.engine.event_history.iter().enumerate().filter_map(
                |(i, event)| match event {
                    E::TwinflameCombatOperationV38 { operation }
                        if operation.input == input.index =>
                    {
                        Some((i, operation))
                    }
                    _ => None,
                },
            );
            let (op_at, operation) = operations
                .next()
                .ok_or("combat input lost actual engine operation")?;
            if operations.next().is_some() || op_at <= input_at || operation.index as usize != op_at
            {
                return Err("combat actual operation repeated or reordered".into());
            }
            if let InputKindV38::Replacement(option) = input.kind {
                let packet = &expected.packets[expected.selected.unwrap()];
                if let PreventionV1::Replacement(r) = &packet.options[option as usize] {
                    if matches!(r.kind,ReplacementEffectKind::PreventNextDamage {..}) && state.engine.event_history[input_at+1..op_at].iter().filter(|event|
                        matches!(event,E::CrystalShieldOperationV1 {operation} if operation.before==*r && operation.damage_amount==packet.remaining_amount)).count()!=1 {
                        return Err("combat answer lost actual shared shield consumption".into());
                    }
                }
            }
            closed = next.remaining.is_empty();
            next.latest = operation.index;
            if operation.after.as_ref() != (!closed).then_some(&next) {
                return Err("combat operation changed actual remaining packets".into());
            }
            if closed {
                validate_outcomes(state, original, &next, operation, input_at, op_at)?;
            } else if !operation.damage.is_empty()
                || !operation.gains.is_empty()
                || !operation.combat_player_damage.is_empty()
            {
                return Err("combat packet committed before all affected choices".into());
            }
            expected = next;
        }
        if !closed {
            if open.replace(expected).is_some() {
                return Err("multiple actual combat waves remain open".into());
            }
        }
    }
    if state.engine.twinflame_combat_v38.0 != open {
        return Err("combat publication lost mandatory live owner or replayed closed wave".into());
    }
    if let Some(pending) = open {
        let original = publication(state, pending.publication)?;
        if state.step != Step::CombatDamage
            || state.turn_key_v1() != original.turn
            || state.active_player != original.active
            || state.engine.combat != original.combat
            || !state.stack.is_empty()
            || state.engine.pending_effect.is_some()
        {
            return Err("waiting combat wave changed original step/combat ownership".into());
        }
        for (slot, packet) in original.packets.iter().enumerate() {
            if state.objects.try_get(packet.source) != Some(&original.sources[slot])
                || match packet.target {
                    Target::Object(id) => {
                        state.objects.try_get(id) != original.recipients[slot].as_ref()
                    }
                    _ => false,
                }
            {
                return Err(
                    "waiting combat changed original source or recipient incarnation/facts".into(),
                );
            }
            let current =
                crate::effect::crystal_prevention_v1::applicable(state, &proposed(packet));
            let expected_options = packet
                .options
                .iter()
                .filter_map(|o| match o {
                    PreventionV1::Replacement(r) => pending
                        .shields
                        .iter()
                        .find(|now| now.id == r.id)
                        .cloned()
                        .map(PreventionV1::Replacement),
                    other => Some(other.clone()),
                })
                .collect::<Vec<_>>();
            if current != expected_options {
                return Err("waiting combat changed actual replacement sources or shields".into());
            }
        }
    }
    for event in &state.engine.event_history {
        match event {
            E::TwinflameCombatInputV38 { input } => {
                publication(state, input.before.publication)?;
            }
            E::TwinflameCombatOperationV38 { operation } => {
                if !matches!(state.engine.event_history.get(operation.input as usize),Some(E::TwinflameCombatInputV38 {input}) if input.index==operation.input)
                {
                    return Err("orphan combat operation".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}
fn validate_outcomes(
    state: &GameState,
    original: &PublicationV38,
    pending: &PendingV38,
    operation: &OperationV38,
    from: usize,
    until: usize,
) -> Result<(), String> {
    let damage = pending
        .outcomes
        .iter()
        .filter_map(|outcome| match outcome {
            BatchOutcomeV2::Damage(d) => Some(d),
            _ => None,
        })
        .collect::<Vec<_>>();
    if pending
        .outcomes
        .iter()
        .any(|o| matches!(o, BatchOutcomeV2::Waiting))
        || operation.damage.len() != damage.len()
    {
        return Err("combat completed without all simultaneous outcomes".into());
    }
    let mut last = from;
    for (expected, index) in damage.iter().zip(&operation.damage) {
        let at = *index as usize;
        if at <= last
            || at >= until
            || !matches!(state.engine.event_history.get(at),Some(E::Damage {source,target,amount}) if *source==expected.source && *target==expected.target && *amount==expected.amount)
        {
            return Err("combat survivor differs from actual simultaneous damage".into());
        }
        last = at;
    }
    let mut gains = Vec::<(ObjectId, PlayerId, i32)>::new();
    let mut combat = Vec::new();
    for (slot, outcome) in pending.outcomes.iter().enumerate() {
        let BatchOutcomeV2::Damage(d) = outcome else {
            continue;
        };
        if let Some(player) = original.lifelink[slot] {
            if let Some((_, _, amount)) = gains
                .iter_mut()
                .find(|(source, owner, _)| *source == d.source && *owner == player)
            {
                *amount = amount
                    .checked_add(d.amount)
                    .ok_or("combat grouped gain overflow")?;
            } else {
                gains.push((d.source, player, d.amount));
            }
        }
        if let Target::Player(player) = d.target {
            combat.push((
                d.source,
                original.sources[slot].zone_change_count,
                player,
                d.amount,
            ));
        }
    }
    if operation.gains.len() != gains.len() || operation.combat_player_damage.len() != combat.len()
    {
        return Err("combat completion lost mandatory gain/player occurrence".into());
    }
    for ((_, player, amount), index) in gains.into_iter().zip(&operation.gains) {
        let at = *index as usize;
        if at <= last
            || at >= until
            || !matches!(state.engine.event_history.get(at),Some(E::LifeGain {player:p,amount:n}) if *p==player && *n==amount)
        {
            return Err("combat completion changed actual grouped lifelink".into());
        }
        last = at;
    }
    for ((source, zcc, player, amount), index) in
        combat.into_iter().zip(&operation.combat_player_damage)
    {
        let at = *index as usize;
        if at <= last
            || at >= until
            || !matches!(state.engine.event_history.get(at),Some(E::CombatDamageToPlayer {source:s,source_zone_change_count:g,player:p,amount:n,..}) if *s==source && *g==zcc && *p==player && *n==amount)
        {
            return Err("combat completion changed actual player-damage history".into());
        }
        last = at;
    }
    Ok(())
}
