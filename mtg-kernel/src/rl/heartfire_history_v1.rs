//! History-only common storage producer, independent of frozen constructors.
use super::*;
use crate::heartfire_history_v1::HeartfireHistoryObservationV1;
use crate::heartfire_public_v1::HeartfireSavedDamageV1;
use crate::policy_observation_v8::{
    HistoricalPublicSourceV8, PolicyObservationExtensionsV8, QueuedWardLifePaymentV8,
    WardLifePaymentV8,
};
struct HistoryParts {
    projection: PublicObservationProjectionV2,
    own_hand: Vec<CardPrivateV1>,
    known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    known_hand_cards: [Vec<CardPrivateV1>; 2],
    known_face_down_cards: Vec<FaceDownCardKnowledgeV1>,
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn observe(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: u32,
    count: u32,
) -> Result<HeartfireHistoryObservationV1> {
    let mut observation = build(
        PolicyObservationBuildV5 {
            state,
            surface,
            acting_player: actor,
            step_index: step,
            physical_decision_id: physical,
            substep_index: substep,
            substep_count: count,
            text_mode: ObservationTextModeV2::FullArtifact,
        },
        true,
    )?;
    let mut value = serde_json::to_value(&observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("history observation is not an object".into()))?
        .remove("visible_projection_hash");
    observation.visible_projection_hash = stable_hash_json(&value)?;
    Ok(observation)
}
pub(crate) fn normalization_extensions(
    state: &GameState,
    actor: PlayerId,
) -> Result<PolicyObservationExtensionsV6> {
    normalization_common(
        state,
        actor,
        ObservationTextModeV2::FlatForbiddenElided,
        true,
    )
}
pub(crate) fn validate_artifact(observation: &HeartfireHistoryObservationV1) -> Result<()> {
    if observation.schema != crate::heartfire_history_v1::SCHEMA
        || observation.source_sha256 != crate::heartfire_history_v1::source_sha256()
        || observation.card_db_hash != KERNEL_CARDDB_HASH
    {
        return Err(RlContractError("history artifact identity mismatch".into()));
    }
    let mut value = serde_json::to_value(observation)?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("history artifact is not an object".into()))?
        .remove("visible_projection_hash");
    if observation.visible_projection_hash != stable_hash_json(&value)? {
        return Err(RlContractError("history artifact digest mismatch".into()));
    }
    let mut previous = None;
    for saved in &observation.saved_damage {
        crate::rules_meaning_heartfire_v1::validate(&saved.root)
            .map_err(|_| RlContractError("history saved root identity mismatch".into()))?;
        let row = observation
            .projection
            .surface
            .stack
            .get(saved.stack_index as usize)
            .ok_or_else(|| {
                RlContractError("history saved source lacks public stack membership".into())
            })?;
        let matching = observation
            .extensions
            .historical_public_sources
            .iter()
            .filter(|source| {
                source.context
                    == crate::policy_observation_v7::HistoricalSourceContextV7::Stack {
                        stack_index: saved.stack_index,
                    }
            })
            .collect::<Vec<_>>();
        if previous.is_some_and(|index| saved.stack_index <= index)
            || matching.len() != 1
            || row.stack_index != saved.stack_index
            || row.stack_item_kind != StackItemKindV2::ActivatedAbility
            || row.controller != saved.captured_controller
            || row.source.card_db_id != saved.root.card_id
            || matching[0].source != row.source
            || matching[0].stack_item_kind != row.stack_item_kind
            || saved.damage_if_target_legal_before_prevention != saved.signed_departing_power.max(0)
        {
            return Err(RlContractError(
                "history saved source relation mismatch".into(),
            ));
        }
        previous = Some(saved.stack_index);
    }
    Ok(())
}
fn captured_source_ref(state: &GameState, item: &StackItem) -> Result<CardStableRefV1> {
    if !matches!(
        item.inline_effect.as_ref(),
        Some(crate::effect::EffectOp::StandardV1(
            crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1
        ))
    ) {
        return stack_source_ref(state, item);
    }
    crate::engine::validated_stack_item_target_spec(item, state).map_err(RlContractError)?;
    crate::source_power_activation_v1::validate_item(state, item).map_err(RlContractError)?;
    let source = item
        .v4
        .ability_source_contract
        .ok_or_else(|| RlContractError("history source capture absent".into()))?;
    let selector = item
        .v4
        .activated_ability_index
        .ok_or_else(|| RlContractError("history printed selector absent".into()))?;
    crate::rules_meaning_heartfire_v1::root_meaning(source.card_def, u32::from(selector))
        .map_err(|_| RlContractError("history unsupported captured printed root".into()))?;
    if source.zone != Zone::Battlefield
        || source.source != item.source
        || source.controller != item.controller
    {
        return Err(RlContractError(
            "history captured source disagrees with producer".into(),
        ));
    }
    Ok(CardStableRefV1 {
        arena_id: source.source.0,
        card_db_id: source.card_def,
        owner: source.owner.into(),
        controller: source.controller.into(),
        zone: source.zone,
        zone_change_count: source.zone_change_count,
    })
}
fn stack_public_history(state: &GameState, actor: PlayerId) -> Result<Vec<StackItemPublicV2>> {
    state
        .stack
        .iter()
        .enumerate()
        .map(|(index, item)| stack_item_public_history(state, actor, index as u32, item))
        .collect()
}
fn stack_item_public_history(
    state: &GameState,
    acting_player: PlayerId,
    stack_index: u32,
    item: &StackItem,
) -> Result<StackItemPublicV2> {
    if (item.kind == StackItemKind::Spell) != item.v4.cast_method.is_some() {
        return Err(RlContractError(
            "spell stack items require a cast method and abilities must not carry one".to_string(),
        ));
    }
    Ok(StackItemPublicV2 {
        stack_index,
        source: captured_source_ref(state, item)?,
        controller: item.controller.into(),
        // Announced stack targets are public historical facts. Project the
        // frozen target contract rather than rebuilding a reference from a
        // potentially later incarnation currently reusing the same arena id.
        targets: stack_target_refs(state, item)?,
        stack_item_kind: item.kind.into(),
        is_copy: item.is_copy,
        is_flashback: item.is_flashback,
        mode_chosen: item.mode_chosen,
        madness_offer: item.madness_offer,
        kicked: item.kicked,
        cast_method: item.v4.cast_method,
        face_index: item.v4.face_index,
        x_value: item.v4.x_value,
        paid_cost_refs: paid_cost_card_refs(state, &item.v4.paid_cost_refs, acting_player),
        granted_ability: public_granted_ability(item.v4.cauldron_grant),
        counter_transfer: public_counter_transfer(
            state,
            item.inline_effect.as_ref(),
            item.v4.ability_source_contract,
        ),
        counter_distribution: public_counter_distribution(
            item.inline_effect.as_ref(),
            stack_target_refs(state, item)?,
        ),
    })
}

