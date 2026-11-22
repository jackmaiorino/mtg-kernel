//! Competing Nomads en-Kor replacement effects and their split damage.
use crate::eternal_legacy_e9a_v1::{read_state, statics::applicable_redirects, TurnV1};
use crate::eternal_pauper_pf1c_v1::ordinary_damage::{OrdinaryV1, Packet};
use crate::event::DamageProposed;
use crate::state::{GameState, Target};

pub(crate) fn has_active_redirects(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && read_state(state).is_some_and(|record| {
            let now = TurnV1::now(state);
            record.redirects.iter().any(|redirect| {
                redirect.turn == now
                    && [redirect.from, redirect.to].iter().all(|link| {
                        crate::eternal_legacy_e9a_v1::link_live_in(
                            state,
                            *link,
                            crate::state::Zone::Battlefield,
                        )
                    })
            })
        })
}

/// A lone point with one destination can use the ordinary path. Splits must
/// keep their surviving damage simultaneous, even when no decision is needed.
pub(crate) fn needs_frontier(state: &GameState, damage: &DamageProposed) -> bool {
    if !matches!(damage.target, Target::Object(_))
        || damage.amount <= 0
        || read_state(state).is_none_or(|records| records.redirects.is_empty())
    {
        return false;
    }
    let mut redirects = applicable_redirects(state, damage);
    let Some((_, first)) = redirects.next() else {
        return false;
    };
    if damage.historical_facts(state).is_err() {
        return false;
    }
    if redirects.any(|(_, record)| record != first || damage.amount > 1) {
        return true;
    }
    if first.from != first.to {
        if damage.amount > 1 {
            return true;
        }
        let mut next = damage.clone();
        next.amount = 1;
        next.target = Target::Object(first.to.object);
        if applicable_redirects(state, &next).next().is_some()
            || super::packet_options(state, &Packet::new(&next)).len() > 1
        {
            // A forced redirect can expose the next affected controller's
            // independent cost choice (for example protection vs Nishoba).
            return true;
        }
    }
    Packet::new(damage)
        .options(state)
        .iter()
        .any(|effect| !matches!(effect, OrdinaryV1::Redirect { .. }))
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
