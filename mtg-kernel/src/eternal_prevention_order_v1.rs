//! Complete-prevention choices without Hallow. A global fog/color shield and
//! a Circle can both prevent the packet, but only the Circle is consumed.
//! Damage that can survive its replacements keeps the existing engine path.
#![cfg_attr(not(feature = "eternal-fixtures"), allow(dead_code))]

use crate::eternal_pauper_pf1c_v1::{
    damage_source_incarnation, ordinary_damage::OrdinaryV1, HallowChoiceV1, HallowPreventionV1,
};
use crate::eternal_sideboard_s1d_v1::statics as circles;
use crate::event::{self, DamageProposed, ProposedEvent, ReplacementEffectKind};
use crate::state::{GameState, Target};

fn has_color_prevention(state: &GameState) -> bool {
    state.engine.active_replacements.iter().any(|replacement| {
        matches!(
            &replacement.kind,
            ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { turn, active_player, .. }
                if *turn == state.turn_key_v1() && *active_player == state.active_player
        )
    })
}

/// This rare state, not every ordinary spell or activation, borrows operation
/// replay. No matching Circle/global prevention leaves the old path intact.
pub(crate) fn may_need_choice(state: &GameState) -> bool {
    cfg!(feature = "eternal-fixtures")
        && !cannot_prevent(state)
        && circles::has_live_circle_shield(state)
        && (crate::eternal_pauper_e6a_v1::has_active_global_fog(state)
            || has_color_prevention(state))
}

fn full_prevention(state: &GameState, damage: &DamageProposed) -> bool {
    if crate::eternal_pauper_e6a_v1::global_prevents_damage(state, damage) {
        return true;
    }
    let event = ProposedEvent::Damage(damage.clone());
    state.engine.active_replacements.iter().any(|replacement| {
        matches!(
            replacement.kind,
            ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn { .. }
        ) && !damage.touched_by.contains(&replacement.id)
            && event::replacement_applies(replacement, &event, state)
    })
}

fn cannot_prevent(state: &GameState) -> bool {
    state.engine.until_end_of_turn.iter().any(|effect| {
        matches!(
            effect,
            crate::engine::UntilEndOfTurnEffect::DamageCannotBePrevented { .. }
        )
    })
}

fn eligible(state: &GameState, damage: &DamageProposed) -> bool {
    damage.amount > 0
        && !cannot_prevent(state)
        && full_prevention(state, damage)
        && circles::circle_shield_position(state, damage).is_some()
}

fn choose_prevention(state: &mut GameState, damage: &DamageProposed) -> Result<bool, String> {
    let Target::Player(chooser) = damage.target else {
        return Err("Circle prevention lost its affected player".into());
    };
    let mut matching = Vec::new();
    for (position, shield) in circles::matching_circle_shields(state, damage) {
        // Removing either identical record has the same result and future
        // coverage; keep the earliest representative and its stable index.
        if !matching.iter().any(|(_, existing)| *existing == shield) {
            matching.push((position, shield));
        }
    }
    if matching.is_empty() {
        // An earlier packet in this same batch may have spent the last shield.
        // The global prevention still prevents this entire event.
        return Ok(true);
    }
    let (source, source_zone) = damage_source_incarnation(state, damage)
        .ok_or("Circle prevention lost its authenticated damage source")?;
    let mut ordinary: Vec<_> = matching
        .into_iter()
        .map(|(position, shield)| OrdinaryV1::Circle { position, shield })
        .collect();
    ordinary.push(OrdinaryV1::PreventAll);
    if ordinary.len() > usize::from(u16::MAX) {
        return Err("complete-prevention options exceed the native decision width".into());
    }
    // Reuse the existing private checkpoint. Empty beneficiaries mean no
    // Hallow gain is possible; the public menu is the existing option action.
    let choice = HallowChoiceV1 {
        cohort: None,
        damage: damage.clone(),
        source,
        source_zone,
        chooser,
        beneficiaries: Vec::new(),
        circle: None,
        ordinary,
    };
    let Some(selected) = crate::linked_aura_operation_v1::choose_hallow(state, choice.clone())?
    else {
        return Ok(false);
    };
    let HallowPreventionV1::Ordinary(index) = selected else {
        return Err("complete prevention changed its captured answer kind".into());
    };
    match choice.ordinary[usize::from(index)].clone() {
        OrdinaryV1::Circle { position, shield } => {
            if !circles::consume_selected_circle_shield(state, damage, position, shield) {
                return Err("selected Circle shield changed before prevention".into());
            }
        }
        OrdinaryV1::PreventAll => {}
        _ => return Err("complete prevention changed its captured operation".into()),
    }
    Ok(true)
}

/// Called before fixed-order reductions/preventions, after ordinary source
/// validation. Both represented outcomes prevent all damage, so nonconsuming
/// amount modifiers cannot introduce another observable outcome in this slice.
pub(crate) fn prevents_damage(state: &mut GameState, damage: &DamageProposed) -> bool {
    if !may_need_choice(state) || !eligible(state, damage) {
        return false;
    }
    if choose_prevention(state, damage).is_err() {
        state.engine.halted = Some((
            crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
            damage.source,
        ));
    }
    true
}

/// Resolve simultaneous affected-player choices in APNAP order and preserve
/// original order within each seat. Actual shield mutations precede the next
/// choice; the enclosing replay owns every suspended prefix.
pub(crate) fn prepare_batch(
    state: &mut GameState,
    events: &mut Vec<ProposedEvent>,
) -> Result<bool, String> {
    if !may_need_choice(state) {
        return Ok(false);
    }
    let mut work: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(at, event)| match event {
            ProposedEvent::Damage(damage) if eligible(state, damage) => Some((at, damage.clone())),
            _ => None,
        })
        .collect();
    work.sort_by_key(|(at, damage)| (damage.target != Target::Player(state.active_player), *at));
    let mut covered = vec![false; events.len()];
    for (at, damage) in work {
        if !event::preflight_hallow_damage_v1(state, &ProposedEvent::Damage(damage.clone())) {
            return Err("complete prevention rejected its original damage source".into());
        }
        if !choose_prevention(state, &damage)? {
            return Ok(true);
        }
        covered[at] = true;
    }
    let mut at = 0;
    events.retain(|_| {
        let keep = !covered[at];
        at += 1;
        keep
    });
    Ok(false)
}
