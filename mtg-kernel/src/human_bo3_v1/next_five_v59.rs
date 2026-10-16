//! Fixed-seat Human consumer for the sealed NextFive V59 owner.
//! Backend bindings, raw decisions and executable facts remain private.
use super::visible::{self, Handles};
use super::*;
use crate::{
    next_five_observation_v59 as observation, next_five_public_v59 as public,
    rl::{ActionSemanticV1, CardStableRefV1, RlContractError},
    rl_session::{
        FastActorDecisionV1, FastActorResponseV1, FastActorSessionV1, NextFiveDecisionV59,
        NextFiveSessionV59, NextFiveSnapshotV59,
    },
};
use std::sync::Arc;
type Error = HumanDecisionErrorV1;

macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $(pub $field: $ty),* }
    };
}

record!(HumanVisibleStateNextFiveV59 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: super::prepared_v57::HumanPreparedFactsV57,
    next_five: HumanNextFiveFactsV59
});
record!(HumanDecisionNextFiveV59 {
    schema: String,
    human_seat: PlayerSeatV1,
    prompt_seq: u64,
    state: HumanVisibleStateNextFiveV59,
    actions: Vec<HumanLegalActionV1>,
    action_meanings: Vec<Option<HumanNextFiveActivationV59>>
});
record!(HumanNextFiveActivationV59 {
    source: HumanCardRefV1,
    kind: crate::rules_meaning_drake_hatcher_v53::CounterV53,
    count: u16,
    before: i32,
    after: i32,
    any_priority: bool,
    costs_paid_before_stack: bool,
    countering_refunds_costs: bool
});
record!(HumanNextFiveRootV59 {
    original_source: HumanCardRefV1,
    current_original_source: Option<HumanCardRefV1>,
    captured_controller: PlayerSeatV1
});
record!(HumanNextFiveCurrentV59 {
    family: crate::rules_meaning_next_five_v59::FamilyV59,
    source: HumanCardRefV1,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
    counters: crate::optional_trigger_public_v1::CountersV1,
    form: crate::optional_trigger_public_v1::FormV1
});
record!(HumanNextFiveCastV59 {
    family: crate::rules_meaning_next_five_v59::FamilyV59,
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_copy: bool,
    payment_pending: bool,
    x: Option<u16>,
    targets: Vec<HumanNextFiveTargetV59>
});
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanNextFiveTargetV59 {
    Player(PlayerSeatV1),
    Object(HumanCardRefV1),
}
record!(HumanNextFiveTriggerV59 {
    family: crate::rules_meaning_next_five_v59::FamilyV59,
    occurrence: u32,
    source: HumanNextFiveRootV59,
    stage: public::TriggerStageV59,
    placement_ordered: bool,
    targets: Vec<HumanNextFiveTargetV59>
});
record!(HumanNextFiveAuraCopyV59 {
    source: HumanNextFiveRootV59,
    target: HumanNextFiveTargetV59,
    template_name: String,
    template_face: u8,
    chooser: PlayerSeatV1,
    expanded_count: u64,
    chosen_hosts: Vec<HumanCardRefV1>,
    candidate_hosts: Vec<HumanCardRefV1>,
    accepted_host_awaiting_advance: Option<HumanCardRefV1>,
    remaining_count: u64,
    untargeted: bool
});
record!(HumanNextFiveDeathV59 {
    occurrence: u32,
    source: HumanNextFiveRootV59,
    captured_not_demon: bool,
    collection_completed: bool,
    exact_graveyard_card_now: Option<HumanCardRefV1>,
    owner_on_return: PlayerSeatV1,
    return_incarnation_available_now: bool
});
record!(HumanNextFiveAngelReplacementV59 {
    source: HumanCardRefV1,
    controller: PlayerSeatV1,
    current_angels: Vec<HumanCardRefV1>,
    original_angel_count_now: u32,
    self_entry_excluded: bool
});
record!(HumanNextFiveRestrictedManaV59 {
    color: crate::mana::ManaColor,
    only_cast_angel_spell: bool,
    current_original_source: Option<HumanCardRefV1>
});
record!(HumanNextFiveGoblinQuoteV59 {
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    original_x: u16,
    target: HumanNextFiveTargetV59,
    target_legal_now: bool,
    creature_lethal_damage_now: Option<i64>,
    quote_can_change_before_resolution: bool
});
record!(HumanNextFiveGoblinOrderingV59 {
    source: HumanCardRefV1,
    target: HumanCardRefV1,
    controller: PlayerSeatV1,
    chooser: PlayerSeatV1,
    original_x: u16,
    lethal_damage: i64,
    remaining_damage: i32
});
record!(HumanNextFiveGoblinCompletedV59 {
    source: HumanCardRefV1,
    target: HumanCardRefV1,
    controller: PlayerSeatV1,
    original_x: u16,
    lethal_damage: i64,
    actual_damage: i32,
    excess: u32,
    tokens_created: u64
});
record!(HumanNextFiveSnackCastV59 {
    source: HumanCardRefV1,
    original_controller: PlayerSeatV1,
    is_copy: bool,
    payment_pending: bool
});
record!(HumanNextFiveSnackRaidV59 {
    source: HumanNextFiveRootV59,
    placement_ordered: bool,
    on_stack: bool
});
record!(HumanNextFiveSnackActivationV59 {
    source: HumanNextFiveRootV59,
    stage: crate::midnight_snack_public_v58::ActivationStageV58,
    target: Option<PlayerSeatV1>,
    target_legal_now: Option<bool>,
    original_controller_gained_this_turn_now: i32
});
record!(HumanNextFiveSnackMeaningV59 {
    printed: crate::rules_meaning_midnight_snack_v58::PrintedV58,
    ordered_rules: Vec<crate::rules_meaning_midnight_snack_v58::RuleV58>,
    food: HumanNextFiveSnackFoodV59
});
record!(HumanNextFiveSnackFoodV59 {
    printed: crate::rules_meaning_midnight_snack_v58::PrintedV58,
    activation: crate::rules_meaning_midnight_snack_v58::OperationV58
});
record!(HumanNextFiveSnackFactsV59 {
    viewer: PlayerSeatV1,
    meaning: HumanNextFiveSnackMeaningV59,
    attacked_this_turn: [bool; 2],
    gained_this_turn: [i32; 2],
    authorized_printings: Vec<HumanCardRefV1>,
    current: Vec<HumanNextFiveCurrentV59>,
    current_food: Vec<HumanNextFiveCurrentV59>,
    casts: Vec<HumanNextFiveSnackCastV59>,
    held_raid: Vec<HumanNextFiveSnackRaidV59>,
    published_raid: Vec<HumanNextFiveSnackRaidV59>,
    activations: Vec<HumanNextFiveSnackActivationV59>
});
record!(HumanNextFiveMeaningV59 {
    family: crate::rules_meaning_next_five_v59::FamilyV59,
    printed: crate::rules_meaning_next_five_v59::PrintedV59,
    ordered_rules: Vec<HumanNextFiveRuleV59>
});
record!(HumanNextFiveTokenV59 {
    printed: crate::rules_meaning_next_five_v59::PrintedV59
});
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanNextFiveInstructionV59 {
    PayPrintedMana,
    MoveOriginalSpellToBattlefieldUnderCaster,
    DealChosenXToOriginalCreature {
        actual_committed_damage_after_replacement_and_prevention: bool,
    },
    CreateExcessGoblins {
        token: HumanNextFiveTokenV59,
        controller: crate::rules_meaning_next_five_v59::ActorV59,
        lethal_uses_predamage_effective_toughness_less_marked_damage: bool,
        source_deathtouch_lethal_one: bool,
        no_sba_between_damage_and_tokens: bool,
        one_complete_replacement_batch: bool,
    },
    CopyOriginalTarget {
        controller: crate::rules_meaning_next_five_v59::ActorV59,
        current_copiable_face: bool,
        face_down_uses_anonymous_characteristics: bool,
        copy_counters_and_noncopy_effects: bool,
        apply_token_replacements_once: bool,
        untargeted_aura_host_before_entry_observers: bool,
        no_legal_host_creates_no_aura: bool,
        one_choice_per_expanded_aura: bool,
    },
    ReturnOriginalGraveyardIncarnation {
        controller: crate::rules_meaning_next_five_v59::ActorV59,
        token_cannot_return: bool,
        requires_exact_successor_generation: bool,
        plus_one_counters: u32,
        added_subtype: crate::card_def::Subtype,
        entry_characteristics_before_observers: bool,
        entry_counter_replacements_apply: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanNextFiveRuleV59 {
    Snack {
        meaning: Box<HumanNextFiveSnackMeaningV59>,
    },
    Cast {
        timing: crate::rules_meaning_next_five_v59::TimingV59,
        origin: crate::state::Zone,
        targets: crate::rules_meaning_next_five_v59::TargetV59,
        ordered: Vec<HumanNextFiveInstructionV59>,
        failed_or_countered_spell_to_owner_graveyard: bool,
        ordinary_zone_replacements_apply: bool,
    },
    Trigger {
        event: crate::rules_meaning_next_five_v59::EventV59,
        actor: crate::rules_meaning_next_five_v59::ActorV59,
        home: crate::state::Zone,
        printed_ability_active_at_event: bool,
        targets: crate::rules_meaning_next_five_v59::TargetV59,
        ordered: Vec<HumanNextFiveInstructionV59>,
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
        color: crate::mana::ManaColor,
        spell_subtype_only: crate::card_def::Subtype,
        creature_tap_sickness_rules_apply: bool,
        not_for_activations: bool,
        restriction_survives_source_departure: bool,
    },
}
record!(HumanNextFiveGoblinOptionV59 {
    kind: String,
    source: HumanCardRefV1,
    shield_remaining: Option<i32>
});
record!(HumanNextFiveFactsV59 {
    viewer: PlayerSeatV1,
    meanings: Vec<HumanNextFiveMeaningV59>,
    snack: HumanNextFiveSnackFactsV59,
    authorized: Vec<(crate::rules_meaning_next_five_v59::FamilyV59, HumanCardRefV1)>,
    current: Vec<HumanNextFiveCurrentV59>,
    casts: Vec<HumanNextFiveCastV59>,
    triggers: Vec<HumanNextFiveTriggerV59>,
    aura_copy: Option<HumanNextFiveAuraCopyV59>,
    infernal_deaths: Vec<HumanNextFiveDeathV59>,
    angel_replacements: Vec<HumanNextFiveAngelReplacementV59>,
    angel_mana: [Vec<HumanNextFiveRestrictedManaV59>; 2],
    goblin_quotes: Vec<HumanNextFiveGoblinQuoteV59>,
    goblin_ordering: Option<HumanNextFiveGoblinOrderingV59>,
    goblin_ordering_options: Vec<HumanNextFiveGoblinOptionV59>,
    goblin_completed: Vec<HumanNextFiveGoblinCompletedV59>
});

fn stable(
    reference: &crate::optional_trigger_public_v1::ReferenceV1,
) -> Result<CardStableRefV1, Error> {
    use crate::optional_trigger_public_v1::IdentityV1;
    let card_db_id = match &reference.identity {
        IdentityV1::Known { card_id } => Some(*card_id),
        IdentityV1::FaceDown { disguised: true } => {
            crate::card_def::card_id_by_name("Face-down creature")
        }
        IdentityV1::FaceDown { disguised: false } => {
            crate::card_def::card_id_by_name("Face-down card")
        }
    }
    .ok_or(Error::InvalidVisibleReference)?;
    Ok(CardStableRefV1 {
        arena_id: reference.object,
        card_db_id,
        owner: reference.owner,
        controller: reference.controller,
        zone: reference.zone,
        zone_change_count: reference.generation,
    })
}
fn href(
    reference: &crate::optional_trigger_public_v1::ReferenceV1,
    handles: &mut Handles,
) -> Result<HumanCardRefV1, Error> {
    handles.insert(&stable(reference)?)
}
fn stable_href(
    reference: &CardStableRefV1,
    handles: &mut Handles,
) -> Result<HumanCardRefV1, Error> {
    handles.insert(reference)
}
fn root(
    value: &crate::optional_trigger_public_v1::RootV1,
    handles: &mut Handles,
) -> Result<HumanNextFiveRootV59, Error> {
    Ok(HumanNextFiveRootV59 {
        original_source: href(&value.original_source, handles)?,
        current_original_source: value
            .current_original_source
            .as_ref()
            .map(|r| href(r, handles))
            .transpose()?,
        captured_controller: value.captured_controller,
    })
}
fn target(
    value: &public::TargetV59,
    handles: &mut Handles,
) -> Result<HumanNextFiveTargetV59, Error> {
    Ok(match value {
        public::TargetV59::Player(player) => HumanNextFiveTargetV59::Player(*player),
        public::TargetV59::Object(reference) => {
            HumanNextFiveTargetV59::Object(href(reference, handles)?)
        }
    })
}
fn current(
    family: crate::rules_meaning_next_five_v59::FamilyV59,
    value: &crate::optional_trigger_public_v1::CurrentV1,
    handles: &mut Handles,
) -> Result<HumanNextFiveCurrentV59, Error> {
    Ok(HumanNextFiveCurrentV59 {
        family,
        source: href(&value.source, handles)?,
        is_token: value.is_token,
        tapped: value.tapped,
        summoning_sick: value.summoning_sick,
        marked_damage: value.marked_damage,
        counters: value.counters.clone(),
        form: value.form.clone(),
    })
}
fn snack_current(
    value: &crate::optional_trigger_public_v1::CurrentV1,
    handles: &mut Handles,
) -> Result<HumanNextFiveCurrentV59, Error> {
    current(
        crate::rules_meaning_next_five_v59::FamilyV59::Snack,
        value,
        handles,
    )
}

fn instruction(
    value: &crate::rules_meaning_next_five_v59::InstructionV59,
) -> HumanNextFiveInstructionV59 {
    use crate::rules_meaning_next_five_v59::InstructionV59 as I;
    match value {
        I::PayPrintedMana => HumanNextFiveInstructionV59::PayPrintedMana,
        I::MoveOriginalSpellToBattlefieldUnderCaster => {
            HumanNextFiveInstructionV59::MoveOriginalSpellToBattlefieldUnderCaster
        }
        I::DealChosenXToOriginalCreature {
            actual_committed_damage_after_replacement_and_prevention,
        } => HumanNextFiveInstructionV59::DealChosenXToOriginalCreature {
            actual_committed_damage_after_replacement_and_prevention:
                *actual_committed_damage_after_replacement_and_prevention,
        },
        I::CreateExcessGoblins {
            token,
            controller,
            lethal_uses_predamage_effective_toughness_less_marked_damage,
            source_deathtouch_lethal_one,
            no_sba_between_damage_and_tokens,
            one_complete_replacement_batch,
        } => HumanNextFiveInstructionV59::CreateExcessGoblins {
            token: HumanNextFiveTokenV59 {
                printed: token.printed.clone(),
            },
            controller: *controller,
            lethal_uses_predamage_effective_toughness_less_marked_damage:
                *lethal_uses_predamage_effective_toughness_less_marked_damage,
            source_deathtouch_lethal_one: *source_deathtouch_lethal_one,
            no_sba_between_damage_and_tokens: *no_sba_between_damage_and_tokens,
            one_complete_replacement_batch: *one_complete_replacement_batch,
        },
        I::CopyOriginalTarget {
            controller,
            current_copiable_face,
            face_down_uses_anonymous_characteristics,
            copy_counters_and_noncopy_effects,
            apply_token_replacements_once,
            untargeted_aura_host_before_entry_observers,
            no_legal_host_creates_no_aura,
            one_choice_per_expanded_aura,
        } => HumanNextFiveInstructionV59::CopyOriginalTarget {
            controller: *controller,
            current_copiable_face: *current_copiable_face,
            face_down_uses_anonymous_characteristics: *face_down_uses_anonymous_characteristics,
            copy_counters_and_noncopy_effects: *copy_counters_and_noncopy_effects,
            apply_token_replacements_once: *apply_token_replacements_once,
            untargeted_aura_host_before_entry_observers:
                *untargeted_aura_host_before_entry_observers,
            no_legal_host_creates_no_aura: *no_legal_host_creates_no_aura,
            one_choice_per_expanded_aura: *one_choice_per_expanded_aura,
        },
        I::ReturnOriginalGraveyardIncarnation {
            controller,
            token_cannot_return,
            requires_exact_successor_generation,
            plus_one_counters,
            added_subtype,
            entry_characteristics_before_observers,
            entry_counter_replacements_apply,
        } => HumanNextFiveInstructionV59::ReturnOriginalGraveyardIncarnation {
            controller: *controller,
            token_cannot_return: *token_cannot_return,
            requires_exact_successor_generation: *requires_exact_successor_generation,
            plus_one_counters: *plus_one_counters,
            added_subtype: *added_subtype,
            entry_characteristics_before_observers: *entry_characteristics_before_observers,
            entry_counter_replacements_apply: *entry_counter_replacements_apply,
        },
    }
}

fn project_facts(
    value: &public::FactsV59,
    handles: &mut Handles,
) -> Result<HumanNextFiveFactsV59, Error> {
    use crate::rules_meaning_next_five_v59::FamilyV59;
    let snack = &value.snack;
    let snack = HumanNextFiveSnackFactsV59 {
        viewer: snack.viewer,
        meaning: HumanNextFiveSnackMeaningV59 {
            printed: snack.meaning.printed.clone(),
            ordered_rules: snack.meaning.ordered_rules.clone(),
            food: HumanNextFiveSnackFoodV59 {
                printed: snack.meaning.food.printed.clone(),
                activation: snack.meaning.food.activation.clone(),
            },
        },
        attacked_this_turn: snack.attacked_this_turn,
        gained_this_turn: snack.gained_this_turn,
        authorized_printings: snack
            .authorized_printings
            .iter()
            .map(|r| href(r, handles))
            .collect::<Result<_, _>>()?,
        current: snack
            .current
            .iter()
            .map(|r| snack_current(r, handles))
            .collect::<Result<_, _>>()?,
        current_food: snack
            .current_food
            .iter()
            .map(|r| snack_current(r, handles))
            .collect::<Result<_, _>>()?,
        casts: snack
            .casts
            .iter()
            .map(|r| {
                Ok(HumanNextFiveSnackCastV59 {
                    source: href(&r.source, handles)?,
                    original_controller: r.original_controller,
                    is_copy: r.is_copy,
                    payment_pending: r.payment_pending,
                })
            })
            .collect::<Result<_, Error>>()?,
        held_raid: snack
            .held_raid
            .iter()
            .map(|r| {
                Ok(HumanNextFiveSnackRaidV59 {
                    source: root(&r.root, handles)?,
                    placement_ordered: r.placement_ordered,
                    on_stack: r.on_stack,
                })
            })
            .collect::<Result<_, Error>>()?,
        published_raid: snack
            .published_raid
            .iter()
            .map(|r| {
                Ok(HumanNextFiveSnackRaidV59 {
                    source: root(&r.root, handles)?,
                    placement_ordered: r.placement_ordered,
                    on_stack: r.on_stack,
                })
            })
            .collect::<Result<_, Error>>()?,
        activations: snack
            .activations
            .iter()
            .map(|r| {
                Ok(HumanNextFiveSnackActivationV59 {
                    source: root(&r.root, handles)?,
                    stage: r.stage,
                    target: r.target,
                    target_legal_now: r.target_legal_now,
                    original_controller_gained_this_turn_now: r
                        .original_controller_gained_this_turn_now,
                })
            })
            .collect::<Result<_, Error>>()?,
    };
    let authorized = value
        .authorized
        .iter()
        .map(|r| Ok((r.family, href(&r.card, handles)?)))
        .collect::<Result<_, Error>>()?;
    let current = value
        .current
        .iter()
        .map(|r| current(r.family, &r.card, handles))
        .collect::<Result<_, Error>>()?;
    let casts = value
        .casts
        .iter()
        .map(|r| {
            Ok(HumanNextFiveCastV59 {
                family: r.family,
                source: href(&r.source, handles)?,
                original_controller: r.original_controller,
                is_copy: r.is_copy,
                payment_pending: r.payment_pending,
                x: r.x,
                targets: r
                    .targets
                    .iter()
                    .map(|t| target(t, handles))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let triggers = value
        .triggers
        .iter()
        .map(|r| {
            Ok(HumanNextFiveTriggerV59 {
                family: r.family,
                occurrence: r.occurrence,
                source: root(&r.source, handles)?,
                stage: r.stage,
                placement_ordered: r.placement_ordered,
                targets: r
                    .targets
                    .iter()
                    .map(|t| target(t, handles))
                    .collect::<Result<_, _>>()?,
            })
        })
        .collect::<Result<_, Error>>()?;
    let aura_copy = value
        .aura_copy
        .as_ref()
        .map(|r| {
            Ok(HumanNextFiveAuraCopyV59 {
                source: root(&r.source, handles)?,
                target: target(&r.target, handles)?,
                template_name: crate::card_def::CARD_DEFS
                    .get(usize::from(r.template_card_id))
                    .map(|def| def.name.to_string())
                    .ok_or(Error::InvalidVisibleReference)?,
                template_face: r.template_face,
                chooser: r.chooser,
                expanded_count: r.expanded_count,
                chosen_hosts: r
                    .chosen_hosts
                    .iter()
                    .map(|r| href(r, handles))
                    .collect::<Result<_, _>>()?,
                candidate_hosts: r
                    .candidate_hosts
                    .iter()
                    .map(|r| href(r, handles))
                    .collect::<Result<_, _>>()?,
                accepted_host_awaiting_advance: r
                    .accepted_host_awaiting_advance
                    .as_ref()
                    .map(|r| href(r, handles))
                    .transpose()?,
                remaining_count: r.remaining_count,
                untargeted: r.untargeted,
            })
        })
        .transpose()?;
    let infernal_deaths = value
        .infernal_deaths
        .iter()
        .map(|r| {
            Ok(HumanNextFiveDeathV59 {
                occurrence: r.occurrence,
                source: root(&r.source, handles)?,
                captured_not_demon: r.captured_not_demon,
                collection_completed: r.collection_completed,
                exact_graveyard_card_now: r
                    .exact_graveyard_card_now
                    .as_ref()
                    .map(|r| href(r, handles))
                    .transpose()?,
                owner_on_return: r.owner_on_return,
                return_incarnation_available_now: r.return_incarnation_available_now,
            })
        })
        .collect::<Result<_, Error>>()?;
    let angel_replacements = value
        .angel_replacements
        .iter()
        .map(|r| {
            Ok(HumanNextFiveAngelReplacementV59 {
                source: href(&r.source, handles)?,
                controller: r.controller,
                current_angels: r
                    .current_angels
                    .iter()
                    .map(|r| href(r, handles))
                    .collect::<Result<_, _>>()?,
                original_angel_count_now: r.original_angel_count_now,
                self_entry_excluded: r.self_entry_excluded,
            })
        })
        .collect::<Result<_, Error>>()?;
    let angel_mana = value.angel_mana.each_ref().map(|rows| {
        rows.iter()
            .map(|r| {
                Ok(HumanNextFiveRestrictedManaV59 {
                    color: r.color,
                    only_cast_angel_spell: r.only_cast_angel_spell,
                    current_original_source: r
                        .current_original_source
                        .as_ref()
                        .map(|r| href(r, handles))
                        .transpose()?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()
    });
    let [angel_left, angel_right] = angel_mana;
    let angel_mana = [angel_left?, angel_right?];
    let goblin_quotes = value
        .goblin_quotes
        .iter()
        .map(|r| {
            Ok(HumanNextFiveGoblinQuoteV59 {
                source: href(&r.source, handles)?,
                original_controller: r.original_controller,
                original_x: r.original_x,
                target: target(&r.target, handles)?,
                target_legal_now: r.target_legal_now,
                creature_lethal_damage_now: r.creature_lethal_damage_now,
                quote_can_change_before_resolution: r.quote_can_change_before_resolution,
            })
        })
        .collect::<Result<_, Error>>()?;
    let goblin_ordering = value
        .goblin_ordering
        .as_ref()
        .map(|r| {
            Ok(HumanNextFiveGoblinOrderingV59 {
                source: stable_href(&r.source, handles)?,
                target: stable_href(&r.target, handles)?,
                controller: r.controller,
                chooser: r.chooser,
                original_x: r.original_x,
                lethal_damage: r.lethal_damage,
                remaining_damage: r.remaining_damage,
            })
        })
        .transpose()?;
    let goblin_completed = value
        .goblin_completed
        .iter()
        .map(|r| {
            Ok(HumanNextFiveGoblinCompletedV59 {
                source: stable_href(&r.source, handles)?,
                target: stable_href(&r.target, handles)?,
                controller: r.controller,
                original_x: r.original_x,
                lethal_damage: r.lethal_damage,
                actual_damage: r.actual_damage,
                excess: r.excess,
                tokens_created: r.tokens_created,
            })
        })
        .collect::<Result<_, Error>>()?;
    let meanings = value
        .meanings
        .iter()
        .map(|row| {
            Ok(HumanNextFiveMeaningV59 {
                family: row.family,
                printed: row.printed.clone(),
                ordered_rules: row
                    .ordered_rules
                    .iter()
                    .map(|rule| match rule {
                        crate::rules_meaning_next_five_v59::RuleV59::Snack { meaning } => {
                            Ok(HumanNextFiveRuleV59::Snack {
                                meaning: Box::new(HumanNextFiveSnackMeaningV59 {
                                    printed: meaning.printed.clone(),
                                    ordered_rules: meaning.ordered_rules.clone(),
                                    food: HumanNextFiveSnackFoodV59 {
                                        printed: meaning.food.printed.clone(),
                                        activation: meaning.food.activation.clone(),
                                    },
                                }),
                            })
                        }
                        crate::rules_meaning_next_five_v59::RuleV59::Cast {
                            timing,
                            origin,
                            targets,
                            ordered,
                            failed_or_countered_spell_to_owner_graveyard,
                            ordinary_zone_replacements_apply,
                        } => Ok(HumanNextFiveRuleV59::Cast {
                            timing: *timing,
                            origin: *origin,
                            targets: *targets,
                            ordered: ordered.iter().map(instruction).collect(),
                            failed_or_countered_spell_to_owner_graveyard:
                                *failed_or_countered_spell_to_owner_graveyard,
                            ordinary_zone_replacements_apply: *ordinary_zone_replacements_apply,
                        }),
                        crate::rules_meaning_next_five_v59::RuleV59::Trigger {
                            event,
                            actor,
                            home,
                            printed_ability_active_at_event,
                            targets,
                            ordered,
                        } => Ok(HumanNextFiveRuleV59::Trigger {
                            event: *event,
                            actor: *actor,
                            home: *home,
                            printed_ability_active_at_event: *printed_ability_active_at_event,
                            targets: *targets,
                            ordered: ordered.iter().map(instruction).collect(),
                        }),
                        crate::rules_meaning_next_five_v59::RuleV59::AngelEntry {
                            source_must_be_active_before_batch,
                            other_controlled_angel,
                            count_already_controlled_angels_before_entry,
                            exclude_entrant_from_count,
                            one_application_per_original_source,
                            counters_exist_before_observers,
                            counter_replacements_apply,
                        } => Ok(HumanNextFiveRuleV59::AngelEntry {
                            source_must_be_active_before_batch: *source_must_be_active_before_batch,
                            other_controlled_angel: *other_controlled_angel,
                            count_already_controlled_angels_before_entry:
                                *count_already_controlled_angels_before_entry,
                            exclude_entrant_from_count: *exclude_entrant_from_count,
                            one_application_per_original_source:
                                *one_application_per_original_source,
                            counters_exist_before_observers: *counters_exist_before_observers,
                            counter_replacements_apply: *counter_replacements_apply,
                        }),
                        crate::rules_meaning_next_five_v59::RuleV59::RestrictedMana {
                            tap_original_source,
                            amount,
                            color,
                            spell_subtype_only,
                            creature_tap_sickness_rules_apply,
                            not_for_activations,
                            restriction_survives_source_departure,
                        } => Ok(HumanNextFiveRuleV59::RestrictedMana {
                            tap_original_source: *tap_original_source,
                            amount: *amount,
                            color: *color,
                            spell_subtype_only: *spell_subtype_only,
                            creature_tap_sickness_rules_apply: *creature_tap_sickness_rules_apply,
                            not_for_activations: *not_for_activations,
                            restriction_survives_source_departure:
                                *restriction_survives_source_departure,
                        }),
                    })
                    .collect::<Result<Vec<_>, Error>>()?,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let goblin_ordering_options = value
        .goblin_ordering_options
        .iter()
        .map(|row| {
            Ok(HumanNextFiveGoblinOptionV59 {
                kind: row.kind.clone(),
                source: stable_href(&row.source, handles)?,
                shield_remaining: row.shield_remaining,
            })
        })
        .collect::<Result<_, Error>>()?;
    Ok(HumanNextFiveFactsV59 {
        viewer: value.viewer,
        meanings,
        snack,
        authorized,
        current,
        casts,
        triggers,
        aura_copy,
        infernal_deaths,
        angel_replacements,
        angel_mana,
        goblin_quotes,
        goblin_ordering,
        goblin_ordering_options,
        goblin_completed,
    })
}

pub(super) fn register(
    value: &public::FactsV59,
    handles: &mut Handles,
) -> Result<HumanNextFiveFactsV59, Error> {
    project_facts(value, handles)
}

pub(super) fn project(
    decision: &NextFiveDecisionV59,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionNextFiveV59, Vec<u32>), Error> {
    let observation = decision.observation();
    if observation.next_five.viewer != human || observation.common.common.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    crate::next_five_observation_v59::validate_artifact(observation)
        .map_err(|_| Error::InvalidVisibleReference)?;
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_next_five_base_with_handles(
            observation,
            decision.actions(),
            human,
            decision.family_projections_v59(),
        )?;
    let prepared = super::prepared_v57::facts_parts(
        &observation.drake,
        &observation.spinner,
        &observation.quandary,
        &observation.phoenix,
        &mut handles,
        false,
    )?;
    let next_five = project_facts(&observation.next_five, &mut handles)?;
    let action_meanings = indexes
        .iter()
        .map(|index| {
            observation.action_meanings[*index as usize]
                .as_ref()
                .map(|row| {
                    Ok(HumanNextFiveActivationV59 {
                        source: handles.get(&row.source)?.clone(),
                        kind: row.kind,
                        count: row.count,
                        before: row.before,
                        after: row.after,
                        any_priority: row.any_priority,
                        costs_paid_before_stack: row.costs_paid_before_stack,
                        countering_refunds_costs: row.countering_refunds_costs,
                    })
                })
                .transpose()
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok((
        HumanDecisionNextFiveV59 {
            schema: "mtg-kernel-human-next-five-decision/v59".into(),
            human_seat: human,
            prompt_seq,
            state: HumanVisibleStateNextFiveV59 {
                common,
                prepared,
                next_five,
            },
            actions,
            action_meanings,
        },
        indexes,
    ))
}

#[derive(Clone)]
struct AuthorityV59 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV59 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV59 {}

pub struct HumanNextFiveSessionV59 {
    inner: NextFiveSessionV59,
    authority: AuthorityV59,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanNextFiveSnapshotV59 {
    inner: NextFiveSnapshotV59,
    owner: Arc<()>,
}
impl HumanNextFiveSessionV59 {
    pub fn from_session(
        inner: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: NextFiveSessionV59::from_session(inner)?,
            human_seat,
            authority: AuthorityV59 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn reset(
        episode_id: u64,
        env_seed: u64,
        limits: (u64, u64),
        deck_ids: crate::rl_session::SessionDeckIdsV1,
        mainboards: [Vec<u16>; 2],
        starting_player: crate::ids::PlayerId,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl_session::RlSessionError> {
        let inner = NextFiveSessionV59::reset(
            episode_id,
            env_seed,
            limits,
            deck_ids,
            mainboards,
            starting_player,
        )?;
        Ok(Self {
            inner,
            human_seat,
            authority: AuthorityV59 {
                owner: Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v59(&self) -> HumanNextFiveSnapshotV59 {
        HumanNextFiveSnapshotV59 {
            inner: self.inner.snapshot_v59(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v59(&mut self, snapshot: &HumanNextFiveSnapshotV59) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v59(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<NextFiveDecisionV59, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = self
            .inner
            .decision_v59()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        self.inner
            .validate_decision_v59(&decision)
            .map_err(|_| Error::StaleDecision)?;
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<observation::NextFiveObservationV59, Error> {
        Ok(self.decision(expected)?.observation().clone())
    }
    /// Host-only opponent callback. It receives the actor-authorized public
    /// observation and legal actions, not a Human DTO or opponent private data.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, RlContractError>
    where
        F: FnMut(
            &observation::NextFiveObservationV59,
            &[ActionSemanticV1],
        ) -> Result<u32, RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(RlContractError(
                "opponent callback cannot act for Human seat".into(),
            ));
        }
        let decision = self
            .inner
            .decision_v59()
            .map_err(|_| RlContractError("stale NextFive opponent decision".into()))?;
        if decision.expected() != expected {
            return Err(RlContractError("stale NextFive opponent decision".into()));
        }
        self.inner.validate_decision_v59(&decision)?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v59(&decision, index)
    }
}

struct BoundNextFiveV59 {
    authority: AuthorityV59,
    decision: NextFiveDecisionV59,
    visible: HumanDecisionNextFiveV59,
    indexes: Vec<u32>,
}
pub struct HumanNextFiveProjectorV59 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundNextFiveV59>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV59)>,
}
impl HumanNextFiveProjectorV59 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }
    pub fn project_current(
        &mut self,
        session: &HumanNextFiveSessionV59,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionNextFiveV59, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v59(&pending.decision)
                    .is_ok()
            {
                let (visible, indexes) =
                    project(&decision, self.human_seat, pending.visible.prompt_seq)?;
                if visible == pending.visible && indexes == pending.indexes {
                    return Ok(visible);
                }
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (visible, indexes) = project(&decision, self.human_seat, self.next_prompt_seq)?;
        self.pending = Some(BoundNextFiveV59 {
            authority: session.authority.clone(),
            decision,
            visible: visible.clone(),
            indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanNextFiveSessionV59,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
        if session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        if let Some((receipt, authority)) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                if *authority != session.authority {
                    return Err(Error::StaleDecision);
                }
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(Error::ConflictingRetry)
                };
            }
        }
        let pending = self.pending.as_ref().ok_or(Error::StaleDecision)?;
        if pending.authority != session.authority
            || request.prompt_seq != pending.visible.prompt_seq
        {
            return Err(Error::StaleDecision);
        }
        if request.action_index as usize >= pending.visible.actions.len() {
            return Err(Error::InvalidAction);
        }
        session
            .inner
            .validate_decision_v59(&pending.decision)
            .map_err(|_| Error::StaleDecision)?;
        let engine_index = *pending
            .indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .step_v59(&pending.decision, engine_index)
            .map_err(|_| Error::ExecutionFailed)?;
        self.pending = None;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}
