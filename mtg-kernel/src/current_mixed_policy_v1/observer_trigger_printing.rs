//! Closed battlefield observers. Executable recipes are compared, never encoded.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, Supertype, TargetSpec},
    effect::{EffectOp, ObjectRef, PlayerRef, TargetRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Ajani's Pridemate"
            | "Marauding Blight-Priest"
            | "Dazzling Angel"
            | "Good-Fortune Unicorn"
            | "Archmage of Runes"
            | "Youthful Valkyrie"
            | "Spitfire Lagac"
            | "Tatyova, Benthic Druid"
            | "Brineborn Cutthroat"
            | "Elementalist Adept"
            | "Beast-Kin Ranger"
            | "Firebrand Archer"
            | "Crackling Cyclops"
            | "Elfsworn Giant"
            | "Clinquant Skymage"
            | "Erudite Wizard"
            | "Firespitter Whelp"
            | "Sanguine Syphoner"
            | "Balmor, Battlemage Captain"
            | "Eager Trufflesnout"
    )
}

/// The caller also checks every additional CardDef field through the shared
/// exclusion guard. This module admits exactly one mandatory untargeted trigger.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (generic, pips, colors, subtypes, power, toughness, keywords) = match d.name {
        "Archmage of Runes" => (
            3,
            &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::U)][..],
            &[ManaColor::U][..],
            &[Subtype::Giant, Subtype::Wizard][..],
            3,
            6,
            Keywords::NONE,
        ),
        "Good-Fortune Unicorn" => (
            1,
            &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::W)][..],
            &[ManaColor::G, ManaColor::W][..],
            &[Subtype::Unicorn][..],
            2,
            2,
            Keywords::NONE,
        ),
        "Eager Trufflesnout" => (
            2,
            &[Pip::Colored(ManaColor::G)][..],
            &[ManaColor::G][..],
            &[Subtype::Boar][..],
            4,
            2,
            Keywords::TRAMPLE,
        ),
        "Balmor, Battlemage Captain" => (
            0,
            &[Pip::Colored(ManaColor::U), Pip::Colored(ManaColor::R)][..],
            &[ManaColor::U, ManaColor::R][..],
            &[Subtype::Bird, Subtype::Wizard][..],
            1,
            3,
            Keywords::FLYING,
        ),
        "Sanguine Syphoner" => (
            1,
            &[Pip::Colored(ManaColor::B)][..],
            &[ManaColor::B][..],
            &[Subtype::Vampire, Subtype::Warlock][..],
            1,
            3,
            Keywords::NONE,
        ),
        "Firespitter Whelp" => (
            2,
            &[Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Dragon][..],
            2,
            2,
            Keywords::FLYING,
        ),
        "Clinquant Skymage" | "Erudite Wizard" => (
            if d.name == "Clinquant Skymage" { 3 } else { 2 },
            &[Pip::Colored(ManaColor::U)][..],
            &[ManaColor::U][..],
            if d.name == "Clinquant Skymage" {
                &[Subtype::Bird, Subtype::Wizard][..]
            } else {
                &[Subtype::Human, Subtype::Wizard][..]
            },
            if d.name == "Clinquant Skymage" { 1 } else { 2 },
            if d.name == "Clinquant Skymage" { 1 } else { 3 },
            if d.name == "Clinquant Skymage" {
                Keywords::FLYING
            } else {
                Keywords::NONE
            },
        ),
        "Elfsworn Giant" => (
            3,
            &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::G)][..],
            &[ManaColor::G][..],
            &[Subtype::Giant][..],
            5,
            3,
            Keywords::REACH,
        ),
        "Firebrand Archer" => (
            1,
            &[Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Human, Subtype::Archer][..],
            2,
            1,
            Keywords::NONE,
        ),
        "Crackling Cyclops" => (
            2,
            &[Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Cyclops, Subtype::Wizard][..],
            0,
            4,
            Keywords::NONE,
        ),
        "Ajani's Pridemate" => (
            1,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Cat, Subtype::Soldier][..],
            2,
            2,
            Keywords::NONE,
        ),
        "Marauding Blight-Priest" => (
            2,
            &[Pip::Colored(ManaColor::B)][..],
            &[ManaColor::B][..],
            &[Subtype::Vampire, Subtype::Cleric][..],
            3,
            2,
            Keywords::NONE,
        ),
        "Dazzling Angel" => (
            2,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Angel][..],
            2,
            3,
            Keywords::FLYING,
        ),
        "Youthful Valkyrie" => (
            1,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Angel][..],
            1,
            3,
            Keywords::FLYING,
        ),
        "Spitfire Lagac" => (
            3,
            &[Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Lizard][..],
            3,
            4,
            Keywords::NONE,
        ),
        "Brineborn Cutthroat" => (
            1,
            &[Pip::Colored(ManaColor::U)][..],
            &[ManaColor::U][..],
            &[Subtype::Merfolk, Subtype::Pirate][..],
            2,
            1,
            Keywords::FLASH,
        ),
        "Elementalist Adept" => (
            1,
            &[Pip::Colored(ManaColor::U)][..],
            &[ManaColor::U][..],
            &[Subtype::Human, Subtype::Wizard][..],
            2,
            1,
            Keywords::FLASH,
        ),
        "Beast-Kin Ranger" => (
            2,
            &[Pip::Colored(ManaColor::G)][..],
            &[ManaColor::G][..],
            &[Subtype::Elf, Subtype::Ranger][..],
            3,
            3,
            Keywords::TRAMPLE,
        ),
        "Tatyova, Benthic Druid" => (
            3,
            &[Pip::Colored(ManaColor::G), Pip::Colored(ManaColor::U)][..],
            &[ManaColor::G, ManaColor::U][..],
            &[Subtype::Merfolk, Subtype::Druid][..],
            3,
            3,
            Keywords::NONE,
        ),
        _ => return Ok(None),
    };
    let cost = Cost {
        generic,
        pips,
        x_count: 0,
    };
    let supertypes = if matches!(
        d.name,
        "Tatyova, Benthic Druid" | "Balmor, Battlemage Captain"
    ) {
        &[Supertype::Legendary][..]
    } else {
        &[][..]
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.cost != cost
        || d.colors != colors
        || d.types != [CardType::Creature]
        || d.subtypes != subtypes
        || d.supertypes != supertypes
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
        || usize::from(d.mana_value) != usize::from(generic) + pips.len()
        || crate::engine::static_instant_sorcery_reduction_for_v1(d.name)
            != if d.name == "Archmage of Runes" {
                Some(1)
            } else {
                None
            }
        || d.is_land
        || d.is_token
        || d.enters_battlefield_tapped
        || d.target_spec != TargetSpec::None
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
    let gain = || EffectOp::GainLife {
        player: PlayerRef::Controller,
        amount: 1,
    };
    let gain_meaning = || {
        json!({"operation":"GainLife","recipient":"CapturedTriggerController","amount":1,
        "normal_life_gain_replacements_and_triggers_apply":true})
    };
    let counter_meaning = || {
        json!({"operation":"AddCounterToOriginalTriggerSource","counter":"PlusOnePlusOne","count":1,
        "binding":"CapturedSourceObjectAndBattlefieldIncarnation","requires_same_battlefield_incarnation":true,
        "missing_or_reentered_source_gets_no_counter":true,"normal_counter_replacements_and_triggers_apply":true})
    };
    let boost_meaning = |power, toughness| {
        json!({"operation":"ModifyOriginalTriggerSourcePowerToughness",
        "power_delta":power,"toughness_delta":toughness,"layer":"AdditivePowerToughness",
        "binding":"CapturedSourceObjectAndBattlefieldIncarnation","requires_same_battlefield_incarnation":true,
        "missing_or_reentered_source_gets_no_boost":true,"duration":"UntilCleanupExpirationAfterDiscard"})
    };
    let (condition, event, program, ordered) = match d.name {
        "Archmage of Runes" => (
            TriggerCondition::CastInstantOrSorcery,
            json!({"event":"ObserverControllerCastsInstantOrSorcery","current_cast_characteristics":true,"opponent_casts_and_copies_without_cast_do_not_trigger":true,"creature_artifact_or_enchantment_only_casts_do_not_trigger":true,"trigger_count_per_cast":1,"countering_cast_spell_does_not_remove_trigger":true}),
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            vec![
                json!({"operation":"DrawCards","recipient":"CapturedTriggerController","count":1,"normal_draw_replacements_and_empty_library_loss_apply":true,"drawn_identity_visible_only_to_authorized_viewers":true,"does_not_require_source_still_on_battlefield":true}),
            ],
        ),
        "Eager Trufflesnout" => {
            let food = crate::card_def::card_id_by_name("Food Token")
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            let definition = super::super::next_five::printing(food)?
                .ok_or(Error::UnsupportedPrintedRoot(food))?;
            (
                TriggerCondition::DealsCombatDamageToPlayer,
                json!({"event":"ThisSourceActuallyDealsCombatDamageToPlayer",
                    "requires_positive_dealt_damage":true,"attacking_without_player_damage_does_not_trigger":true,
                    "noncombat_damage_and_damage_to_creatures_or_planeswalkers_do_not_trigger":true,
                    "replaced_or_prevented_damage_does_not_count_as_dealt":true,
                    "trigger_count_per_matching_damage_event":1,"does_not_scale_with_damage_amount":true}),
                EffectOp::CreateToken {
                    token_def: food,
                    controller: PlayerRef::Controller,
                },
                vec![
                    json!({"operation":"CreateToken","controller":"CapturedTriggerController","count":1,
                    "token_definition":definition,"entry_tapped":false,
                    "does_not_require_source_to_remain":true,"normal_token_replacements_and_entry_triggers_apply":true}),
                ],
            )
        }
        "Balmor, Battlemage Captain" => (
            TriggerCondition::CastInstantOrSorcery,
            json!({"event":"ObserverControllerCastsInstantOrSorcery","current_cast_characteristics":true,
                "artifact_or_enchantment_only_casts_do_not_trigger":true,"creature_casts_including_flash_do_not_trigger":true,
                "opponent_casts_and_copies_without_cast_do_not_trigger":true,"trigger_count_per_cast":1,
                "trigger_does_not_require_cast_spell_to_resolve":true}),
            EffectOp::BoostControlledCreaturesUntilEndOfTurn {
                power: 1,
                toughness: 0,
                keywords: Keywords::TRAMPLE,
            },
            vec![
                json!({"operation":"ModifyCurrentControlledCreatureSet","controller":"CapturedTriggerController",
                "samples_set_at_trigger_resolution":true,"binds_each_current_battlefield_incarnation":true,
                "includes_source_if_still_a_controlled_creature":true,"does_not_require_source_to_remain":true,
                "power_delta":1,"toughness_delta":0,"grant_keyword_bits":Keywords::TRAMPLE.0,
                "places_no_counters":true,"later_entering_creatures_not_included":true,
                "duration":"UntilCleanupExpirationAfterDiscard","normal_continuous_layers_and_timestamps_apply":true}),
            ],
        ),
        "Sanguine Syphoner" => (
            TriggerCondition::Attacks,
            json!({"event":"ThisSourceIsDeclaredAsAttacker","requires_actual_attack_declaration":true,
                "entering_attacking_does_not_trigger":true,"trigger_count_per_declaration":1}),
            EffectOp::Sequence(vec![
                EffectOp::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 1,
                },
                EffectOp::GainLife {
                    player: PlayerRef::Controller,
                    amount: 1,
                },
            ]),
            vec![
                json!({"operation":"LoseLife","recipient":"OpponentOfCapturedTriggerController","amount":1,
                    "is_damage":false,"does_not_target":true,"normal_life_loss_rules_apply":true}),
                json!({"operation":"GainLife","recipient":"CapturedTriggerController","amount":1,
                    "does_not_require_successful_opponent_life_loss":true,"normal_life_gain_replacements_apply":true}),
            ],
        ),
        "Clinquant Skymage" | "Erudite Wizard" => (
            if d.name == "Erudite Wizard" {
                TriggerCondition::DrawNth(2)
            } else {
                TriggerCondition::ControllerDraws
            },
            json!({"event":"ObserverControllerActuallyDrawsCard","multiple_card_draws_are_individual_events":true,
                "requires_this_turn_draw_ordinal":if d.name == "Erudite Wizard" { Some(2) } else { None },
                "uses_native_per_player_turn_draw_count":true,"trigger_count_per_matching_draw":1}),
            EffectOp::BindPlusOnePlusOneCounterToTriggerSource,
            vec![counter_meaning()],
        ),
        "Brineborn Cutthroat" => (
            TriggerCondition::CastSpellDuringOpponentsTurn,
            json!({"event":"ObserverControllerCastsSpell","requires_opponents_active_turn":true,
                "copies_without_cast_do_not_count":true,"trigger_count_per_cast":1}),
            EffectOp::BindPlusOnePlusOneCounterToTriggerSource,
            vec![counter_meaning()],
        ),
        "Elementalist Adept" | "Crackling Cyclops" => (
            TriggerCondition::CastNoncreatureSpell,
            json!({"event":"ObserverControllerCastsNoncreatureSpell","copies_without_cast_do_not_count":true,
                "current_spell_characteristics":true,"trigger_count_per_cast":1}),
            EffectOp::BindTemporaryBoostToTriggerSource {
                power: if d.name == "Crackling Cyclops" { 3 } else { 1 },
                toughness: if d.name == "Crackling Cyclops" { 0 } else { 1 },
            },
            vec![if d.name == "Crackling Cyclops" {
                boost_meaning(3, 0)
            } else {
                boost_meaning(1, 1)
            }],
        ),
        "Firebrand Archer" | "Firespitter Whelp" => (
            if d.name == "Firespitter Whelp" {
                TriggerCondition::CastNoncreatureOrSubtype(Subtype::Dragon)
            } else {
                TriggerCondition::CastNoncreatureSpell
            },
            if d.name == "Firespitter Whelp" {
                json!({"event":"ObserverControllerCastsSpell","copies_without_cast_do_not_count":true,
                    "current_spell_characteristics":true,"matches_any_of":[{"lacks_type":"Creature"},{"has_subtype":"Dragon"}],
                    "opponent_casts_do_not_trigger":true,"trigger_count_per_matching_cast":1,
                    "matching_both_predicates_still_triggers_once":true})
            } else {
                json!({"event":"ObserverControllerCastsNoncreatureSpell","copies_without_cast_do_not_count":true,
                    "current_spell_characteristics":true,"trigger_count_per_cast":1})
            },
            EffectOp::DealDamage {
                target: TargetRef::Opponent,
                amount: 1,
            },
            vec![
                json!({"operation":"DealDamage","recipient":"OpponentOfCapturedTriggerController","amount":1,
                "source":"OriginalTriggerSource","damage_class":"Noncombat",
                "normal_damage_replacements_and_prevention_apply":true}),
            ],
        ),
        "Ajani's Pridemate" | "Marauding Blight-Priest" => (
            TriggerCondition::ControllerGainsLife,
            json!({"event":"ObserverControllerActuallyGainsLife","requires_positive_actual_gain":true,
                "trigger_count_per_gain_event":1,"does_not_scale_with_life_amount":true}),
            if d.name == "Ajani's Pridemate" {
                EffectOp::BindPlusOnePlusOneCounterToTriggerSource
            } else {
                EffectOp::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 1,
                }
            },
            if d.name == "Ajani's Pridemate" {
                vec![counter_meaning()]
            } else {
                vec![
                    json!({"operation":"LoseLife","recipient":"OpponentOfCapturedTriggerController","amount":1,
                    "is_damage":false,"normal_life_loss_replacements_apply":true}),
                ]
            },
        ),
        "Dazzling Angel" | "Youthful Valkyrie" | "Beast-Kin Ranger" | "Good-Fortune Unicorn" => (
            TriggerCondition::OtherControlledCreatureEnters {
                subtype: if d.name == "Youthful Valkyrie" {
                    Some(Subtype::Angel)
                } else {
                    None
                },
            },
            json!({"event":"AnotherCreatureEntersUnderObserverControl","excludes_this_source":true,
                "required_current_subtype":if d.name == "Youthful Valkyrie" { Some("Angel") } else { None },
                "current_effective_entry_characteristics":true,"tokens_count":true,"trigger_count_per_entering_creature":1}),
            if d.name == "Dazzling Angel" {
                gain()
            } else if d.name == "Good-Fortune Unicorn" {
                EffectOp::BindPlusOnePlusOneCounterToTriggerEventObject
            } else if d.name == "Beast-Kin Ranger" {
                EffectOp::BindTemporaryBoostToTriggerSource {
                    power: 1,
                    toughness: 0,
                }
            } else {
                EffectOp::BindPlusOnePlusOneCounterToTriggerSource
            },
            if d.name == "Dazzling Angel" {
                vec![gain_meaning()]
            } else if d.name == "Good-Fortune Unicorn" {
                vec![
                    json!({"operation":"PutPlusOnePlusOneCounter","count":1,"recipient":"OriginalEnteringCreatureIncarnation","binding":"CapturedEventObjectAndBattlefieldIncarnation","not_a_target":true,"does_not_require_source_still_present":true,"current_controller_change_does_not_change_recipient":true,"missing_or_reentered_entrant_gets_no_counter":true,"normal_counter_replacements_and_triggers_apply":true}),
                ]
            } else if d.name == "Beast-Kin Ranger" {
                vec![boost_meaning(1, 0)]
            } else {
                vec![counter_meaning()]
            },
        ),
        "Spitfire Lagac" | "Tatyova, Benthic Druid" | "Elfsworn Giant" => (
            TriggerCondition::ControlledLandEnters,
            json!({"event":"LandEntersUnderObserverControl","requires_land_play":false,
                "tokens_count":true,"current_effective_entry_characteristics":true,"trigger_count_per_entering_land":1}),
            if d.name == "Spitfire Lagac" {
                EffectOp::DealDamage {
                    target: TargetRef::Opponent,
                    amount: 1,
                }
            } else if d.name == "Elfsworn Giant" {
                EffectOp::CreateToken {
                    token_def: crate::card_def::card_id_by_name("Elf Warrior Token")
                        .ok_or(Error::UnsupportedPrintedRoot(id))?,
                    controller: PlayerRef::Controller,
                }
            } else {
                EffectOp::Sequence(vec![
                    gain(),
                    EffectOp::DrawCards {
                        player: PlayerRef::Controller,
                        count: 1,
                    },
                ])
            },
            if d.name == "Spitfire Lagac" {
                vec![
                    json!({"operation":"DealDamage","recipient":"OpponentOfCapturedTriggerController","amount":1,
                    "source":"OriginalTriggerSource","damage_class":"Noncombat",
                    "normal_damage_replacements_and_prevention_apply":true}),
                ]
            } else if d.name == "Elfsworn Giant" {
                vec![
                    json!({"operation":"CreateToken","controller":"CapturedTriggerController","count":1,
                    "token_definition":super::baseline(crate::card_def::card_id_by_name("Elf Warrior Token").ok_or(Error::UnsupportedPrintedRoot(id))?)?,
                    "normal_token_replacements_and_entry_triggers_apply":true}),
                ]
            } else {
                vec![
                    gain_meaning(),
                    json!({"operation":"DrawCards","recipient":"CapturedTriggerController","count":1,
                    "does_not_require_successful_life_gain":true,"normal_draw_replacements_apply":true}),
                ]
            },
        ),
        _ => unreachable!(),
    };
    let [trigger] = crate::trigger::triggers_for(id) else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let crate::trigger::TriggeredAbilityDef {
        condition: actual,
        home_zone,
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        face_index,
        effect,
    } = trigger;
    if *actual != condition
        || *home_zone != Zone::Battlefield
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || *face_index != 0
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let mut meaning = json!({"card_id":id,"observer_trigger_printing":{
        "schema":"printed-battlefield-observer/v1","cost":cost,"types":d.types,"subtypes":d.subtypes,
        "supertypes":if supertypes.is_empty() { json!([]) } else { json!(["Legendary"]) },"colors":d.colors,"power":power,"toughness":toughness,
        "keywords":keywords.0,"mana_value":d.mana_value,"token":false,
        "ordinary_cast":{"method":"Normal","route":"Hand","origin":"OwnHand","origin_owner_must_be_caster":true,
            "timing":if keywords.has(Keywords::FLASH) {"InstantWithPriority"} else {"OwnMainPhaseEmptyStackWithPriority"},"ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],
            "targets":[],"ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
            "other_cast_permissions_require_current_candidate_authority":true,
            "countered_or_failed_spell_destination":"OwnersGraveyard","normal_zone_replacements_and_entry_triggers_apply":true},
        "ordered_triggers":[{"event":event,"home_zone":"Battlefield","source_face":0,"intervening_if":null,
            "mandatory":true,"targets":[],"uses_stack":true,"controller":"ObserverControllerAtEvent",
            "captures_source_incarnation_and_controller_at_event":true,"survives_source_departure_and_later_control_change":true,
            "ordered_resolution":ordered}]
    }});
    if d.name == "Archmage of Runes" {
        meaning["observer_trigger_printing"]["ordered_static_roots"] = json!([{"operation":"ReduceControllerInstantOrSorcerySpellGenericCost","source_zone":"Battlefield","requires_active_printed_source_abilities":true,"controller":"CurrentSourceController","current_effective_copied_printing_selects_rule":true,"spell_types_match_any":["Instant","Sorcery"],"reduction_per_live_source":1,"independent_sources_add":true,"minimum_generic_component":0,"does_not_reduce_colored_pips":true,"apply_to_total_cost_after_additional_costs_and_chosen_x":true,"normal_final_cost_floor_rules_apply_afterward":true,"does_not_reduce_activated_abilities_or_spell_mana_value":true,"recompute_at_cost_determination":true,"does_not_reprice_already_paid_spell":true,"ends_when_source_leaves_or_loses_printed_abilities":true}]);
    }
    Ok(Some(meaning))
}
