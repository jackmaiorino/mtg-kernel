//! Definition-scoped native Tinybones prerequisites. No generic exile permission.
use super::*;

/// A lexical mana-matching grant. It is neither serialized nor constructible by
/// a public caller and does not authorize playing or casting its bound card.
#[derive(Debug, Clone)]
pub(crate) struct ManaGrantV72 {
    _source: AbilitySourceContractV4,
    _card: EffectObjectBinding,
    _controller: PlayerId,
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 490
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
            d.name == "Tinybones, Bauble Burglar"
                && d.is_executable()
                && d.types == [CardType::Creature]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.cost.generic == 1
                && d.cost.pips == [mana::Pip::Colored(ManaColor::B)]
                && d.cost.x_count == 0
                && (d.spell_effect)()
                    == Some(EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    })
        })
}
fn active_source(state: &GameState, player: PlayerId) -> Option<AbilitySourceContractV4> {
    state.players[player.index()]
        .battlefield
        .iter()
        .find_map(|&id| {
            let o = state.objects.try_get(id)?;
            (o.zone == Zone::Battlefield
                && o.controller == player
                && o.v4.face_index == 0
                && o.v4.face_down_v1.is_none()
                && definition(o.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
            .then(|| AbilitySourceContractV4::capture(state, id))
        })
}

/// Mana-as-though is distinct from the own-turn permission to play a card.
/// An independently authorized foreign stashed spell may use the current
/// controller's actual Tinybones on another turn as well. The stash issuer is
/// deliberately absent: any current printed source grants the static ability.
pub(super) fn mana_grant(
    state: &GameState,
    card: ObjectId,
    controller: PlayerId,
) -> Option<ManaGrantV72> {
    let o = state.objects.try_get(card)?;
    if o.owner == controller || o.v4.is_token || o.spell_copy_origin.is_some() {
        return None;
    }
    let stash = match o.zone {
        Zone::Exile if state.exile.contains(&card) => o.counters.stash,
        Zone::Stack if o.controller == controller => {
            let origin = o.v4.spell_cast_origin?;
            if origin.origin_zone != Zone::Exile
                || origin.origin_zone_change_count.checked_add(1) != Some(o.zone_change_count)
            {
                return None;
            }
            state
                .counter_lki_for(card, origin.origin_zone_change_count)?
                .stash
        }
        _ => return None,
    };
    if stash <= 0 {
        return None;
    }
    Some(ManaGrantV72 {
        _source: active_source(state, controller)?,
        _card: EffectObjectBinding {
            object: card,
            expected_zone: o.zone,
            expected_zone_change_count: o.zone_change_count,
        },
        _controller: controller,
    })
}