fn build_history_parts(
    state: &GameState,
    surface: &crate::surface_v2::HarnessSurfaceV2,
    acting_player: PlayerId,
    _step_index: u64,
    text_mode: ObservationTextModeV2,
) -> Result<HistoryParts> {
    let projection = PublicObservationProjectionV2 {
        turn: state.turn,
        phase: state.step.into(),
        active_player: state.active_player.into(),
        priority_player: state.priority_player.into(),
        initiative: state.initiative.map(Into::into),
        life_totals: [state.players[0].life, state.players[1].life],
        mana_pools: [state.players[0].mana_pool, state.players[1].mana_pool],
        hand_counts: [state.players[0].hand.len(), state.players[1].hand.len()],
        library_counts: [
            state.players[0].library.len(),
            state.players[1].library.len(),
        ],
        player_status: [
            player_status_v1(&state.players[0]),
            player_status_v1(&state.players[1]),
        ],
        battlefield: [
            public_cards_v2(state, &state.players[0].battlefield, text_mode)?,
            public_cards_v2(state, &state.players[1].battlefield, text_mode)?,
        ],
        graveyards: [
            public_cards_v2(state, &state.players[0].graveyard, text_mode)?,
            public_cards_v2(state, &state.players[1].graveyard, text_mode)?,
        ],
        exile: public_cards_v2(state, &state.exile, text_mode)?,
        stack: stack_public_history(state, acting_player)?,
        combat: combat_public_v2(state, acting_player)?,
        continuous_effects: continuous_effects_common_v2(
            state,
            acting_player,
            (false, false),
            false,
            false,
        )?,
        object_relations: object_relations_public_v4(state, acting_player)?,
        exile_play_permissions: exile_play_permissions_public_v2(state)?,
        engine_context: engine_context_v2(state, acting_player, false)?,
        surface_context: surface_context_v2(state, surface, acting_player)?,
    };
    let own_hand = state.players[acting_player.index()]
        .hand
        .iter()
        .map(|&id| private_card(state, id, text_mode))
        .collect::<Result<Vec<_>>>()?;
    Ok(HistoryParts {
        projection,
        own_hand,
        known_library_cards: known_library_cards_v4(state, acting_player, text_mode)?,
        known_hand_cards: known_hand_cards_v4(state, acting_player, text_mode)?,
        known_face_down_cards: known_face_down_cards_v1(state, acting_player, text_mode)?,
    })
}

