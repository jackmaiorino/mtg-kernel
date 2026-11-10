//! Actor-visible labels for the finite FEB owner.
use super::*;
use crate::premodern_ordered_public_v1::{
    decision::activation_label, observe_human, public_name, GraveyardV1, HumanFullTextFormV1,
    HumanFullTextProducerV1, HumanGraveyardV1, HumanObservationV1, HumanOrderPromptV1,
};
use crate::{
    full_text_v1::FullTextAbilityV1,
    rl::{ActionSemanticV1 as A, CardStableRefV1, TargetRefV1},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FebHumanActionV1 {
    Supported {
        native_ordinal: usize,
        label: String,
    },
    Unsupported {
        native_ordinal: usize,
        action_kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FebHumanSceneV1 {
    pub turn: u32,
    pub phase: crate::rl::ZoneIndependentStepV1,
    pub life: [i32; 2],
    pub hand_counts: [usize; 2],
    pub library_counts: [usize; 2],
    pub own_hand: Vec<String>,
    pub battlefield: [Vec<String>; 2],
}

/// Presentation of a native order prompt. Its authority stays with FebDecisionV1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FebHumanOrderV1 {
    pub schema: &'static str,
    pub viewer: PlayerSeatV1,
    pub chosen_bottom_first: Vec<&'static str>,
    pub action_labels: Vec<String>,
}

impl From<HumanOrderPromptV1> for FebHumanOrderV1 {
    fn from(prompt: HumanOrderPromptV1) -> Self {
        Self {
            schema: prompt.schema,
            viewer: prompt.viewer,
            chosen_bottom_first: prompt.chosen_bottom_first,
            action_labels: prompt.action_labels,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FebHumanOrderedV1 {
    pub schema: &'static str,
    pub card_db_hash: u64,
    pub viewer: PlayerSeatV1,
    pub graveyards: [HumanGraveyardV1; 2],
    pub order_choice: Option<FebHumanOrderV1>,
    pub full_text: Option<Vec<HumanFullTextFormV1>>,
    pub producers: Vec<HumanFullTextProducerV1>,
}

impl From<HumanObservationV1> for FebHumanOrderedV1 {
    fn from(view: HumanObservationV1) -> Self {
        Self {
            schema: view.schema,
            card_db_hash: view.card_db_hash,
            viewer: view.viewer,
            graveyards: view.graveyards,
            order_choice: view.order_choice.map(Into::into),
            full_text: view.full_text,
            producers: view.producers,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FebHumanViewV1 {
    pub profile: &'static str,
    pub step: u64,
    pub actor: PlayerSeatV1,
    pub ordered: FebHumanOrderedV1,
    pub scene: Option<FebHumanSceneV1>,
    pub actions: Vec<FebHumanActionV1>,
}

fn card_label(
    card: &CardStableRefV1,
    viewer: PlayerSeatV1,
    graveyards: &[GraveyardV1; 2],
) -> Result<String, String> {
    let name = public_name(card)?;
    if card.zone != crate::state::Zone::Graveyard {
        return Ok(name.into());
    }
    let position = graveyards[native_seat(card.owner).index()]
        .bottom_to_top
        .iter()
        .position(|visible| visible == card)
        .ok_or("FEB action lacks its ordered graveyard position")?;
    let owner = if card.owner == viewer {
        "your"
    } else {
        "your opponent's"
    };
    Ok(format!(
        "{name} ({} from bottom of {owner} graveyard)",
        position + 1
    ))
}

fn target_name(
    target: &TargetRefV1,
    viewer: PlayerSeatV1,
    graveyards: &[GraveyardV1; 2],
) -> Result<String, String> {
    Ok(match target {
        TargetRefV1::Player { player } => if *player == viewer {
            "you"
        } else {
            "your opponent"
        }
        .into(),
        TargetRefV1::Object { object } => card_label(object, viewer, graveyards)?,
        TargetRefV1::StackItem {
            source,
            stack_index,
            ..
        } => {
            let name = public_name(source)?;
            stack_index.map_or_else(
                || format!("{name} on the stack"),
                |index| format!("{name} at stack position {}", u64::from(index) + 1),
            )
        }
    })
}

fn names(cards: &[CardStableRefV1]) -> Result<String, String> {
    if cards.is_empty() {
        return Ok("none".into());
    }
    Ok(cards
        .iter()
        .map(public_name)
        .collect::<Result<Vec<_>, _>>()?
        .join(", "))
}

fn label(
    state: &GameState,
    row: &NumericNativeActionV1,
    viewer: PlayerSeatV1,
    graveyards: &[GraveyardV1; 2],
) -> Result<Option<String>, String> {
    let action = &row.action.semantic;
    Ok(Some(match action {
        A::Pass { .. } => "Pass priority".into(),
        A::ChooseLondonMulligan { mulligan, .. } => if *mulligan {
            "Take a mulligan"
        } else {
            "Keep this hand"
        }
        .into(),
        A::ChooseLondonBottom { card, .. } => {
            format!("Put {} on the bottom of your library", public_name(card)?)
        }
        A::PlayLand { source, .. } => format!("Play {}", public_name(source)?),
        A::CastSpell { source, .. } => format!("Begin casting {}", public_name(source)?),
        A::ActivateManaAbility {
            source,
            mana_choice,
            cost_target,
            ..
        } => {
            if cost_target.is_some() {
                return Ok(None);
            }
            let name = public_name(source)?;
            match (donated_mana(state, action), mana_choice) {
                (Some(DonatedManaV1::BirdsOfParadise), Some(color)) => format!("Tap {name} to add {{{color:?}}} (Birds of Paradise text)"),
                (Some(DonatedManaV1::BirdsOfParadise), None) => return Ok(None),
                (Some(DonatedManaV1::WallOfRoots), _) => format!("Put a -0/-1 counter on {name} to add {{G}} (Wall of Roots text, once each turn)"),
                (None, Some(color)) => format!("Activate the mana ability of {name} for {{{color:?}}}"),
                (None, None) => format!("Activate the mana ability of {name}"),
            }
        }
        A::ActivateAbility {
            source,
            ability_index,
            granted_ability,
            ..
        } => {
            let name = public_name(source)?;
            let program = full_text_action(row).or_else(|| {
                if granted_ability.as_ref().is_some() {
                    return None;
                }
                FullTextAbilityV1::Printed(*ability_index)
                    .public_program(&CARD_DEFS[usize::from(source.card_db_id)])
            });
            let description = match (
                program,
                name,
                *ability_index,
                source.zone,
                granted_ability.as_ref(),
            ) {
                (Some(program), ..) => activation_label(program)?,
                (None, "Survival of the Fittest", 0, crate::state::Zone::Battlefield, None) => {
                    "Pay {G} and discard a creature card to search for a creature card"
                }
                (None, "Elvish Spirit Guide", 0, crate::state::Zone::Hand, None) => {
                    "Exile this card from your hand to add {G}"
                }
                _ => return Ok(None),
            };
            format!("{name}: {description}")
        }
        A::ChooseTarget { source, target, .. } | A::ChooseEffectTarget { source, target, .. } => {
            format!(
                "Choose {} for {}",
                target_name(target, viewer, graveyards)?,
                public_name(source)?
            )
        }
        A::ChooseCostTarget {
            source, candidate, ..
        } => format!(
            "Choose {} to pay the cost for {}",
            card_label(candidate, viewer, graveyards)?,
            public_name(source)?
        ),
        A::FinishEffectSelection { .. } | A::FinishTargetSelection { .. } => {
            "Finish selecting".into()
        }
        A::ChooseEffectColor { color, .. } => format!("Choose {color:?}"),
        A::ChooseEffectNumber { number, .. } => format!("Choose {number}"),
        A::Discard { cards, .. } => format!("Discard {}", names(cards)?),
        A::DeclareAttackers { attackers, .. } => format!("Attack with {}", names(attackers)?),
        A::DeclareBlockersForAttacker {
            attacker, blockers, ..
        } => format!("Block {} with {}", public_name(attacker)?, names(blockers)?),
        A::ChooseAttackTarget {
            attacker, target, ..
        } => format!(
            "Attack {} with {}",
            target_name(target, viewer, graveyards)?,
            public_name(attacker)?
        ),
        A::ChooseLegendPermanent { keep, .. } => format!("Keep {}", public_name(keep)?),
        A::OrderTriggers {
            pending_sources,
            order,
            ..
        } => {
            let ordered = order
                .iter()
                .map(|&index| {
                    pending_sources
                        .get(index)
                        .ok_or("FEB trigger ordinal outside the menu")
                        .cloned()
                })
                .collect::<Result<Vec<_>, _>>()?;
            format!("Stack triggers bottom first: {}", names(&ordered)?)
        }
        _ => return Ok(None),
    }))
}

impl FebSessionV1 {
    pub fn human_current(&self, viewer: PlayerSeatV1) -> Result<Option<FebHumanViewV1>, String> {
        let Ok(BoundBoundaryV1::Decision(binding)) = &self.inner.current else {
            return Ok(None);
        };
        let seat = native_seat(viewer);
        let state = &self.inner.game;
        let view = binding
            .view(state, &self.inner.surface, seat, self.inner.step)
            .map_err(|_| "FEB ordered Human observation is unsupported")?;
        let (_, action_values) = visible_numeric_input(state, &view)?;
        let ordered =
            observe_human(state, seat).map_err(|_| "FEB ordered Human facts are unsupported")?;
        let actions = match view.menu {
            None => Vec::new(),
            Some(NumericMenuV1::GraveyardOrder) => ordered
                .order_choice
                .as_ref()
                .ok_or("FEB ordered Human choice is absent")?
                .action_labels()
                .iter()
                .enumerate()
                .map(|(native_ordinal, label)| FebHumanActionV1::Supported {
                    native_ordinal,
                    label: label.clone(),
                })
                .collect(),
            Some(NumericMenuV1::Native { actions }) => actions
                .iter()
                .zip(action_values)
                .enumerate()
                .map(|(native_ordinal, (row, action))| {
                    Ok(match label(state, row, viewer, &view.observation.ordered.graveyards)? {
                        Some(label) => FebHumanActionV1::Supported {
                            native_ordinal,
                            label,
                        },
                        None => FebHumanActionV1::Unsupported {
                            native_ordinal,
                            action_kind: action["action"]["action_kind"]
                                .as_str()
                                .unwrap_or("unknown")
                                .into(),
                        },
                    })
                })
                .collect::<Result<Vec<_>, String>>()?,
        };
        let scene = view.observation.stable.map(|stable| FebHumanSceneV1 {
            turn: stable.projection.turn,
            phase: stable.projection.phase,
            life: stable.projection.life_totals,
            hand_counts: stable.projection.hand_counts,
            library_counts: stable.projection.library_counts,
            own_hand: stable
                .own_hand
                .into_iter()
                .map(|card| card.card_name)
                .collect(),
            battlefield: stable
                .projection
                .battlefield
                .map(|cards| cards.into_iter().map(|card| card.card_name).collect()),
        });
        Ok(Some(FebHumanViewV1 {
            profile: PROFILE_V1,
            step: self.inner.step,
            actor: binding.actor.into(),
            ordered: ordered.into(),
            scene,
            actions,
        }))
    }

    pub fn step_human(
        &mut self,
        decision: &FebDecisionV1,
        native_ordinal: usize,
    ) -> Result<(), String> {
        if !Arc::ptr_eq(&self.owner, &decision.owner) || self.revision != decision.revision {
            return Err("FEB Human decision is foreign or stale".into());
        }
        let view = self
            .human_current(decision.actor.into())?
            .ok_or("FEB Human session has no current decision")?;
        if !matches!(
            view.actions.get(native_ordinal),
            Some(FebHumanActionV1::Supported { .. })
        ) {
            return Err("FEB Human action meaning is unsupported or outside the menu".into());
        }
        self.step(decision, native_ordinal)
    }
}
