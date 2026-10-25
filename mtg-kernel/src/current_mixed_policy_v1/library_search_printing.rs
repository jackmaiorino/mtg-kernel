//! Exact library-search printings. Private resolution selections are not targets.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, LibraryCardFilter, LibrarySearchDestinationV1, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::{TriggerCondition, TriggeredAbilityDef},
};
use serde_json::{json, Value};
pub(super) fn handles_permanent(name: &str) -> bool {
    matches!(name, "Campus Guide" | "Rune-Scarred Demon" | "Micromancer")
}
pub(super) fn handles_spell(name: &str) -> bool {
    name == "Grim Tutor"
}
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    if !handles_permanent(d.name) && !handles_spell(d.name) {
        return Ok(None);
    }
    let campus = d.name == "Campus Guide";
    let micromancer = d.name == "Micromancer";
    let spell = handles_spell(d.name);
    let cost = Cost {
        generic: if campus {
            2
        } else if spell {
            1
        } else if micromancer {
            3
        } else {
            5
        },
        pips: if campus {
            &[]
        } else if micromancer {
            &[Pip::Colored(ManaColor::U)]
        } else {
            &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)]
        },
        x_count: 0,
    };
    let types = if campus {
        &[CardType::Artifact, CardType::Creature][..]
    } else if spell {
        &[CardType::Sorcery][..]
    } else {
        &[CardType::Creature][..]
    };
    let subtypes = if campus {
        &[Subtype::Golem][..]
    } else if spell {
        &[][..]
    } else if micromancer {
        &[Subtype::Human, Subtype::Wizard][..]
    } else {
        &[Subtype::Demon][..]
    };
    let power = if campus {
        Some(2)
    } else if spell {
        None
    } else if micromancer {
        Some(3)
    } else {
        Some(6)
    };
    let toughness = if campus {
        Some(1)
    } else if spell {
        None
    } else if micromancer {
        Some(3)
    } else {
        Some(6)
    };
    let keywords = if !campus && !spell && !micromancer {
        Keywords::FLYING
    } else {
        Keywords::NONE
    };
    let search = if campus {
        EffectOp::SearchLibraryCardsToDestination {
            player: PlayerRef::Controller,
            filter: LibraryCardFilter::BasicLand,
            max_targets: 1,
            destination: LibrarySearchDestinationV1::LibraryTopAfterShuffle,
        }
    } else {
        EffectOp::SearchLibraryToHand {
            player: PlayerRef::Controller,
            filter: if micromancer {
                LibraryCardFilter::InstantOrSorceryManaValue(1)
            } else {
                LibraryCardFilter::AnyCard
            },
        }
    };
    let program = if campus {
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![EffectOp::Sequence(vec![]), search],
        }
    } else if micromancer {
        EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: vec![search, EffectOp::Sequence(vec![])],
        }
    } else if spell {
        EffectOp::Sequence(vec![
            search,
            EffectOp::LoseLife {
                player: PlayerRef::Controller,
                amount: 3,
            },
        ])
    } else {
        search
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(cost.generic) + cost.pips.len()
        || d.types != types
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.colors
            != if campus {
                &[][..]
            } else if micromancer {
                &[ManaColor::U][..]
            } else {
                &[ManaColor::B][..]
            }
        || d.power != power
        || d.toughness != toughness
        || d.keywords != keywords
        || d.target_spec != TargetSpec::None
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || !d.activated_abilities.is_empty()
        || !d.produces_mana.is_empty()
        || !d.mana_ability_choices.is_empty()
        || (d.mana_ability)().is_some()
        || (d.spell_effect)()
            != Some(if spell {
                program.clone()
            } else {
                EffectOp::MoveObject {
                    object: ObjectRef::ThisSource,
                    to_zone: Zone::Battlefield,
                }
            })
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    if spell {
        if !crate::trigger::triggers_for(id).is_empty() {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    } else {
        let [trigger] = crate::trigger::triggers_for(id) else {
            return Err(Error::UnsupportedPrintedRoot(id));
        };
        let TriggeredAbilityDef {
            condition,
            home_zone,
            face_index,
            intervening_if_kicked,
            intervening_if_controls_another_source_card,
            effect,
        } = trigger;
        if *condition != TriggerCondition::Etb
            || *home_zone != Zone::Battlefield
            || *face_index != 0
            || *intervening_if_kicked
            || *intervening_if_controls_another_source_card
            || effect() != program
            || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
            || crate::trigger::unselected_trigger_modes(id, &program).is_some()
        {
            return Err(Error::UnsupportedPrintedRoot(id));
        }
    }
    let select = json!({"operation":"SearchOwnLibrary","player":"CapturedResolutionController","selection_is_not_targeting":true,
        "chooser":"CapturedResolutionController","choice_visibility":"ChooserOnly","scope":"EntireCurrentOwnLibrary",
        "filter":if campus{json!("BasicLandCard")}else if micromancer{json!({"any_front_face_type":["Instant","Sorcery"],"printed_mana_value_equals":1})}else{json!("AnyCard")},"maximum":1,
        "minimum":if campus || micromancer{"ZeroEvenWhenMatchesExist"}else{"OneIfLibraryNonemptyOtherwiseZero"},
        "binds_exact_live_selected_incarnation":true,"may_not_select_another_players_library":true,
        "selected_card_revealed":campus || micromancer,"normal_zone_change_replacements_apply":true});
    let resolution = if campus {
        json!([{"operation":"ChooseWhetherToSearch","chooser":"CapturedResolutionController","at":"TriggerResolution","options":[
            {"index":0,"ordered_instructions":[],"searches":false,"shuffles":false,"preserves_library_order_knowledge_and_randomness":true},
            {"index":1,"ordered_instructions":[select,{"operation":"RevealSelectedCardIfAny"},{"operation":"ShuffleOwnLibrary","required_even_when_no_card_found":true},{"operation":"PutSelectedCardOnTopAfterShuffleIfAny","top_identity_known_to_both_players":true}]}]}])
    } else if micromancer {
        json!([{"operation":"ChooseWhetherToSearch","chooser":"CapturedResolutionController","at":"TriggerResolution","options":[
            {"index":0,"ordered_instructions":[select,{"operation":"MoveSelectedCardToOwnersHandIfAny","reveal":false},{"operation":"RevealSelectedHandCardToBothPlayersIfAny","only_if_selected_object_successfully_ended_in_owners_hand":true},{"operation":"ShuffleOwnLibrary","required_even_when_no_card_found":true}]},
            {"index":1,"ordered_instructions":[],"searches":false,"shuffles":false,"preserves_library_order_knowledge_and_randomness":true}]}])
    } else {
        let mut instructions = vec![
            select,
            json!({"operation":"MoveSelectedCardToOwnersHandIfAny","reveal":false}),
            json!({"operation":"ShuffleOwnLibrary","required_even_when_library_empty":true}),
        ];
        if spell {
            instructions.push(json!({"operation":"LoseLife","player":"CapturedResolutionController","amount":3,"not_damage":true,"follows_search_and_shuffle_even_when_library_empty":true}));
        }
        Value::Array(instructions)
    };
    let cast = json!({"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
        "timing":"OwnMainPhaseEmptyStackWithPriority","targets":[],"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
        "ordered_resolution":if spell{resolution.clone()}else{json!([{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}])},
        "countering_skips_resolution_and_does_not_refund_cost":true,"countered_spell_destination":"OwnersGraveyard",
        "physical_spell_departure":if spell{"OwnersGraveyardAfterResolutionOrCounter"}else{"BattlefieldOnResolutionOtherwiseOwnersGraveyard"},
        "normal_zone_replacements_and_entry_triggers_apply":true,"other_cast_permissions_require_current_candidate_authority":true});
    let triggers = if spell {
        json!([])
    } else {
        json!([{"event":"ThisPermanentEntersBattlefield","controller":"CapturedSourceControllerAtEntry","home_zone":"Battlefield","source_face":0,"mandatory_trigger":true,"uses_stack":true,"targets":[],"requires_active_printed_abilities_at_capture":true,"survives_source_departure_and_later_control_change":true,"ordered_resolution":resolution}])
    };
    Ok(Some(
        json!({"card_id":id,"library_search_printing":{"schema":"printed-owned-library-search/v1","cost":cost,"mana_value":d.mana_value,"types":d.types,"subtypes":d.subtypes,"supertypes":[],"colors":d.colors,"power":power,"toughness":toughness,"keywords":keywords.0,"token":false,"ordered_cast_roots":[cast],"ordered_triggers":triggers}}),
    ))
}
