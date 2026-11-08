//! Complete physical-cast custody for the opt-in Storm consumer.
use super::{append, JournalV1};
use crate::{
    card_def::{self, CardType},
    cast_identity_v1::{self, PublicIdentityV1, ReceiptV1},
    event::CommittedEvent as E,
    ids::StackItemId,
    state::{CastMethodV4, GameState, SpellCastOriginV4, StackItem, StackItemKind, Zone},
};
use serde::{Deserialize, Serialize};

/// The actual finalized form, recorded even for non-instants and non-sorceries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastFormV1 {
    pub publication: ReceiptV1,
    pub origin: SpellCastOriginV4,
}

pub(super) fn instant_or_sorcery(card_id: u16, method: CastMethodV4) -> Result<bool, String> {
    let definition = card_def::CARD_DEFS.get(usize::from(card_id))
        .ok_or("Storm spell definition absent")?;
    let types = if method == CastMethodV4::Omen {
        definition.omen_spell_form().ok_or("Storm alternative form absent")?.1
    } else {
        definition.types
    };
    Ok(types.contains(&CardType::Instant) || types.contains(&CardType::Sorcery))
}

fn eligibility(form: &CastFormV1) -> Result<bool, String> {
    let method = form.origin.finalized_method.ok_or("Storm actual cast form is not finalized")?;
    match form.publication.public_identity {
        PublicIdentityV1::Known { card_def } => instant_or_sorcery(card_def, method),
        PublicIdentityV1::Concealed { .. } => Ok(false),
    }
}

fn public_cast(state: &GameState, event: usize) -> Result<ReceiptV1, String> {
    let Some(E::PublishedCastIdentityV1 { receipt }) = event.checked_sub(1)
        .and_then(|at| state.engine.event_history.get(at))
    else {
        return Err("Storm original native published identity absent".into());
    };
    if usize::try_from(receipt.spell_cast_history_index).ok() != Some(event)
        || receipt.stack_item_id == StackItemId::default()
        || receipt.origin_zone == Zone::Stack
        || receipt.origin_zone_change_count.checked_add(1) != Some(receipt.stack_zone_change_count)
        || state.engine.event_history.get(event)
            != Some(&E::SpellCast { spell: receipt.spell, controller: receipt.controller })
    {
        return Err("Storm actual SpellCast and published identity disagree".into());
    }
    Ok(*receipt)
}

fn validate_origin(form: &CastFormV1) -> Result<(), String> {
    if form.origin.origin_zone != form.publication.origin_zone
        || form.origin.origin_zone_change_count != form.publication.origin_zone_change_count
        || form.origin.finalized_method.is_none()
    {
        return Err("Storm finalized cast form differs from native publication".into());
    }
    Ok(())
}

fn live_identity(live: &crate::state::GameObject) -> PublicIdentityV1 {
    live.v4.face_down_v1.map_or(
        PublicIdentityV1::Known { card_def: live.card_def },
        |face| PublicIdentityV1::Concealed { disguised: face.disguised },
    )
}

fn matches_snapshot(form: &CastFormV1, spell: &StackItem) -> bool {
    let Some(source) = spell.v4.source_contract else { return false };
    let publication = form.publication;
    spell.kind == StackItemKind::Spell && !spell.is_copy && source.spell_copy_origin.is_none()
        && spell.source == publication.spell && source.source == publication.spell
        && spell.controller == publication.controller && source.controller == publication.controller
        && source.owner == publication.owner && source.zone == Zone::Stack
        && source.zone_change_count == publication.stack_zone_change_count
        && spell.v4.stack_item_id == publication.stack_item_id
        && source.spell_cast_origin == Some(form.origin)
        && Some(source.cast_method) == form.origin.finalized_method
        && spell.v4.cast_method == form.origin.finalized_method
        && publication.public_identity == (PublicIdentityV1::Known { card_def: source.card_def })
}

