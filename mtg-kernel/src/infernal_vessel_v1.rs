//! Genuine Partial Infernal Vessel source, pinned to Mage a5c90fe.
//! Death eligibility is captured before the departing incarnation is reset.
use crate::{
    card_def::Subtype,
    effect::{EffectOp, ExecCtx},
    event::{self, ProposedEvent},
    state::{GameState, Zone},
    trigger::{TriggerCondition, TriggeredAbilityDef},
};

pub(crate) const TRIGGERS: [TriggeredAbilityDef; 1] = [TriggeredAbilityDef {
    condition: TriggerCondition::DiesUnlessSubtype(Subtype::Demon),
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: recipe,
}];

pub fn recipe() -> EffectOp {
    EffectOp::ReturnSourceAsDemonWithTwoCountersV1
}

pub(crate) fn resolve(state: &mut GameState, ctx: &ExecCtx) {
    let Some(source) = ctx.ability_source_contract else {
        return;
    };
    // A different incarnation in the graveyard is a different game object.
    // Tokens cannot return after leaving the battlefield (111.8).
    if source.source != ctx.source
        || source.zone != Zone::Battlefield
        || state.objects.try_get(source.source).is_none_or(|object| {
            object.zone != Zone::Graveyard
                || Some(object.zone_change_count) != source.zone_change_count.checked_add(1)
                || object.owner != source.owner
                || object.v4.is_token
        })
    {
        return;
    }
    let mut proposal = ProposedEvent::zone_change(source.source, Zone::Battlefield);
    if let ProposedEvent::ZoneChange(change) = &mut proposal {
        // This is the shared effect-entry counter input, despite its original
        // X-counter name. Counters exist before entry observers and use the
        // returning owner's counter replacements. X is not the card's cost.
        change.battlefield_x_v48 = Some(crate::doubling_season_v48::EffectXEntryV48 {
            original: 2,
            host: None,
        });
        // 611.2e makes this resolving effect's characteristic simultaneous
        // with entry. The entry driver installs it before static observers.
        change.battlefield_added_subtype_v1 = Some(Subtype::Demon);
    }
    event::propose_and_commit(state, proposal);
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
