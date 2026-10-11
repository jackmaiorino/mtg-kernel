//! Foundations combat assignment for explicitly opted-in custom games.
//!
//! Every integer allocation is reachable through bounded binary ranges. The
//! continuation holds assignments, never dealt damage: the complete wave is
//! committed simultaneously after the last answer, before granting priority.

use crate::card_def::Keywords;
use crate::engine::{self, Decision};
use crate::event::ProposedEvent;
use crate::ids::PlayerId;
use crate::state::{GameState, ObjectLinkV4, Step, Target, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
enum DamagePhaseV1 {
    #[default]
    BeforeDamage,
    FirstStrike,
    Normal,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FoundationsCombatV1 {
    phase: DamagePhaseV1,
    first_strikers: Vec<ObjectLinkV4>,
    pending: Option<PendingDamageV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct DamageRecipientV1 {
    target: Target,
    incarnation: Option<ObjectLinkV4>,
    lethal: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct CreatureDamageV1 {
    source: ObjectLinkV4,
    controller: PlayerId,
    power: i32,
    recipients: Vec<DamageRecipientV1>,
    trample: bool,
    amounts: Vec<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct DamageRangeV1 {
    minimum: i32,
    maximum: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct PendingDamageV1 {
    creatures: Vec<CreatureDamageV1>,
    creature_index: usize,
    range: Option<DamageRangeV1>,
}

/// Activate only before the first combat. The extension's absence preserves
/// existing state hashes, serialized snapshots and reference-AI behavior.
pub fn enable_foundations_combat_v1(state: &mut GameState) -> Result<(), String> {
    if !state.engine.combat.attackers.is_empty()
        || state.engine.combat.attackers_declared
        || state.engine.combat.blockers_declared
    {
        return Err("combat rules must be selected before combat begins".to_string());
    }
    state.engine.combat.foundations_v1 = Some(FoundationsCombatV1::default());
    Ok(())
}

fn link(state: &GameState, object: crate::ids::ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn live(state: &GameState, reference: ObjectLinkV4) -> bool {
    state
        .objects
        .try_get(reference.object)
        .is_some_and(|object| {
            object.zone == Zone::Battlefield
                && object.zone_change_count == reference.zone_change_count
        })
}

pub(crate) fn has_pending_assignment(state: &GameState) -> bool {
    state
        .engine
        .combat
        .foundations_v1
        .as_ref()
        .is_some_and(|rules| rules.pending.is_some())
}

pub(crate) fn needs_normal_wave(state: &GameState) -> bool {
    state
        .engine
        .combat
        .foundations_v1
        .as_ref()
        .is_some_and(|rules| rules.phase == DamagePhaseV1::FirstStrike && rules.pending.is_none())
}

pub(crate) struct AssignedCombatDamageIdsV1 {
    pub source: crate::ids::ObjectId,
    pub recipient: Target,
    pub amount: i32,
}

pub(crate) struct PublicCombatAssignmentIdsV1 {
    pub phase: &'static str,
    pub assignments: Vec<AssignedCombatDamageIdsV1>,
}

pub(crate) fn public_assignment_ids_v1(state: &GameState) -> Option<PublicCombatAssignmentIdsV1> {
    let rules = state.engine.combat.foundations_v1.as_ref()?;
    let phase = match rules.phase {
        DamagePhaseV1::BeforeDamage => "before_damage",
        DamagePhaseV1::FirstStrike => "first_strike",
        DamagePhaseV1::Normal => "normal",
    };
    let assignments = rules
        .pending
        .iter()
        .flat_map(|pending| &pending.creatures)
        .flat_map(|creature| {
            creature
                .amounts
                .iter()
                .zip(&creature.recipients)
                .map(|(&amount, recipient)| AssignedCombatDamageIdsV1 {
                    source: creature.source.object,
                    recipient: recipient.target,
                    amount,
                })
        })
        .collect();
    Some(PublicCombatAssignmentIdsV1 { phase, assignments })
}

pub(crate) fn start_first_wave(state: &mut GameState) {
    let first_strikers = state
        .engine
        .combat
        .attackers
        .iter()
        .copied()
        .chain(
            state
                .engine
                .combat
                .blocked_by
                .iter()
                .flat_map(|(_, blockers)| blockers.iter().copied()),
        )
        .filter(|&id| {
            state.objects.get(id).zone == Zone::Battlefield
                && (engine::has_effective_keyword(state, id, Keywords::FIRST_STRIKE)
                    || engine::has_effective_keyword(state, id, Keywords::DOUBLE_STRIKE))
        })
        .map(|id| link(state, id))
        .collect::<Vec<_>>();
    let phase = if first_strikers.is_empty() {
        DamagePhaseV1::Normal
    } else {
        DamagePhaseV1::FirstStrike
    };
    let pending = build_wave(state, phase, &first_strikers);
    state.engine.combat.foundations_v1 = Some(FoundationsCombatV1 {
        phase,
        first_strikers,
        pending: Some(pending),
    });
}

pub(crate) fn start_normal_wave(state: &mut GameState) {
    let first_strikers = state
        .engine
        .combat
        .foundations_v1
        .as_ref()
        .expect("rules enabled")
        .first_strikers
        .clone();
    let pending = build_wave(state, DamagePhaseV1::Normal, &first_strikers);
    let rules = state
        .engine
        .combat
        .foundations_v1
        .as_mut()
        .expect("rules enabled");
    rules.phase = DamagePhaseV1::Normal;
    rules.pending = Some(pending);
}

fn participates(
    state: &GameState,
    source: ObjectLinkV4,
    phase: DamagePhaseV1,
    first: &[ObjectLinkV4],
) -> bool {
    if !live(state, source) {
        return false;
    }
    match phase {
        DamagePhaseV1::FirstStrike => first.contains(&source),
        DamagePhaseV1::Normal => {
            !first.contains(&source)
                || engine::has_effective_keyword(state, source.object, Keywords::DOUBLE_STRIKE)
        }
        DamagePhaseV1::BeforeDamage => false,
    }
}

fn build_wave(state: &GameState, phase: DamagePhaseV1, first: &[ObjectLinkV4]) -> PendingDamageV1 {
    let mut creatures = Vec::new();
    for &attacker in &state.engine.combat.attackers {
        let source = link(state, attacker);
        if !participates(state, source, phase, first) {
            continue;
        }
        let power = engine::effective_power(state, attacker).max(0);
        if power == 0 {
            continue;
        }
        let controller = state.objects.get(attacker).controller;
        // The defending player, the attacked planeswalker, or nothing once
        // that planeswalker has left (506.4c, 702.19e). A trampler without an
        // excess recipient divides its damage like any blocked creature.
        let excess = crate::attack_target_v1::damage_recipient(state, attacker);
        let trample =
            engine::has_effective_keyword(state, attacker, Keywords::TRAMPLE) && excess.is_some();
        let mut recipients = Vec::new();
        if let Some((_, blockers)) = state
            .engine
            .combat
            .blocked_by
            .iter()
            .find(|(id, _)| *id == attacker)
        {
            for &blocker in blockers {
                let incarnation = link(state, blocker);
                if !live(state, incarnation) {
                    continue;
                }
                let lethal = i32::try_from(
                    (i64::from(engine::effective_toughness(state, blocker))
                        - i64::from(state.objects.get(blocker).damage))
                    .max(0),
                )
                .expect("remaining toughness is no greater than its i32 value");
                let lethal = if engine::has_effective_keyword(state, attacker, Keywords::DEATHTOUCH)
                {
                    lethal.min(1)
                } else {
                    lethal
                };
                recipients.push(DamageRecipientV1 {
                    target: Target::Object(blocker),
                    incarnation: Some(incarnation),
                    lethal,
                });
            }
            if trample {
                if let Some((target, incarnation)) = excess {
                    recipients.push(DamageRecipientV1 {
                        target,
                        incarnation,
                        lethal: 0,
                    });
                }
            }
        } else if let Some((target, incarnation)) = excess {
            recipients.push(DamageRecipientV1 {
                target,
                incarnation,
                lethal: 0,
            });
        }
        if !recipients.is_empty() {
            creatures.push(CreatureDamageV1 {
                source,
                controller,
                power,
                recipients,
                trample,
                amounts: Vec::new(),
            });
        }
    }
    // Blocking creatures in this engine each block one attacker, so their
    // recipient is forced. They still join the same simultaneous batch.
    for (attacker, blockers) in &state.engine.combat.blocked_by {
        let recipient = link(state, *attacker);
        if !live(state, recipient) {
            continue;
        }
        for &blocker in blockers {
            let source = link(state, blocker);
            if !participates(state, source, phase, first) {
                continue;
            }
            let power = engine::effective_power(state, blocker).max(0);
            if power == 0 {
                continue;
            }
            creatures.push(CreatureDamageV1 {
                source,
                controller: state.objects.get(blocker).controller,
                power,
                recipients: vec![DamageRecipientV1 {
                    target: Target::Object(*attacker),
                    incarnation: Some(recipient),
                    lethal: 0,
                }],
                trample: false,
                amounts: Vec::new(),
            });
        }
    }
    PendingDamageV1 {
        creatures,
        creature_index: 0,
        range: None,
    }
}

impl CreatureDamageV1 {
    fn next_range(&self) -> Result<DamageRangeV1, String> {
        let assigned = self.amounts.iter().try_fold(0_i32, |total, amount| {
            if *amount < 0 {
                return Err("negative damage assignment".to_string());
            }
            total
                .checked_add(*amount)
                .ok_or_else(|| "damage assignment overflow".to_string())
        })?;
        let remaining = self
            .power
            .checked_sub(assigned)
            .filter(|value| *value >= 0)
            .ok_or("damage exceeds source power")?;
        let index = self.amounts.len();
        let recipient = self
            .recipients
            .get(index)
            .ok_or("damage recipient missing")?;
        if index + 1 == self.recipients.len() {
            // A trampler's last recipient is always its excess recipient.
            if self.trample
                && remaining > 0
                && self
                    .amounts
                    .iter()
                    .zip(&self.recipients)
                    .any(|(amount, prior)| *amount < prior.lethal)
            {
                return Err("trample assignment bypasses lethal blocker damage".to_string());
            }
            return Ok(DamageRangeV1 {
                minimum: remaining,
                maximum: remaining,
            });
        }
        let last_blocker = self.trample && index + 2 == self.recipients.len();
        let minimum = if last_blocker {
            if self
                .amounts
                .iter()
                .zip(&self.recipients)
                .any(|(amount, prior)| *amount < prior.lethal)
            {
                remaining
            } else {
                recipient.lethal.min(remaining)
            }
        } else {
            0
        };
        Ok(DamageRangeV1 {
            minimum,
            maximum: remaining,
        })
    }
}

fn validate_pending(state: &GameState, pending: &PendingDamageV1) -> Result<(), String> {
    if state.step != Step::CombatDamage
        || !state.stack.is_empty()
        || pending.creature_index > pending.creatures.len()
    {
        return Err("invalid combat damage continuation".to_string());
    }
    for creature in &pending.creatures {
        if !live(state, creature.source)
            || state.objects.get(creature.source.object).controller != creature.controller
            || engine::effective_power(state, creature.source.object).max(0) != creature.power
            || creature.amounts.len() > creature.recipients.len()
            || creature.recipients.is_empty()
        {
            return Err("combat damage source changed".to_string());
        }
        for recipient in &creature.recipients {
            if let Some(reference) = recipient.incarnation {
                if recipient.target != Target::Object(reference.object) || !live(state, reference) {
                    return Err("combat damage recipient changed".to_string());
                }
            }
        }
    }
    Ok(())
}

/// Resume assignments, automatically choosing forced amounts. This routine
/// never performs SBAs or trigger placement between individual assignments.
pub(crate) fn drain_or_decide(state: &mut GameState) -> Result<Option<Decision>, String> {
    let mut pending = state
        .engine
        .combat
        .foundations_v1
        .as_ref()
        .and_then(|rules| rules.pending.clone())
        .ok_or("no pending combat damage")?;
    validate_pending(state, &pending)?;
    while let Some(creature) = pending.creatures.get_mut(pending.creature_index) {
        if creature.amounts.len() == creature.recipients.len() {
            if creature.amounts.iter().sum::<i32>() != creature.power {
                return Err("incomplete combat damage allocation".to_string());
            }
            pending.creature_index += 1;
            continue;
        }
        let allowed = creature.next_range()?;
        let range = pending.range.unwrap_or(allowed);
        if range.minimum < allowed.minimum
            || range.maximum > allowed.maximum
            || range.minimum > range.maximum
        {
            return Err("invalid combat damage amount range".to_string());
        }
        if range.minimum == range.maximum {
            creature.amounts.push(range.minimum);
            pending.range = None;
            continue;
        }
        pending.range = Some(range);
        let decision = Decision::ChooseCombatDamageRange {
            player: creature.controller,
            source: creature.source.object,
            recipient: creature.recipients[creature.amounts.len()].target,
            minimum: range.minimum,
            maximum: range.maximum,
            split_at: range.minimum + (range.maximum - range.minimum) / 2,
        };
        state
            .engine
            .combat
            .foundations_v1
            .as_mut()
            .expect("rules enabled")
            .pending = Some(pending);
        return Ok(Some(decision));
    }
    let mut events = Vec::new();
    for creature in &pending.creatures {
        for (recipient, &amount) in creature.recipients.iter().zip(&creature.amounts) {
            if amount > 0 {
                events.push(ProposedEvent::damage(
                    creature.source.object,
                    recipient.target,
                    amount,
                ));
            }
        }
    }
    state
        .engine
        .combat
        .foundations_v1
        .as_mut()
        .expect("rules enabled")
        .pending = None;
    engine::commit_combat_damage_events(state, events);
    Ok(None)
}

pub(crate) fn answer_range(state: &mut GameState, upper_half: bool) -> Result<(), String> {
    let rules = state
        .engine
        .combat
        .foundations_v1
        .as_ref()
        .ok_or("combat rules disabled")?;
    let pending = rules.pending.as_ref().ok_or("no pending combat damage")?;
    validate_pending(state, pending)?;
    let range = pending
        .range
        .ok_or("combat damage choice has not been offered")?;
    let creature = pending
        .creatures
        .get(pending.creature_index)
        .ok_or("no current damage source")?;
    let allowed = creature.next_range()?;
    if range.minimum < allowed.minimum
        || range.maximum > allowed.maximum
        || range.minimum >= range.maximum
    {
        return Err("invalid combat damage choice range".to_string());
    }
    let split = range.minimum + (range.maximum - range.minimum) / 2;
    let next = if upper_half {
        DamageRangeV1 {
            minimum: split + 1,
            maximum: range.maximum,
        }
    } else {
        DamageRangeV1 {
            minimum: range.minimum,
            maximum: split,
        }
    };
    state
        .engine
        .combat
        .foundations_v1
        .as_mut()
        .expect("rules enabled")
        .pending
        .as_mut()
        .expect("pending damage")
        .range = Some(next);
    Ok(())
}