fn build(
    request: PolicyObservationBuildV5<'_>,
    accept_life_ward: bool,
) -> Result<HeartfireHistoryObservationV1> {
    let PolicyObservationBuildV5 {
        state,
        surface,
        acting_player,
        step_index,
        physical_decision_id,
        substep_index,
        substep_count,
        text_mode,
    } = request;
    if substep_count == 0 || substep_index >= substep_count {
        return Err(RlContractError(format!(
            "invalid physical decision substep {substep_index}/{substep_count}"
        )));
    }
    let extensions = normalization_common(state, acting_player, text_mode, accept_life_ward)?;
    let mut base = build_history_parts(
        state,
        surface.harness_surface(),
        acting_player,
        step_index,
        text_mode,
    )?;
    if let Some(search) = &extensions.decision_local_library {
        let Some(PendingEffectChoiceSemanticV4::Targets { legal_targets, .. }) = base
            .projection
            .engine_context
            .pending_effect
            .as_mut()
            .and_then(|pending| pending.choice.as_mut())
        else {
            return Err(RlContractError(
                "decision-local library lost its search context".into(),
            ));
        };
        let ordinal = |target: &TargetRefV1| -> Result<usize> {
            let TargetRefV1::Object { object } = target else {
                return Err(RlContractError(
                    "library search has a non-card target".into(),
                ));
            };
            search
                .cards
                .iter()
                .position(|card| card.stable == *object)
                .ok_or_else(|| RlContractError("library target has no decision-local card".into()))
        };
        let mut ordered = legal_targets
            .iter()
            .map(|target| Ok((ordinal(target)?, target.clone())))
            .collect::<Result<Vec<_>>>()?;
        ordered.sort_by_key(|(index, _)| *index);
        *legal_targets = ordered.into_iter().map(|(_, target)| target).collect();
    }
    let policy_surface_context = policy_surface_context_v5(
        state,
        surface
            .scan_context_for(acting_player)
            .map_err(RlContractError)?,
    )?;
    let extensions = history_extensions(state, acting_player, &base, extensions)?;
    Ok(HeartfireHistoryObservationV1 {
        schema: crate::heartfire_history_v1::SCHEMA.into(),
        source_sha256: crate::heartfire_history_v1::source_sha256(),
        kernel_version: text_mode.kernel_version(),
        surface_version: H2_PREDICATE_VERSION,
        policy_surface_version: POLICY_SURFACE_VERSION,
        card_db_hash: KERNEL_CARDDB_HASH,
        acting_player: acting_player.into(),
        step_index,
        physical_decision_id,
        substep_index,
        substep_count,
        projection: PublicObservationProjectionV5 {
            poison_counters: public_poison_counters_v1(state),
            ninja_emblems: crate::standard_cards_v1::ninja_emblems(state),
            poison_prevention: {
                let flags = [
                    state.players[0].poison_prevention_v1.0,
                    state.players[1].poison_prevention_v1.0,
                ];
                flags.iter().any(|x| *x).then_some(flags)
            },
            creatures_attacked_this_turn: (state.creatures_attacked_this_turn_v1() != 0)
                .then(|| state.creatures_attacked_this_turn_v1()),
            restricted_mana: public_restricted_mana_v1(state)?,
            surface: base.projection,
            policy_surface_context,
            foundations_combat: public_foundations_combat_v1(state)?,
            london_mulligans: public_london_mulligans_v1(state),
        },
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        saved_damage: saved_damage(state)?,
        visible_projection_hash: 0,
    })
}

