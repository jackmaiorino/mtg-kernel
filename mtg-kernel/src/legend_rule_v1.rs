//! Resumable legend choices within one simultaneous state-based-action pass.

use crate::card_def::{self, AttachmentDef, CardType, Keywords, Supertype};
use crate::engine::{self, Decision};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, ObjectLinkV4, Zone};
use crate::trigger::{self, PendingTrigger};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const HAS_LEGENDARY_DEFINITIONS: bool = {
    let mut index = 0;
    let mut found = false;
    while index < card_def::CARD_DEFS.len() {
        let supertypes = card_def::CARD_DEFS[index].supertypes;
        let mut supertype = 0;
        while supertype < supertypes.len() {
            if matches!(supertypes[supertype], Supertype::Legendary) {
                found = true;
            }
            supertype += 1;
        }
        index += 1;
    }
    found
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LegendGroupV1 {
    controller: PlayerId,
    name: String,
    objects: Vec<ObjectLinkV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparedPassV1 {
    groups: Vec<LegendGroupV1>,
    graveyard: Vec<ObjectLinkV4>,
    losses: Vec<PlayerId>,
    ceasing: Vec<ObjectLinkV4>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingLegendRuleV1 {
    pass: PreparedPassV1,
    kept: Vec<ObjectLinkV4>,
    waiting_triggers: Vec<PendingTrigger>,
}

impl PendingLegendRuleV1 {
    pub(crate) fn waiting_triggers(&self) -> &[PendingTrigger] {
        &self.waiting_triggers
    }

    pub(crate) fn public_groups(
        &self,
    ) -> impl Iterator<Item = (PlayerId, Vec<ObjectId>, Option<ObjectId>)> + '_ {
        self.pass.groups.iter().enumerate().map(|(index, group)| {
            (
                group.controller,
                group.objects.iter().map(|binding| binding.object).collect(),
                self.kept.get(index).map(|binding| binding.object),
            )
        })
    }
}

fn link(state: &GameState, object: ObjectId) -> ObjectLinkV4 {
    ObjectLinkV4 {
        object,
        zone_change_count: state.objects.get(object).zone_change_count,
    }
}

fn groups(state: &GameState) -> Vec<LegendGroupV1> {
    let mut groups = Vec::new();
    if !HAS_LEGENDARY_DEFINITIONS {
        return groups;
    }
    for controller in [state.active_player, state.active_player.opponent()] {
        let mut names: BTreeMap<&str, Vec<ObjectLinkV4>> = BTreeMap::new();
        for (id, object) in state.objects.iter() {
            let definition = &card_def::CARD_DEFS[object.card_def as usize];
            if object.zone == Zone::Battlefield
                && object.controller == controller
                && definition.supertypes.contains(&Supertype::Legendary)
            {
                names
                    .entry(engine::effective_name(state, id))
                    .or_default()
                    .push(link(state, id));
            }
        }
        for (name, objects) in names {
            if objects.len() > 1 {
                groups.push(LegendGroupV1 {
                    controller,
                    name: name.to_owned(),
                    objects,
                });
            }
        }
    }
    groups
}

// Snapshot other applicable SBAs before any legend moves. In particular, a
// lethally damaged legend remains a legal keep choice and still dies in this
// pass. The existing fixed-point sweep handles passes without legend choices.
fn prepare(state: &GameState, waiting: &[PendingTrigger]) -> PreparedPassV1 {
    let mut graveyard = Vec::new();
    let mut ceasing = Vec::new();
    for (id, object) in state.objects.iter() {
        let definition = &card_def::CARD_DEFS[object.card_def as usize];
        if object.zone != Zone::Battlefield {
            if definition.is_token {
                ceasing.push(link(state, id));
            }
            continue;
        }
        let lethal = engine::object_has_type(state, id, CardType::Creature)
            && trigger::creature_dies_to_state_based_actions(
                engine::effective_toughness(state, id),
                i64::from(object.damage),
                object.v4.deathtouch_damage,
                engine::has_effective_keyword(state, id, Keywords::INDESTRUCTIBLE),
            );
        let invalid_aura = definition
            .attachment
            .is_some_and(AttachmentDef::is_creature_aura)
            && !object.v4.attached_to.is_some_and(|host_link| {
                state.objects.try_get(host_link.object).is_some_and(|host| {
                    host.zone == Zone::Battlefield
                        && host.zone_change_count == host_link.zone_change_count
                        && engine::object_has_type(state, host_link.object, CardType::Creature)
                        && host.attachments.contains(&id)
                })
            });
        let completed_saga = object.v4.face_index == 0
            && definition.saga.as_ref().is_some_and(|saga| {
                object.counters.lore >= saga.chapter_effects.len() as i16
                    && saga.chapter_effects.last().is_some_and(|effect| {
                        !trigger::saga_final_chapter_is_pending(
                            state,
                            id,
                            object.zone_change_count,
                            &effect(),
                            waiting,
                        )
                    })
            });
        if lethal
            || invalid_aura
            || completed_saga
            || crate::planeswalker_v1::zero_loyalty(state, id)
        {
            graveyard.push(link(state, id));
        }
    }
    PreparedPassV1 {
        groups: groups(state),
        graveyard,
        losses: [PlayerId::P0, PlayerId::P1]
            .into_iter()
            .filter(|player| {
                let ps = &state.players[player.index()];
                !ps.has_lost && (ps.life <= 0 || ps.drew_from_empty)
            })
            .collect(),
        ceasing,
    }
}

pub(crate) fn stage(state: &mut GameState, waiting: &[PendingTrigger]) -> bool {
    if state.pending_legend_rule_v1.is_some() {
        return true;
    }
    if groups(state).is_empty() {
        return false;
    }
    state.pending_legend_rule_v1 = Some(PendingLegendRuleV1 {
        pass: prepare(state, waiting),
        kept: Vec::new(),
        waiting_triggers: waiting.to_vec(),
    });
    true
}

fn validate(state: &GameState, pending: &PendingLegendRuleV1) -> Result<(), String> {
    if pending.pass.groups.is_empty()
        || pending.pass != prepare(state, &pending.waiting_triggers)
        || pending.kept.len() > pending.pass.groups.len()
        || pending
            .kept
            .iter()
            .zip(&pending.pass.groups)
            .any(|(kept, group)| !group.objects.contains(kept))
    {
        return Err(
            "legend-rule continuation does not match its exact state-based-action pass".into(),
        );
    }
    Ok(())
}

pub(crate) fn answer(state: &mut GameState, keep: ObjectId) -> Result<(), String> {
    let pending = state
        .pending_legend_rule_v1
        .as_ref()
        .ok_or("no legend-rule choice is pending")?;
    validate(state, pending)?;
    let group = pending
        .pass
        .groups
        .get(pending.kept.len())
        .ok_or("all legend-rule choices have already been answered")?;
    let kept = group
        .objects
        .iter()
        .find(|candidate| candidate.object == keep)
        .copied()
        .ok_or("chosen permanent is not in the current legend group")?;
    state
        .pending_legend_rule_v1
        .as_mut()
        .unwrap()
        .kept
        .push(kept);
    Ok(())
}

pub(crate) fn drain_or_decide(state: &mut GameState) -> Result<Option<Decision>, String> {
    let Some(pending) = state.pending_legend_rule_v1.as_ref() else {
        return Ok(None);
    };
    validate(state, pending)?;
    if let Some(group) = pending.pass.groups.get(pending.kept.len()) {
        return Ok(Some(Decision::ChooseLegendPermanent {
            player: group.controller,
            candidates: group.objects.iter().map(|binding| binding.object).collect(),
        }));
    }
    let pending = state.pending_legend_rule_v1.take().unwrap();
    let mut graveyard = pending.pass.graveyard;
    for (group, kept) in pending.pass.groups.iter().zip(pending.kept) {
        graveyard.extend(
            group
                .objects
                .iter()
                .copied()
                .filter(|object| *object != kept),
        );
    }
    graveyard.sort_by_key(|binding| binding.object);
    graveyard.dedup();
    event::propose_and_commit_batch(
        state,
        graveyard
            .into_iter()
            .map(|binding| ProposedEvent::zone_change(binding.object, Zone::Graveyard))
            .collect(),
    );
    for player in pending.pass.losses {
        state.players[player.index()].has_lost = true;
    }
    for binding in pending.pass.ceasing {
        event::cease_to_exist(state, binding.object);
    }
    let triggers = trigger::collect_and_process_with_waiting(state, pending.waiting_triggers);
    state.engine.pending_triggers.extend(triggers);
    Ok(None)
}
