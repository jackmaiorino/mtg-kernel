use super::{
    choose_order, observe_human, public_name, DonatedActivationV1, FullTextProgramV1,
    HumanObservationV1, NumericObservationV1, OrderChoiceV1,
};
use crate::{
    engine::Decision,
    full_text_v1::{self, FullTextAbilityV1},
    graveyard_order_v1,
    ids::PlayerId,
    rl::{self, ActionSemanticV1, LegalActionCandidateV1, LegalActionV1, PlayerSeatV1},
    state::{GameState, Target},
    surface::SurfaceDecision,
    surface_v2::{HarnessSurfaceV2, PriorityModeV1},
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NumericNativeActionV1 {
    pub action: LegalActionV1,
    pub full_text: Option<FullTextProgramV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NumericMenuV1 {
    /// The actual ordered menu is in `observation.ordered.order_choice`.
    GraveyardOrder,
    Native { actions: Vec<NumericNativeActionV1> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NumericDecisionViewV1 {
    pub observation: NumericObservationV1,
    pub menu: Option<NumericMenuV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanFullTextActionV1 {
    pub native_ordinal: usize,
    pub physical_name: &'static str,
    pub label: &'static str,
}

/// Human rendering of the ordered and full-text parts of the current
/// decision. Ordinary action rendering remains with its existing consumer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionViewV1 {
    pub observation: HumanObservationV1,
    pub step_index: u64,
    pub full_text_actions: Vec<HumanFullTextActionV1>,
}

pub enum NumericBoundaryV1<'a> {
    Decision(NumericDecisionV1<'a>),
    GameOver { winner: Option<PlayerSeatV1> },
    Halted,
}

enum BoundMenu {
    GraveyardOrder(OrderChoiceV1),
    Native(Vec<LegalActionCandidateV1>),
}

/// One real native decision. The exclusive borrow keeps the game and surface
/// fixed until the checked action is applied or the caller releases it.
pub struct NumericDecisionV1<'a> {
    state: &'a mut GameState,
    surface: &'a mut HarnessSurfaceV2,
    actor: PlayerId,
    step_index: u64,
    menu: Option<BoundMenu>,
}

pub fn advance_numeric<'a>(
    state: &'a mut GameState,
    surface: &'a mut HarnessSurfaceV2,
    step_index: u64,
) -> Result<NumericBoundaryV1<'a>, String> {
    if !graveyard_order_v1::enabled(state) {
        return Err("Premodern numeric decision requires its native reset profile".into());
    }
    if surface.priority_mode_v1() != PriorityModeV1::EngineWindowsV1 {
        return Err("Premodern numeric decision requires native priority windows".into());
    }
    let decision = surface.next_decision(state);
    match decision {
        SurfaceDecision::Decision(Decision::GameOver { winner }) => {
            return Ok(NumericBoundaryV1::GameOver {
                winner: winner.map(Into::into),
            });
        }
        SurfaceDecision::Decision(Decision::Halted { .. }) => {
            return Ok(NumericBoundaryV1::Halted);
        }
        _ => {}
    }
    let actor = rl::acting_player_for_surface_decision(&decision, state)
        .ok_or("Premodern native decision has no actor")?;
    // Check the owning observation before offering any action. Other native
    // guards can refuse worlds outside this observation's scope.
    let view =
        rl::observe_premodern_ordered_v1(state, surface, actor, step_index).map_err(|e| e.to_string())?;
    let menu = if let Some(choice) = view.ordered.order_choice {
        let SurfaceDecision::Decision(Decision::ChooseEffectTargets {
            legal_targets,
            can_finish: false,
            ..
        }) = &decision
        else {
            return Err("Premodern order is missing its actual native decision".into());
        };
        if !legal_targets
            .iter()
            .copied()
            .eq(choice.native_candidates.iter().map(|card| {
                Target::Object(crate::ids::ObjectId(card.arena_id))
            }))
        {
            return Err("Premodern order differs from its actual native menu".into());
        }
        BoundMenu::GraveyardOrder(choice)
    } else {
        if view.stable.is_none() {
            return Err("Premodern native actor has no ordered decision".into());
        }
        BoundMenu::Native(
            rl::legal_action_candidates_v1(&decision, state).map_err(|e| e.to_string())?,
        )
    };
    Ok(NumericBoundaryV1::Decision(NumericDecisionV1 {
        state,
        surface,
        actor,
        step_index,
        menu: Some(menu),
    }))
}

impl NumericDecisionV1<'_> {
    pub fn view(&self, viewer: PlayerId) -> Result<NumericDecisionViewV1, String> {
        let bound = self.menu.as_ref().ok_or("Premodern decision was used")?;
        let observation =
            rl::observe_premodern_ordered_v1(self.state, self.surface, viewer, self.step_index)
                .map_err(|e| e.to_string())?;
        let menu = (viewer == self.actor).then(|| match bound {
            BoundMenu::GraveyardOrder(_) => NumericMenuV1::GraveyardOrder,
            BoundMenu::Native(candidates) => NumericMenuV1::Native {
                actions: candidates
                    .iter()
                    .map(|candidate| NumericNativeActionV1 {
                        full_text: activation_program(self.state, &candidate.record.semantic)
                            .map(|(_, program)| program),
                        action: candidate.record.clone(),
                    })
                    .collect(),
            },
        });
        Ok(NumericDecisionViewV1 { observation, menu })
    }

    pub fn human_view(&self, viewer: PlayerId) -> Result<HumanDecisionViewV1, String> {
        let bound = self.menu.as_ref().ok_or("Premodern decision was used")?;
        let mut full_text_actions = Vec::new();
        if viewer == self.actor {
            if let BoundMenu::Native(actions) = bound {
                for (native_ordinal, candidate) in actions.iter().enumerate() {
                    if let Some((source, program)) =
                        activation_program(self.state, &candidate.record.semantic)
                    {
                        full_text_actions.push(HumanFullTextActionV1 {
                            native_ordinal,
                            physical_name: public_name(source)?,
                            label: activation_label(program)?,
                        });
                    }
                }
            }
        }
        Ok(HumanDecisionViewV1 {
            observation: observe_human(self.state, viewer)?,
            step_index: self.step_index,
            full_text_actions,
        })
    }

    pub fn choose_order(
        &mut self,
        viewer: PlayerId,
        expected_step: u64,
        native_ordinal: usize,
    ) -> Result<(), String> {
        self.require_actor(viewer, expected_step)?;
        let Some(BoundMenu::GraveyardOrder(choice)) = self.menu.as_ref() else {
            return Err("no current Premodern graveyard order choice".into());
        };
        choose_order(self.state, viewer, choice, native_ordinal)?;
        self.menu = None;
        Ok(())
    }

    pub fn choose_native(
        &mut self,
        viewer: PlayerId,
        expected_step: u64,
        native_ordinal: usize,
        action_id: &str,
    ) -> Result<(), String> {
        self.require_actor(viewer, expected_step)?;
        let Some(BoundMenu::Native(actions)) = self.menu.as_ref() else {
            return Err("no current Premodern native action menu".into());
        };
        rl::validate_selected_action(actions, native_ordinal, action_id)
            .map_err(|e| e.to_string())?;
        let mut state = self.state.clone();
        let mut surface = self.surface.clone();
        surface.apply(&mut state, actions[native_ordinal].surface_action.clone())?;
        *self.state = state;
        *self.surface = surface;
        self.menu = None;
        Ok(())
    }

    pub fn choose_human_full_text(
        &mut self,
        viewer: PlayerId,
        expected_step: u64,
        native_ordinal: usize,
    ) -> Result<(), String> {
        self.require_actor(viewer, expected_step)?;
        let Some(BoundMenu::Native(actions)) = self.menu.as_ref() else {
            return Err("no current Premodern native action menu".into());
        };
        let candidate = actions
            .get(native_ordinal)
            .filter(|candidate| {
                activation_program(self.state, &candidate.record.semantic).is_some()
            })
            .ok_or("ordinal is not a current full-text activation")?;
        let action_id = candidate.record.stable_id.clone();
        self.choose_native(viewer, expected_step, native_ordinal, &action_id)
    }

    fn require_actor(&self, viewer: PlayerId, expected_step: u64) -> Result<(), String> {
        if viewer != self.actor || expected_step != self.step_index || self.menu.is_none() {
            return Err("Premodern decision is stale or belongs to another viewer".into());
        }
        Ok(())
    }
}

