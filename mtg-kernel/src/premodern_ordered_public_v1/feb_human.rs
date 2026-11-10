//! Actor-visible labels for the finite FEB owner.
use super::*;
use crate::premodern_ordered_public_v1::{
    captured_card, decision::activation_label, observe_human, public_name, GraveyardV1,
    HumanFullTextFormV1, HumanFullTextProducerV1, HumanGraveyardV1, HumanObservationV1,
    HumanOrderPromptV1,
};
use crate::{
    card_def::{CostComponent, TargetSpec},
    effect::{
        self, EffectContinuation, EffectOp, EffectOptionChoicePurpose, PendingEffectChoice,
        PlayerRef,
    },
    eternal_cards_v1::EternalOpV1,
    eternal_legacy_e7a_v1::E7aOpV1,
    eternal_premodern_p2c_v1::KARMIC_GUIDE,
    eternal_premodern_p2d_v1 as echo,
    full_text_v1::FullTextAbilityV1,
    mana::Cost,
    rl::{ActionSemanticV1 as A, CardStableRefV1, TargetRefV1},
    standard_cards_v1::StandardOpV1,
    state::{StackItem, StackItemKind, Zone},
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

pub(super) struct NativeEffectOptionMenu {
    source: CardStableRefV1,
    labels: Vec<String>,
}

impl NativeEffectOptionMenu {
    pub(super) fn bind_guide_echo_source(&self, actions: &mut [NumericNativeActionV1]) {
        for row in actions {
            let A::ChooseEffectOption {
                actor,
                source,
                option_index,
                option_count,
            } = &mut row.action.semantic
            else {
                continue;
            };
            if *actor == self.source.controller
                && source.arena_id == self.source.arena_id
                && usize::from(*option_count) == self.labels.len()
                && *option_index < *option_count
            {
                *source = self.source.clone();
            }
        }
    }
}

fn cabal_therapy_name(option: &EffectOp) -> Option<&'static str> {
    let EffectOp::StandardV1(StandardOpV1::EternalV1(EternalOpV1::E7aV1(E7aOpV1::TherapyV1 {
        named,
    }))) = option
    else {
        return None;
    };
    Some(CARD_DEFS.get(usize::from(*named))?.object_name)
}

fn pending_generic_options(
    state: &GameState,
    viewer: PlayerId,
) -> Option<(&EffectContinuation, &[u16], &[EffectOp])> {
    let pending = state.engine.pending_effect.as_ref()?;
    let PendingEffectChoice::ChooseOption {
        player,
        path,
        options,
        purpose: EffectOptionChoicePurpose::Generic,
    } = pending.choice.as_ref()?
    else {
        return None;
    };
    let item = &pending.resolving_item;
    if *player != viewer
        || item.controller != viewer
        || item.is_copy
        || !pending.frames.is_empty()
        || pending.answered_choice_guard.is_some()
    {
        return None;
    }
    Some((pending, path, options))
}

fn cabal_therapy_menu(state: &GameState, viewer: PlayerId) -> Option<NativeEffectOptionMenu> {
    let (pending, path, options) = pending_generic_options(state, viewer)?;
    let item = &pending.resolving_item;
    if item.kind != StackItemKind::Spell {
        return None;
    }
    let source = rl::card_ref(state, item.source).ok()?;
    let labels = cabal_therapy_labels(item, &source, path, options)?;
    effect::validate_pending_effect_choice(state).ok()?;
    Some(NativeEffectOptionMenu { source, labels })
}

fn cabal_therapy_labels(
    spell: &StackItem,
    source: &CardStableRefV1,
    path: &[u16],
    options: &[EffectOp],
) -> Option<Vec<String>> {
    if spell.inline_effect.is_some()
        || source.zone != Zone::Stack
        || Some(source.card_db_id) != card_id_by_name("Cabal Therapy")
        || path.len() > 1
    {
        return None;
    }
    // Generic native choices validate their operations individually. Bind this
    // menu to the ordered options of the printed spell at the same path.
    let definition = &CARD_DEFS[usize::from(source.card_db_id)];
    let printed = definition.printed_mode_effect(spell.mode_chosen)?;
    let EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: printed_options,
    } = effect::effect_op_at_structural_path(&printed, path)?
    else {
        return None;
    };
    if options != printed_options.as_slice() {
        return None;
    }

    options
        .iter()
        .map(|option| {
            if path.is_empty() {
                let EffectOp::Choice {
                    controller: PlayerRef::Controller,
                    options,
                } = option
                else {
                    return None;
                };
                let letter = cabal_therapy_name(options.first()?)?
                    .chars()
                    .next()?
                    .to_ascii_uppercase();
                Some(format!(
                    "Choose names starting with {letter} for Cabal Therapy"
                ))
            } else {
                let name = cabal_therapy_name(option)?;
                Some(format!("Name {name} for Cabal Therapy"))
            }
        })
        .collect()
}

