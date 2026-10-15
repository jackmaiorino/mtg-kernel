//! Sealed Twinflame successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::rl::ActionSemanticV1;
use crate::rl_session::twinflame_v38::TwinflameDecisionBindingV38;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use crate::twinflame_observation_v1::TwinflameObservationV1;
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV38 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV38 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV38 {}
pub struct HumanTwinflameSessionV38 {
    inner: FastActorSessionV1,
    authority: AuthorityV38,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanTwinflameSnapshotV38 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanTwinflameSessionV38 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_twinflame_v38()?,
            human_seat,
            authority: AuthorityV38 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v38(&self) -> HumanTwinflameSnapshotV38 {
        HumanTwinflameSnapshotV38 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v38(&mut self, snapshot: &HumanTwinflameSnapshotV38) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .twinflame_restore_v38(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::twinflame_observation_v1::TwinflameObservationV1, crate::rl::RlContractError>
    {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Twinflame Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .twinflame_current_decision_input_v38(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Twinflame public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &TwinflameObservationV1,
            &[ActionSemanticV1],
        ) -> Result<u32, crate::rl::RlContractError>,
    {
        if expected.acting_player == self.human_seat {
            return Err(crate::rl::RlContractError(
                "opponent callback cannot act for the Human seat".into(),
            ));
        }
        let (observation, actions, binding) = self
            .inner
            .twinflame_current_decision_input_v38(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .twinflame_validate_bound_decision_v38(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionTwinflameV38 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanTwinflameExtensionsV38>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanTwinflameSourceV38 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanMultiplierSourceV38 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
    pub doubles_controlled_source_damage_to_opponents: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanTwinflameOptionV38 {
    pub kind: String,
    pub source: HumanCardRefV1,
    pub shield_remaining: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanTwinflameMenuV38 {
    pub chooser: PlayerSeatV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_history: Option<crate::crystal_public_v1::SavedDamageSourceV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixed_source_history: Option<crate::crystal_public_v1::SavedFixedDamageSourceV3>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raid_source_history: Option<crate::crystal_public_v1::SavedRaidDamageSourceV4>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simultaneous_choices_remaining: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creature_power_at_resolution: Option<i32>,
    pub source: HumanCardRefV1,
    pub resolving_source: HumanCardRefV1,
    pub recipient: HumanDamageRecipientV38,
    pub is_combat: bool,
    pub original_damage: i32,
    pub applied_multipliers: Vec<HumanCardRefV1>,
    pub remaining_damage: i32,
    pub options: Vec<HumanTwinflameOptionV38>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanTwinflameDiscardMenuV38 {
    pub chooser: PlayerSeatV1,
    pub source: HumanCardRefV1,
    pub maximum_cards: u16,
    pub draws_if_discarded: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanTwinflameProgramMenuV38 {
    pub chooser: PlayerSeatV1,
    pub source: HumanCardRefV1,
    pub requires_original_source_creature: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanEquipmentDamageSourceV38 {
    pub stack_ordinal: u32,
    pub source: HumanCardRefV1,
    pub equipment: HumanCardRefV1,
    pub ability_controller: PlayerSeatV1,
    pub damage_controller: PlayerSeatV1,
    pub historical: bool,
    pub color_mask: u8,
    pub lifelink: bool,
    pub deathtouch: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanTwinflameExtensionsV38 {
    pub predecessor: HumanExtensionsV9,
    pub static_sources: Vec<HumanTwinflameSourceV38>,
    pub multiplier_sources: Vec<HumanMultiplierSourceV38>,
    pub combat_allocation: Option<HumanAllocationMenuV38>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub equipment_damage_sources: Vec<HumanEquipmentDamageSourceV38>,
    pub prevention_menu: Option<HumanTwinflameMenuV38>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional_discard_menu: Option<HumanTwinflameDiscardMenuV38>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional_fight_menu: Option<HumanTwinflameProgramMenuV38>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HumanDamageRecipientV38 {
    You,
    Opponent,
    Object { object: HumanCardRefV1 },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanIncomingPacketV38 {
    pub source: HumanCardRefV1,
    pub recipient: HumanDamageRecipientV38,
    pub amount: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanAllocationMenuV38 {
    pub chooser: PlayerSeatV1,
    pub packets: Vec<HumanIncomingPacketV38>,
}
fn recipient(
    row: &crate::rl::TargetRefV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<HumanDamageRecipientV38, Error> {
    match row {
        crate::rl::TargetRefV1::Player { player } => Ok(if *player == human {
            HumanDamageRecipientV38::You
        } else {
            HumanDamageRecipientV38::Opponent
        }),
        crate::rl::TargetRefV1::Object { object } => Ok(HumanDamageRecipientV38::Object {
            object: handles.get(object)?.clone(),
        }),
        _ => Err(Error::InvalidVisibleReference),
    }
}
fn target_name(
    row: &crate::rl::TargetRefV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    match row {
        crate::rl::TargetRefV1::Player { player } => Ok(if *player == human {
            "you".into()
        } else {
            "the opponent".into()
        }),
        crate::rl::TargetRefV1::Object { object } => handles.name(object),
        _ => Err(Error::InvalidVisibleReference),
    }
}
fn insert_target(row: &crate::rl::TargetRefV1, handles: &mut Handles) -> Result<(), Error> {
    if let crate::rl::TargetRefV1::Object { object } = row {
        handles.insert(object)?;
    }
    Ok(())
}
fn parts(observation: &TwinflameObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
    super::life_ward_v8::ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions,
    }
}
fn state(
    observation: &TwinflameObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanTwinflameExtensionsV38>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let static_sources = observation
        .static_sources
        .iter()
        .map(|row| {
            Ok(HumanTwinflameSourceV38 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let optional_discard_menu = observation
        .optional_discard_menu
        .as_ref()
        .map(|row| {
            Ok::<_, Error>(HumanTwinflameDiscardMenuV38 {
                chooser: row.chooser,
                source: handles.get(&row.source)?.clone(),
                maximum_cards: row.maximum_cards,
                draws_if_discarded: row.draws_if_discarded,
            })
        })
        .transpose()?;
    let optional_fight_menu = observation
        .optional_fight_menu
        .as_ref()
        .map(|row| {
            Ok::<_, Error>(HumanTwinflameProgramMenuV38 {
                chooser: row.chooser,
                source: handles.get(&row.source)?.clone(),
                requires_original_source_creature: row.requires_original_source_creature,
            })
        })
        .transpose()?;
    let prevention_menu = observation
        .prevention_menu
        .as_ref()
        .map(|row| {
            let options = row
                .options
                .iter()
                .map(|option| {
                    Ok(HumanTwinflameOptionV38 {
                        kind: option.kind.clone(),
                        source: handles.get(&option.source)?.clone(),
                        shield_remaining: option.shield_remaining,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok::<_, Error>(HumanTwinflameMenuV38 {
                chooser: row.chooser,
                source_history: row.source_history.clone(),
                fixed_source_history: row.fixed_source_history.clone(),
                raid_source_history: row.raid_source_history.clone(),
                simultaneous_choices_remaining: row.simultaneous_choices_remaining,
                creature_power_at_resolution: row.creature_power_at_resolution,
                source: handles.get(&row.source)?.clone(),
                resolving_source: handles.get(&row.resolving_source)?.clone(),
                recipient: recipient(&row.recipient, handles, human)?,
                is_combat: row.is_combat,
                original_damage: row.original_damage,
                applied_multipliers: row
                    .applied_multipliers
                    .iter()
                    .map(|source| handles.get(source).cloned())
                    .collect::<Result<Vec<_>, Error>>()?,
                remaining_damage: row.remaining_damage,
                options,
            })
        })
        .transpose()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanTwinflameExtensionsV38 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            static_sources,
            multiplier_sources: observation
                .multiplier_sources
                .iter()
                .map(|row| {
                    Ok(HumanMultiplierSourceV38 {
                        source: handles.get(&row.source)?.clone(),
                        controller: row.controller,
                        doubles_controlled_source_damage_to_opponents: true,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?,
            combat_allocation: observation
                .combat_allocation
                .as_ref()
                .map(|menu| {
                    Ok::<_, Error>(HumanAllocationMenuV38 {
                        chooser: menu.chooser,
                        packets: menu
                            .packets
                            .iter()
                            .map(|packet| {
                                Ok(HumanIncomingPacketV38 {
                                    source: handles.get(&packet.source)?.clone(),
                                    recipient: recipient(&packet.recipient, handles, human)?,
                                    amount: packet.amount,
                                })
                            })
                            .collect::<Result<Vec<_>, Error>>()?,
                    })
                })
                .transpose()?,
            equipment_damage_sources: observation
                .equipment_damage_sources
                .iter()
                .map(|row| {
                    Ok(HumanEquipmentDamageSourceV38 {
                        stack_ordinal: row.stack_ordinal,
                        source: handles.get(&row.source)?.clone(),
                        equipment: handles.get(&row.equipment)?.clone(),
                        ability_controller: row.ability_controller,
                        damage_controller: row.damage_controller,
                        historical: row.historical,
                        color_mask: row.color_mask,
                        lifelink: row.lifelink,
                        deathtouch: row.deathtouch,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?,
            prevention_menu,
            optional_discard_menu,
            optional_fight_menu,
        },
        policy_context: old.policy_context,
        known_face_down_cards: old.known_face_down_cards,
        poison_counters: old.poison_counters,
        poison_prevention: old.poison_prevention,
        creatures_attacked_this_turn: old.creatures_attacked_this_turn,
        ninja_emblems: old.ninja_emblems,
        restricted_mana: old.restricted_mana,
    })
}
fn label(
    action: &ActionSemanticV1,
    observation: &TwinflameObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(menu) = observation.combat_allocation.as_ref() {
        let ActionSemanticV1::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } = action
        else {
            return Err(Error::UnsupportedPrompt);
        };
        if *actor != human
            || menu.chooser != human
            || *source != menu.action_source
            || usize::from(*option_count) != menu.packets.len()
        {
            return Err(Error::InvalidVisibleReference);
        }
        let packet = menu
            .packets
            .get(*option_index as usize)
            .ok_or(Error::InvalidAction)?;
        return Ok(format!(
            "Apply replacements to {} combat damage from {} to {}",
            packet.amount,
            handles.name(&packet.source)?,
            target_name(&packet.recipient, handles, human)?
        ));
    }
    if let Some(menu) = observation.optional_fight_menu.as_ref() {
        let ActionSemanticV1::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } = action
        else {
            return Err(Error::UnsupportedPrompt);
        };
        if *actor != human
            || menu.chooser != human
            || *source != menu.source
            || *option_count != 2
            || menu.decline_index != 0
            || menu.fight_index != 1
            || !menu.requires_original_source_creature
        {
            return Err(Error::InvalidVisibleReference);
        }
        return match *option_index {
            0 => Ok("Decline the optional fight".into()),
            1 => Ok(format!(
                "Have {} fight the chosen opposing creature",
                handles.name(&menu.source)?
            )),
            _ => Err(Error::InvalidAction),
        };
    }
    if let Some(menu) = observation.optional_discard_menu.as_ref() {
        if menu.chooser != human || menu.maximum_cards != 1 || menu.draws_if_discarded != 1 {
            return Err(Error::InvalidVisibleReference);
        }
        return match action {
            ActionSemanticV1::ChooseEffectTarget {
                actor,
                source,
                target,
                selected_count,
                min_targets,
                max_targets,
            } if *actor == human
                && *source == menu.source
                && *selected_count == 0
                && *min_targets == 0
                && *max_targets == 1 =>
            {
                let crate::rl::TargetRefV1::Object { object } = target else {
                    return Err(Error::InvalidVisibleReference);
                };
                if !observation.own_hand.iter().any(|row| row.stable == *object) {
                    return Err(Error::InvalidVisibleReference);
                }
                Ok(format!(
                    "Discard {}, then draw a card",
                    handles.name(object)?
                ))
            }
            ActionSemanticV1::FinishEffectSelection {
                actor,
                source,
                selected_count,
            } if *actor == human && *source == menu.source && *selected_count == 0 => {
                Ok("Decline the optional discard".into())
            }
            _ => Err(Error::UnsupportedPrompt),
        };
    }
    if let Some(menu) = observation.prevention_menu.as_ref() {
        let ActionSemanticV1::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } = action
        else {
            return Err(Error::UnsupportedPrompt);
        };
        if *actor != human
            || menu.chooser != human
            || *source != menu.resolving_source
            || usize::from(*option_count) != menu.options.len()
        {
            return Err(Error::InvalidVisibleReference);
        }
        let option = menu
            .options
            .get(usize::from(*option_index))
            .ok_or(Error::InvalidAction)?;
        let name = handles.name(&option.source)?;
        let target = target_name(&menu.recipient, handles, human)?;
        return Ok(match option.kind.as_str() {
            "multiplier" => format!("Apply {name}: double the remaining damage to {target}"),
            "combat-prevention" if menu.is_combat => {
                format!("Apply {name}: prevent the remaining combat damage to {target}")
            }
            "barricade" => {
                format!("Apply {name}: prevent all remaining noncombat damage to {target}")
            }
            "protection" => format!("Apply {target}'s protection: prevent the remaining damage"),
            "color" => format!("Apply color prevention from {name}: prevent the remaining damage"),
            "shield" => format!(
                "Apply {name}'s shield ({} remaining) to damage to {target}",
                option
                    .shield_remaining
                    .ok_or(Error::InvalidVisibleReference)?
            ),
            _ => return Err(Error::UnsupportedPrompt),
        });
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}
type ProjectedTwinflameV38 = (
    HumanVisibleStateV1<HumanTwinflameExtensionsV38>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &TwinflameObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedTwinflameV38, Error> {
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    observation
        .validate_artifact()
        .map_err(|_| Error::InvalidVisibleReference)?;
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        observation.extensions.decision_local_library.as_ref(),
    )?;
    register_for_akroma_parts(observation, &mut handles)?;
    let initial = state(observation, &mut handles, human)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    unary_for_akroma_parts(&initial.extensions, &mut unary)?;
    handles.canonicalize_parts(&initial, unary)?;
    let state = state(observation, &mut handles, human)?;
    let mut choices = actions
        .iter()
        .enumerate()
        .map(|(index, action)| {
            Ok((
                label(action, observation, &handles, human)?,
                u32::try_from(index).map_err(|_| Error::InvalidAction)?,
            ))
        })
        .collect::<Result<Vec<_>, Error>>()?;
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let indexes = choices.iter().map(|(_, index)| *index).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| HumanLegalActionV1 {
            action_index: index as u32,
            label,
        })
        .collect();
    Ok((state, actions, indexes))
}

pub(super) fn register_for_akroma_parts(
    observation: &TwinflameObservationV1,
    handles: &mut Handles,
) -> Result<(), Error> {
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &observation.extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.static_sources {
        handles.insert(&row.source)?;
    }
    for row in &observation.equipment_damage_sources {
        handles.insert(&row.source)?;
        handles.insert(&row.equipment)?;
    }
    if let Some(row) = observation.prevention_menu.as_ref() {
        handles.insert(&row.source)?;
        handles.insert(&row.resolving_source)?;
        insert_target(&row.recipient, handles)?;
        for source in &row.applied_multipliers {
            handles.insert(source)?;
        }
        for option in &row.options {
            handles.insert(&option.source)?;
        }
    }
    if let Some(row) = observation.optional_discard_menu.as_ref() {
        handles.insert(&row.source)?;
    }
    for row in &observation.multiplier_sources {
        handles.insert(&row.source)?;
    }
    if let Some(menu) = &observation.combat_allocation {
        handles.insert(&menu.action_source)?;
        for packet in &menu.packets {
            handles.insert(&packet.source)?;
            insert_target(&packet.recipient, handles)?;
        }
    }
    Ok(())
}
pub(super) fn state_for_akroma_parts(
    observation: &TwinflameObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanTwinflameExtensionsV38, Error> {
    Ok(state(observation, handles, human)?.extensions)
}
pub(super) fn label_for_akroma_parts(
    action: &ActionSemanticV1,
    observation: &TwinflameObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    label(action, observation, handles, human)
}
pub(super) fn unary_for_akroma_parts(
    extension: &HumanTwinflameExtensionsV38,
    unary: &mut BTreeMap<String, Vec<String>>,
) -> Result<(), Error> {
    for row in &extension.predecessor.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.equipment_damage_sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.static_sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    if let Some(row) = extension.prevention_menu.as_ref() {
        if let HumanDamageRecipientV38::Object { object } = &row.recipient {
            unary
                .entry(object.handle.clone())
                .or_default()
                .push(visible::without_handles(row)?);
        }
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for option in &row.options {
            unary
                .entry(option.source.handle.clone())
                .or_default()
                .push(visible::without_handles(option)?);
        }
    }
    if let Some(row) = extension.optional_discard_menu.as_ref() {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for row in &extension.multiplier_sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(format!(
                "controlled-source-double:{}",
                visible::without_handles(row)?
            ));
    }
    if let Some(menu) = &extension.combat_allocation {
        for packet in &menu.packets {
            unary
                .entry(packet.source.handle.clone())
                .or_default()
                .push(visible::without_handles(packet)?);
            if let HumanDamageRecipientV38::Object { object } = &packet.recipient {
                unary
                    .entry(object.handle.clone())
                    .or_default()
                    .push(visible::without_handles(packet)?);
            }
        }
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    Ok(())
}
struct BoundTwinflameV38 {
    authority: AuthorityV38,
    expected: FastActorDecisionV1,
    binding: TwinflameDecisionBindingV38,
    public: crate::twinflame_observation_v1::TwinflameObservationV1,
    visible: HumanDecisionTwinflameV38,
    engine_indexes: Vec<u32>,
}
pub struct HumanTwinflameProjectorV38 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundTwinflameV38>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV38)>,
}
impl HumanTwinflameProjectorV38 {
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
        session: &HumanTwinflameSessionV38,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionTwinflameV38, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .twinflame_current_decision_input_v38(expected, self.human_seat)
            .map_err(|_| Error::StaleDecision)?;
        let public = session
            .public_observation(expected)
            .map_err(|_| Error::StaleDecision)?;
        if let Some(pending) = &self.pending {
            if pending.authority == session.authority
                && pending.expected == expected
                && pending.binding == binding
                && pending.public == public
            {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (state, actions, engine_indexes) =
            project_base(&observation, &actions, self.human_seat)?;
        let visible = HumanDecisionTwinflameV38 {
            schema: "mtg-kernel-human-twinflame-decision/v38".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundTwinflameV38 {
            authority: session.authority.clone(),
            expected,
            binding,
            public,
            visible: visible.clone(),
            engine_indexes,
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }
    pub fn submit(
        &mut self,
        session: &mut HumanTwinflameSessionV38,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, Error> {
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
        let index = *pending
            .engine_indexes
            .get(request.action_index as usize)
            .ok_or(Error::InvalidAction)?;
        session
            .inner
            .twinflame_validate_bound_decision_v38(
                pending.expected,
                self.human_seat,
                &pending.binding,
            )
            .map_err(|_| Error::StaleDecision)?;
        if session
            .public_observation(pending.expected)
            .map_err(|_| Error::StaleDecision)?
            != pending.public
        {
            return Err(Error::StaleDecision);
        }
        let result = session
            .inner
            .step(pending.expected.episode_id, pending.expected.step, index);
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
mod tests;