fn activation_program<'a>(
    state: &GameState,
    action: &'a ActionSemanticV1,
) -> Option<(&'a crate::rl::CardStableRefV1, FullTextProgramV1)> {
    let ActionSemanticV1::ActivateAbility {
        source,
        ability_index,
        ..
    } = action
    else {
        return None;
    };
    let text = full_text_v1::active_text(state, crate::ids::ObjectId(source.arena_id))?;
    let program = FullTextAbilityV1::from_action_index(*ability_index, text.first_donated_slot(state))?
        .public_program(text.definition())?;
    Some((source, program))
}

fn activation_label(program: FullTextProgramV1) -> Result<&'static str, String> {
    use DonatedActivationV1::*;

    Ok(match program {
        FullTextProgramV1::RetainedDiscard => "Pay {2} to discard a card",
        FullTextProgramV1::DonatedActivation { effect, .. } => match effect {
            PalinchronReturnHost => "Pay {2}{U}{U} to return this creature to its owner's hand",
            DevourerExileTopForCounters => {
                "Exile your library's top card to put its mana value in +1/+1 counters here"
            }
            TriskelionRemoveCounterForDamage => "Remove a +1/+1 counter here to deal 1 damage to any target",
            ScroungerThresholdPump => {
                "At threshold, put three graveyard cards on your library's bottom for +3/+3 this turn. Once per turn"
            }
            HermitDruidRevealThroughBasic => {
                "Pay {G} and tap this creature to reveal through a basic land, take it, and put the other revealed cards into your graveyard"
            }
            PsychatogDiscardPump => "Discard a card to give this creature +1/+1 this turn",
            PsychatogExilePump => "Exile two graveyard cards to give this creature +1/+1 this turn",
        },
        _ => return Err("full-text activation has a trigger-only program".into()),
    })
}