fn normalization_common(
    state: &GameState,
    acting_player: PlayerId,
    text_mode: ObservationTextModeV2,
    accept_life_ward: bool,
) -> Result<PolicyObservationExtensionsV6> {
    let pending = state
        .engine
        .pending_effect
        .as_ref()
        .map(|pending| &pending.resolving_item);
    if !accept_life_ward
        && state.stack.iter().chain(pending).any(|item| {
            matches!(
                item.inline_effect.as_ref(),
                Some(crate::effect::EffectOp::CounterUnlessPaysLifeWithChoice { .. })
            )
        })
    {
        return Err(RlContractError(
            "life-payment Ward requires its typed observation successor".into(),
        ));
    }
    let pending_cast_object_cost = if let Some(pending) = &state.engine.pending_cast {
        engine::validate_pending_cast(state, pending).map_err(RlContractError)?;
        if pending.source_contract.cast_method == CastMethodV4::Escape {
            let definition = CARD_DEFS
                .get(pending.source_contract.card_def as usize)
                .ok_or_else(|| RlContractError("Escape source definition is absent".into()))?;
            let escape = definition
                .escape
                .as_ref()
                .ok_or_else(|| RlContractError("Escape cost definition is absent".into()))?;
            let required_count = escape
                .cost
                .iter()
                .find_map(|component| match component {
                    crate::card_def::CostComponent::ExileOtherCardsFromOwnGraveyard(count) => {
                        Some(u32::from(*count))
                    }
                    _ => None,
                })
                .ok_or_else(|| RlContractError("Escape exile cost is absent".into()))?;
            let mut seen = HashSet::new();
            let selected = pending
                .sacrifice_chosen
                .iter()
                .map(|&id| {
                    let reference = card_ref(state, id)?;
                    if id == pending.spell
                        || !seen.insert(id)
                        || reference.owner != pending.controller.into()
                        || reference.zone != Zone::Graveyard
                        || !state.players[pending.controller.index()]
                            .graveyard
                            .contains(&id)
                    {
                        return Err(RlContractError("invalid Escape selection prefix".into()));
                    }
                    Ok(reference)
                })
                .collect::<Result<Vec<_>>>()?;
            let selected_count = u32::try_from(selected.len())
                .map_err(|_| RlContractError("Escape prefix exceeds u32".into()))?;
            let remaining_count = required_count
                .checked_sub(selected_count)
                .ok_or_else(|| RlContractError("Escape prefix exceeds its cost".into()))?;
            Some(PendingCastObjectCostV6 {
                source: card_ref(state, pending.spell)?,
                controller: pending.controller.into(),
                cast_method: CastMethodV4::Escape,
                cost_kind: CostKind::ExileFromGraveyard,
                required_count,
                selected,
                remaining_count,
            })
        } else {
            None
        }
    } else {
        None
    };

    let pending_chosen_creature_cost = state
        .engine
        .pending_cast
        .as_ref()
        .filter(|pending| pending.controller == acting_player)
        .and_then(|pending| {
            pending
                .chosen_creature_cost_zone
                .map(|zone| (pending, zone))
        })
        .map(|(pending, selected_zone)| {
            Ok::<_, RlContractError>(PendingChosenCreatureCostV6 {
                source: card_ref(state, pending.spell)?,
                controller: pending.controller.into(),
                selected_zone,
            })
        })
        .transpose()?;
    let mut finalized_chosen_creature_costs = Vec::new();
    let mut historical_public_sources = Vec::new();
    let mut queued_ward_payments = Vec::new();
    let ward_payment = |item: &StackItem| -> Result<Option<WardPaymentV6>> {
        crate::effect::validated_ward_observation_targeter(state, item)
            .map_err(RlContractError)?
            .map(|(targeter, generic)| {
                let index = state
                    .stack
                    .iter()
                    .position(|live| live.v4.stack_item_id == targeter.v4.stack_item_id)
                    .ok_or_else(|| {
                        RlContractError("Ward targeter lost public stack membership".into())
                    })?;
                Ok(WardPaymentV6 {
                    targeting_stack_index: u32::try_from(index)
                        .map_err(|_| RlContractError("Ward targeter index exceeds u32".into()))?,
                    payer: targeter.controller.into(),
                    generic,
                })
            })
            .transpose()
    };
    for (index, item) in state.stack.iter().enumerate() {
        let resolving = state.engine.pending_effect.as_ref().is_some_and(|pending| {
            pending.resolving_item.v4.stack_item_id == item.v4.stack_item_id
        });
        if let Some(payment) = if resolving { None } else { ward_payment(item)? } {
            queued_ward_payments.push(QueuedWardPaymentV6 {
                stack_index: u32::try_from(index)
                    .map_err(|_| RlContractError("Ward trigger index exceeds u32".into()))?,
                payment,
            });
        }
        // stack_source_ref validates the independent finalized cast binding,
        // including equality with the complete paid-cost record and its LKI.
        if item.kind == StackItemKind::Spell {
            let source = captured_source_ref(state, item)?;
            let chosen_creature_cost = CARD_DEFS[source.card_db_id as usize].additional_cost
                .is_some_and(|cost| cost.iter().any(|component| matches!(component,
                    crate::card_def::CostComponent::ChooseControlledCreatureOrRevealCreatureCardFromHand)));
            for paid in item
                .v4
                .paid_cost_refs
                .iter()
                .filter(|paid| chosen_creature_cost && paid.visible_to(acting_player))
            {
                let power_lki = paid.power_lki.ok_or_else(|| {
                    RlContractError("chosen-creature cost lost its recorded power".into())
                })?;
                finalized_chosen_creature_costs.push(FinalizedChosenCreatureCostV6 {
                    stack_index: u32::try_from(index)
                        .map_err(|_| RlContractError("stack index exceeds u32".into()))?,
                    source: source.clone(),
                    chosen: paid_cost_card_refs(state, &[*paid], acting_player)[0].clone(),
                    power_lki,
                });
            }
        }
        if item.kind != StackItemKind::Spell {
            historical_public_sources.push(HistoricalPublicSourceV6 {
                context: HistoricalSourceContextV6::Stack {
                    stack_index: u32::try_from(index)
                        .map_err(|_| RlContractError("stack index exceeds u32".into()))?,
                },
                source: captured_source_ref(state, item)?,
                stack_item_kind: item.kind.into(),
            });
        }
    }
    let mut decision_local_library = None;
    let mut pending_ward_payment = None;
    if let Some(pending) = &state.engine.pending_effect {
        crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
        if matches!(
            pending.choice.as_ref(),
            Some(crate::effect::PendingEffectChoice::ChooseBoolean {
                purpose: crate::effect::EffectBooleanChoicePurpose::CounterUnlessPaysGeneric { .. },
                ..
            })
        ) {
            pending_ward_payment =
                Some(ward_payment(&pending.resolving_item)?.ok_or_else(|| {
                    RlContractError("pending Ward payment has no live bound targeter".into())
                })?);
        }
        historical_public_sources.push(HistoricalPublicSourceV6 {
            context: HistoricalSourceContextV6::PendingEffect,
            source: captured_source_ref(state, &pending.resolving_item)?,
            stack_item_kind: pending.resolving_item.kind.into(),
        });
        if let Some(crate::effect::PendingEffectChoice::SelectTargets {
            player,
            selected,
            legal,
            purpose,
            ..
        }) = &pending.choice
        {
            use crate::effect::EffectTargetSelectionPurpose;
            let library_owner = match purpose {
                EffectTargetSelectionPurpose::SearchLibraryToHand { player, .. }
                | EffectTargetSelectionPurpose::SearchLibraryToHandMany { player, .. }
                | EffectTargetSelectionPurpose::SearchLibraryCardsToDestination {
                    player, ..
                }
                | EffectTargetSelectionPurpose::SearchLibraryToBattlefieldTapped {
                    player, ..
                } => Some(*player),
                _ => None,
            };
            if *player == acting_player {
                if let Some(library_owner) = library_owner {
                    let mut seen = HashSet::new();
                    let mut cards = Vec::new();
                    for candidate in selected.iter().chain(legal.iter()) {
                        let Target::Object(id) = candidate.target else {
                            return Err(RlContractError("library search names a non-card".into()));
                        };
                        let object = state.objects.try_get(id).ok_or_else(|| {
                            RlContractError("library search card is absent".into())
                        })?;
                        if object.owner != library_owner
                            || object.zone != Zone::Library
                            || !state.players[library_owner.index()].library.contains(&id)
                            || candidate.expected_object.is_none_or(|binding| {
                                binding.object != id
                                    || binding.expected_zone != Zone::Library
                                    || binding.expected_zone_change_count
                                        != object.zone_change_count
                            })
                        {
                            return Err(RlContractError(
                                "invalid decision-local library card".into(),
                            ));
                        }
                        if seen.insert(id) {
                            cards.push(private_card(state, id, text_mode)?);
                        }
                    }
                    // Only observable card identity determines class order.
                    // The selected prefix is visible and distinguishes copies.
                    // Arena ids break ties inside indistinguishable classes
                    // for private authority; scorers must erase that tie-break.
                    cards.sort_by_key(|card| {
                        (
                            card.stable.card_db_id,
                            card.stable.owner as u8,
                            card.stable.controller as u8,
                            selected
                                .iter()
                                .position(|target| {
                                    matches!(target.target,
                                Target::Object(id) if id.0 == card.stable.arena_id)
                                })
                                .unwrap_or(usize::MAX),
                            card.stable.arena_id,
                        )
                    });
                    decision_local_library = Some(DecisionLocalLibraryV6 {
                        chooser: (*player).into(),
                        library_owner: library_owner.into(),
                        cards,
                    });
                }
            }
        }
    }
    Ok(PolicyObservationExtensionsV6 {
        pending_cast_object_cost,
        decision_local_library,
        historical_public_sources,
        pending_chosen_creature_cost,
        finalized_chosen_creature_costs,
        pending_ward_payment,
        queued_ward_payments,
    })
}

