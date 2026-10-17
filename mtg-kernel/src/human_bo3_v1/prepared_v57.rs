//! Combined fixed-seat Prepared V57 Human view over the sealed owning backend.
use super::visible::{self, Handles};
use super::*;
use crate::rl::{ActionSemanticV1, CardStableRefV1, RlContractError};
use crate::rl_session::{
    FastActorResponseV1, PreparedDecisionV57, PreparedSessionV57, PreparedSnapshotV57,
};
use crate::{
    drake_public_v53 as drake, optional_trigger_public_v1 as optional,
    phoenix_public_v56 as phoenix, prepared_observation_v57 as observation,
    rules_meaning_drake_hatcher_v53 as dm, rules_meaning_phoenix_v56 as pm,
    rules_meaning_quandary_v55 as qm, rules_meaning_spinner_v54 as sm,
};
use std::{collections::BTreeMap, sync::Arc};
type Error = HumanDecisionErrorV1;

macro_rules! record {
    ($name:ident { $($field:ident : $ty:ty),* $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        pub struct $name { $(pub $field: $ty),* }
    };
}
record!(HumanDrakeMeaningV57 {
    root: dm::RootV53,
    printed: dm::PrintedV53,
    program: HumanDrakeProgramV57
});
record!(HumanDrakeTokenBlueprintV57 {
    power:i32, toughness:i32, colors:Vec<crate::mana::ManaColor>, types:Vec<crate::card_def::CardType>,
    subtypes:Vec<crate::card_def::Subtype>, keyword_bits:u32
});
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanDrakeInstructionV57 {
    BoostUntilEndOfTurn {
        source: dm::SourceV53,
        power: i32,
        toughness: i32,
    },
    PlaceCounters {
        source: dm::SourceV53,
        kind: dm::CounterV53,
        amount: dm::AmountV53,
        replacements_from_resolution_world: bool,
    },
    CreateToken {
        controller: dm::ActorV53,
        count: u32,
        token: HumanDrakeTokenBlueprintV57,
        independent_of_source_survival: bool,
    },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanDrakeProgramV57 {
    Triggered {
        event: dm::EventV53,
        controller: dm::ActorV53,
        printed_ability_required_at_event: bool,
        untargeted: bool,
        original_source_incarnation_required_for_application: bool,
        ordered_instructions: Vec<HumanDrakeInstructionV57>,
    },
    Activated {
        activation_zone: crate::state::Zone,
        source: dm::SourceV53,
        controller: dm::ActorV53,
        any_priority_timing: bool,
        untargeted: bool,
        ordered_costs: Vec<dm::CostV53>,
        costs_paid_before_stack: bool,
        countering_refunds_costs: bool,
        ordered_instructions: Vec<HumanDrakeInstructionV57>,
    },
}
fn drake_program(program: &dm::ProgramV53) -> HumanDrakeProgramV57 {
    fn instructions(rows: &[dm::InstructionV53]) -> Vec<HumanDrakeInstructionV57> {
        rows.iter()
            .map(|row| match row {
                dm::InstructionV53::BoostUntilEndOfTurn {
                    source,
                    power,
                    toughness,
                } => HumanDrakeInstructionV57::BoostUntilEndOfTurn {
                    source: *source,
                    power: *power,
                    toughness: *toughness,
                },
                dm::InstructionV53::PlaceCounters {
                    source,
                    kind,
                    amount,
                    replacements_from_resolution_world,
                } => HumanDrakeInstructionV57::PlaceCounters {
                    source: *source,
                    kind: *kind,
                    amount: *amount,
                    replacements_from_resolution_world: *replacements_from_resolution_world,
                },
                dm::InstructionV53::CreateToken {
                    controller,
                    count,
                    token,
                    independent_of_source_survival,
                } => HumanDrakeInstructionV57::CreateToken {
                    controller: *controller,
                    count: *count,
                    token: HumanDrakeTokenBlueprintV57 {
                        power: token.power,
                        toughness: token.toughness,
                        colors: token.colors.clone(),
                        types: token.types.clone(),
                        subtypes: token.subtypes.clone(),
                        keyword_bits: token.keyword_bits,
                    },
                    independent_of_source_survival: *independent_of_source_survival,
                },
            })
            .collect()
    }
    match program {
        dm::ProgramV53::Triggered {
            event,
            controller,
            printed_ability_required_at_event,
            untargeted,
            original_source_incarnation_required_for_application,
            ordered_instructions,
        } => HumanDrakeProgramV57::Triggered {
            event: *event,
            controller: *controller,
            printed_ability_required_at_event: *printed_ability_required_at_event,
            untargeted: *untargeted,
            original_source_incarnation_required_for_application:
                *original_source_incarnation_required_for_application,
            ordered_instructions: instructions(ordered_instructions),
        },
        dm::ProgramV53::Activated {
            activation_zone,
            source,
            controller,
            any_priority_timing,
            untargeted,
            ordered_costs,
            costs_paid_before_stack,
            countering_refunds_costs,
            ordered_instructions,
        } => HumanDrakeProgramV57::Activated {
            activation_zone: *activation_zone,
            source: *source,
            controller: *controller,
            any_priority_timing: *any_priority_timing,
            untargeted: *untargeted,
            ordered_costs: ordered_costs.clone(),
            costs_paid_before_stack: *costs_paid_before_stack,
            countering_refunds_costs: *countering_refunds_costs,
            ordered_instructions: instructions(ordered_instructions),
        },
    }
}
record!(HumanPreparedCardV57 {
    source: HumanCardRefV1, face_index: Option<u8>, is_token: bool, tapped: bool,
    summoning_sick: bool, marked_damage: u32, counters: drake::CountersV53,
    form: drake::FormV53, temporary_boosts: Vec<drake::TemporaryBoostV53>,
    attachments: Vec<HumanCardRefV1>
});
record!(HumanDrakeCombatV57 {
    occurrence: usize, original_source: HumanCardRefV1, current_original_source: Option<HumanCardRefV1>,
    original_controller: PlayerSeatV1, damaged_player: PlayerSeatV1,
    final_positive_damage: i32, outcome: drake::CombatOutcomeV53
});
record!(HumanDrakeTokenV57 {
    created: HumanCardRefV1, current_original_token: Option<HumanCardRefV1>, original_controller: PlayerSeatV1
});
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanDrakeActivationOutcomeV57 {
    Published,
    Countered,
    Resolved { tokens: Vec<HumanDrakeTokenV57> },
}
record!(HumanDrakeActivationV57 {
    occurrence: usize, original_source: HumanCardRefV1, current_original_source: Option<HumanCardRefV1>,
    original_controller: PlayerSeatV1, payment: drake::PaymentV53, outcome: HumanDrakeActivationOutcomeV57
});
record!(HumanDrakeFactsV57 {
    meanings: Vec<HumanDrakeMeaningV57>,
    // Mandatory complete exact roster for every battlefield object. The common
    // predecessor's narrow slots are compatibility fields, not complete values.
    battlefield: [Vec<HumanPreparedCardV57>; 2],
    combat: Vec<HumanDrakeCombatV57>, activations: Vec<HumanDrakeActivationV57>
});
record!(HumanOptionalCurrentV57 {
    source: HumanCardRefV1,
    is_token: bool,
    tapped: bool,
    summoning_sick: bool,
    marked_damage: u32,
    counters: optional::CountersV1,
    form: optional::FormV1
});
record!(HumanPreparedRootV57 {
    original_source: HumanCardRefV1, current_original_source: Option<HumanCardRefV1>,
    captured_controller: PlayerSeatV1
});
record!(HumanSpinnerMeaningV57 {
    printed: sm::PrintedV1, cast: sm::CastV1, ordered_rules: Vec<sm::InstructionV54>
});
record!(HumanSpinnerStackV57 {
    stack_position: usize,
    root: HumanPreparedRootV57,
    stage: crate::spinner_public_v54::StageV54
});
record!(HumanSpinnerFactsV57 {
    meaning: HumanSpinnerMeaningV57, current: Vec<HumanOptionalCurrentV57>,
    held: Vec<HumanPreparedRootV57>, stack: Vec<HumanSpinnerStackV57>
});
record!(HumanQuandaryMeaningV57 {
    printed: qm::PrintedV1, cast: qm::CastV1, ordered_rules: Vec<qm::InstructionV55>
});
record!(HumanQuandaryHandV57 { original_hand: Vec<HumanCardRefV1>, selected: Vec<HumanCardRefV1> });
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum HumanQuandaryStageV57 {
    OnStack,
    AwaitingPayment {
        payer: PlayerSeatV1,
        minimum: u16,
        maximum: u16,
        payer_hand: Option<HumanQuandaryHandV57>,
    },
    Answered {
        payer: PlayerSeatV1,
        payer_selected: Option<Vec<HumanCardRefV1>>,
    },
    PrintedUnpaidContinuation {
        payer: PlayerSeatV1,
        life_loss: i32,
    },
    InitialPrintedContinuation,
}
record!(HumanQuandaryStackV57 {
    stack_position: usize,
    root: HumanPreparedRootV57,
    stage: HumanQuandaryStageV57
});
record!(HumanQuandaryFactsV57 {
    viewer: PlayerSeatV1, meaning: HumanQuandaryMeaningV57, current: Vec<HumanOptionalCurrentV57>,
    held: Vec<HumanPreparedRootV57>, stack: Vec<HumanQuandaryStackV57>
});
record!(HumanPhoenixMeaningV57 { printed: pm::PrintedV56, battlefield_entry: pm::EntryV56, ordered_rules: Vec<pm::RuleV56> });
record!(HumanPhoenixCurrentV57 {
    source: HumanCardRefV1, token: bool, effective_names: Vec<String>, effective_types: Vec<crate::card_def::CardType>,
    effective_subtype_ids: Vec<u16>, color_mask: u8, printed_abilities_active: bool,
    power: Option<i32>, toughness: Option<i32>, flying: bool, haste: bool,
    attacks_each_combat_if_able: bool, graveyard_ability_functions_here: bool
});
record!(HumanPhoenixSpellV57 {
    source: HumanCardRefV1,
    is_copy: bool
});
record!(HumanPhoenixTriggerV57 { original_source: HumanCardRefV1, current_original_source: Option<HumanCardRefV1>, original_actor: PlayerSeatV1, stage: phoenix::StageV56 });
record!(HumanPhoenixFactsV57 {
    meaning: HumanPhoenixMeaningV57, ferocious_now: [bool; 2], current: Vec<HumanPhoenixCurrentV57>,
    spells: Vec<HumanPhoenixSpellV57>, triggers: Vec<HumanPhoenixTriggerV57>
});
record!(HumanPreparedActivationV57 {
    source: HumanCardRefV1,
    kind: dm::CounterV53,
    count: u16,
    before: i32,
    after: i32,
    any_priority: bool,
    costs_paid_before_stack: bool,
    countering_refunds_costs: bool
});
record!(HumanPreparedFactsV57 {
    drake: HumanDrakeFactsV57,
    spinner: HumanSpinnerFactsV57,
    quandary: HumanQuandaryFactsV57,
    phoenix: HumanPhoenixFactsV57
});
record!(HumanVisibleStatePreparedV57 {
    common: super::elenda_v52::HumanVisibleStateElendaV52,
    prepared: HumanPreparedFactsV57
});
record!(HumanDecisionPreparedV57 {
    schema: String, human_seat: PlayerSeatV1, prompt_seq: u64,
    state: HumanVisibleStatePreparedV57, actions: Vec<HumanLegalActionV1>,
    action_meanings: Vec<Option<HumanPreparedActivationV57>>
});

trait SafeReference {
    fn stable(&self) -> Result<CardStableRefV1, Error>;
}
fn stable(
    object: u32,
    generation: u32,
    owner: PlayerSeatV1,
    controller: PlayerSeatV1,
    zone: crate::state::Zone,
    known: Option<u16>,
    disguised: bool,
) -> Result<CardStableRefV1, Error> {
    let card_db_id = match known {
        Some(id) => id,
        None => crate::card_def::card_id_by_name(if disguised {
            "Face-down creature"
        } else {
            "Face-down card"
        })
        .ok_or(Error::InvalidVisibleReference)?,
    };
    Ok(CardStableRefV1 {
        arena_id: object,
        zone_change_count: generation,
        owner,
        controller,
        zone,
        card_db_id,
    })
}
impl SafeReference for drake::ReferenceV53 {
    fn stable(&self) -> Result<CardStableRefV1, Error> {
        let (known, disguised) = match self.identity {
            drake::IdentityV53::Known { card_id } => (Some(card_id), false),
            drake::IdentityV53::FaceDown { disguised } => (None, disguised),
        };
        stable(
            self.incarnation.object,
            self.incarnation.generation,
            self.owner,
            self.controller,
            self.zone,
            known,
            disguised,
        )
    }
}
impl SafeReference for optional::ReferenceV1 {
    fn stable(&self) -> Result<CardStableRefV1, Error> {
        let (known, disguised) = match &self.identity {
            optional::IdentityV1::Known { card_id } => (Some(*card_id), false),
            optional::IdentityV1::FaceDown { disguised } => (None, *disguised),
        };
        stable(
            self.object,
            self.generation,
            self.owner,
            self.controller,
            self.zone,
            known,
            disguised,
        )
    }
}
impl SafeReference for phoenix::ReferenceV56 {
    fn stable(&self) -> Result<CardStableRefV1, Error> {
        let (known, disguised) = match self.identity {
            phoenix::IdentityV56::Known { card_id } => (Some(card_id), false),
            phoenix::IdentityV56::FaceDown { disguised } => (None, disguised),
        };
        stable(
            self.object,
            self.generation,
            self.owner,
            self.controller,
            self.zone,
            known,
            disguised,
        )
    }
}
struct References<'a> {
    handles: &'a mut Handles,
    register: bool,
}
impl References<'_> {
    fn reference(&mut self, source: &impl SafeReference) -> Result<HumanCardRefV1, Error> {
        let source = source.stable()?;
        if self.register {
            self.handles.insert(&source)
        } else {
            Ok(self.handles.get(&source)?.clone())
        }
    }
    fn optional<T: SafeReference>(
        &mut self,
        source: &Option<T>,
    ) -> Result<Option<HumanCardRefV1>, Error> {
        source.as_ref().map(|s| self.reference(s)).transpose()
    }
    fn root(&mut self, root: &optional::RootV1) -> Result<HumanPreparedRootV57, Error> {
        Ok(HumanPreparedRootV57 {
            original_source: self.reference(&root.original_source)?,
            current_original_source: self.optional(&root.current_original_source)?,
            captured_controller: root.captured_controller,
        })
    }
    fn current(&mut self, row: &optional::CurrentV1) -> Result<HumanOptionalCurrentV57, Error> {
        Ok(HumanOptionalCurrentV57 {
            source: self.reference(&row.source)?,
            is_token: row.is_token,
            tapped: row.tapped,
            summoning_sick: row.summoning_sick,
            marked_damage: row.marked_damage,
            counters: row.counters.clone(),
            form: row.form.clone(),
        })
    }
}

