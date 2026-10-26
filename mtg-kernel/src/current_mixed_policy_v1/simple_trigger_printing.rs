//! Closed entry/death printings. Only typed meanings cross the model boundary.
use super::{Error, Result};
use crate::{
    card_def::{CardDef, CardType, Keywords, Subtype, TargetSpec},
    effect::{EffectOp, ObjectRef, PlayerRef},
    mana::{Cost, ManaColor, Pip},
    state::Zone,
    trigger::TriggerCondition,
};
use serde_json::{json, Value};

pub(super) fn handles(name: &str) -> bool {
    matches!(
        name,
        "Helpful Hunter"
            | "Icewind Elemental"
            | "Burglar Rat"
            | "Infestation Sage"
            | "Dragon Trainer"
            | "Resolute Reinforcements"
            | "Gleaming Barrier"
            | "Prideful Parent"
            | "Guarded Heir"
            | "Hare Apparent"
    )
}

/// The caller must also apply the common exclusion guard. Every additional
/// cast, static, activation or replacement field remains forbidden there.
pub(super) fn printing(id: u16, d: &CardDef) -> Result<Option<Value>> {
    let (generic, pips, colors, subtypes, power, toughness, keywords) = match d.name {
        "Hare Apparent" => (
            1,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Rabbit, Subtype::Noble][..],
            2,
            2,
            Keywords::NONE,
        ),
        "Guarded Heir" => (
            5,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Human, Subtype::Noble][..],
            1,
            1,
            Keywords::LIFELINK,
        ),
        "Helpful Hunter" => (
            1,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Cat][..],
            1,
            1,
            Keywords::NONE,
        ),
        "Prideful Parent" => (
            2,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Cat][..],
            2,
            2,
            Keywords::VIGILANCE,
        ),
        "Icewind Elemental" => (
            4,
            &[Pip::Colored(ManaColor::U)][..],
            &[ManaColor::U][..],
            &[Subtype::Elemental][..],
            3,
            4,
            Keywords::FLYING,
        ),
        "Burglar Rat" => (
            1,
            &[Pip::Colored(ManaColor::B)][..],
            &[ManaColor::B][..],
            &[Subtype::Rat][..],
            1,
            1,
            Keywords::NONE,
        ),
        "Infestation Sage" => (
            0,
            &[Pip::Colored(ManaColor::B)][..],
            &[ManaColor::B][..],
            &[Subtype::Elf, Subtype::Warlock][..],
            1,
            1,
            Keywords::NONE,
        ),
        "Dragon Trainer" => (
            3,
            &[Pip::Colored(ManaColor::R), Pip::Colored(ManaColor::R)][..],
            &[ManaColor::R][..],
            &[Subtype::Human][..],
            1,
            1,
            Keywords::NONE,
        ),
        "Resolute Reinforcements" => (
            1,
            &[Pip::Colored(ManaColor::W)][..],
            &[ManaColor::W][..],
            &[Subtype::Human, Subtype::Soldier][..],
            1,
            1,
            Keywords::FLASH,
        ),
        "Gleaming Barrier" => (
            2,
            &[][..],
            &[][..],
            &[Subtype::Wall][..],
            0,
            4,
            Keywords::DEFENDER,
        ),
        _ => return Ok(None),
    };
    let cost = Cost {
        generic,
        pips,
        x_count: 0,
    };
    let types = if d.name == "Gleaming Barrier" {
        &[CardType::Artifact, CardType::Creature][..]
    } else {
        &[CardType::Creature][..]
    };
    if crate::card_def::card_id_by_name(d.name) != Some(id)
        || d.types != types
        || d.subtypes != subtypes
        || !d.supertypes.is_empty()
        || d.cost != cost
        || usize::from(d.mana_value) != usize::from(generic) + pips.len()
        || d.colors != colors
        || d.power != Some(power)
        || d.toughness != Some(toughness)
        || d.keywords != keywords
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
    let dies = matches!(d.name, "Infestation Sage" | "Gleaming Barrier");
    let (program, ordered) = match d.name {
        "Hare Apparent" => {
            let token_def = crate::card_def::card_id_by_name("Rabbit Token")
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            (
                EffectOp::CreateTokensForOtherControlledCreaturesNamed {
                    name: "Hare Apparent".into(),
                    token_def,
                },
                vec![
                    json!({"operation":"CreateTokensForOtherControlledCreaturesNamed","name":"Hare Apparent",
                 "count_at_resolution":"CurrentBattlefieldCreaturesControlledByCapturedTriggerControllerWithMatchingEffectiveName",
                 "exclude_only_original_captured_source_incarnation":true,"source_departure_does_not_disable_count":true,
                 "reentered_source_is_another_incarnation_and_may_count":true,
                 "current_creature_type_and_effective_name_and_control_apply":true,"token_creatures_may_count":true,
                 "opposing_creatures_and_matching_name_noncreatures_do_not_count":true,
                 "snapshot_count_once_before_creation":true,"count_is_not_targeted_or_capped":true,
                 "zero_count_creates_no_tokens":true,"controller":"CapturedTriggerController",
                 "token_definition":super::baseline(token_def)?,"all_created_tokens_enter_as_one_simultaneous_batch":true,
                 "normal_token_replacements_and_battlefield_entry_triggers_apply":true}),
                ],
            )
        }
        "Helpful Hunter" => (
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
            vec![
                json!({"operation":"DrawCards","recipient":"CapturedTriggerController","count":1}),
            ],
        ),
        "Icewind Elemental" => (
            EffectOp::Sequence(vec![
                EffectOp::DrawCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
                EffectOp::DiscardCards {
                    player: PlayerRef::Controller,
                    count: 1,
                },
            ]),
            vec![
                json!({"operation":"DrawCards","recipient":"CapturedTriggerController","count":1}),
                json!({"operation":"DiscardCards","recipient":"CapturedTriggerController","chooser":"DiscardingPlayer","count":1,"choose_from":"CurrentOwnHand","does_not_target":true}),
            ],
        ),
        "Burglar Rat" => (
            EffectOp::DiscardCards {
                player: PlayerRef::Opponent,
                count: 1,
            },
            vec![
                json!({"operation":"DiscardCards","recipient":"OpponentOfCapturedTriggerController","chooser":"DiscardingPlayer","count":1,"choose_from":"CurrentOwnHand","does_not_target":true}),
            ],
        ),
        "Guarded Heir" => {
            let token_def = crate::card_def::card_id_by_name("Knight Token")
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            let token = EffectOp::CreateToken {
                token_def,
                controller: PlayerRef::Controller,
            };
            let meaning = json!({"operation":"CreateToken","controller":"CapturedTriggerController","count":1,"token_definition":super::baseline(token_def)?,"normal_token_replacements_and_entry_triggers_apply":true});
            (
                EffectOp::Sequence(vec![token.clone(), token]),
                vec![meaning.clone(), meaning],
            )
        }
        name => {
            let token_name = match name {
                "Infestation Sage" => "Insect Token",
                "Gleaming Barrier" => "Treasure Token",
                "Dragon Trainer" => "Dragon Token",
                "Resolute Reinforcements" => "Soldier Token",
                "Prideful Parent" => "Cat Token",
                _ => unreachable!(),
            };
            let token_def = crate::card_def::card_id_by_name(token_name)
                .ok_or(Error::UnsupportedPrintedRoot(id))?;
            (
                EffectOp::CreateToken {
                    token_def,
                    controller: PlayerRef::Controller,
                },
                vec![
                    json!({"operation":"CreateToken","controller":"CapturedTriggerController","count":1,
                    "token_definition":super::baseline(token_def)?,"normal_token_replacements_and_entry_triggers_apply":true}),
                ],
            )
        }
    };
    let [trigger] = crate::trigger::triggers_for(id) else {
        return Err(Error::UnsupportedPrintedRoot(id));
    };
    let crate::trigger::TriggeredAbilityDef {
        condition,
        home_zone,
        intervening_if_kicked,
        intervening_if_controls_another_source_card,
        face_index,
        effect,
    } = trigger;
    if *condition
        != if dies {
            TriggerCondition::LeftBattlefieldToGraveyard
        } else {
            TriggerCondition::Etb
        }
        || *home_zone
            != if dies {
                Zone::Graveyard
            } else {
                Zone::Battlefield
            }
        || *intervening_if_kicked
        || *intervening_if_controls_another_source_card
        || *face_index != 0
        || effect() != program
        || crate::trigger::target_spec_for_trigger(id, &program) != Some(TargetSpec::None)
    {
        return Err(Error::UnsupportedPrintedRoot(id));
    }
    let mut meaning = json!({"card_id":id,"simple_trigger_printing":{
        "schema":"printed-entry-death/v1","cost":cost,"types":d.types,"subtypes":d.subtypes,
        "supertypes":[],"colors":d.colors,"power":power,"toughness":toughness,
        "keywords":keywords.0,"mana_value":d.mana_value,"token":false,
        "ordinary_cast":{"method":"Normal","route":"Hand","origin":"OwnHand",
            "origin_owner_must_be_caster":true,
            "timing":if keywords == Keywords::FLASH {"InstantWithPriority"}else{"OwnMainPhaseEmptyStackWithPriority"},
            "ordered_costs":[{"operation":"PayPrintedMana","cost":cost}],"targets":[],
            "ordered_resolution":[{"operation":"MoveOriginalSpellToBattlefield","controller":"Caster"}],
            "other_cast_permissions_require_current_candidate_authority":true,
            "countered_or_failed_spell_destination":"OwnersGraveyard","normal_zone_replacements_and_entry_triggers_apply":true},
        "ordered_triggers":[{"event":if dies {"ThisCreatureDies"}else{"ThisPermanentEntersBattlefield"},
            "source_face":0,"intervening_if":null,"mandatory":true,"targets":[],"uses_stack":true,
            "controller":if dies {"ControllerImmediatelyBeforeLeavingBattlefield"}else{"ControllerAtEntry"},
            "death_requires_battlefield_to_graveyard_transition":dies,
            "source_incarnation":if dies {"DepartingBattlefieldIncarnation"}else{"EnteringBattlefieldIncarnation"},
            "survives_source_departure_and_later_control_change":true,"ordered_resolution":ordered}]
    }});
    if d.name == "Hare Apparent" {
        meaning["simple_trigger_printing"]["deck_construction"] = json!({"any_number_of_cards_named_hare_apparent":true,"other_limited_deck_size_and_pool_rules_still_apply":true});
    }
    Ok(Some(meaning))
}
