//! What each attacking creature attacks (506.3, 508.1b).
//!
//! When the defending player controls no planeswalker, every attacker
//! attacks that player and this extension stays absent, so earlier state
//! hashes, snapshots and decision streams are unchanged. When they control
//! one or more planeswalkers, declaring attackers asks the attacking player,
//! one attacker at a time in declaration order, whether it attacks the
//! player or one of those planeswalkers. The declaration (tapping, attack
//! triggers, priority) completes only after the last answer.
//!
//! An attacker records the exact planeswalker incarnation it attacks. If that
//! permanent leaves the battlefield the creature keeps attacking but an
//! unblocked attacker deals no combat damage (506.4c), and a trampler's
//! excess has nowhere to go (702.19e).

use crate::card_def::CardType;
use crate::engine::{self, Decision};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, ObjectLinkV4, Target, Zone};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackTargetsV1 {
    /// Attackers attacking a planeswalker, with the planeswalker's exact
    /// incarnation. Attackers not listed attack the defending player.
    planeswalker_attacks: Vec<(ObjectId, ObjectLinkV4)>,
    /// A declaration whose attack targets are still being chosen.
    pending: Option<PendingAttackDeclarationV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingAttackDeclarationV1 {
    attackers: Vec<ObjectId>,
    chosen: Vec<Target>,
}

/// Planeswalkers `defender` controls on the battlefield, in battlefield order.
pub(crate) fn defending_planeswalkers(state: &GameState, defender: PlayerId) -> Vec<ObjectId> {
    state.players[defender.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| {
            let live = state.objects.get(object);
            live.zone == Zone::Battlefield
                && live.controller == defender
                && engine::object_has_type(state, object, CardType::Planeswalker)
        })
        .collect()
}

pub(crate) fn has_pending(state: &GameState) -> bool {
    state
        .engine
        .combat
        .attack_targets_v1
        .as_ref()
        .is_some_and(|targets| targets.pending.is_some())
}

/// Starts choosing attack targets for an already validated declaration.
/// Returns false, leaving state untouched, when every attacker must attack
/// the defending player.
pub(crate) fn begin(state: &mut GameState, attackers: &[ObjectId]) -> bool {
    if attackers.is_empty()
        || defending_planeswalkers(state, state.active_player.opponent()).is_empty()
    {
        return false;
    }
    state.engine.combat.attack_targets_v1 = Some(AttackTargetsV1 {
        planeswalker_attacks: Vec::new(),
        pending: Some(PendingAttackDeclarationV1 {
            attackers: attackers.to_vec(),
            chosen: Vec::new(),
        }),
    });
    true
}

fn candidates(state: &GameState) -> Vec<Target> {
    let defender = state.active_player.opponent();
    std::iter::once(Target::Player(defender))
        .chain(
            defending_planeswalkers(state, defender)
                .into_iter()
                .map(Target::Object),
        )
        .collect()
}

fn pending(state: &GameState) -> Result<&PendingAttackDeclarationV1, String> {
    let pending = state
        .engine
        .combat
        .attack_targets_v1
        .as_ref()
        .and_then(|targets| targets.pending.as_ref())
        .ok_or("no attack target choice is pending")?;
    if state.step != crate::state::Step::DeclareAttackers
        || state.engine.combat.attackers_declared
        || pending.chosen.len() >= pending.attackers.len()
    {
        return Err("invalid attack target continuation".to_string());
    }
    engine::validate_declare_attackers(state, &pending.attackers)?;
    Ok(pending)
}

pub(crate) fn decision(state: &GameState) -> Option<Decision> {
    let pending = pending(state).ok()?;
    Some(Decision::ChooseAttackTarget {
        player: state.active_player,
        attacker: pending.attackers[pending.chosen.len()],
        candidates: candidates(state),
    })
}

pub(crate) fn answer(state: &mut GameState, target: Target) -> Result<(), String> {
    pending(state)?;
    if !candidates(state).contains(&target) {
        return Err("illegal attack target".to_string());
    }
    let targets = state
        .engine
        .combat
        .attack_targets_v1
        .as_mut()
        .expect("pending attack targets were just validated");
    let pending = targets.pending.as_mut().expect("validated");
    pending.chosen.push(target);
    if pending.chosen.len() < pending.attackers.len() {
        return Ok(());
    }
    let pending = targets.pending.take().expect("validated");
    for (&attacker, &target) in pending.attackers.iter().zip(&pending.chosen) {
        if let Target::Object(planeswalker) = target {
            targets.planeswalker_attacks.push((
                attacker,
                ObjectLinkV4 {
                    object: planeswalker,
                    zone_change_count: state.objects.get(planeswalker).zone_change_count,
                },
            ));
        }
    }
    if targets.planeswalker_attacks.is_empty() {
        state.engine.combat.attack_targets_v1 = None;
    }
    engine::finish_declare_attackers(state, pending.attackers);
    Ok(())
}

/// The planeswalker incarnation `attacker` attacks, if it attacks one.
pub fn attacked_planeswalker(state: &GameState, attacker: ObjectId) -> Option<ObjectLinkV4> {
    state
        .engine
        .combat
        .attack_targets_v1
        .as_ref()?
        .planeswalker_attacks
        .iter()
        .find(|(id, _)| *id == attacker)
        .map(|(_, planeswalker)| *planeswalker)
}

/// Where `attacker`'s unblocked (or trample excess) combat damage goes:
/// the defending player, the attacked planeswalker while that exact
/// incarnation remains on the battlefield, or nowhere once it has left.
pub(crate) fn damage_recipient(
    state: &GameState,
    attacker: ObjectId,
) -> Option<(Target, Option<ObjectLinkV4>)> {
    match attacked_planeswalker(state, attacker) {
        None => Some((
            Target::Player(state.objects.get(attacker).controller.opponent()),
            None,
        )),
        Some(planeswalker) => state
            .objects
            .try_get(planeswalker.object)
            .is_some_and(|live| {
                live.zone == Zone::Battlefield
                    && live.zone_change_count == planeswalker.zone_change_count
            })
            .then_some((Target::Object(planeswalker.object), Some(planeswalker))),
    }
}

/// 506.4: an attacker removed from combat no longer attacks anything.
pub(crate) fn remove_from_combat(targets: &mut Option<AttackTargetsV1>, id: ObjectId) {
    if let Some(state) = targets {
        state.planeswalker_attacks.retain(|(attacker, _)| *attacker != id);
        if state.planeswalker_attacks.is_empty() && state.pending.is_none() {
            *targets = None;
        }
    }
}
