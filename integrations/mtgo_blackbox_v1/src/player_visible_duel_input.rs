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
    b"mtgo-player-visible-duel-decision-input-v1-exile-zone-owner-v1";

/// Player identity relative to the person seated at the approved MTGO
/// account. Kernel seat labels are adapter bookkeeping and never enter the
/// competitive model payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MtgoPlayerRelativeRoleV1 {
    SeatedPlayer,
    Opponent,
}

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
        player: MtgoPlayerRelativeRoleV1,
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

/// One card in the visible exile zone. A face-down plotted card remains a
/// visible object, but its name is present only for the player who owns it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleExileCardV1 {
    pub object_ref: MtgoPlayerVisibleObjectRefV1,
    /// The player-relative exile panel containing this visible object.
    pub zone_owner: MtgoPlayerRelativeRoleV1,
    pub visible_card_name: Option<String>,
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
    pub visible_effective_power: Option<i32>,
    pub visible_effective_toughness: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MtgoPlayerVisibleStackItemV1 {
    pub visible_stack_position: u32,
    pub source_object_ref: MtgoPlayerVisibleObjectRefV1,
    /// The current aggregate kernel observation does not carry a separately
    /// reconstructed visible stack label. Leave this absent rather than infer
    /// a name from the private card-database identifier in `source`.
    pub visible_source_name: Option<String>,
    pub controller: MtgoPlayerRelativeRoleV1,
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
        actor: MtgoPlayerRelativeRoleV1,
    },
    PlayLand {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
    },
    CastSpell {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
    },
    ActivateManaAbility {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        mana_choice: Option<ManaColor>,
    },
    ActivateAbility {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        visible_choice_ordinal: u32,
    },
    PlotSpell {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
    },
    ChooseTarget {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        remaining: u8,
        target: MtgoPlayerVisibleTargetRefV1,
    },
    ChooseCostTarget {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        cost_kind: CostKind,
        remaining: u8,
        candidate: MtgoPlayerVisibleObjectRefV1,
    },
    ChooseCastMode {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        mode: CastMode,
    },
    ChooseKicker {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        pay: bool,
    },
    ChooseSpellMode {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        visible_choice_ordinal: u32,
    },
    ChooseEffectOption {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        visible_choice_ordinal: u32,
    },
    ChooseEffectTarget {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        target: MtgoPlayerVisibleTargetRefV1,
        selected_count: u16,
        min_targets: u16,
        max_targets: u16,
    },
    FinishEffectSelection {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        selected_count: u16,
    },
    ChooseEffectColor {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        color: ManaColor,
    },
    ChooseEffectNumber {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        number: i32,
        minimum: i32,
        maximum: i32,
    },
    ChooseEffectBoolean {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        value: bool,
    },
    FinishTargetSelection {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        selected_count: u16,
    },
    ChooseOptionalCostUse {
        actor: MtgoPlayerRelativeRoleV1,
        use_cost: bool,
    },
    ChooseOptionalCostWhich {
        actor: MtgoPlayerRelativeRoleV1,
        choice: OptionalCostChoice,
    },
    ChooseSpellCopyPayment {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        pay: bool,
    },
    ChooseSpellCopyRetarget {
        actor: MtgoPlayerRelativeRoleV1,
        source: MtgoPlayerVisibleObjectRefV1,
        change_target: bool,
    },
    ChooseMadnessCast {
        actor: MtgoPlayerRelativeRoleV1,
        card: MtgoPlayerVisibleObjectRefV1,
        cast_it: bool,
    },
    Discard {
        actor: MtgoPlayerRelativeRoleV1,
        cards: Vec<MtgoPlayerVisibleObjectRefV1>,
    },
    DeclareAttackers {
        actor: MtgoPlayerRelativeRoleV1,
        attackers: Vec<MtgoPlayerVisibleObjectRefV1>,
    },
    DeclareBlockersForAttacker {
        actor: MtgoPlayerRelativeRoleV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
        blockers: Vec<MtgoPlayerVisibleObjectRefV1>,
    },
    ChooseAttackerInclusion {
        actor: MtgoPlayerRelativeRoleV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
        include: bool,
    },
    ChooseBlockerInclusion {
        actor: MtgoPlayerRelativeRoleV1,
        attacker: MtgoPlayerVisibleObjectRefV1,
        blocker: MtgoPlayerVisibleObjectRefV1,
        include: bool,
    },
    OrderTriggers {
        actor: MtgoPlayerRelativeRoleV1,
        pending_sources: Vec<MtgoPlayerVisibleObjectRefV1>,
        ordered_sources: Vec<MtgoPlayerVisibleObjectRefV1>,
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
    pub acting_player: MtgoPlayerRelativeRoleV1,
    pub turn: u32,
    pub phase: ZoneIndependentStepV1,
    pub active_player: MtgoPlayerRelativeRoleV1,
    pub priority_player: MtgoPlayerRelativeRoleV1,
    pub initiative: Option<MtgoPlayerRelativeRoleV1>,
    pub life_totals: [i32; 2],
    pub mana_pools: [[u8; 6]; 2],
    pub hand_counts: [usize; 2],
    pub library_counts: [usize; 2],
    pub battlefield: [Vec<MtgoPlayerVisibleBattlefieldCardV1>; 2],
    pub graveyards: [Vec<MtgoPlayerVisibleNamedCardV1>; 2],
    pub exile: Vec<MtgoPlayerVisibleExileCardV1>,
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
    let seated_player = observation.acting_player;
    let opponent = other_player_v1(seated_player);
    let seated_index = player_seat_index_v1(seated_player);
    let opponent_index = player_seat_index_v1(opponent);
    let mut refs = VisibleObjectRefsV1::default();
    refs.register_public_cards_v1(&surface.battlefield[seated_index])?;
    refs.register_public_cards_v1(&surface.battlefield[opponent_index])?;
    refs.register_public_cards_v1(&surface.graveyards[seated_index])?;
    refs.register_public_cards_v1(&surface.graveyards[opponent_index])?;
    refs.register_public_cards_v1(&surface.exile)?;
    for item in &surface.stack {
        refs.register_v1(&item.source)?;
        for target in &item.targets {
            refs.register_target_v1(target)?;
        }
    }
    refs.register_private_cards_v1(&observation.own_hand)?;
    for card in &observation.known_library_cards[seated_index] {
        refs.register_v1(&card.card.stable)?;
    }
    for card in &observation.known_library_cards[opponent_index] {
        refs.register_v1(&card.card.stable)?;
    }
    refs.register_private_cards_v1(&observation.known_hand_cards[seated_index])?;
    refs.register_private_cards_v1(&observation.known_hand_cards[opponent_index])?;
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
            acting_player: MtgoPlayerRelativeRoleV1::SeatedPlayer,
            turn: surface.turn,
            phase: surface.phase,
            active_player: relative_player_v1(surface.active_player, seated_player),
            priority_player: relative_player_v1(surface.priority_player, seated_player),
            initiative: surface
                .initiative
                .map(|player| relative_player_v1(player, seated_player)),
            life_totals: [
                surface.life_totals[seated_index],
                surface.life_totals[opponent_index],
            ],
            mana_pools: [
                surface.mana_pools[seated_index],
                surface.mana_pools[opponent_index],
            ],
            hand_counts: [
                surface.hand_counts[seated_index],
                surface.hand_counts[opponent_index],
            ],
            library_counts: [
                surface.library_counts[seated_index],
                surface.library_counts[opponent_index],
            ],
            battlefield: [
                map_results_v1(surface.battlefield[seated_index].iter(), |card| {
                    visible_battlefield_card_v1(card, &refs)
                })?,
                map_results_v1(surface.battlefield[opponent_index].iter(), |card| {
                    visible_battlefield_card_v1(card, &refs)
                })?,
            ],
            graveyards: [
                map_results_v1(surface.graveyards[seated_index].iter(), |card| {
                    visible_named_card_v1(&card.stable, &card.card_name, &refs)
                })?,
                map_results_v1(surface.graveyards[opponent_index].iter(), |card| {
                    visible_named_card_v1(&card.stable, &card.card_name, &refs)
                })?,
            ],
            exile: map_results_v1(surface.exile.iter(), |card| {
                visible_exile_card_v1(card, seated_player, &refs)
            })?,
            stack: map_results_v1(surface.stack.iter(), |item| {
                visible_stack_item_v1(item, seated_player, &refs)
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
                map_results_v1(
                    observation.known_library_cards[seated_index].iter(),
                    |known| visible_known_library_card_v1(known, &refs),
                )?,
                map_results_v1(
                    observation.known_library_cards[opponent_index].iter(),
                    |known| visible_known_library_card_v1(known, &refs),
                )?,
            ],
            known_hand_cards: [
                map_results_v1(observation.known_hand_cards[seated_index].iter(), |card| {
                    visible_named_card_v1(&card.stable, &card.card_name, &refs)
                })?,
                map_results_v1(
                    observation.known_hand_cards[opponent_index].iter(),
                    |card| visible_named_card_v1(&card.stable, &card.card_name, &refs),
                )?,
            ],
        },
        ordered_legal_actions: ordered_actions
            .iter()
            .enumerate()
            .map(|(visible_choice_ordinal, action)| {
                let visible_choice_ordinal =
                    u32::try_from(visible_choice_ordinal).map_err(|_| {
                        error_v1(
                            "player_visible_action_count",
                            "visible legal-action count exceeds the supported ordinal range",
                        )
                    })?;
                visible_action_v1(action, visible_choice_ordinal, seated_player, &refs)
            })
            .collect::<Result<Vec<_>, _>>()?,
    })
}

