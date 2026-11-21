//! Competing Nomads en-Kor replacement effects and their split damage.
use crate::eternal_legacy_e9a_v1::{read_state, statics::applicable_redirects, TurnV1};
use crate::eternal_pauper_pf1c_v1::ordinary_damage::{OrdinaryV1, Packet};
use crate::event::DamageProposed;
use crate::state::{GameState, Target};

pub(crate) fn has_active_redirects(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && read_state(state).is_some_and(|record| {
            let now = TurnV1::now(state);
            record.redirects.iter().any(|redirect| redirect.turn == now)
        })
}

/// A lone redirect with an ordinary recipient keeps its existing event shape.
/// A competing effect or another redirect on the next recipient needs the entire frontier.
pub(crate) fn needs_frontier(state: &GameState, damage: &DamageProposed) -> bool {
    let mut redirects = applicable_redirects(state, damage);
    let Some((_, first)) = redirects.next() else {
        return false;
    };
    if redirects.next().is_some() {
        return true;
    }
    if first.from != first.to {
        let mut next = damage.clone();
        next.amount = 1;
        next.target = Target::Object(first.to.object);
        if applicable_redirects(state, &next).next().is_some() {
            return true;
        }
    }
    damage.historical_facts(state).is_ok() && Packet::new(damage).options(state).len() > 1
}

pub(super) fn normalize(state: &GameState, packet: &Packet, options: &mut Vec<OrdinaryV1>) {
    if !options
        .iter()
        .any(|option| matches!(option, OrdinaryV1::Redirect { .. }))
    {
        return;
    }
    let mut redirects = Vec::new();
    let original_count = options.len();
    options.retain(|option| match option {
        OrdinaryV1::Redirect { record, .. } if redirects.contains(record) => false,
        OrdinaryV1::Redirect { record, .. } => {
            redirects.push(*record);
            true
        }
        _ => true,
    });
    // If every charge must be spent and each recipient simply takes its point,
    // the order changes neither the damage nor any remaining shield.
    if options.len() > 1
        && options.len() == redirects.len()
        && usize::try_from(packet.damage.amount).is_ok_and(|amount| amount >= original_count)
        && redirects.iter().all(|redirect| {
            if redirect.from == redirect.to {
                return false;
            }
            let mut next = packet.damage.clone();
            next.amount = 1;
            next.target = Target::Object(redirect.to.object);
            Packet::new(&next).options(state).is_empty()
        })
    {
        options.truncate(1);
    }
}