pub(super) fn capture_form(state: &GameState, event: usize, spell: &StackItem) -> Result<(CastFormV1, bool), String> {
    let source = spell.v4.source_contract.ok_or("Storm finalized cast source absent")?;
    let origin = source.spell_cast_origin.ok_or("Storm finalized cast origin absent")?;
    let publication = public_cast(state, event)?;
    let form = CastFormV1 { publication, origin };
    validate_origin(&form)?;
    let live = state.objects.get(spell.source);
    if spell.kind != StackItemKind::Spell || spell.is_copy || source.spell_copy_origin.is_some()
        || source.zone != Zone::Stack || source.source != publication.spell
        || spell.source != publication.spell || source.owner != publication.owner
        || source.controller != publication.controller || spell.controller != publication.controller
        || source.zone_change_count != publication.stack_zone_change_count
        || source.card_def != live.card_def || live.owner != source.owner
        || live.zone != Zone::Stack || live.controller != source.controller
        || live.zone_change_count != source.zone_change_count
        || source.spell_cast_origin != live.v4.spell_cast_origin
        || Some(source.cast_method) != form.origin.finalized_method
        || spell.v4.cast_method != form.origin.finalized_method
        || spell.v4.stack_item_id != publication.stack_item_id
        || live_identity(live) != publication.public_identity
    {
        return Err("Storm native finalized cast form changed at publication".into());
    }
    Ok((form, eligibility(&form)?))
}

pub(super) fn record_form(state: &mut GameState, form: CastFormV1) {
    append(state, JournalV1::CastForm { form }, false);
}

fn cast_form_at(state: &GameState, event: usize) -> Result<(usize, Option<&StackItem>), String> {
    let publication = public_cast(state, event)?;
    let history = &state.engine.event_history;
    let following = event.checked_add(1).ok_or("Storm native cast index width")?;
    let (form_at, snapshot) = match history.get(following) {
        Some(E::ThousandYearStormV1 { journal: JournalV1::Cast { event, spell, .. } })
            if *event == publication.spell_cast_history_index =>
            (following.checked_add(1).ok_or("Storm cast form index width")?, Some(spell.as_ref())),
        _ => (following, None),
    };
    let Some(E::ThousandYearStormV1 { journal: JournalV1::CastForm { form } }) = history.get(form_at)
    else {
        return Err("Storm native cast has no adjacent finalized form".into());
    };
    if form.publication != publication {
        return Err("Storm finalized cast form differs from native publication".into());
    }
    validate_origin(form)?;
    if eligibility(form)? != snapshot.is_some() {
        return Err("Storm eligible cast roster differs from native finalized form".into());
    }
    if snapshot.is_some_and(|spell| !matches_snapshot(form, spell)) {
        return Err("Storm original cast snapshot differs from native publication".into());
    }
    let live = state.objects.try_get(publication.spell).ok_or("Storm native cast object absent")?;
    if live.owner != publication.owner || live.zone_change_count < publication.stack_zone_change_count
        || live.zone_change_count == publication.stack_zone_change_count
            && (live.zone != Zone::Stack || live.controller != publication.controller
                || live.v4.spell_cast_origin != Some(form.origin)
                || live_identity(live) != publication.public_identity)
    {
        return Err("Storm finalized cast form lost original incarnation".into());
    }
    Ok((form_at, snapshot))
}

pub(super) fn validate_snapshot(state: &GameState, event: u64, spell: &StackItem) -> Result<(), String> {
    let event = usize::try_from(event).map_err(|_| "Storm native cast index width")?;
    let (_, snapshot) = cast_form_at(state, event)?;
    if snapshot != Some(spell) {
        return Err("Storm original cast snapshot differs from native publication".into());
    }
    Ok(())
}