pub(super) fn facts(
    obs: &observation::PreparedObservationV57,
    handles: &mut Handles,
    register: bool,
) -> Result<HumanPreparedFactsV57, Error> {
    facts_parts(
        &obs.drake,
        &obs.spinner,
        &obs.quandary,
        &obs.phoenix,
        handles,
        register,
    )
}

/// Field-level inherited family mapper shared only after each owning artifact
/// has passed its own validator. This avoids constructing or validating a
/// Prepared V57 envelope for the distinct NextFive V59 observation.
pub(super) fn facts_parts(
    drake_facts: &crate::drake_public_v53::FactsV53,
    spinner_facts: &crate::spinner_public_v54::FactsV54,
    quandary_facts: &crate::quandary_public_v55::FactsV55,
    phoenix_facts: &crate::phoenix_public_v56::FactsV56,
    handles: &mut Handles,
    register: bool,
) -> Result<HumanPreparedFactsV57, Error> {
    let mut r = References { handles, register };
    let battlefield = drake_facts.battlefield.each_ref().map(|rows| {
        rows.iter()
            .map(|row| {
                Ok(HumanPreparedCardV57 {
                    source: r.reference(&row.reference)?,
                    face_index: row.face_index,
                    is_token: row.is_token,
                    tapped: row.tapped,
                    summoning_sick: row.summoning_sick,
                    marked_damage: row.marked_damage,
                    counters: row.counters.clone(),
                    form: row.form.clone(),
                    temporary_boosts: row.temporary_boosts.clone(),
                    attachments: row
                        .attachments
                        .iter()
                        .map(|s| r.reference(s))
                        .collect::<Result<_, Error>>()?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()
    });
    let [left, right] = battlefield;
    let drake = HumanDrakeFactsV57 {
        meanings: drake_facts
            .meanings
            .iter()
            .map(|m| HumanDrakeMeaningV57 {
                root: m.root,
                printed: m.printed.clone(),
                program: drake_program(&m.program),
            })
            .collect(),
        battlefield: [left?, right?],
        combat: drake_facts
            .combat
            .iter()
            .map(|row| {
                Ok(HumanDrakeCombatV57 {
                    occurrence: row.occurrence,
                    original_source: r.reference(&row.original_source)?,
                    current_original_source: r.optional(&row.current_original_source)?,
                    original_controller: row.original_controller,
                    damaged_player: row.damaged_player,
                    final_positive_damage: row.final_positive_damage,
                    outcome: row.outcome.clone(),
                })
            })
            .collect::<Result<_, Error>>()?,
        activations: drake_facts
            .activations
            .iter()
            .map(|row| {
                let outcome = match &row.outcome {
                    drake::ActivationOutcomeV53::Published => {
                        HumanDrakeActivationOutcomeV57::Published
                    }
                    drake::ActivationOutcomeV53::Countered => {
                        HumanDrakeActivationOutcomeV57::Countered
                    }
                    drake::ActivationOutcomeV53::Resolved { tokens } => {
                        HumanDrakeActivationOutcomeV57::Resolved {
                            tokens: tokens
                                .iter()
                                .map(|t| {
                                    Ok(HumanDrakeTokenV57 {
                                        created: r.reference(&t.created)?,
                                        current_original_token: r
                                            .optional(&t.current_original_token)?,
                                        original_controller: t.original_controller,
                                    })
                                })
                                .collect::<Result<_, Error>>()?,
                        }
                    }
                };
                Ok(HumanDrakeActivationV57 {
                    occurrence: row.occurrence,
                    original_source: r.reference(&row.original_source)?,
                    current_original_source: r.optional(&row.current_original_source)?,
                    original_controller: row.original_controller,
                    payment: row.payment.clone(),
                    outcome,
                })
            })
            .collect::<Result<_, Error>>()?,
    };
    let spinner = HumanSpinnerFactsV57 {
        meaning: HumanSpinnerMeaningV57 {
            printed: spinner_facts.meaning.printed.clone(),
            cast: spinner_facts.meaning.cast.clone(),
            ordered_rules: spinner_facts.meaning.ordered_rules.clone(),
        },
        current: spinner_facts
            .current
            .iter()
            .map(|row| r.current(row))
            .collect::<Result<_, Error>>()?,
        held: spinner_facts
            .held
            .iter()
            .map(|root| r.root(root))
            .collect::<Result<_, Error>>()?,
        stack: spinner_facts
            .stack
            .iter()
            .map(|row| {
                Ok(HumanSpinnerStackV57 {
                    stack_position: row.stack_position,
                    root: r.root(&row.root)?,
                    stage: row.stage.clone(),
                })
            })
            .collect::<Result<_, Error>>()?,
    };
    let quandary = HumanQuandaryFactsV57 {
        viewer: quandary_facts.viewer,
        meaning: HumanQuandaryMeaningV57 {
            printed: quandary_facts.meaning.printed.clone(),
            cast: quandary_facts.meaning.cast.clone(),
            ordered_rules: quandary_facts.meaning.ordered_rules.clone(),
        },
        current: quandary_facts
            .current
            .iter()
            .map(|row| r.current(row))
            .collect::<Result<_, Error>>()?,
        held: quandary_facts
            .held
            .iter()
            .map(|root| r.root(root))
            .collect::<Result<_, Error>>()?,
        stack: quandary_facts
            .stack
            .iter()
            .map(|row| {
                use crate::quandary_public_v55::StageV55 as S;
                let stage = match &row.stage {
                    S::OnStack => HumanQuandaryStageV57::OnStack,
                    S::AwaitingPayment {
                        payer,
                        minimum,
                        maximum,
                        payer_hand,
                    } => HumanQuandaryStageV57::AwaitingPayment {
                        payer: *payer,
                        minimum: *minimum,
                        maximum: *maximum,
                        payer_hand: payer_hand
                            .as_ref()
                            .map(|h| {
                                Ok::<_, Error>(HumanQuandaryHandV57 {
                                    original_hand: h
                                        .original_hand
                                        .iter()
                                        .map(|s| r.reference(s))
                                        .collect::<Result<_, Error>>()?,
                                    selected: h
                                        .selected
                                        .iter()
                                        .map(|s| r.reference(s))
                                        .collect::<Result<_, Error>>()?,
                                })
                            })
                            .transpose()?,
                    },
                    S::Answered {
                        payer,
                        payer_selected,
                    } => HumanQuandaryStageV57::Answered {
                        payer: *payer,
                        payer_selected: payer_selected
                            .as_ref()
                            .map(|cards| {
                                cards
                                    .iter()
                                    .map(|s| r.reference(s))
                                    .collect::<Result<_, Error>>()
                            })
                            .transpose()?,
                    },
                    S::PrintedUnpaidContinuation { payer, life_loss } => {
                        HumanQuandaryStageV57::PrintedUnpaidContinuation {
                            payer: *payer,
                            life_loss: *life_loss,
                        }
                    }
                    S::InitialPrintedContinuation => {
                        HumanQuandaryStageV57::InitialPrintedContinuation
                    }
                };
                Ok(HumanQuandaryStackV57 {
                    stack_position: row.stack_position,
                    root: r.root(&row.root)?,
                    stage,
                })
            })
            .collect::<Result<_, Error>>()?,
    };
    let phoenix = HumanPhoenixFactsV57 {
        meaning: HumanPhoenixMeaningV57 {
            printed: phoenix_facts.meaning.printed.clone(),
            battlefield_entry: phoenix_facts.meaning.battlefield_entry,
            ordered_rules: phoenix_facts.meaning.ordered_rules.clone(),
        },
        ferocious_now: phoenix_facts.ferocious_now,
        current: phoenix_facts
            .current
            .iter()
            .map(|row| {
                Ok(HumanPhoenixCurrentV57 {
                    source: r.reference(&row.source)?,
                    token: row.token,
                    effective_names: row.effective_names.clone(),
                    effective_types: row.effective_types.clone(),
                    effective_subtype_ids: row.effective_subtype_ids.clone(),
                    color_mask: row.color_mask,
                    printed_abilities_active: row.printed_abilities_active,
                    power: row.power,
                    toughness: row.toughness,
                    flying: row.flying,
                    haste: row.haste,
                    attacks_each_combat_if_able: row.attacks_each_combat_if_able,
                    graveyard_ability_functions_here: row.graveyard_ability_functions_here,
                })
            })
            .collect::<Result<_, Error>>()?,
        spells: phoenix_facts
            .spells
            .iter()
            .map(|row| {
                Ok(HumanPhoenixSpellV57 {
                    source: r.reference(&row.source)?,
                    is_copy: row.is_copy,
                })
            })
            .collect::<Result<_, Error>>()?,
        triggers: phoenix_facts
            .triggers
            .iter()
            .map(|row| {
                Ok(HumanPhoenixTriggerV57 {
                    original_source: r.reference(&row.original_source)?,
                    current_original_source: r.optional(&row.current_original_source)?,
                    original_actor: row.original_actor,
                    stage: row.stage,
                })
            })
            .collect::<Result<_, Error>>()?,
    };
    let mut result = HumanPreparedFactsV57 {
        drake,
        spinner,
        quandary,
        phoenix,
    };
    for rows in &mut result.drake.battlefield {
        for row in rows.iter_mut() {
            row.attachments.sort_by(|a, b| a.handle.cmp(&b.handle));
        }
        rows.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    }
    result
        .spinner
        .current
        .sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    result
        .quandary
        .current
        .sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    result
        .phoenix
        .current
        .sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    Ok(result)
}

pub(super) fn unary(
    facts: &HumanPreparedFactsV57,
    traits: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    fn add<T: Serialize>(
        source: &HumanCardRefV1,
        value: &T,
        traits: &mut BTreeMap<String, Vec<String>>,
    ) -> Result<(), Error> {
        traits
            .entry(source.handle.clone())
            .or_default()
            .push(visible::without_handles(value)?);
        Ok(())
    }
    for row in facts.drake.battlefield.iter().flatten() {
        add(&row.source, row, traits)?;
    }
    for row in &facts.spinner.current {
        add(&row.source, row, traits)?;
    }
    for row in &facts.quandary.current {
        add(&row.source, row, traits)?;
    }
    for row in &facts.phoenix.current {
        add(&row.source, row, traits)?;
    }
    Ok(())
}
fn action_meaning(
    row: &observation::PreparedActivationV57,
    handles: &Handles,
) -> Result<HumanPreparedActivationV57, Error> {
    Ok(HumanPreparedActivationV57 {
        source: handles.get(&row.source)?.clone(),
        kind: row.kind,
        count: row.count,
        before: row.before,
        after: row.after,
        any_priority: row.any_priority,
        costs_paid_before_stack: row.costs_paid_before_stack,
        countering_refunds_costs: row.countering_refunds_costs,
    })
}
pub(super) fn activation_label(
    obs: &observation::PreparedObservationV57,
    index: usize,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    obs.action_meanings.get(index).ok_or(Error::InvalidAction)?.as_ref().map(|row| {
        let m=action_meaning(row,handles)?;
        Ok(format!("Activate {} [{}]: remove {} incubation counters ({} to {}); create a 2/2 blue Drake token with flying",m.source.name,m.source.handle,m.count,m.before,m.after))
    }).transpose()
}
pub(super) fn activation_label_next_five(
    meanings: &[Option<crate::next_five_observation_v59::NextFiveActivationV59>],
    index: usize,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    meanings.get(index).ok_or(Error::InvalidAction)?.as_ref().map(|row| {
        let source = handles.get(&row.source)?;
        Ok(format!(
            "Activate {} [{}]: remove {} incubation counters ({} to {}); create a 2/2 blue Drake token with flying",
            source.name, source.handle, row.count, row.before, row.after
        ))
    }).transpose()
}
fn trigger_group(
    common: &crate::ravenous_observation_v51::RavenousObservationV51,
    actor: PlayerSeatV1,
) -> Result<&[crate::rl::PendingTriggerSemanticV2], Error> {
    let context = &common.projection.surface.engine_context;
    if common.acting_player != actor
        || context.current_stage != crate::rl::EngineDecisionStageV2::PendingTriggers
    {
        return Err(Error::InvalidVisibleReference);
    }
    // This is the complete current APNAP controller group, including genuine
    // earlier-family rows. Do not deduplicate identical source incarnations.
    let count = context
        .pending_triggers
        .iter()
        .take_while(|row| row.controller == actor)
        .count();
    if count < 2
        || context.pending_triggers[..count]
            .iter()
            .any(|row| row.source.is_none())
    {
        return Err(Error::InvalidVisibleReference);
    }
    Ok(&context.pending_triggers[..count])
}
fn trigger_occurrence(
    group: &[crate::rl::PendingTriggerSemanticV2],
    index: usize,
    handles: &Handles,
) -> Result<String, Error> {
    let row = group.get(index).ok_or(Error::InvalidVisibleReference)?;
    let source = row.source.as_ref().ok_or(Error::InvalidVisibleReference)?;
    // Kind is the authenticated native row's public kind, never a printed
    // program inferred from its host name. Granted roots may share a host.
    let kind = match row.trigger_kind {
        crate::rl::PendingTriggerKindV2::TriggeredAbility => "Triggered ability",
        crate::rl::PendingTriggerKindV2::MadnessOffer => "Madness cast offer",
    };
    Ok(format!(
        "{kind} occurrence #{} from {}",
        index + 1,
        handles.name(source)?
    ))
}
fn order_label(
    common: &crate::ravenous_observation_v51::RavenousObservationV51,
    _trigger_order: Option<&crate::policy_surface_v5::TriggerOrderContextV45>,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    if let ActionSemanticV1::OrderTriggers {
        actor,
        pending_sources,
        order,
    } = action
    {
        let group = trigger_group(common, *actor)?;
        if pending_sources.len() != group.len()
            || order.len() != group.len()
            || pending_sources
                .iter()
                .zip(group)
                .any(|(source, row)| row.source.as_ref() != Some(source))
        {
            return Err(Error::InvalidVisibleReference);
        }
        let mut permutation = order.clone();
        permutation.sort_unstable();
        if permutation != (0..group.len()).collect::<Vec<_>>() {
            return Err(Error::InvalidVisibleReference);
        }
        let labels = order
            .iter()
            .map(|index| trigger_occurrence(group, *index, handles))
            .collect::<Result<Vec<_>, Error>>()?;
        return Ok(Some(format!(
            "Put trigger occurrences on the stack from bottom to top (last resolves first): {}",
            labels.join("; then ")
        )));
    }
    #[cfg(feature = "limited-fdn-fixtures")]
    if let ActionSemanticV1::ChooseTriggerOrderNext {
        actor,
        source,
        trigger_index,
        ordered_prefix,
        pending_count,
    } = action
    {
        let group = trigger_group(common, *actor)?;
        let stage = trigger_order.ok_or(Error::InvalidVisibleReference)?;
        let mut selected = vec![false; group.len()];
        for index in ordered_prefix {
            let taken = selected
                .get_mut(*index)
                .ok_or(Error::InvalidVisibleReference)?;
            if *taken {
                return Err(Error::InvalidVisibleReference);
            }
            *taken = true;
        }
        if stage.controller != *actor
            || stage.pending_count != group.len()
            || *pending_count != group.len()
            || stage.ordered_prefix != *ordered_prefix
            || ordered_prefix.len() >= group.len()
            || selected.get(*trigger_index) != Some(&false)
            || group
                .get(*trigger_index)
                .and_then(|row| row.source.as_ref())
                != Some(source)
        {
            return Err(Error::InvalidVisibleReference);
        }
        let prefix = ordered_prefix
            .iter()
            .map(|index| format!("#{}", index + 1))
            .collect::<Vec<_>>()
            .join(", ");
        let prefix = if prefix.is_empty() {
            "none".into()
        } else {
            prefix
        };
        return Ok(Some(format!(
            "Place {} next on the stack (already ordered: {prefix}; last placed resolves first)",
            trigger_occurrence(group, *trigger_index, handles)?
        )));
    }
    Ok(None)
}

pub(super) fn choice_label(
    obs: &observation::PreparedObservationV57,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}

pub(super) fn choice_label_next_five(
    obs: &crate::next_five_observation_v59::NextFiveObservationV59,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}

pub(super) fn choice_label_call(
    obs: &crate::call_observation_v62::CallObservationV62,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}

pub(super) fn choice_label_end_turn(
    obs: &crate::end_turn_observation_v68::EndTurnObservationV68,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}

pub(super) fn choice_label_genesis_wave(
    obs: &crate::genesis_wave_observation_v69::GenesisWaveObservationV69,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}
pub(super) fn choice_label_vivien(
    obs: &crate::vivien_observation_v70::VivienObservationV70,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}
pub(super) fn choice_label_muldrotha(
    obs: &crate::muldrotha_observation_v71::MuldrothaObservationV71,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}

pub(super) fn choice_label_graveyard_return(
    obs: &crate::graveyard_return_observation_v61::GraveyardReturnObservationV61,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    choice_label_parts(
        &obs.common.common,
        obs.common.common.trigger_order.as_ref(),
        &obs.spinner,
        &obs.phoenix,
        &obs.quandary,
        action,
        handles,
    )
}

fn choice_label_parts(
    common: &crate::ravenous_observation_v51::RavenousObservationV51,
    trigger_order: Option<&crate::policy_surface_v5::TriggerOrderContextV45>,
    spinner: &crate::spinner_public_v54::FactsV54,
    phoenix: &crate::phoenix_public_v56::FactsV56,
    quandary: &crate::quandary_public_v55::FactsV55,
    action: &ActionSemanticV1,
    handles: &Handles,
) -> Result<Option<String>, Error> {
    if let Some(label) = order_label(common, trigger_order, action, handles)? {
        return Ok(Some(label));
    }
    for row in &spinner.stack {
        if row.stage != crate::spinner_public_v54::StageV54::AwaitingReveal {
            continue;
        }
        let source = row.root.original_source.stable()?;
        if let ActionSemanticV1::ChooseEffectOption {
            actor,
            source: actual,
            option_index,
            option_count,
        } = action
        {
            if actual == &source {
                if *actor != row.root.captured_controller || *option_count != 2 {
                    return Err(Error::InvalidAction);
                }
                return Ok(Some(match *option_index {
                    0=>"Decline Spinner reveal; leave the library unchanged".into(),
                    1=>"Reveal through the first creature into your hand; put the other revealed cards on bottom randomly".into(),
                    _=>return Err(Error::InvalidAction)
                }));
            }
        }
    }
    for row in &phoenix.triggers {
        if !matches!(row.stage, phoenix::StageV56::PaymentOffered { .. }) {
            continue;
        }
        if let ActionSemanticV1::ChooseEffectBoolean {
            actor,
            source,
            value,
        } = action
        {
            if *source == row.original_source.stable()? {
                if *actor != row.original_actor {
                    return Err(Error::InvalidAction);
                }
                return Ok(Some(
                    if *value {
                        "Pay {R}; return the original Phoenix if it remains in your graveyard"
                    } else {
                        "Decline Phoenix return; pay no mana"
                    }
                    .into(),
                ));
            }
        }
    }
    for row in &quandary.stack {
        let crate::quandary_public_v55::StageV55::AwaitingPayment {
            payer,
            minimum,
            maximum,
            payer_hand,
        } = &row.stage
        else {
            continue;
        };
        let source = row.root.original_source.stable()?;
        match action {
            ActionSemanticV1::ChooseEffectTarget {
                actor,
                source: actual,
                target: crate::rl::TargetRefV1::Object { object },
                selected_count,
                min_targets,
                max_targets,
            } if actual == &source => {
                if actor != payer
                    || *minimum != 0
                    || *maximum != 1
                    || *selected_count != 0
                    || *min_targets != 0
                    || *max_targets != 1
                    || !payer_hand.as_ref().is_some_and(|hand| {
                        hand.original_hand
                            .iter()
                            .any(|card| card.stable().is_ok_and(|r| r == *object))
                    })
                {
                    return Err(Error::InvalidAction);
                }
                return Ok(Some(format!(
                    "Discard {} to pay for Painful Quandary",
                    handles.name(object)?
                )));
            }
            ActionSemanticV1::FinishEffectSelection {
                actor,
                source: actual,
                selected_count,
            } if actual == &source => {
                if actor != payer || *selected_count != 0 {
                    return Err(Error::InvalidAction);
                }
                return Ok(Some("Decline Quandary discard; lose 5 life".into()));
            }
            _ => {}
        }
    }
    Ok(None)
}

type PreparedProjectionWithHandlesV57 = (
    HumanVisibleStatePreparedV57,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
    Handles,
    Vec<Option<HumanPreparedActivationV57>>,
);
pub(super) fn project_with_handles(
    decision: &PreparedDecisionV57,
    human: PlayerSeatV1,
) -> Result<PreparedProjectionWithHandlesV57, Error> {
    let observation = decision.observation();
    let (common, actions, indexes, mut handles) =
        super::ravenous_v51::project_prepared_base_with_handles(
            observation,
            decision.actions(),
            human,
            decision.family_projections_v57(),
        )?;
    let prepared = facts(observation, &mut handles, false)?;
    let action_meanings = indexes
        .iter()
        .map(|index| {
            observation.action_meanings[*index as usize]
                .as_ref()
                .map(|row| action_meaning(row, &handles))
                .transpose()
        })
        .collect::<Result<_, Error>>()?;
    Ok((
        HumanVisibleStatePreparedV57 { common, prepared },
        actions,
        indexes,
        handles,
        action_meanings,
    ))
}
fn project(
    decision: &PreparedDecisionV57,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanDecisionPreparedV57, Vec<u32>), Error> {
    let (state, actions, indexes, _, action_meanings) = project_with_handles(decision, human)?;
    Ok((
        HumanDecisionPreparedV57 {
            schema: "mtg-kernel-human-prepared-decision/v57".into(),
            human_seat: human,
            prompt_seq,
            state,
            actions,
            action_meanings,
        },
        indexes,
    ))
}

#[derive(Clone)]
struct AuthorityV57 {
    owner: Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV57 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV57 {}
pub struct HumanPreparedSessionV57 {
    inner: PreparedSessionV57,
    authority: AuthorityV57,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanPreparedSnapshotV57 {
    inner: PreparedSnapshotV57,
    owner: Arc<()>,
}
impl HumanPreparedSessionV57 {
    pub fn from_session(inner: PreparedSessionV57, human_seat: PlayerSeatV1) -> Self {
        Self {
            inner,
            human_seat,
            authority: AuthorityV57 {
                owner: Arc::new(()),
                generation: 0,
            },
        }
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v57(&self) -> HumanPreparedSnapshotV57 {
        HumanPreparedSnapshotV57 {
            inner: self.inner.snapshot_v57(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v57(&mut self, snapshot: &HumanPreparedSnapshotV57) -> Result<(), Error> {
        if !Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let generation = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .restore_v57(&snapshot.inner)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = generation;
        Ok(())
    }
    fn decision(&self, expected: FastActorDecisionV1) -> Result<PreparedDecisionV57, Error> {
        let decision = self
            .inner
            .decision_v57()
            .map_err(|_| Error::StaleDecision)?;
        if decision.expected() != expected {
            return Err(Error::StaleDecision);
        }
        Ok(decision)
    }
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::prepared_observation_v57::PreparedObservationV57, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        Ok(self.decision(expected)?.observation().clone())
    }
    /// Host-only opponent callback. Its private hand is never a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, RlContractError>
    where
        F: FnMut(
            &crate::prepared_observation_v57::PreparedObservationV57,
            &[ActionSemanticV1],
        ) -> Result<u32, RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(RlContractError(
                "opponent callback cannot act for Human seat".into(),
            ));
        }
        let decision = self
            .decision(expected)
            .map_err(|_| RlContractError("stale opponent decision".into()))?;
        let index = policy(decision.observation(), decision.actions())?;
        self.inner.step_v57(&decision, index)
    }
}
struct BoundPreparedV57 {
    authority: AuthorityV57,
    decision: PreparedDecisionV57,
    visible: HumanDecisionPreparedV57,
    indexes: Vec<u32>,
}
pub struct HumanPreparedProjectorV57 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundPreparedV57>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV57)>,
}
impl HumanPreparedProjectorV57 {
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
        session: &HumanPreparedSessionV57,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionPreparedV57, Error> {
        if expected.acting_player != self.human_seat || session.human_seat != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let decision = session.decision(expected)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.decision.expected() == expected
                && session
                    .inner
                    .validate_decision_v57(&pending.decision)
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
        self.pending = Some(BoundPreparedV57 {
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
        session: &mut HumanPreparedSessionV57,
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
        session
            .inner
            .validate_decision_v57(&pending.decision)
            .map_err(|_| Error::StaleDecision)?;
        let (visible, indexes) = project(
            &pending.decision,
            self.human_seat,
            pending.visible.prompt_seq,
        )?;
        if visible != pending.visible || indexes != pending.indexes {
            return Err(Error::StaleDecision);
        }
        let index = *pending
            .indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        let result = session.inner.step_v57(&pending.decision, index);
        self.pending = None;
        result.map_err(|_| Error::ExecutionFailed)?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.accepted = Some((receipt.clone(), session.authority.clone()));
        Ok(receipt)
    }
}
#[cfg(test)]
#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests;