pub(crate) fn build_player_visible_confirmed_duel_decision_v1(
    decision: &ValidatedMtgoObservedDecisionV1,
    selected_index: usize,
) -> Result<MtgoPlayerVisibleConfirmedDuelDecisionV1, MtgoContractErrorV1> {
    let input = build_player_visible_duel_decision_input_v1(decision)?;
    let selected_action = input
        .ordered_legal_actions
        .get(selected_index)
        .cloned()
        .ok_or_else(|| {
            error_v1(
                "player_visible_selected_action_index",
                "selected action is outside the visible legal-action vector",
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

fn visible_exile_card_v1(
    card: &CardPublicV2,
    acting_player: PlayerSeatV1,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleExileCardV1, MtgoContractErrorV1> {
    let visible_card_name = if card.plotted_turn.is_some() && card.stable.owner != acting_player {
        None
    } else {
        Some(card.card_name.clone())
    };
    Ok(MtgoPlayerVisibleExileCardV1 {
        object_ref: refs.get_v1(&card.stable)?,
        zone_owner: relative_player_v1(card.stable.owner, acting_player),
        visible_card_name,
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
        visible_effective_power: card.characteristics.effective_power,
        visible_effective_toughness: card.characteristics.effective_toughness,
    })
}

fn visible_stack_item_v1(
    item: &StackItemPublicV2,
    seated_player: PlayerSeatV1,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleStackItemV1, MtgoContractErrorV1> {
    Ok(MtgoPlayerVisibleStackItemV1 {
        visible_stack_position: item.stack_index,
        source_object_ref: refs.get_v1(&item.source)?,
        visible_source_name: None,
        controller: relative_player_v1(item.controller, seated_player),
        visible_targets: map_results_v1(item.targets.iter(), |target| {
            visible_target_v1(target, seated_player, refs)
        })?,
        item_kind: item.stack_item_kind,
    })
}

fn visible_target_v1(
    target: &TargetRefV1,
    seated_player: PlayerSeatV1,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleTargetRefV1, MtgoContractErrorV1> {
    match target {
        TargetRefV1::Player { player } => Ok(MtgoPlayerVisibleTargetRefV1::Player {
            player: relative_player_v1(*player, seated_player),
        }),
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
    visible_choice_ordinal: u32,
    seated_player: PlayerSeatV1,
    refs: &VisibleObjectRefsV1,
) -> Result<MtgoPlayerVisibleDuelActionV1, MtgoContractErrorV1> {
    use ActionSemanticV1 as A;
    use MtgoPlayerVisibleDuelActionV1 as V;
    Ok(match action {
        A::Pass { actor } => V::Pass {
            actor: relative_player_v1(*actor, seated_player),
        },
        A::PlayLand { actor, source } => V::PlayLand {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
        },
        A::CastSpell { actor, source } => V::CastSpell {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
        },
        A::ActivateManaAbility {
            actor,
            source,
            mana_choice,
        } => V::ActivateManaAbility {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            mana_choice: *mana_choice,
        },
        A::ActivateAbility {
            actor,
            source,
            ability_index: _,
        } => V::ActivateAbility {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            visible_choice_ordinal,
        },
        A::PlotSpell { actor, source } => V::PlotSpell {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
        },
        A::ChooseTarget {
            actor,
            source,
            remaining,
            target,
        } => V::ChooseTarget {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            remaining: *remaining,
            target: visible_target_v1(target, seated_player, refs)?,
        },
        A::ChooseCostTarget {
            actor,
            source,
            cost_kind,
            remaining,
            candidate,
        } => V::ChooseCostTarget {
            actor: relative_player_v1(*actor, seated_player),
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
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            mode: *mode,
        },
        A::ChooseKicker { actor, source, pay } => V::ChooseKicker {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            pay: *pay,
        },
        A::ChooseSpellMode {
            actor,
            source,
            mode_index: _,
            mode_count: _,
        } => V::ChooseSpellMode {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            visible_choice_ordinal,
        },
        A::ChooseEffectOption {
            actor,
            source,
            option_index: _,
            option_count: _,
        } => V::ChooseEffectOption {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            visible_choice_ordinal,
        },
        A::ChooseEffectTarget {
            actor,
            source,
            target,
            selected_count,
            min_targets,
            max_targets,
        } => V::ChooseEffectTarget {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            target: visible_target_v1(target, seated_player, refs)?,
            selected_count: *selected_count,
            min_targets: *min_targets,
            max_targets: *max_targets,
        },
        A::FinishEffectSelection {
            actor,
            source,
            selected_count,
        } => V::FinishEffectSelection {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            selected_count: *selected_count,
        },
        A::ChooseEffectColor {
            actor,
            source,
            color,
        } => V::ChooseEffectColor {
            actor: relative_player_v1(*actor, seated_player),
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
            actor: relative_player_v1(*actor, seated_player),
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
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            value: *value,
        },
        A::FinishTargetSelection {
            actor,
            source,
            selected_count,
        } => V::FinishTargetSelection {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            selected_count: *selected_count,
        },
        A::ChooseOptionalCostUse { actor, use_cost } => V::ChooseOptionalCostUse {
            actor: relative_player_v1(*actor, seated_player),
            use_cost: *use_cost,
        },
        A::ChooseOptionalCostWhich { actor, choice } => V::ChooseOptionalCostWhich {
            actor: relative_player_v1(*actor, seated_player),
            choice: *choice,
        },
        A::ChooseSpellCopyPayment { actor, source, pay } => V::ChooseSpellCopyPayment {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            pay: *pay,
        },
        A::ChooseSpellCopyRetarget {
            actor,
            source,
            change_target,
        } => V::ChooseSpellCopyRetarget {
            actor: relative_player_v1(*actor, seated_player),
            source: refs.get_v1(source)?,
            change_target: *change_target,
        },
        A::ChooseMadnessCast {
            actor,
            card,
            cast_it,
        } => V::ChooseMadnessCast {
            actor: relative_player_v1(*actor, seated_player),
            card: refs.get_v1(card)?,
            cast_it: *cast_it,
        },
        A::Discard { actor, cards } => V::Discard {
            actor: relative_player_v1(*actor, seated_player),
            cards: map_results_v1(cards.iter(), |card| refs.get_v1(card))?,
        },
        A::DeclareAttackers { actor, attackers } => V::DeclareAttackers {
            actor: relative_player_v1(*actor, seated_player),
            attackers: map_results_v1(attackers.iter(), |attacker| refs.get_v1(attacker))?,
        },
        A::DeclareBlockersForAttacker {
            actor,
            attacker,
            blockers,
        } => V::DeclareBlockersForAttacker {
            actor: relative_player_v1(*actor, seated_player),
            attacker: refs.get_v1(attacker)?,
            blockers: map_results_v1(blockers.iter(), |blocker| refs.get_v1(blocker))?,
        },
        A::ChooseAttackerInclusion {
            actor,
            attacker,
            include,
        } => V::ChooseAttackerInclusion {
            actor: relative_player_v1(*actor, seated_player),
            attacker: refs.get_v1(attacker)?,
            include: *include,
        },
        A::ChooseBlockerInclusion {
            actor,
            attacker,
            blocker,
            include,
        } => V::ChooseBlockerInclusion {
            actor: relative_player_v1(*actor, seated_player),
            attacker: refs.get_v1(attacker)?,
            blocker: refs.get_v1(blocker)?,
            include: *include,
        },
        A::OrderTriggers {
            actor,
            pending_sources,
            order,
        } => {
            let visible_pending_sources =
                map_results_v1(pending_sources.iter(), |source| refs.get_v1(source))?;
            let ordered_sources = order
                .iter()
                .map(|index| {
                    visible_pending_sources.get(*index).copied().ok_or_else(|| {
                        error_v1(
                            "player_visible_trigger_order",
                            "trigger order references a source outside the visible pending set",
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;
            V::OrderTriggers {
                actor: relative_player_v1(*actor, seated_player),
                pending_sources: visible_pending_sources,
                ordered_sources,
            }
        }
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

fn relative_player_v1(
    player: PlayerSeatV1,
    seated_player: PlayerSeatV1,
) -> MtgoPlayerRelativeRoleV1 {
    if player == seated_player {
        MtgoPlayerRelativeRoleV1::SeatedPlayer
    } else {
        MtgoPlayerRelativeRoleV1::Opponent
    }
}

fn other_player_v1(player: PlayerSeatV1) -> PlayerSeatV1 {
    match player {
        PlayerSeatV1::P0 => PlayerSeatV1::P1,
        PlayerSeatV1::P1 => PlayerSeatV1::P0,
    }
}

fn player_seat_index_v1(player: PlayerSeatV1) -> usize {
    match player {
        PlayerSeatV1::P0 => 0,
        PlayerSeatV1::P1 => 1,
    }
}

fn error_v1(code: &'static str, detail: &'static str) -> MtgoContractErrorV1 {
    MtgoContractErrorV1::new(code, detail)
}