fn projected_card_sources(
    base: &HistoryParts,
    extensions: &PolicyObservationExtensionsV6,
) -> Vec<CardStableRefV1> {
    let public = &base.projection;
    let mut sources: Vec<_> = public
        .battlefield
        .iter()
        .chain(public.graveyards.iter())
        .flat_map(|cards| cards.iter())
        .chain(public.exile.iter())
        .map(|card| card.stable.clone())
        .collect();
    sources.extend(public.stack.iter().map(|item| item.source.clone()));
    sources.extend(base.own_hand.iter().map(|card| card.stable.clone()));
    sources.extend(
        base.known_hand_cards
            .iter()
            .flatten()
            .map(|card| card.stable.clone()),
    );
    sources.extend(
        base.known_library_cards
            .iter()
            .flatten()
            .map(|known| known.card.stable.clone()),
    );
    if let Some(search) = &extensions.decision_local_library {
        sources.extend(search.cards.iter().map(|card| card.stable.clone()));
    }
    if let Some(cost) = &extensions.pending_cast_object_cost {
        sources.push(cost.source.clone());
        sources.extend(cost.selected.iter().cloned());
    }
    if let Some(cost) = &extensions.pending_chosen_creature_cost {
        sources.push(cost.source.clone());
    }
    for cost in &extensions.finalized_chosen_creature_costs {
        sources.extend([cost.source.clone(), cost.chosen.clone()]);
    }
    sources
}

