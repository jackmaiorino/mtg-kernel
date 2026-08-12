use crate::{
    ActionSemanticV1, CardPrivateV1, CardPublicV2, CardStableRefV1, KnownLibraryCardV4,
    MtgoContractErrorV1, ObjectRelationPublicV4, PlayerSeatV1, StackItemKindV2, StackItemPublicV2,
    TargetRefV1, ValidatedMtgoObservedDecisionV1, ZoneIndependentStepV1,
};
use mtg_kernel::engine::{CastMode, CostKind, OptionalCostChoice};
use mtg_kernel::mana::ManaColor;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const PLAYER_VISIBLE_DUEL_DECISION_INPUT_DOMAIN_V1: &[u8] =
    b"mtgo-player-visible-duel-decision-input-v1";

/// Adapter-local ordinal used only to link visible objects within one input.
/// It is assigned from visible traversal and action order. No MTGO or kernel
/// object, card-database, owner, controller, zone, or incarnation identifier is
/// copied into this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleObjectRefV1 {
    pub visible_ordinal: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoPlayerVisibleTargetRefV1 {
    Player {
        player: PlayerSeatV1,
    },
    Object {
        object: MtgoPlayerVisibleObjectRefV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleNamedCardV1 {
    pub object_ref: MtgoPlayerVisibleObjectRefV1,
    pub card_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleKnownLibraryCardV1 {
    pub visible_known_position: u32,
    pub card: MtgoPlayerVisibleNamedCardV1,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleCounterStateV1 {
    pub plus_one_plus_one: i16,
    pub minus_one_minus_one: i16,
    pub minus_zero_minus_one: i16,
    pub stun: i16,
    pub lore: i16,
}

/// Field-by-field visible state for one battlefield object. Engine timing,
/// ability-use, untap, goad-expiry, and full characteristic records are absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleBattlefieldCardV1 {
    pub object_ref: MtgoPlayerVisibleObjectRefV1,
    pub card_name: String,
    pub tapped: bool,
    pub marked_damage: u16,
    pub counters: MtgoPlayerVisibleCounterStateV1,
    pub is_token: bool,
    pub visible_face_index: u8,
    pub visible_effective_power: Option<i32>,
    pub visible_effective_toughness: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleStackItemV1 {
    pub visible_stack_position: u32,
    pub source_object_ref: MtgoPlayerVisibleObjectRefV1,
    pub controller: PlayerSeatV1,
    pub visible_targets: Vec<MtgoPlayerVisibleTargetRefV1>,
    pub item_kind: StackItemKindV2,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "relation_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoPlayerVisibleObjectRelationV1 {
    AttachedTo {
        object: MtgoPlayerVisibleObjectRefV1,
        attached_to: MtgoPlayerVisibleObjectRefV1,
    },
    ExiledBy {
        object: MtgoPlayerVisibleObjectRefV1,
        exiled_by: MtgoPlayerVisibleObjectRefV1,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleBlockerAssignmentV1 {
    pub attacker: MtgoPlayerVisibleObjectRefV1,
    pub ordered_blockers: Vec<MtgoPlayerVisibleObjectRefV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleCombatStateV1 {
    pub attackers_declared: bool,
    pub blockers_declared: bool,
    pub ordered_attackers: Vec<MtgoPlayerVisibleObjectRefV1>,
    pub blocker_assignments: Vec<MtgoPlayerVisibleBlockerAssignmentV1>,
}

/// Player-visible legal choice. Object references are sanitized ordinals and
/// every other value identifies a choice present in the visible legal set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action_kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MtgoPlayerVisibleDuelActionV1 {
    Pass {
        actor: PlayerSeatV1,
    },
    PlayLand {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
    },
    CastSpell {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
    },
    ActivateManaAbility {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        mana_choice: Option<ManaColor>,
    },
    ActivateAbility {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        visible_ability_ordinal: u8,
    },
    PlotSpell {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
    },
    ChooseTarget {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        remaining: u8,
        target: MtgoPlayerVisibleTargetRefV1,
    },
    ChooseCostTarget {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        cost_kind: CostKind,
        remaining: u8,
        candidate: MtgoPlayerVisibleObjectRefV1,
    },
    ChooseCastMode {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        mode: CastMode,
    },
    ChooseKicker {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        pay: bool,
    },
    ChooseSpellMode {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        visible_mode_ordinal: u8,
        visible_mode_count: u8,
    },
    ChooseEffectOption {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        visible_option_ordinal: u16,
        visible_option_count: u16,
    },
    ChooseEffectTarget {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        target: MtgoPlayerVisibleTargetRefV1,
        selected_count: u16,
        min_targets: u16,
        max_targets: u16,
    },
    FinishEffectSelection {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        selected_count: u16,
    },
    ChooseEffectColor {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        color: ManaColor,
    },
    ChooseEffectNumber {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        number: i32,
        minimum: i32,
        maximum: i32,
    },
    ChooseEffectBoolean {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        value: bool,
    },
    FinishTargetSelection {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        selected_count: u16,
    },
    ChooseOptionalCostUse {
        actor: PlayerSeatV1,
        use_cost: bool,
    },
    ChooseOptionalCostWhich {
        actor: PlayerSeatV1,
        choice: OptionalCostChoice,
    },
    ChooseSpellCopyPayment {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        pay: bool,
    },
    ChooseSpellCopyRetarget {
        actor: PlayerSeatV1,
        source: MtgoPlayerVisibleObjectRefV1,
        change_target: bool,
    },
    ChooseMadnessCast {
        actor: PlayerSeatV1,
        card: MtgoPlayerVisibleObjectRefV1,
        cast_it: bool,
    },
    Discard {
        actor: PlayerSeatV1,
        cards: Vec<MtgoPlayerVisibleObjectRefV1>,
    },
    DeclareAttackers {
        actor: PlayerSeatV1,
        attackers: Vec<MtgoPlayerVisibleObjectRefV1>,
    },
    DeclareBlockersForAttacker {
        actor: PlayerSeatV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
        blockers: Vec<MtgoPlayerVisibleObjectRefV1>,
    },
    ChooseAttackerInclusion {
        actor: PlayerSeatV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
        include: bool,
    },
    ChooseBlockerInclusion {
        actor: PlayerSeatV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
        blocker: MtgoPlayerVisibleObjectRefV1,
        include: bool,
    },
    OrderTriggers {
        actor: PlayerSeatV1,
        pending_sources: Vec<MtgoPlayerVisibleObjectRefV1>,
        order: Vec<usize>,
    },
}

/// Owned current game state eligible for competitive model features. It has
/// no aggregate kernel observation and no kernel, MTGO, transport, or source
/// identifiers.
///
/// ```compile_fail
/// use mtgo_blackbox_v1::MtgoPlayerVisibleDuelStateV1;
/// fn cannot_read_kernel_or_transport_metadata(value: &MtgoPlayerVisibleDuelStateV1) {
///     let _ = value.engine_context;
///     let _ = value.surface_context;
///     let _ = value.policy_surface_context;
///     let _ = value.frame_id;
///     let _ = value.card_db_hash;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDuelStateV1 {
    pub acting_player: PlayerSeatV1,
    pub turn: u32,
    pub phase: ZoneIndependentStepV1,
    pub active_player: PlayerSeatV1,
    pub priority_player: PlayerSeatV1,
    pub initiative: Option<PlayerSeatV1>,
    pub life_totals: [i32; 2],
    pub mana_pools: [[u8; 6]; 2],
    pub hand_counts: [usize; 2],
    pub library_counts: [usize; 2],
    pub battlefield: [Vec<MtgoPlayerVisibleBattlefieldCardV1>; 2],
    pub graveyards: [Vec<MtgoPlayerVisibleNamedCardV1>; 2],
    pub exile: Vec<MtgoPlayerVisibleNamedCardV1>,
    pub stack: Vec<MtgoPlayerVisibleStackItemV1>,
    pub combat: MtgoPlayerVisibleCombatStateV1,
    pub visible_object_relations: Vec<MtgoPlayerVisibleObjectRelationV1>,
    pub own_hand: Vec<MtgoPlayerVisibleNamedCardV1>,
    pub known_library_cards: [Vec<MtgoPlayerVisibleKnownLibraryCardV1>; 2],
    pub known_hand_cards: [Vec<MtgoPlayerVisibleNamedCardV1>; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleDuelDecisionInputV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub ordered_legal_actions: Vec<MtgoPlayerVisibleDuelActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleConfirmedDuelDecisionV1 {
    pub current_state: MtgoPlayerVisibleDuelStateV1,
    pub selected_action: MtgoPlayerVisibleDuelActionV1,
}

impl MtgoPlayerVisibleDuelDecisionInputV1 {
    pub fn commitment_sha256_v1(&self) -> Result<String, MtgoContractErrorV1> {
        player_visible_duel_decision_input_commitment_v1(self)
    }

    pub fn safe_for_live_input_v1(&self) -> bool {
        false
    }

    pub fn permits_event_entry_v1(&self) -> bool {
        false
    }

    pub fn permits_spending_v1(&self) -> bool {
        false
    }
}

/// Projects one fully validated visible decision into the exact owned fact set
/// eligible for the competitive model. Internal stable references are used
/// only transiently for equality joins and never enter the returned value.
pub fn build_player_visible_duel_decision_input_v1(
    decision: &ValidatedMtgoObservedDecisionV1,
) -> Result<MtgoPlayerVisibleDuelDecisionInputV1, MtgoContractErrorV1> {
    build_player_visible_duel_decision_input_from_parts_v1(
        decision.observation(),
        decision.legal_actions(),
    )
}

pub(crate) fn build_player_visible_duel_decision_input_from_parts_v1(
    observation: &crate::ObservationV5,
    ordered_actions: &[ActionSemanticV1],
) -> Result<MtgoPlayerVisibleDuelDecisionInputV1, MtgoContractErrorV1> {
    let surface = &observation.projection.surface;
    let mut refs = VisibleObjectRefsV1::default();
    for cards in &surface.battlefield {
        refs.register_public_cards_v1(cards)?;
    }
    for cards in &surface.graveyards {
        refs.register_public_cards_v1(cards)?;
    }
    refs.register_public_cards_v1(&surface.exile)?;
    for item in &surface.stack {
        refs.register_v1(&item.source)?;
        for target in &item.targets {
            refs.register_target_v1(target)?;
        }
    }
    refs.register_private_cards_v1(&observation.own_hand)?;
    for cards in &observation.known_library_cards {
        for card in cards {
            refs.register_v1(&card.card.stable)?;
        }
    }
    for cards in &observation.known_hand_cards {
        refs.register_private_cards_v1(cards)?;
    }
    for attacker in &surface.combat.ordered_attackers {
        refs.register_v1(attacker)?;
    }
    for (attacker, blockers) in &surface.combat.attacker_to_ordered_blockers {
        refs.register_v1(attacker)?;
        for blocker in blockers {
            refs.register_v1(blocker)?;
        }
    }
    for relation in &surface.object_relations {
        match relation {
            ObjectRelationPublicV4::AttachedTo {
                object,
                attached_to,
            } => {
                refs.register_v1(object)?;
                refs.register_v1(attached_to)?;
            }
            ObjectRelationPublicV4::ExiledBy { object, exiled_by } => {
                refs.register_v1(object)?;
                refs.register_v1(exiled_by)?;
            }
        }
    }
    for action in ordered_actions {
        refs.register_action_v1(action)?;
    }

    Ok(MtgoPlayerVisibleDuelDecisionInputV1 {
        current_state: MtgoPlayerVisibleDuelStateV1 {
            acting_player: observation.acting_player,
            turn: surface.turn,
            phase: surface.phase,
            active_player: surface.active_player,
            priority_player: surface.priority_player,
            initiative: surface.initiative,
            life_totals: surface.life_totals,
            mana_pools: surface.mana_pools,
            hand_counts: surface.hand_counts,
            library_counts: surface.library_counts,
            battlefield: [
                map_results_v1(surface.battlefield[0].iter(), |card| {
                    visible_battlefield_card_v1(card, &refs)
                })?,
                map_results_v1(surface.battlefield[1].iter(), |card| {
                    visible_battlefield_card_v1(card, &refs)
                })?,
            ],
            graveyards: [
                map_results_v1(surface.graveyards[0].iter(), |card| {
                    visible_named_card_v1(&card.stable, &card.card_name, &refs)
                })?,
                map_results_v1(surface.graveyards[1].iter(), |card| {
                    visible_named_card_v1(&card.stable, &card.card_name, &refs)
                })?,
            ],
            exile: map_results_v1(surface.exile.iter(), |card| {
                visible_named_card_v1(&card.stable, &card.card_name, &refs)
            })?,
            stack: map_results_v1(surface.stack.iter(), |item| {
                visible_stack_item_v1(item, &refs)
            })?,
            combat: visible_combat_state_v1(surface, &refs)?,
            visible_object_relations: map_results_v1(
                surface.object_relations.iter(),
                |relation| visible_object_relation_v1(relation, &refs),
            )?,
            own_hand: map_results_v1(observation.own_hand.iter(), |card| {
                visible_named_card_v1(&card.stable, &card.card_name, &refs)
            })?,
            known_library_cards: [
                map_results_v1(observation.known_library_cards[0].iter(), |known| {
                    visible_known_library_card_v1(known, &refs)
                })?,
                map_results_v1(observation.known_library_cards[1].iter(), |known| {
                    visible_known_library_card_v1(known, &refs)
                })?,
            ],
            known_hand_cards: [
                map_results_v1(observation.known_hand_cards[0].iter(), |card| {
                    visible_named_card_v1(&card.stable, &card.card_name, &refs)
                })?,
                map_results_v1(observation.known_hand_cards[1].iter(), |card| {
                    visible_named_card_v1(&card.stable, &card.card_name, &refs)
                })?,
            ],
        },
        ordered_legal_actions: map_results_v1(ordered_actions.iter(), |action| {
            visible_action_v1(action, &refs)
        })?,
    })
}

pub(crate) fn build_player_visible_confirmed_duel_decision_v1(
    observation: &crate::ObservationV5,
    selected_action: &ActionSemanticV1,
) -> Result<MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoContractErrorV1> {
    let input = build_player_visible_duel_decision_input_from_parts_v1(
        observation,
        std::slice::from_ref(selected_action),
    )?;
    let [selected_action] = input.ordered_legal_actions.try_into().map_err(|_| {
        error_v1(
            "player_visible_selected_action_shape",
            "expected one selected action",
        )
    })?;
    Ok(MtgoPlayerVisibleConfirmedDuelDecisionV1 {
        current_state: input.current_state,
        selected_action,
    })
}

pub fn player_visible_duel_decision_input_commitment_v1(
    input: &MtgoPlayerVisibleDuelDecisionInputV1,
) -> Result<String, MtgoContractErrorV1> {
    let bytes = serde_json::to_vec(input).map_err(|error| {
        MtgoContractErrorV1::new(
            "player_visible_duel_input_serialization_failed",
            error.to_string(),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(PLAYER_VISIBLE_DUEL_DECISION_INPUT_DOMAIN_V1);
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

#[derive(Default)]
struct VisibleObjectRefsV1 {
    by_stable: HashMap<CardStableRefV1, MtgoPlayerVisibleObjectRefV1>,
}

impl VisibleObjectRefsV1 {
    fn register_v1(&mut self, stable: &CardStableRefV1) -> Result<(), MtgoContractErrorV1> {
        if !self.by_stable.contains_key(stable) {
            let visible_ordinal = u32::try_from(self.by_stable.len()).map_err(|_| {
                error_v1(
                    "player_visible_object_ordinal_overflow",
                    "too many visible objects",
                )
            })?;
            self.by_stable.insert(
                stable.clone(),
                MtgoPlayerVisibleObjectRefV1 { visible_ordinal },
            );
        }
        Ok(())
    }

    fn register_public_cards_v1(
        &mut self,
        cards: &[CardPublicV2],
    ) -> Result<(), MtgoContractErrorV1> {
        for card in cards {
            self.register_v1(&card.stable)?;
        }
        Ok(())
    }

    fn register_private_cards_v1(
        &mut self,
        cards: &[CardPrivateV1],
    ) -> Result<(), MtgoContractErrorV1> {
        for card in cards {
            self.register_v1(&card.stable)?;
        }
        Ok(())
    }

    fn register_target_v1(&mut self, target: &TargetRefV1) -> Result<(), MtgoContractErrorV1> {
        if let TargetRefV1::Object { object } = target {
            self.register_v1(object)?;
        }
        Ok(())
    }

    fn register_action_v1(&mut self, action: &ActionSemanticV1) -> Result<(), MtgoContractErrorV1> {
        match action {
            ActionSemanticV1::Pass { .. }
            | ActionSemanticV1::ChooseOptionalCostUse { .. }
            | ActionSemanticV1::ChooseOptionalCostWhich { .. } => {}
            ActionSemanticV1::PlayLand { source, .. }
            | ActionSemanticV1::CastSpell { source, .. }
            | ActionSemanticV1::ActivateManaAbility { source, .. }
            | ActionSemanticV1::ActivateAbility { source, .. }
            | ActionSemanticV1::PlotSpell { source, .. }
            | ActionSemanticV1::ChooseCastMode { source, .. }
            | ActionSemanticV1::ChooseKicker { source, .. }
            | ActionSemanticV1::ChooseSpellMode { source, .. }
            | ActionSemanticV1::ChooseEffectOption { source, .. }
            | ActionSemanticV1::FinishEffectSelection { source, .. }
            | ActionSemanticV1::ChooseEffectColor { source, .. }
            | ActionSemanticV1::ChooseEffectNumber { source, .. }
            | ActionSemanticV1::ChooseEffectBoolean { source, .. }
            | ActionSemanticV1::FinishTargetSelection { source, .. }
            | ActionSemanticV1::ChooseSpellCopyPayment { source, .. }
            | ActionSemanticV1::ChooseSpellCopyRetarget { source, .. } => {
                self.register_v1(source)?
            }
            ActionSemanticV1::ChooseTarget { source, target, .. }
            | ActionSemanticV1::ChooseEffectTarget { source, target, .. } => {
                self.register_v1(source)?;
                self.register_target_v1(target)?;
            }
            ActionSemanticV1::ChooseCostTarget {
                source, candidate, ..
            } => {
                self.register_v1(source)?;
                self.register_v1(candidate)?;
            }
            ActionSemanticV1::ChooseMadnessCast { card, .. } => self.register_v1(card)?,
            ActionSemanticV1::Discard { cards, .. } => {
                for card in cards {
                    self.register_v1(card)?;
                }
            }
            ActionSemanticV1::DeclareAttackers { attackers, .. } => {
                for attacker in attackers {
                    self.register_v1(attacker)?;
                }
            }
            ActionSemanticV1::DeclareBlockersForAttacker {
                attacker, blockers, ..
            } => {
                self.register_v1(attacker)?;
                for blocker in blockers {
                    self.register_v1(blocker)?;
                }
            }
            ActionSemanticV1::ChooseAttackerInclusion { attacker, .. } => {
                self.register_v1(attacker)?
            }
            ActionSemanticV1::ChooseBlockerInclusion {
                attacker, blocker, ..
            } => {
                self.register_v1(attacker)?;
                self.register_v1(blocker)?;
            }
            ActionSemanticV1::OrderTriggers {
                pending_sources, ..
            } => {
                for source in pending_sources {
                    self.register_v1(source)?;
                }
            }
            ActionSemanticV1::Ambiguous { .. } => {
                return Err(error_v1(
                    "player_visible_duel_action_ambiguous",
                    "validated competitive legal actions cannot be ambiguous",
                ));
            }
        }
        Ok(())
    }

    fn get_v1(
        &self,
        stable: &CardStableRefV1,
    ) -> Result<MtgoPlayerVisibleObjectRefV1, MtgoContractErrorV1> {
        self.by_stable.get(stable).copied().ok_or_else(|| {
            error_v1(
                "player_visible_object_reference_missing",
                "a visible object was not registered in the current decision",
            )
        })
    }
}

fn visible_named_card_v1(
    stable: &CardStableRefV1,
    card_name: &str,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleNamedCardV1, MtgoContractErrorV1> {
    Ok(MtgoPlayerVisibleNamedCardV1 {
        object_ref: refs.get_v1(stable)?,
        card_name: card_name.to_owned(),
    })
}

fn visible_known_library_card_v1(
    known: &KnownLibraryCardV4,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleKnownLibraryCardV1, MtgoContractErrorV1> {
    Ok(MtgoPlayerVisibleKnownLibraryCardV1 {
        visible_known_position: known.position,
        card: visible_named_card_v1(&known.card.stable, &known.card.card_name, refs)?,
    })
}

fn visible_battlefield_card_v1(
    card: &CardPublicV2,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleBattlefieldCardV1, MtgoContractErrorV1> {
    Ok(MtgoPlayerVisibleBattlefieldCardV1 {
        object_ref: refs.get_v1(&card.stable)?,
        card_name: card.card_name.clone(),
        tapped: card.tapped,
        marked_damage: card.damage,
        counters: MtgoPlayerVisibleCounterStateV1 {
            plus_one_plus_one: card.counters.plus1_plus1,
            minus_one_minus_one: card.counters.minus1_minus1,
            minus_zero_minus_one: card.counters.minus0_minus1,
            stun: card.counters.stun,
            lore: card.counters.lore,
        },
        is_token: card.is_token,
        visible_face_index: card.face_index,
        visible_effective_power: card.characteristics.effective_power,
        visible_effective_toughness: card.characteristics.effective_toughness,
    })
}

fn visible_stack_item_v1(
    item: &StackItemPublicV2,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleStackItemV1, MtgoContractErrorV1> {
    Ok(MtgoPlayerVisibleStackItemV1 {
        visible_stack_position: item.stack_index,
        source_object_ref: refs.get_v1(&item.source)?,
        controller: item.controller,
        visible_targets: map_results_v1(item.targets.iter(), |target| {
            visible_target_v1(target, refs)
        })?,
        item_kind: item.stack_item_kind,
    })
}

fn visible_target_v1(
    target: &TargetRefV1,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleTargetRefV1, MtgoContractErrorV1> {
    match target {
        TargetRefV1::Player { player } => {
            Ok(MtgoPlayerVisibleTargetRefV1::Player { player: *player })
        }
        TargetRefV1::Object { object } => Ok(MtgoPlayerVisibleTargetRefV1::Object {
            object: refs.get_v1(object)?,
        }),
    }
}

fn visible_combat_state_v1(
    surface: &mtg_kernel::rl::PublicObservationProjectionV2,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleCombatStateV1, MtgoContractErrorV1> {
    Ok(MtgoPlayerVisibleCombatStateV1 {
        attackers_declared: surface.combat.attackers_declared,
        blockers_declared: surface.combat.blockers_declared,
        ordered_attackers: map_results_v1(surface.combat.ordered_attackers.iter(), |object| {
            refs.get_v1(object)
        })?,
        blocker_assignments: map_results_v1(
            surface.combat.attacker_to_ordered_blockers.iter(),
            |(attacker, blockers)| {
                Ok(MtgoPlayerVisibleBlockerAssignmentV1 {
                    attacker: refs.get_v1(attacker)?,
                    ordered_blockers: map_results_v1(blockers.iter(), |blocker| {
                        refs.get_v1(blocker)
                    })?,
                })
            },
        )?,
    })
}

fn visible_object_relation_v1(
    relation: &ObjectRelationPublicV4,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleObjectRelationV1, MtgoContractErrorV1> {
    match relation {
        ObjectRelationPublicV4::AttachedTo {
            object,
            attached_to,
        } => Ok(MtgoPlayerVisibleObjectRelationV1::AttachedTo {
            object: refs.get_v1(object)?,
            attached_to: refs.get_v1(attached_to)?,
        }),
        ObjectRelationPublicV4::ExiledBy { object, exiled_by } => {
            Ok(MtgoPlayerVisibleObjectRelationV1::ExiledBy {
                object: refs.get_v1(object)?,
                exiled_by: refs.get_v1(exiled_by)?,
            })
        }
    }
}

fn visible_action_v1(
    action: &ActionSemanticV1,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleDuelActionV1, MtgoContractErrorV1> {
    use ActionSemanticV1 as A;
    use MtgoPlayerVisibleDuelActionV1 as V;
    Ok(match action {
        A::Pass { actor } => V::Pass { actor: *actor },
        A::PlayLand { actor, source } => V::PlayLand {
            actor: *actor,
            source: refs.get_v1(source)?,
        },
        A::CastSpell { actor, source } => V::CastSpell {
            actor: *actor,
            source: refs.get_v1(source)?,
        },
        A::ActivateManaAbility {
            actor,
            source,
            mana_choice,
        } => V::ActivateManaAbility {
            actor: *actor,
            source: refs.get_v1(source)?,
            mana_choice: *mana_choice,
        },
        A::ActivateAbility {
            actor,
            source,
            ability_index,
        } => V::ActivateAbility {
            actor: *actor,
            source: refs.get_v1(source)?,
            visible_ability_ordinal: *ability_index,
        },
        A::PlotSpell { actor, source } => V::PlotSpell {
            actor: *actor,
            source: refs.get_v1(source)?,
        },
        A::ChooseTarget {
            actor,
            source,
            remaining,
            target,
        } => V::ChooseTarget {
            actor: *actor,
            source: refs.get_v1(source)?,
            remaining: *remaining,
            target: visible_target_v1(target, refs)?,
        },
        A::ChooseCostTarget {
            actor,
            source,
            cost_kind,
            remaining,
            candidate,
        } => V::ChooseCostTarget {
            actor: *actor,
            source: refs.get_v1(source)?,
            cost_kind: *cost_kind,
            remaining: *remaining,
            candidate: refs.get_v1(candidate)?,
        },
        A::ChooseCastMode {
            actor,
            source,
            mode,
        } => V::ChooseCastMode {
            actor: *actor,
            source: refs.get_v1(source)?,
            mode: *mode,
        },
        A::ChooseKicker { actor, source, pay } => V::ChooseKicker {
            actor: *actor,
            source: refs.get_v1(source)?,
            pay: *pay,
        },
        A::ChooseSpellMode {
            actor,
            source,
            mode_index,
            mode_count,
        } => V::ChooseSpellMode {
            actor: *actor,
            source: refs.get_v1(source)?,
            visible_mode_ordinal: *mode_index,
            visible_mode_count: *mode_count,
        },
        A::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } => V::ChooseEffectOption {
            actor: *actor,
            source: refs.get_v1(source)?,
            visible_option_ordinal: *option_index,
            visible_option_count: *option_count,
        },
        A::ChooseEffectTarget {
            actor,
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
        } => V::ChooseEffectTarget {
            actor: *actor,
            source: refs.get_v1(source)?,
            target: visible_target_v1(target, refs)?,
            selected_count: *selected_count,
            min_targets: *min_targets,
            max_targets: *max_targets,
        },
        A::FinishEffectSelection {
            actor,
            source,
            selected_count,
        } => V::FinishEffectSelection {
            actor: *actor,
            source: refs.get_v1(source)?,
            selected_count: *selected_count,
        },
        A::ChooseEffectColor {
            actor,
            source,
            color,
        } => V::ChooseEffectColor {
            actor: *actor,
            source: refs.get_v1(source)?,
            color: *color,
        },
        A::ChooseEffectNumber {
            actor,
            source,
            number,
            minimum,
            maximum,
        } => V::ChooseEffectNumber {
            actor: *actor,
            source: refs.get_v1(source)?,
            number: *number,
            minimum: *minimum,
            maximum: *maximum,
        },
        A::ChooseEffectBoolean {
            actor,
            source,
            value,
        } => V::ChooseEffectBoolean {
            actor: *actor,
            source: refs.get_v1(source)?,
            value: *value,
        },
        A::FinishTargetSelection {
            actor,
            source,
            selected_count,
        } => V::FinishTargetSelection {
            actor: *actor,
            source: refs.get_v1(source)?,
            selected_count: *selected_count,
        },
        A::ChooseOptionalCostUse { actor, use_cost } => V::ChooseOptionalCostUse {
            actor: *actor,
            use_cost: *use_cost,
        },
        A::ChooseOptionalCostWhich { actor, choice } => V::ChooseOptionalCostWhich {
            actor: *actor,
            choice: *choice,
        },
        A::ChooseSpellCopyPayment { actor, source, pay } => V::ChooseSpellCopyPayment {
            actor: *actor,
            source: refs.get_v1(source)?,
            pay: *pay,
        },
        A::ChooseSpellCopyRetarget {
            actor,
            source,
            change_target,
        } => V::ChooseSpellCopyRetarget {
            actor: *actor,
            source: refs.get_v1(source)?,
            change_target: *change_target,
        },
        A::ChooseMadnessCast {
            actor,
            card,
            cast_it,
        } => V::ChooseMadnessCast {
            actor: *actor,
            card: refs.get_v1(card)?,
            cast_it: *cast_it,
        },
        A::Discard { actor, cards } => V::Discard {
            actor: *actor,
            cards: map_results_v1(cards.iter(), |card| refs.get_v1(card))?,
        },
        A::DeclareAttackers { actor, attackers } => V::DeclareAttackers {
            actor: *actor,
            attackers: map_results_v1(attackers.iter(), |attacker| refs.get_v1(attacker))?,
        },
        A::DeclareBlockersForAttacker {
            actor,
            attacker,
            blockers,
        } => V::DeclareBlockersForAttacker {
            actor: *actor,
            attacker: refs.get_v1(attacker)?,
            blockers: map_results_v1(blockers.iter(), |blocker| refs.get_v1(blocker))?,
        },
        A::ChooseAttackerInclusion {
            actor,
            attacker,
            include,
        } => V::ChooseAttackerInclusion {
            actor: *actor,
            attacker: refs.get_v1(attacker)?,
            include: *include,
        },
        A::ChooseBlockerInclusion {
            actor,
            attacker,
            blocker,
            include,
        } => V::ChooseBlockerInclusion {
            actor: *actor,
            attacker: refs.get_v1(attacker)?,
            blocker: refs.get_v1(blocker)?,
            include: *include,
        },
        A::OrderTriggers {
            actor,
            pending_sources,
            order,
        } => V::OrderTriggers {
            actor: *actor,
            pending_sources: map_results_v1(pending_sources.iter(), |source| refs.get_v1(source))?,
            order: order.clone(),
        },
        A::Ambiguous { .. } => {
            return Err(error_v1(
                "player_visible_duel_action_ambiguous",
                "validated competitive legal actions cannot be ambiguous",
            ))
        }
    })
}

fn map_results_v1<'a, T: 'a, U, I, F>(values: I, mut map: F) -> Result<Vec<U>, MtgoContractErrorV1>
where
    I: IntoIterator<Item = &'a T>,
    F: FnMut(&'a T) -> Result<U, MtgoContractErrorV1>,
{
    values.into_iter().map(&mut map).collect()
}

fn error_v1(code: &'static str, detail: &'static str) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}
