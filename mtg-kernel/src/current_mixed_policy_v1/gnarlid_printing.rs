//! Exact optional kicker, entry replacement and continuously checked counter grant.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, EntersWithPlusOneCountersDef, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    name == "Gnarlid Colony"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles(d.name) {
        return Ok(None);
    }
    let cost = Cost {
        generic: 1,
        pips: &[Pip::Colored(ManaColor::G)],
        x_count: 0,
    };
    let kicker = Cost { generic: 2, ..cost };
    let total = Cost {
        generic: 3,
        pips: &[Pip::Colored(ManaColor::G); 2],
        x_count: 0,
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.kicker_cost != Some(kicker)
        || d.mana_value != 2
        || d.types != [CardType::Creature]
        || d.subtypes != [Subtype::Beast]
        || !d.supertypes.is_empty()
        || d.colors != [ManaColor::G]
        || d.power != Some(2)
        || d.toughness != Some(2)
        || d.keywords != Keywords::NONE
        || d.enters_with_plus_one_counters
            != Some(EntersWithPlusOneCountersDef {
                count: 2,
                if_kicked: true,
            })
        || d.controlled_counter_keyword != Some(Keywords::TRAMPLE)
        || crate::engine::static_controller_turn_self_keyword_for(d.name).is_some()
        || crate::engine::static_counter_keyword_controller_turn_only_for(d.name)
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !crate::trigger::triggers_for(id).is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(EffectOp::MoveObject {
                object: ObjectRef::ThisSource,
                to_zone: Zone::Battlefield,
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let roots = [false, true].into_iter().map(|kicked| json!({
        "method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],
        "optional_additional_cost":{"kind":"Kicker","selected":kicked,"repeatable":false,"cost":kicker},
        "ordered_costs":[{"operation":"PayPrintedManaAndChosenAdditionalKicker","printed":cost,
            "additional":if kicked { Some(kicker) } else { None },
            "base_total_before_increases_and_reductions":if kicked { total } else { cost }}],
        "printed_mana_value_is_unchanged":true,
        "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
        "capture_paid_kicker_on_original_spell":true,"countering_skips_entry_and_does_not_refund_cost":true,
        "countered_spell_destination":"OwnersGraveyard","normal_zone_replacements_and_entry_triggers_apply":true,
        "other_cast_permissions_require_current_candidate_authority":true
    })).collect::<Vec<_>>();
    Ok(Some(json!({"card_id":id,"gnarlid_printing":{
        "schema":"printed-kicker-entry-counter-keyword/v1","cost":cost,"mana_value":2,
        "types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,
        "power":2,"toughness":2,"keywords":0,"token":false,"ordered_cast_roots":roots,
        "battlefield_entry_replacement":{"operation":"EnterWithPlusOnePlusOneCounters","count":2,
            "condition":"OriginalEnteringSpellWasKicked","not_a_trigger":true,
            "noncast_entry_does_not_inherit_old_kicker":true,"normal_counter_replacements_apply":true},
        "ordered_static_roots":[{"operation":"GrantKeywordToCurrentControlledCreatureSet","keyword_bits":Keywords::TRAMPLE.0,
            "source_zone":"Battlefield","source_face":0,"requires_active_printed_abilities":true,
            "controller":"CurrentSourceController","recipient_type":"CurrentEffectiveCreature",
            "required_counter":{"kind":"PlusOnePlusOne","count":"StrictlyPositive"},
            "other_counter_kinds_do_not_qualify":true,"opponents_creatures_excluded":true,
            "includes_source_only_if_it_has_a_qualifying_counter":true,"applies_on_both_players_turns":true,
            "rechecks_current_control_type_and_counter_count":true,"includes_later_qualifying_creatures":true,
            "normal_layers_and_timestamped_ability_removal_apply":true,
            "ends_with_source_departure_or_inactive_printed_ability":true}]
    }})))
}