pub(super) fn validate_roster(state: &GameState) -> Result<(), String> {
    if !state.thousand_year_storm_consumer_v1 {
        return Ok(());
    }
    cast_identity_v1::unsupported_public_history(state, |_| false)?;
    for (index, row) in state.engine.event_history.iter().enumerate() {
        match row {
            E::SpellCast { .. } => { cast_form_at(state, index)?; }
            E::ThousandYearStormV1 { journal: JournalV1::Cast { event, spell, .. } } => {
                if event.checked_add(1) != u64::try_from(index).ok() {
                    return Err("Storm cast roster contains an orphaned snapshot".into());
                }
                validate_snapshot(state, *event, spell)?;
            }
            E::ThousandYearStormV1 { journal: JournalV1::CastForm { form } } => {
                let event = usize::try_from(form.publication.spell_cast_history_index)
                    .map_err(|_| "Storm native cast index width")?;
                if cast_form_at(state, event)?.0 != index {
                    return Err("Storm cast roster contains a duplicate or orphaned form".into());
                }
            }
            _ => {}
        }
    }
    Ok(())
}




#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::{
        effect::{thousand_year_storm_v1 as storm, thousand_year_storm_v1_tests as native},
        event::{self, ProposedEvent},
        ids::{ObjectId, PlayerId},
        state::Target,
    };

    fn paid_before_observer(actor: PlayerId) -> (GameState, ObjectId) {
        let mut state = native::ready(actor, "Forest");
        state.thousand_year_storm_consumer_v1 = true;
        let bolt = native::cast(
            &mut state, actor, "Lightning Bolt", &[Target::Player(actor.opponent())], 1,
        );
        native::finish(&mut state);
        (state, bolt)
    }

    fn restored(state: &GameState) -> GameState {
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap()
    }

    #[test]
    fn restored_historical_snapshot_cannot_relabel_a_native_public_cast() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, bolt) = paid_before_observer(actor);
            event::propose_and_commit(&mut state, ProposedEvent::zone_change(bolt, Zone::Library));
            let mut state = restored(&state);
            storm::validate_state(&state).unwrap();

            let (at, snapshot) = state.engine.event_history.iter_mut().enumerate()
                .find_map(|(at, event)| match event {
                    E::ThousandYearStormV1 { journal: JournalV1::Cast { spell, .. } }
                        if spell.source == bolt => Some((at, spell)),
                    _ => None,
                }).unwrap();
            snapshot.v4.source_contract.as_mut().unwrap().card_def =
                card_def::card_id_by_name("Burst Lightning").unwrap();
            let error = storm::cast_at(&state, at as u64).unwrap_err();
            assert_eq!(error, "Storm original cast snapshot differs from native publication");
            assert!(storm::validate_state(&state).is_err());
        }
    }

    #[test]
    fn restored_roster_cannot_omit_a_paid_cast_before_first_storm_observer() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (mut state, bolt) = paid_before_observer(actor);
            let enchantment = native::paid_storm(&mut state, actor);
            let mut state = restored(&state);
            storm::validate_state(&state).unwrap();
            let (at, native_at) = state.engine.event_history.iter().enumerate()
                .find_map(|(at, row)| match row {
                    E::ThousandYearStormV1 { journal: JournalV1::Cast { event, spell, observers } }
                        if spell.source == bolt => {
                            assert!(observers.is_empty());
                            Some((at, *event))
                        }
                    _ => None,
                }).unwrap();
            assert!(matches!(state.engine.event_history.get(native_at as usize),
                Some(E::SpellCast { spell, controller }) if *spell == bolt && *controller == actor));
            assert!(state.engine.event_history.iter().any(|event| matches!(event,
                E::ThousandYearStormV1 { journal: JournalV1::CastForm { form } }
                    if form.publication.spell == enchantment)));

            // Keep all native publication indices fixed, including the
            // enchantment's later physical cast. Only erase the old Storm
            // snapshot and its zero-observer collector mark.
            for (index, row) in state.engine.event_history.iter_mut().enumerate() {
                if index == at || matches!(row,
                    E::ThousandYearStormV1 { journal: JournalV1::Collected { cast } }
                        if *cast == at as u64)
                {
                    *row = E::BeginningPrecombatMainV1 { active_player: actor };
                }
            }
            cast_identity_v1::unsupported_public_history(&state, |_| false).unwrap();
            assert_eq!(storm::validate_state(&state).unwrap_err(),
                "Storm native cast has no adjacent finalized form");
        }
    }
}