fn history_extensions(
    state: &GameState,
    actor: PlayerId,
    base: &HistoryParts,
    extensions: PolicyObservationExtensionsV6,
) -> Result<PolicyObservationExtensionsV8> {
    let mut sources = projected_card_sources(base, &extensions);
    let v7 = crate::policy_observation_v7::policy_observation_extensions_v7_from_v6(
        state, actor, extensions,
    )?;
    sources.extend(
        v7.historical_public_sources
            .iter()
            .map(|row| row.source.clone()),
    );
    let ward_life_card_traits =
        crate::policy_observation_v8::life_ward_card_traits(state, sources)?;
    let payment = |trigger: &StackItem| -> Result<Option<WardLifePaymentV8>> {
        crate::effect::validated_life_ward_observation_targeter(state, trigger)
            .map_err(RlContractError)?
            .map(|(targeter, life)| {
                let index = state
                    .stack
                    .iter()
                    .position(|live| live.v4.stack_item_id == targeter.v4.stack_item_id)
                    .ok_or_else(|| {
                        RlContractError("life Ward targeter lost public membership".into())
                    })?;
                Ok(WardLifePaymentV8 {
                    targeting_stack_index: u32::try_from(index).map_err(|_| {
                        RlContractError("life Ward targeter index exceeds u32".into())
                    })?,
                    payer: targeter.controller.into(),
                    life,
                })
            })
            .transpose()
    };
    let mut queued_ward_life_payments = Vec::new();
    for (index, trigger) in state.stack.iter().enumerate() {
        if state.engine.pending_effect.as_ref().is_some_and(|pending| {
            pending.resolving_item.v4.stack_item_id == trigger.v4.stack_item_id
        }) {
            continue;
        }
        if let Some(payment) = payment(trigger)? {
            queued_ward_life_payments.push(QueuedWardLifePaymentV8 {
                stack_index: u32::try_from(index)
                    .map_err(|_| RlContractError("life Ward trigger index exceeds u32".into()))?,
                payment,
            });
        }
    }
    let mut pending_ward_life_payment = None;
    if let Some(pending) = &state.engine.pending_effect {
        crate::effect::validate_pending_effect_choice(state).map_err(RlContractError)?;
        if matches!(
            pending.choice.as_ref(),
            Some(crate::effect::PendingEffectChoice::ChooseBoolean {
                purpose: crate::effect::EffectBooleanChoicePurpose::CounterUnlessPaysLife { .. },
                ..
            })
        ) {
            pending_ward_life_payment =
                Some(payment(&pending.resolving_item)?.ok_or_else(|| {
                    RlContractError("pending life Ward has no live targeter".into())
                })?);
        }
    }

    Ok(PolicyObservationExtensionsV8 {
        pending_cast_object_cost: v7.pending_cast_object_cost,
        decision_local_library: v7.decision_local_library,
        historical_public_sources: v7
            .historical_public_sources
            .into_iter()
            .map(|row| HistoricalPublicSourceV8 {
                context: row.context,
                source: row.source,
                stack_item_kind: row.stack_item_kind,
            })
            .collect(),
        pending_chosen_creature_cost: v7.pending_chosen_creature_cost,
        finalized_chosen_creature_costs: v7.finalized_chosen_creature_costs,
        pending_ward_payment: v7.pending_ward_payment,
        queued_ward_payments: v7.queued_ward_payments,
        pending_ward_life_payment,
        queued_ward_life_payments,
        ward_life_card_traits,
    })
}
fn saved_damage(
    state: &GameState,
) -> Result<Vec<crate::heartfire_public_v1::HeartfireSavedDamageV1>> {
    crate::source_power_activation_v1::validate_state(state).map_err(RlContractError)?;
    let mut saved_damage = Vec::new();
    for (index, item) in state.stack.iter().enumerate() {
        if !matches!(
            item.inline_effect.as_ref(),
            Some(crate::effect::EffectOp::StandardV1(
                crate::standard_cards_v1::StandardOpV1::SourceLastPowerDamageV1
            ))
        ) {
            continue;
        }
        crate::source_power_activation_v1::validate_item(state, item).map_err(RlContractError)?;
        let source = item
            .v4
            .ability_source_contract
            .ok_or_else(|| RlContractError("Heartfire missing source contract".into()))?;
        let local = item
            .v4
            .activated_ability_index
            .ok_or_else(|| RlContractError("Heartfire missing printed selector".into()))?;
        let root =
            crate::rules_meaning_heartfire_v1::root_meaning(source.card_def, u32::from(local))
                .map_err(|_| RlContractError("Heartfire unsupported selected root".into()))?;
        let facts = crate::source_power_activation_v1::damage_source(
            state,
            item.source,
            item.v4.stack_item_id,
        )
        .map_err(RlContractError)?;
        let power = state
            .engine
            .event_history
            .iter()
            .find_map(|event| match event {
                crate::event::CommittedEvent::SourcePowerActivationCommittedV1 {
                    item: saved,
                    power,
                    ..
                } if saved.v4.stack_item_id == item.v4.stack_item_id => Some(*power),
                _ => None,
            })
            .ok_or_else(|| RlContractError("Heartfire lost authenticated power receipt".into()))?;
        let ordinal = u32::try_from(index)
            .map_err(|_| RlContractError("Heartfire stack ordinal exceeds u32".into()))?;
        saved_damage.push(HeartfireSavedDamageV1 {
            stack_index: ordinal,
            root,
            signed_departing_power: power,
            damage_if_target_legal_before_prevention: power.max(0),
            captured_controller: facts.source.controller.into(),
            captured_color_mask: facts.color_mask,
            captured_lifelink: facts.lifelink,
            captured_deathtouch: facts.deathtouch,
            captured_werewolf: facts.werewolf,
        });
    }
    Ok(saved_damage)
}