pub(super) fn guide_echo_menu(
    state: &GameState,
    viewer: PlayerId,
) -> Option<NativeEffectOptionMenu> {
    let (pending, path, options) = pending_generic_options(state, viewer)?;
    let item = &pending.resolving_item;
    let captured = item.v4.ability_source_contract?;
    if item.kind != StackItemKind::TriggeredAbility
        || !path.is_empty()
        || captured.zone != Zone::Battlefield
        || captured.controller != viewer
        || captured.source != item.source
    {
        return None;
    }
    let inline = item.inline_effect.as_ref()?;
    let donated = full_text_v1::trigger_program(state, item).ok()?;
    let program = match donated.as_ref() {
        Some((program, TargetSpec::None)) if *program == echo::donated_guide_echo_trigger() => {
            program
        }
        None if Some(captured.card_def) == card_id_by_name(KARMIC_GUIDE)
            && echo::delayed_effect_matches(KARMIC_GUIDE, inline) =>
        {
            inline
        }
        _ => return None,
    };
    let EffectOp::Choice {
        controller: PlayerRef::Controller,
        options: canonical,
    } = program
    else {
        return None;
    };
    if options != canonical.as_slice() {
        return None;
    }
    crate::engine::validate_historical_ability_source_contract(state, captured).ok()?;
    effect::validate_pending_effect_choice(state).ok()?;

    let source = captured_card(captured);
    let name = public_name(&source).ok()?;
    let (colors, generic) = echo::echo_cost(KARMIC_GUIDE)?;
    let colored_cost: String = colors
        .iter()
        .map(|color| format!("{{{color:?}}}"))
        .collect();
    let payment = format!("{{{generic}}}{colored_cost}");
    let live = crate::eternal_premodern_p1a_v1::live_ability_source(&pending.ctx, state);
    let labels = match live {
        Some(live) if state.objects.get(live).controller == viewer => vec![
            format!("Pay {payment} to keep {name} (Karmic Guide echo, sacrifice if unpaid)"),
            format!("Sacrifice {name} (Karmic Guide echo)"),
        ],
        Some(_) => vec![
            format!("Pay {payment} for Karmic Guide echo from {name} (source changed control)"),
            format!("Decline Karmic Guide echo from {name} (source changed control)"),
        ],
        None => vec![
            if donated.is_some() {
                format!("Pay {payment} for Karmic Guide echo from {name} (original source left)")
            } else {
                format!("Choose payment for Karmic Guide echo from {name} (original source left, no mana due)")
            },
            format!("Decline Karmic Guide echo from {name} (original source left)"),
        ],
    };
    Some(NativeEffectOptionMenu { source, labels })
}

fn unearth_cycling(state: &GameState, source: &CardStableRefV1, viewer: PlayerSeatV1) -> bool {
    if source.owner != viewer
        || source.controller != viewer
        || Some(source.card_db_id) != card_id_by_name("Unearth")
        || !rl::card_ref(state, ObjectId(source.arena_id)).is_ok_and(|live| live == *source)
    {
        return false;
    }
    let Some(ability) = CARD_DEFS[usize::from(source.card_db_id)]
        .activated_abilities
        .first()
    else {
        return false;
    };
    ability.activation_zone == Zone::Hand
        && ability.target_spec == TargetSpec::None
        && ability.cost
            == [
                CostComponent::Mana(Cost {
                    pips: &[],
                    generic: 2,
                    x_count: 0,
                }),
                CostComponent::DiscardSelf,
            ]
        && (ability.effect)()
            == (EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            })
}

fn label(
    state: &GameState,
    row: &NumericNativeActionV1,
    viewer: PlayerSeatV1,
    graveyards: &[GraveyardV1; 2],
    effect_options: Option<&NativeEffectOptionMenu>,
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
            actor,
            source,
            ability_index,
            granted_ability,
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
                (None, "Unearth", 0, Zone::Hand, None)
                    if *actor == viewer && unearth_cycling(state, source, viewer) =>
                {
                    "Pay {2} and discard this card to draw a card (cycling)"
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
        A::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } => {
            return Ok(effect_options
                .filter(|menu| {
                    *actor == viewer
                        && *source == menu.source
                        && usize::from(*option_count) == menu.labels.len()
                })
                .and_then(|menu| menu.labels.get(usize::from(*option_index)))
                .cloned());
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
        let (view, guide_echo) = self
            .decision_view(binding, seat)
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
            Some(NumericMenuV1::Native { actions }) => {
                let effect_options = guide_echo.or_else(|| cabal_therapy_menu(state, seat));
                actions
                    .iter()
                    .zip(action_values)
                    .enumerate()
                    .map(|(native_ordinal, (row, action))| {
                        Ok(
                            match label(
                                state,
                                row,
                                viewer,
                                &view.observation.ordered.graveyards,
                                effect_options.as_ref(),
                            )? {
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
                            },
                        )
                    })
                    .collect::<Result<Vec<_>, String>>()?
            }
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
