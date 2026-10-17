//! Closed typed next-five rules. Inspection of immutable definitions is private;
//! executable programs and runtime ownership never cross this public boundary.
use crate::{
    card_def::{self, CardCapability, CardDef, CardType, Keywords, Subtype, TargetSpec, CARD_DEFS},
    effect::{EffectOp, ObjectRef},
    mana::{ManaColor, Pip},
    optional_trigger_public_v1::SupertypeV1,
    state::Zone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "rules-meaning/next-five/v59";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FamilyV59 {
    Snack,
    Replication,
    Infernal,
    Giada,
    Goblin,
}
impl FamilyV59 {
    pub const ALL: [Self; 5] = [
        Self::Snack,
        Self::Replication,
        Self::Infernal,
        Self::Giada,
        Self::Goblin,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Snack => "Midnight Snack",
            Self::Replication => "Extravagant Replication",
            Self::Infernal => "Infernal Vessel",
            Self::Giada => "Giada, Font of Hope",
            Self::Goblin => "Goblin Negotiation",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManaV59 {
    pub generic: u8,
    pub colored: Vec<ManaColor>,
    pub x_multiplicity: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrintedV59 {
    pub mana: ManaV59,
    pub mana_value: u16,
    pub types: Vec<CardType>,
    pub subtypes: Vec<Subtype>,
    pub supertypes: Vec<SupertypeV1>,
    pub colors: Vec<ManaColor>,
    pub power: Option<i16>,
    pub toughness: Option<i16>,
    pub keyword_bits: u32,
    pub is_token: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimingV59 {
    MainPhaseEmptyStackPriority,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActorV59 {
    OriginalSpellController,
    CapturedAbilityController,
    PhysicalCardOwner,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetV59 {
    None,
    OneCreature,
    AnotherControlledNonlandPermanentExcludingOriginalPhysicalSource,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventV59 {
    ControllerUpkeep,
    SelfDiesWhileNotDemonOnPrebatchBattlefield,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenV59 {
    pub card_id: u16,
    pub printed: PrintedV59,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExcessIntersectionRuleV59 {
    pub schema: String,
    pub greatest_of_creature_and_planeswalker_excess: bool,
    pub planeswalker_uses_predamage_unsigned_loyalty: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum InstructionV59 {
    PayPrintedMana,
    MoveOriginalSpellToBattlefieldUnderCaster,
    DealChosenXToOriginalCreature {
        actual_committed_damage_after_replacement_and_prevention: bool,
    },
    CreateExcessGoblins {
        token: TokenV59,
        controller: ActorV59,
        lethal_uses_predamage_effective_toughness_less_marked_damage: bool,
        source_deathtouch_lethal_one: bool,
        no_sba_between_damage_and_tokens: bool,
        one_complete_replacement_batch: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        creature_planeswalker_intersection: Option<ExcessIntersectionRuleV59>,
    },
    CopyOriginalTarget {
        controller: ActorV59,
        current_copiable_face: bool,
        face_down_uses_anonymous_characteristics: bool,
        copy_counters_and_noncopy_effects: bool,
        apply_token_replacements_once: bool,
        untargeted_aura_host_before_entry_observers: bool,
        no_legal_host_creates_no_aura: bool,
        one_choice_per_expanded_aura: bool,
    },
    ReturnOriginalGraveyardIncarnation {
        controller: ActorV59,
        token_cannot_return: bool,
        requires_exact_successor_generation: bool,
        plus_one_counters: u32,
        added_subtype: Subtype,
        entry_characteristics_before_observers: bool,
        entry_counter_replacements_apply: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum RuleV59 {
    Snack {
        meaning: Box<crate::rules_meaning_midnight_snack_v58::MeaningV58>,
    },
    Cast {
        timing: TimingV59,
        origin: Zone,
        targets: TargetV59,
        ordered: Vec<InstructionV59>,
        failed_or_countered_spell_to_owner_graveyard: bool,
        ordinary_zone_replacements_apply: bool,
    },
    Trigger {
        event: EventV59,
        actor: ActorV59,
        home: Zone,
        printed_ability_active_at_event: bool,
        targets: TargetV59,
        ordered: Vec<InstructionV59>,
    },
    AngelEntry {
        source_must_be_active_before_batch: bool,
        other_controlled_angel: bool,
        count_already_controlled_angels_before_entry: bool,
        exclude_entrant_from_count: bool,
        one_application_per_original_source: bool,
        counters_exist_before_observers: bool,
        counter_replacements_apply: bool,
    },
    RestrictedMana {
        tap_original_source: bool,
        amount: u8,
        color: ManaColor,
        spell_subtype_only: Subtype,
        creature_tap_sickness_rules_apply: bool,
        not_for_activations: bool,
        restriction_survives_source_departure: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeaningV59 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub family: FamilyV59,
    pub card_id: u16,
    pub printed: PrintedV59,
    pub ordered_rules: Vec<RuleV59>,
    pub full_engine_admitted: bool,
    pub model_admitted: bool,
    pub sampler_admitted: bool,
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for part in [
            SCHEMA,
            include_str!("rules_meaning_next_five_v59.rs"),
            include_str!("next_five_public_v59.rs"),
            include_str!("snack_wide_public_v59.rs"),
            include_str!("panda_public_v47.rs"),
            include_str!("conqueror_public_v49.rs"),
            include_str!("rules_meaning_midnight_snack_v58.rs"),
            include_str!("midnight_snack_public_v58.rs"),
            include_str!("optional_trigger_public_v1.rs"),
            include_str!("midnight_snack_v1.rs"),
            include_str!("extravagant_replication_v1.rs"),
            include_str!("infernal_vessel_v1.rs"),
            include_str!("giada_v1.rs"),
            include_str!("goblin_negotiation_v1.rs"),
            include_str!("goblin_negotiation_v1/owned.rs"),
            include_str!("effect/nonland_copy_v1.rs"),
            include_str!("effect/crystal_prevention_v1.rs"),
            include_str!("crystal_public_v1.rs"),
            include_str!("doubling_season_v48.rs"),
            include_str!("creature_death_capture_v1.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("effect/curator_v1.rs"),
            include_str!("effect/loot_v1.rs"),
            include_str!("effect/kykar_v1.rs"),
            include_str!("effect/greatwurm_v1.rs"),
            include_str!("effect/source_token_copy_v1.rs"),
            include_str!("engine/greatwurm_cast_v1.rs"),
            include_str!("effect/sphinx_lore_v1.rs"),
            include_str!("effect/harvester_v1.rs"),
            include_str!("leyline_axe_v1.rs"),
            include_str!("london_mulligan_v1.rs"),
            include_str!("soul_shackled_zombie_v1.rs"),
            include_str!("engine/sphinx_lore_cast_v1.rs"),
            include_str!("spell_costs_v1.rs"),
            include_str!("effect/squad_rallier_resolution_v1.rs"),
            include_str!("state.rs"),
            include_str!("linked_aura_operation_v1.rs"),
            include_str!("card_def.rs"),
            include_str!("trigger.rs"),
            include_str!("mana.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("standard_creatures_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("standard_statics_v1.rs"),
            include_str!("standard_legends_v1.rs"),
            include_str!("source_type_hexproof_v1.rs"),
            include_str!("starting_life_statics_v1.rs"),
            include_str!("life_loss_turn_v1.rs"),
            include_str!("rl.rs"),
            include_str!("../build.rs"),
            include_str!("counter_target_effects_v1.rs"),
            include_str!("zimone_paradox_sculptor_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("human_bo3_v1/next_five_v59.rs"),
            include_str!("../../python/tools/limited_next_five_session_v6.py"),
            include_str!("engine/omniscience_cast_v1.rs"),
            include_str!("resolution_cast_v1.rs"),
            include_str!("progenitus_v1.rs"),
            include_str!("source_power_activation_v1.rs"),
            include_str!("rl_session/flat_action_v4/search_state/effect_refs.rs"),
            include_str!("effect/inspiration_resolution_v1.rs"),
            include_str!("effect/standard_exile.rs"),
            include_str!("snack_wide_life_v59.rs"),
            include_str!("snack_wide_life_v59/custody.rs"),
            include_str!("bloodthirsty_conqueror_v49.rs"),
            include_str!("fiendish_panda_v1.rs"),
            include_str!("fiendish_panda_v1/custody.rs"),
            include_str!("life_gain_turn_v1.rs"),
            include_str!("legend_rule_v1.rs"),
            include_str!("standard_cast_convoke.rs"),
            include_str!("engine/mandatory_cast_cost_v1.rs"),
            include_str!("effect/thousand_year_storm_v1.rs"),
            include_str!("effect/standard_copy.rs"),
            include_str!("standard_spell_copy.rs"),
            include_str!("event_history_v1.rs"),
            crate::engine::time_stop_v1::source_sha256_v1(),
        ] {
            h.update((part.len() as u64).to_le_bytes());
            h.update(part.as_bytes());
        }
        h.update(b"legacy-graveyard-return-guard/v61\0");
        h.update(crate::graveyard_return_public_v61::producer_sha256().as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
fn printed(d: &CardDef) -> Result<PrintedV59, String> {
    let colored = d
        .cost
        .pips
        .iter()
        .map(|p| match p {
            Pip::Colored(c) => Ok(*c),
            _ => Err("V59 requires exact colored-pip recipe".to_string()),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(PrintedV59 {
        mana: ManaV59 {
            generic: d.cost.generic,
            colored,
            x_multiplicity: d.cost.x_count,
        },
        mana_value: d.mana_value,
        types: d.types.to_vec(),
        subtypes: d.subtypes.to_vec(),
        supertypes: d
            .supertypes
            .iter()
            .map(|s| match s {
                card_def::Supertype::Basic => SupertypeV1::Basic,
                card_def::Supertype::Snow => SupertypeV1::Snow,
                card_def::Supertype::Legendary => SupertypeV1::Legendary,
            })
            .collect(),
        colors: d.colors.to_vec(),
        power: d.power,
        toughness: d.toughness,
        keyword_bits: d.keywords.0,
        is_token: d.is_token,
    })
}
pub(crate) fn selected_id(family: FamilyV59) -> Result<u16, String> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Err("V59 requires pure Limited catalog".into());
    }
    let id = card_def::card_id_by_name(family.name()).ok_or("V59 actual printing absent")?;
    let d = &CARD_DEFS[usize::from(id)];
    if !owns_definition(family, id, d) {
        return Err("V59 printing changed its complete exact recipe".into());
    }
    Ok(id)
}
pub fn printed_meaning(family: FamilyV59) -> Result<MeaningV59, String> {
    let id = selected_id(family)?;
    let d = &CARD_DEFS[usize::from(id)];
    let mut rules = if family == FamilyV59::Snack {
        vec![RuleV59::Snack {
            meaning: Box::new(crate::rules_meaning_midnight_snack_v58::printed_meaning(
                id,
            )?),
        }]
    } else {
        let mut ordered = vec![InstructionV59::PayPrintedMana];
        if family == FamilyV59::Goblin {
            let token =
                card_def::card_id_by_name("Goblin Token").ok_or("Goblin blueprint absent")?;
            validate_goblin(token)?;
            ordered.extend([
                InstructionV59::DealChosenXToOriginalCreature {
                    actual_committed_damage_after_replacement_and_prevention: true,
                },
                InstructionV59::CreateExcessGoblins {
                    token: TokenV59 {
                        card_id: token,
                        printed: printed(&CARD_DEFS[usize::from(token)])?,
                    },
                    controller: ActorV59::OriginalSpellController,
                    lethal_uses_predamage_effective_toughness_less_marked_damage: true,
                    source_deathtouch_lethal_one: true,
                    no_sba_between_damage_and_tokens: true,
                    one_complete_replacement_batch: true,
                    creature_planeswalker_intersection: Some(ExcessIntersectionRuleV59 {
                        schema: "rules-meaning/goblin-excess-intersection/v1".into(),
                        greatest_of_creature_and_planeswalker_excess: true,
                        planeswalker_uses_predamage_unsigned_loyalty: true,
                    }),
                },
            ]);
        } else {
            ordered.push(InstructionV59::MoveOriginalSpellToBattlefieldUnderCaster);
        }
        vec![RuleV59::Cast {
            timing: TimingV59::MainPhaseEmptyStackPriority,
            origin: Zone::Hand,
            targets: if family == FamilyV59::Goblin {
                TargetV59::OneCreature
            } else {
                TargetV59::None
            },
            ordered,
            failed_or_countered_spell_to_owner_graveyard: true,
            ordinary_zone_replacements_apply: true,
        }]
    };
    match family {
        FamilyV59::Replication => rules.push(RuleV59::Trigger {
            event: EventV59::ControllerUpkeep,
            actor: ActorV59::CapturedAbilityController,
            home: Zone::Battlefield,
            printed_ability_active_at_event: true,
            targets: TargetV59::AnotherControlledNonlandPermanentExcludingOriginalPhysicalSource,
            ordered: vec![InstructionV59::CopyOriginalTarget {
                controller: ActorV59::CapturedAbilityController,
                current_copiable_face: true,
                face_down_uses_anonymous_characteristics: true,
                copy_counters_and_noncopy_effects: false,
                apply_token_replacements_once: true,
                untargeted_aura_host_before_entry_observers: true,
                no_legal_host_creates_no_aura: true,
                one_choice_per_expanded_aura: true,
            }],
        }),
        FamilyV59::Infernal => rules.push(RuleV59::Trigger {
            event: EventV59::SelfDiesWhileNotDemonOnPrebatchBattlefield,
            actor: ActorV59::CapturedAbilityController,
            home: Zone::Battlefield,
            printed_ability_active_at_event: true,
            targets: TargetV59::None,
            ordered: vec![InstructionV59::ReturnOriginalGraveyardIncarnation {
                controller: ActorV59::PhysicalCardOwner,
                token_cannot_return: true,
                requires_exact_successor_generation: true,
                plus_one_counters: 2,
                added_subtype: Subtype::Demon,
                entry_characteristics_before_observers: true,
                entry_counter_replacements_apply: true,
            }],
        }),
        FamilyV59::Giada => rules.extend([
            RuleV59::AngelEntry {
                source_must_be_active_before_batch: true,
                other_controlled_angel: true,
                count_already_controlled_angels_before_entry: true,
                exclude_entrant_from_count: true,
                one_application_per_original_source: true,
                counters_exist_before_observers: true,
                counter_replacements_apply: true,
            },
            RuleV59::RestrictedMana {
                tap_original_source: true,
                amount: 1,
                color: ManaColor::W,
                spell_subtype_only: Subtype::Angel,
                creature_tap_sickness_rules_apply: true,
                not_for_activations: true,
                restriction_survives_source_departure: true,
            },
        ]),
        _ => {}
    }
    Ok(MeaningV59 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: card_def::KERNEL_CARDDB_HASH,
        family,
        card_id: id,
        printed: printed(d)?,
        ordered_rules: rules,
        full_engine_admitted: d.has_full_support(),
        model_admitted: false,
        sampler_admitted: false,
    })
}
pub fn validate(value: &MeaningV59) -> Result<(), String> {
    if *value == printed_meaning(value.family)? {
        Ok(())
    } else {
        Err("V59 complete canonical meaning changed".into())
    }
}

fn closed_fields(d: &CardDef) {
    let CardDef {
        name: _,
        capability: _,
        cost: _,
        generic_cost_reduction: _,
        ward_cost: _,
        spell_cannot_be_countered: _,
        records_mana_spent: _,
        equipment: _,
        types: _,
        subtypes: _,
        supertypes: _,
        power: _,
        toughness: _,
        starting_loyalty: _,
        enters_with_plus_one_counters: _,
        controlled_counter_keyword: _,
        game_outcome_protection_v1: _,
        controlled_creature_grant: _,
        is_land: _,
        produces_mana: _,
        colors: _,
        target_spec: _,
        keywords: _,
        spell_effect: _,
        mana_ability: _,
        alt_cost: _,
        kicker_cost: _,
        additional_cost: _,
        flashback: _,
        activated_abilities: _,
        plot_cost: _,
        madness_cost: _,
        mode2: _,
        mode3: _,
        additional_modes: _,
        kicked_target_spec: _,
        is_token: _,
        escape: _,
        mana_ability_choices: _,
        enters_battlefield_tapped: _,
        mana_ability_def: _,
        minimum_blockers: _,
        omen: _,
        mana_value: _,
        mana_ability_includes_chosen_color: _,
        as_enters_choose_color_other_than: _,
        additional_mana_abilities: _,
        object_name: _,
        enters_battlefield_tapped_unless: _,
        attachment: _,
        transform_face: _,
        saga: _,
        optional_additional_cost: _,
        changeling: _,
        bestow: _,
        delve: _,
        adventure: _,
        cant_be_blocked_by_monarchs_creatures: _,
        conditional_tap_yield: _,
        enters_tapped_unless_controller: _,
        additional_mana_ability_conditions: _,
        restricted_mana_abilities: _,
        animation: _,
        activated_ability_generic_reductions: _,
        combat_static_rules: _,
        as_enters_color_choice_v1: _,
        chosen_color_creature_boost_v1: _,
        damage_replacement_v1: _,
        token_counter_replacement_v48: _,
        starting_life_self_bonuses_v1: _,
        source_type_hexproof_v1: _,
        other_controlled_nontoken_death_v1: _,
        creature_type_entry_v1: _,
    } = d;
}
fn ordinary_foreign_fields_absent(d: &CardDef, giada: bool) -> bool {
    closed_fields(d);
    d.object_name == d.name
        && !d.is_land
        && d.generic_cost_reduction.is_none()
        && d.ward_cost.is_none()
        && !d.spell_cannot_be_countered
        && !d.records_mana_spent
        && d.equipment.is_none()
        && d.starting_loyalty.is_none()
        && d.enters_with_plus_one_counters.is_none()
        && d.controlled_counter_keyword.is_none()
        && d.game_outcome_protection_v1.is_none()
        && d.controlled_creature_grant.is_none()
        && d.activated_abilities.is_empty()
        && (d.mana_ability)().is_none()
        && d.alt_cost.is_none()
        && d.kicker_cost.is_none()
        && d.additional_cost.is_none()
        && d.flashback.is_none()
        && d.plot_cost.is_none()
        && d.madness_cost.is_none()
        && d.mode2.is_none()
        && d.mode3.is_none()
        && d.additional_modes.is_empty()
        && d.kicked_target_spec.is_none()
        && d.escape.is_none()
        && d.mana_ability_choices.is_empty()
        && !d.enters_battlefield_tapped
        && d.mana_ability_def.is_none()
        && d.minimum_blockers == 1
        && d.omen.is_none()
        && !d.mana_ability_includes_chosen_color
        && d.as_enters_choose_color_other_than.is_none()
        && d.additional_mana_abilities.is_empty()
        && d.enters_battlefield_tapped_unless.is_none()
        && d.attachment.is_none()
        && d.transform_face.is_none()
        && d.saga.is_none()
        && d.optional_additional_cost.is_none()
        && !d.changeling
        && d.bestow.is_none()
        && !d.delve
        && d.adventure.is_none()
        && !d.cant_be_blocked_by_monarchs_creatures
        && d.conditional_tap_yield.is_none()
        && d.enters_tapped_unless_controller.is_none()
        && d.additional_mana_ability_conditions.is_empty()
        && d.animation.is_none()
        && d.activated_ability_generic_reductions.is_empty()
        && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
        && d.as_enters_color_choice_v1.is_none()
        && d.chosen_color_creature_boost_v1.is_none()
        && d.damage_replacement_v1.is_none()
        && d.starting_life_self_bonuses_v1.is_empty()
        && d.source_type_hexproof_v1.is_none()
        && d.creature_type_entry_v1.is_none()
        && d.other_controlled_nontoken_death_v1.is_none()
        && if giada {
            d.produces_mana == [ManaColor::W]
                && matches!(d.restricted_mana_abilities,[a]if a.colors==[ManaColor::W] && a.restriction==card_def::ManaSpendRestrictionDef::SpellWithSubtype(Subtype::Angel))
                && d.token_counter_replacement_v48
                    == Some(
                        card_def::TokenCounterReplacementDefV48::OtherControlledSubtypeEntry(
                            Subtype::Angel,
                        ),
                    )
        } else {
            d.produces_mana.is_empty()
                && d.restricted_mana_abilities.is_empty()
                && d.token_counter_replacement_v48.is_none()
        }
}
pub(crate) fn owns_definition(family: FamilyV59, id: u16, d: &CardDef) -> bool {
    if family == FamilyV59::Snack {
        return crate::rules_meaning_midnight_snack_v58::selected_definition(id, d);
    }
    if d.name != family.name()
        || !matches!(d.capability, CardCapability::Partial | CardCapability::Full)
        || d.is_token
        || !ordinary_foreign_fields_absent(d, family == FamilyV59::Giada)
    {
        return false;
    }
    let (generic, pips, x, types, subtypes, legendary, pt, keywords, target) = match family {
        FamilyV59::Replication => (
            4,
            vec![ManaColor::U; 2],
            0,
            vec![CardType::Enchantment],
            vec![],
            false,
            (None, None),
            Keywords::NONE,
            TargetSpec::None,
        ),
        FamilyV59::Infernal => (
            2,
            vec![ManaColor::B],
            0,
            vec![CardType::Creature],
            vec![Subtype::Human, Subtype::Cleric],
            false,
            (Some(2), Some(1)),
            Keywords::NONE,
            TargetSpec::None,
        ),
        FamilyV59::Giada => (
            1,
            vec![ManaColor::W],
            0,
            vec![CardType::Creature],
            vec![Subtype::Angel],
            true,
            (Some(2), Some(2)),
            Keywords::FLYING | Keywords::VIGILANCE,
            TargetSpec::None,
        ),
        FamilyV59::Goblin => (
            0,
            vec![ManaColor::R; 2],
            1,
            vec![CardType::Sorcery],
            vec![],
            false,
            (None, None),
            Keywords::NONE,
            TargetSpec::Creature,
        ),
        FamilyV59::Snack => unreachable!(),
    };
    if d.cost.generic != generic
        || d.cost.x_count != x
        || d.cost
            .pips
            .iter()
            .copied()
            .ne(pips.iter().copied().map(Pip::Colored))
        || d.colors != [pips[0]]
        || d.mana_value != u16::from(generic) + pips.len() as u16
        || d.types != types
        || d.subtypes != subtypes
        || (d.power, d.toughness) != pt
        || d.keywords != keywords
        || d.target_spec != target
        || d.supertypes
            != if legendary {
                &[card_def::Supertype::Legendary][..]
            } else {
                &[][..]
            }
    {
        return false;
    }
    let effect = if family == FamilyV59::Goblin {
        let Some(token_def) = card_def::card_id_by_name("Goblin Token") else {
            return false;
        };
        EffectOp::DealXDamageThenCreateTokensForExcess {
            target_index: 0,
            token_def,
        }
    } else {
        EffectOp::MoveObject {
            object: ObjectRef::ThisSource,
            to_zone: Zone::Battlefield,
        }
    };
    if (d.spell_effect)() != Some(effect) {
        return false;
    }
    let triggers = crate::trigger::triggers_for(id);
    match family {
        FamilyV59::Replication | FamilyV59::Infernal => {
            let [t] = triggers else {
                return false;
            };
            t.home_zone == Zone::Battlefield
                && t.face_index == 0
                && !t.intervening_if_kicked
                && !t.intervening_if_controls_another_source_card
                && if family == FamilyV59::Replication {
                    t.condition
                        == crate::trigger::TriggerCondition::BeginningOfUpkeep {
                            controller_only: true,
                        }
                        && (t.effect)() == crate::extravagant_replication_v1::root()
                } else {
                    t.condition
                        == crate::trigger::TriggerCondition::DiesUnlessSubtype(Subtype::Demon)
                        && (t.effect)() == crate::infernal_vessel_v1::recipe()
                }
        }
        _ => triggers.is_empty(),
    }
}
fn validate_goblin(id: u16) -> Result<(), String> {
    let d = CARD_DEFS
        .get(usize::from(id))
        .ok_or("Goblin token absent")?;
    let expected = PrintedV59 {
        mana: ManaV59 {
            generic: 0,
            colored: vec![],
            x_multiplicity: 0,
        },
        mana_value: 0,
        types: vec![CardType::Creature],
        subtypes: vec![Subtype::Goblin],
        supertypes: vec![],
        colors: vec![ManaColor::R],
        power: Some(1),
        toughness: Some(1),
        keyword_bits: 0,
        is_token: true,
    };
    if d.name == "Goblin Token"
        && d.capability == CardCapability::Full
        && ordinary_foreign_fields_absent(d, false)
        && printed(d)? == expected
        && d.target_spec == TargetSpec::None
        && (d.spell_effect)().is_none()
        && crate::trigger::triggers_for(id).is_empty()
    {
        Ok(())
    } else {
        Err("Goblin full blueprint changed".into())
    }
}
