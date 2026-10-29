//! Phasing (702.26) for Talon Gates of Madara: "up to one target creature
//! phases out."
//!
//! A phased-out permanent is treated as though it doesn't exist. It leaves
//! every zone list and takes the command-zone marker without a zone change,
//! so it keeps its incarnation (`zone_change_count`), counters, damage,
//! tapped state and summoning sickness; nothing sees it as a battlefield
//! permanent, and effects that remember it by incarnation apply again once
//! it returns. Permanents attached to it phase out with it (indirectly).
//! During its controller's next untap step, before permanents untap, it
//! phases in under that player's control (502.1). Phasing in or out is not
//! a zone change, so no enter or leave abilities trigger.

use super::{e5a_state, live_link, read_state};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{GameState, ObjectLinkV4, Zone};
use serde::{Deserialize, Serialize};

/// One phased-out permanent and the player who controlled it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhasedOutV1 {
    pub object: ObjectLinkV4,
    pub controller: PlayerId,
}

/// `object` and the permanents attached to it phase out.
pub(crate) fn phase_out(state: &mut GameState, object: ObjectId) {
    let mut objects = vec![object];
    objects.extend(
        state
            .objects
            .get(object)
            .attachments
            .iter()
            .copied()
            .filter(|&attached| state.objects.get(attached).zone == Zone::Battlefield),
    );
    for id in objects {
        phase_out_one(state, id);
    }
}

fn phase_out_one(state: &mut GameState, id: ObjectId) {
    let live = state.objects.get(id);
    if live.zone != Zone::Battlefield {
        return;
    }
    let controller = live.controller;
    let link = live_link(state, id);
    for seat in [PlayerId::P0, PlayerId::P1] {
        state.players[seat.index()]
            .battlefield
            .retain(|&object| object != id);
    }
    state.engine.combat.remove_from_combat(id);
    state.objects.get_mut(id).zone = Zone::Command;
    e5a_state(state).phased_out.push(PhasedOutV1 {
        object: link,
        controller,
    });
}

/// Whether `object` is phased out.
pub(crate) fn phased_out(state: &GameState, object: ObjectId) -> bool {
    read_state(state).is_some_and(|e5a| {
        e5a.phased_out.iter().any(|phased| {
            phased.object.object == object
                && state.objects.try_get(object).is_some_and(|live| {
                    live.zone == Zone::Command
                        && live.zone_change_count == phased.object.zone_change_count
                })
        })
    })
}

/// `player`'s untap step: their phased-out permanents phase in.
pub(crate) fn phase_in(state: &mut GameState, player: PlayerId) {
    let Some(eternal) = state.eternal_v1.as_mut() else {
        return;
    };
    if eternal.e5a.phased_out.is_empty() {
        return;
    }
    let (mine, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut eternal.e5a.phased_out)
        .into_iter()
        .partition(|phased| phased.controller == player);
    eternal.e5a.phased_out = rest;
    for phased in mine {
        let id = phased.object.object;
        let returning = state.objects.try_get(id).is_some_and(|live| {
            live.zone == Zone::Command && live.zone_change_count == phased.object.zone_change_count
        });
        if returning {
            let live = state.objects.get_mut(id);
            live.zone = Zone::Battlefield;
            live.controller = player;
            state.players[player.index()].battlefield.push(id);
        }
    }
}
