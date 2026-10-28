//! Selected spell totals, pure cost quotes and synchronous prepared payment.
//! Timing, targets and restored PendingCast authentication belong to the caller.

use super::{
    card_def, mana, normal_cast_reduction_count, supported_adventure, supported_bestow,
    supported_omen, with_spree_surcharge, CardType, CastMethodV4, Cost, CostComponent, GameState,
    ObjectId, PlayerId, Target,
};

pub(super) struct SpellManaPaymentV1 {
    pub(super) mana: mana::PaymentPlan,
    pub(super) delve_exiled: Vec<ObjectId>,
    pub(super) convoke_tapped: Vec<ObjectId>,
}

/// Local, synchronous payment actions. No restored payload can assert that
/// these were prepared, and commitment never selects counters or hand cards.
pub(super) struct PreparedSpellNonmanaV1 {
    actions: Vec<PreparedNonmanaActionV1>,
}

enum PreparedNonmanaActionV1 {
    Component(CostComponent, Vec<ObjectId>),
    CounterRemovals(Vec<ObjectId>),
    RevealHand(Vec<ObjectId>),
}

impl PreparedNonmanaActionV1 {
    fn commit(&self, state: &mut GameState, player: PlayerId, source: ObjectId) {
        match self {
            Self::Component(component, chosen) => {
                // Mana, Convoke, counters and reveal are excluded at preparation.
                // Remaining legacy arms have no fallible resource selection.
                let spent = super::commit_cost_components_from_plan_v1(
                    state,
                    player,
                    source,
                    &[*component],
                    chosen,
                    None,
                );
                debug_assert_eq!(spent, Some(0));
            }
            Self::CounterRemovals(objects) => {
                for &object in objects {
                    state.objects.get_mut(object).counters.plus1_plus1 -= 1;
                }
            }
            Self::RevealHand(objects) => {
                for &object in objects {
                    for observer in [PlayerId::P0, PlayerId::P1] {
                        state
                            .reveal_hand_card(observer, player, object)
                            .expect("synchronous prepared hand reveal remains live");
                    }
                }
            }
        }
    }
}

impl PreparedSpellNonmanaV1 {
    pub(super) fn commit(self, state: &mut GameState, player: PlayerId, source: ObjectId) {
        self.apply(state, player, source);
    }

    fn apply(&self, state: &mut GameState, player: PlayerId, source: ObjectId) {
        for action in &self.actions {
            action.commit(state, player, source);
        }
    }
}

/// Prepare against the resource state after the caller's frozen mana/discard
/// payment. A private projection applies each ordered component so later
/// groups cannot double-spend counters, life or departing objects. The caller
/// validates chosen-creature and optional-cost bindings separately.
pub(super) fn prepare_spell_nonmana_v1(
    state: &GameState,
    player: PlayerId,
    source: ObjectId,
    groups: &[(&[CostComponent], &[ObjectId])],
) -> Option<PreparedSpellNonmanaV1> {
    let mut projected = state.clone();
    let mut actions = Vec::new();
    for (components, chosen) in groups {
        let nonmana = components
            .iter()
            .copied()
            .filter(|component| {
                !matches!(
                    component,
                    CostComponent::Mana(_)
                        | CostComponent::ConvokeMana(_)
                        | CostComponent::DiscardCards(_)
                        | CostComponent::ChooseControlledCreatureOrRevealCreatureCardFromHand
                )
            })
            .collect::<Vec<_>>();
        // The legacy predicates below dereference hand entries. Authenticate
        // them first so malformed resource projections refuse rather than panic.
        if nonmana
            .iter()
            .any(|component| matches!(component, CostComponent::RevealHandIfNoCardsWithType(_)))
            && projected.players[player.index()].hand.iter().any(|object| {
                projected.objects.try_get(*object).is_none_or(|card| {
                    card.owner != player
                        || card.zone != crate::state::Zone::Hand
                        || card_def::CARD_DEFS.get(card.card_def as usize).is_none()
                })
            })
        {
            return None;
        }
        super::validate_cost_component_choices_v1(&projected, player, source, &nonmana, chosen)?;
        for component in nonmana {
            if !super::can_pay_components(&[component], player, source, &projected) {
                return None;
            }
            let action = match component {
                CostComponent::RemovePlusOneCountersFromControlledCreatures(_) => {
                    // Validation bound the exact selected counters above. Do
                    // not replace the player's selection with arena order.
                    PreparedNonmanaActionV1::CounterRemovals(chosen.to_vec())
                }
                CostComponent::RevealHandIfNoCardsWithType(_) => {
                    let hand = projected.players[player.index()].hand.clone();
                    PreparedNonmanaActionV1::RevealHand(hand)
                }
                _ => PreparedNonmanaActionV1::Component(component, chosen.to_vec()),
            };
            action.commit(&mut projected, player, source);
            actions.push(action);
        }
    }
    Some(PreparedSpellNonmanaV1 { actions })
}

/// Object choices retain the same base/additional ownership as the staged
/// cast. This result is local preparation, never serialized payment evidence.
pub(super) struct SpellComponentChoicesV1 {
    pub(super) ordered_objects: Vec<ObjectId>,
    pub(super) chosen_by_group: Vec<Vec<ObjectId>>,
}

pub(super) struct SelectedSpellManaCostsV1 {
    granted_flashback_v1: bool,
    tinybones_mana_v72: Option<super::tinybones_v72::ManaGrantV72>,
    /// Whether the last component group is retrace's land discard
    /// (Wrenn and Six's emblem), added after any definition-owned groups.
    pub(super) retrace: bool,
    pub(super) legendary_spell: bool,
    pub(super) spell_subtypes: Vec<u16>,
    pub(super) all_creature_types_v60: bool,
    pub(super) costs: Vec<Cost>,
    pub(super) component_groups: Vec<&'static [CostComponent]>,
    pub(super) types: &'static [CardType],
    pub(super) own_generic_reduction: u32,
    /// Generic increase for this spell alone (Anointed Peacekeeper).
    pub(super) own_generic_increase: u32,
    pub(super) delve: bool,
    pub(super) convoke: bool,
    /// Improvise (Kappa Cannoneer): the convoke payment taps artifacts, each
    /// for one generic mana.
    pub(super) improvise: bool,
    /// Replicate's cost, paid once for each unit of the announced X.
    pub(super) replicate: Option<Cost>,
    /// Whether this is a colorless Eldrazi spell, for Eldrazi Temple's
    /// restricted mana.
    pub(super) colorless_eldrazi: bool,
    /// Teamwork N (Crossover Collaboration's alternative cost): untapped
    /// creatures the caster controls with total power N or more are tapped
    /// along with the mana payment, and are not mana sources for it.
    pub(super) teamwork: Option<u8>,
    /// "You can't spend mana to cast this spell" (Hogaak, Arisen
    /// Necropolis): only its convoke and delve payment pays.
    pub(super) no_mana: bool,
    /// The spell's own card, which never pays for itself by delve.
    pub(super) no_mana_source: Option<ObjectId>,
}

impl SelectedSpellManaCostsV1 {
    /// Assign the pending cast's one interactive object family to the original
    /// definition-owned group. Escape picks are ordered by the live graveyard,
    /// matching the existing public-equivalence and paid-provenance contract.
    pub(super) fn component_choices_v1(
        &self,
        definition: &card_def::CardDef,
        method: CastMethodV4,
        player: PlayerId,
        state: &GameState,
        chosen: &[ObjectId],
    ) -> Option<SpellComponentChoicesV1> {
        let base = match method {
            CastMethodV4::Alternative => Some(definition.alt_cost?.components),
            CastMethodV4::Flashback if self.granted_flashback_v1 => Some(&[][..]),
            CastMethodV4::Flashback => Some(flashback_cost_v1(definition)?),
            CastMethodV4::Escape => Some(escape_cost_v1(definition)?),
            _ => None,
        };
        let additional_cost = additional_cost_for_method_v1(definition, method);
        let retrace = self.retrace.then_some(RETRACE_COST);
        let additional_slot = self
            .component_groups
            .len()
            .checked_sub(1 + usize::from(retrace.is_some()));
        let original_groups = base
            .into_iter()
            .chain(additional_cost.map(|group| {
                if super::mandatory_cast_cost_v1::mana_branch(definition).is_some() {
                    additional_slot
                        .and_then(|slot| self.component_groups.get(slot))
                        .copied()
                        .unwrap_or(group)
                } else {
                    group
                }
            }))
            .chain(retrace)
            .collect::<Vec<_>>();
        if original_groups != self.component_groups {
            return None;
        }
        let ordered_objects = if method == CastMethodV4::Escape {
            let graveyard = &state.players[player.index()].graveyard;
            let ordered = graveyard
                .iter()
                .copied()
                .filter(|object| chosen.contains(object))
                .collect::<Vec<_>>();
            // Do not silently drop a stale or duplicate pick when preparing.
            if ordered.len() != chosen.len()
                || chosen
                    .iter()
                    .enumerate()
                    .any(|(index, object)| chosen[..index].contains(object))
            {
                return None;
            }
            ordered
        } else {
            chosen.to_vec()
        };
        let additional_owns_objects = additional_cost.is_some()
            && original_groups.last().is_some_and(|components| {
                super::controlled_permanent_sacrifice_in(components).is_some()
                    || super::tap_permanent_filter_in(components).is_some()
                    // Abhorrent Oculus: "exile six cards from your graveyard"
                    // as an additional cost of an ordinary cast.
                    || (cfg!(feature = "eternal-fixtures")
                        && components.iter().any(|component| {
                            matches!(component, CostComponent::ExileOtherCardsFromOwnGraveyard(_))
                        }))
            });
        let mut chosen_by_group = Vec::with_capacity(original_groups.len());
        if base.is_some() {
            chosen_by_group.push(if additional_owns_objects || retrace.is_some() {
                Vec::new()
            } else {
                ordered_objects.clone()
            });
        }
        if additional_cost.is_some() {
            chosen_by_group.push(if additional_owns_objects && retrace.is_none() {
                ordered_objects.clone()
            } else {
                Vec::new()
            });
        }
        // Retrace's discarded land cards are the cast's one object family.
        if retrace.is_some() {
            chosen_by_group.push(ordered_objects.clone());
        }
        Some(SpellComponentChoicesV1 {
            ordered_objects,
            chosen_by_group,
        })
    }

    /// Validate one chosen-object slice per collected component group before
    /// deriving the combined payment. The casting caller independently validates
    /// discard and chosen-creature bindings, and freezes generic modifiers before
    /// any cost changes state. A projected resource state can then include paid
    /// discards without changing that frozen total. Separately selected graveyard
    /// costs cannot reuse mandatory exile picks or supply Delve payment.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn payment_plan_with_component_choices_v1(
        &self,
        x: u16,
        player: PlayerId,
        source: ObjectId,
        state: &GameState,
        generic_modifiers: (u32, u32),
        chosen_by_group: &[&[ObjectId]],
        separately_reserved_graveyard: &[ObjectId],
    ) -> Option<SpellManaPaymentV1> {
        if chosen_by_group.len() != self.component_groups.len() {
            return None;
        }
        let mut reserved = separately_reserved_graveyard.to_vec();
        for (components, chosen) in self.component_groups.iter().zip(chosen_by_group) {
            reserved.extend(super::validate_cost_component_choices_v1(
                state, player, source, components, chosen,
            )?);
            // DiscardCards has already been staged/paid by FinishCast. Its
            // original hand-count check must not be repeated after discard.
            // Mana and life share the complete selected total below. The
            // chosen-creature binding belongs to PendingCast validation.
            let nonmana = components
                .iter()
                .copied()
                .filter(|component| {
                    !matches!(
                        component,
                        CostComponent::Mana(_)
                            | CostComponent::ConvokeMana(_)
                            | CostComponent::DiscardCards(_)
                            | CostComponent::PayLife(_)
                            | CostComponent::ChooseControlledCreatureOrRevealCreatureCardFromHand
                    )
                })
                .collect::<Vec<_>>();
            if !super::can_pay_components(&nonmana, player, source, state) {
                return None;
            }
            if components.iter().any(|component| {
                matches!(
                    component,
                    CostComponent::ExileOtherCardsFromOwnGraveyard(_)
                        | CostComponent::ExileOtherGraveyardCardsWithCardTypesV1(_)
                )
            }) {
                if chosen.iter().any(|object| reserved.contains(object)) {
                    return None;
                }
                reserved.extend(chosen.iter().copied());
            }
        }
        let (increase, reduction) = generic_modifiers;
        self.payment_plan(x, player, state, increase, reduction, &reserved)
            .filter(|payment| {
                super::checked_spell_mana_plan_v59(state, player, &payment.mana).is_some()
            })
    }

    /// Determine the payment without changing state. Freeze this result before
    /// any nonmana sacrifice can remove a cost reducer. Reserved objects are
    /// unavailable as mana sources or as Delve cards.
    pub(super) fn payment_plan(
        &self,
        x: u16,
        player: PlayerId,
        state: &GameState,
        increase: u32,
        reduction: u32,
        reserved: &[ObjectId],
    ) -> Option<SpellManaPaymentV1> {
        let Some(power) = self.teamwork else {
            return self.mana_payment_plan(x, player, state, increase, reduction, reserved);
        };
        // Teamwork: some untapped creatures with total power `power` or more
        // that the mana payment can do without. Creatures without positive
        // power never help reach the total, so at most `power` creatures
        // matter.
        let candidates = super::standard_cast_convoke::teamwork_candidates(state, player)
            .into_iter()
            .filter(|object| {
                !reserved.contains(object) && super::effective_power(state, *object) > 0
            })
            .collect::<Vec<_>>();
        let plan = |chosen: &[ObjectId]| {
            self.teamwork_payment(x, player, state, (increase, reduction), reserved, chosen)
        };
        teamwork_search(&candidates, &mut Vec::new(), i64::from(power), &plan, state)
    }

    /// The teamwork payment that taps `chosen`: their total power must reach
    /// the teamwork requirement, and they pay no mana.
    pub(super) fn teamwork_payment(
        &self,
        x: u16,
        player: PlayerId,
        state: &GameState,
        generic_modifiers: (u32, u32),
        reserved: &[ObjectId],
        chosen: &[ObjectId],
    ) -> Option<SpellManaPaymentV1> {
        let power = self.teamwork?;
        if chosen.is_empty() || total_power(state, chosen) < i64::from(power) {
            return None;
        }
        let mut excluded = reserved.to_vec();
        excluded.extend(chosen.iter().copied());
        let (increase, reduction) = generic_modifiers;
        let mut payment =
            self.mana_payment_plan(x, player, state, increase, reduction, &excluded)?;
        payment.convoke_tapped = chosen.to_vec();
        Some(payment)
    }

    /// The mana payment alone, without teamwork's tapping.
    fn mana_payment_plan(
        &self,
        x: u16,
        player: PlayerId,
        state: &GameState,
        increase: u32,
        reduction: u32,
        reserved: &[ObjectId],
    ) -> Option<SpellManaPaymentV1> {
        // No current selected cost combines Delve and Convoke.
        if self.convoke && self.delve {
            return None;
        }
        let replicated = self
            .replicate
            .iter()
            .flat_map(|cost| std::iter::repeat_n(*cost, usize::from(x)));
        let pips = self
            .costs
            .iter()
            .copied()
            .chain(replicated.clone())
            .flat_map(|cost| cost.pips.iter().copied())
            .collect::<Vec<_>>();
        let generic = self
            .costs
            .iter()
            .map(|cost| u32::from(cost.generic) + u32::from(cost.x_count) * u32::from(x))
            .chain(replicated.map(|cost| u32::from(cost.generic)))
            .sum::<u32>()
            .saturating_add(increase)
            .saturating_add(self.own_generic_increase)
            .saturating_sub(reduction.saturating_add(self.own_generic_reduction));
        // Trinisphere raises a total below three mana to three.
        #[cfg(feature = "eternal-fixtures")]
        let generic = generic.saturating_add(crate::eternal_vintage_v1::trinisphere_increase(
            state,
            pips.len(),
            generic,
        ));
        // Hogaak: creatures and graveyard cards pay; mana never does.
        if self.no_mana {
            if !self.convoke {
                return None;
            }
            let mut excluded = reserved.to_vec();
            excluded.extend(self.no_mana_source);
            let (convoke_tapped, delve_exiled) =
                crate::eternal_legacy_e8a_v1::statics::hogaak_payment(
                    &pips, generic, player, state, &excluded, None,
                )?;
            return Some(SpellManaPaymentV1 {
                mana: crate::mana::PaymentPlan::default(),
                convoke_tapped,
                delve_exiled,
            });
        }
        let additional_life = self
            .component_groups
            .iter()
            .flat_map(|group| group.iter())
            .filter_map(|component| match component {
                CostComponent::PayLife(amount) => Some(u32::from(*amount)),
                _ => None,
            })
            .sum::<u32>();
        if self.convoke {
            let (mana, convoke_tapped) =
                mana::plan_spell_mana_total_with_convoke_subtypes_with_grant_v72(
                    &pips,
                    generic,
                    player,
                    state,
                    self.types.contains(&CardType::Creature),
                    reserved,
                    additional_life,
                    self.legendary_spell,
                    &self.spell_subtypes,
                    self.all_creature_types_v60,
                    self.tinybones_mana_v72.as_ref(),
                    self.types.contains(&CardType::Artifact),
                    self.colorless_eldrazi,
                    self.improvise,
                )?;
            return Some(SpellManaPaymentV1 {
                mana,
                convoke_tapped,
                delve_exiled: Vec::new(),
            });
        }
        let graveyard = if self.delve {
            state.players[player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|object| !reserved.contains(object))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        // Preserve deterministic minimum-exile, oldest-first Delve payment.
        let max_exiled = graveyard.len().min(generic as usize);
        for exiled in 0..=max_exiled {
            if let Some(mana) = mana::plan_spell_mana_total_with_restrictions_with_grant_v72(
                &pips,
                generic - exiled as u32,
                player,
                state,
                self.types.contains(&CardType::Creature),
                reserved,
                additional_life,
                self.legendary_spell,
                &self.spell_subtypes,
                self.all_creature_types_v60,
                self.tinybones_mana_v72.as_ref(),
                self.types.contains(&CardType::Artifact),
                self.colorless_eldrazi,
            ) {
                return Some(SpellManaPaymentV1 {
                    mana,
                    delve_exiled: graveyard[..exiled].to_vec(),
                    convoke_tapped: Vec::new(),
                });
            }
        }
        None
    }

    pub(super) fn live_mana_only_plan(
        &self,
        x: u16,
        player: PlayerId,
        state: &GameState,
    ) -> Option<mana::PaymentPlan> {
        let (increase, reduction) =
            super::spell_cost_generic_modifiers_v1(state, self.types, player);
        self.mana_only_plan(x, player, state, increase, reduction)
    }

    fn components(&mut self, components: &'static [CostComponent]) {
        for component in components {
            match component {
                CostComponent::Mana(cost) => self.costs.push(*cost),
                CostComponent::ConvokeMana(cost) => {
                    self.costs.push(*cost);
                    self.convoke = true;
                }
                _ => {}
            }
        }
        self.component_groups.push(components);
    }

    /// Ordinary mana-only solution. Delve/Convoke require their additional
    /// payment planners and cannot silently fall back to this path.
    pub(super) fn mana_only_plan(
        &self,
        x: u16,
        player: PlayerId,
        state: &GameState,
        increase: u32,
        reduction: u32,
    ) -> Option<mana::PaymentPlan> {
        if self.delve || self.convoke {
            return None;
        }
        self.payment_plan(x, player, state, increase, reduction, &[])
            .map(|payment| payment.mana)
    }
}

/// The summed current power of `objects`.
pub(super) fn total_power(state: &GameState, objects: &[ObjectId]) -> i64 {
    objects
        .iter()
        .map(|object| i64::from(super::effective_power(state, *object)))
        .sum()
}

/// A completion of `chosen` from `candidates` whose total power reaches
/// `needed` and that `plan` can pay. Adding creatures past the requirement
/// only removes mana sources, so the search stops there.
pub(super) fn teamwork_search(
    candidates: &[ObjectId],
    chosen: &mut Vec<ObjectId>,
    needed: i64,
    plan: &dyn Fn(&[ObjectId]) -> Option<SpellManaPaymentV1>,
    state: &GameState,
) -> Option<SpellManaPaymentV1> {
    if total_power(state, chosen) >= needed {
        return plan(chosen);
    }
    for (index, object) in candidates.iter().enumerate() {
        if chosen.contains(object) {
            continue;
        }
        chosen.push(*object);
        if let Some(payment) =
            teamwork_search(&candidates[index + 1..], chosen, needed, plan, state)
        {
            return Some(payment);
        }
        chosen.pop();
    }
    None
}

pub(super) fn selected_spell_mana_costs_v1(
    definition: &card_def::CardDef,
    method: CastMethodV4,
    kicked: bool,
    mode: u8,
    targets: &[Target],
    player: PlayerId,
    state: &GameState,
) -> Option<SelectedSpellManaCostsV1> {
    let mut selected = SelectedSpellManaCostsV1 {
        granted_flashback_v1: false,
        tinybones_mana_v72: None,
        retrace: false,
        legendary_spell: definition
            .supertypes
            .contains(&card_def::Supertype::Legendary)
            && method != CastMethodV4::Omen,
        all_creature_types_v60: definition.changeling
            && method != CastMethodV4::Omen
            && method != CastMethodV4::Bestow
            && !(method == CastMethodV4::Alternative
                && definition
                    .alt_cost
                    .is_some_and(|a| a.condition == card_def::AltCostCondition::DisguiseFromHand)),
        spell_subtypes: definition
            .subtypes
            .iter()
            .map(|subtype| subtype.stable_id())
            .collect(),
        costs: Vec::new(),
        component_groups: Vec::new(),
        types: definition.types,
        own_generic_reduction: 0,
        own_generic_increase: 0,
        delve: false,
        convoke: false,
        improvise: cfg!(feature = "eternal-fixtures")
            && crate::eternal_legacy_e7a_v1::improvise_definition(definition.name),
        // Splice onto Arcane is paid like replicate: once per spliced card.
        replicate: crate::eternal_cards_v1::replicate_cost(definition.name)
            .or_else(|| crate::eternal_pauper_e8b_v1::splice_cost(definition)),
        colorless_eldrazi: definition.colors.is_empty()
            && definition.subtypes.contains(&card_def::Subtype::Eldrazi),
        teamwork: None,
        no_mana: cfg!(feature = "eternal-fixtures")
            && crate::eternal_legacy_e8a_v1::statics::no_mana_spell(definition.name),
        no_mana_source: None,
    };
    match method {
        CastMethodV4::Normal => {
            selected.costs.push(definition.cost);
            selected.delve = definition.delve;
            if let Some(reducer) = definition.generic_cost_reduction {
                selected.own_generic_reduction =
                    normal_cast_reduction_count(reducer.count, player, targets, state)
                        .saturating_mul(u32::from(reducer.generic_per_count));
            }
        }
        CastMethodV4::Alternative => {
            let alt = definition.alt_cost?;
            selected.components(alt.components);
            if let card_def::AltCostCondition::TeamworkV1(power) = alt.condition {
                selected.teamwork = Some(power);
            }
        }
        CastMethodV4::Flashback => selected.components(flashback_cost_v1(definition)?),
        CastMethodV4::Escape => selected.components(escape_cost_v1(definition)?),
        CastMethodV4::Madness => selected.costs.push(definition.madness_cost?),
        CastMethodV4::Plotted => {
            definition.plot_cost?;
            // Casting without paying its mana cost still pays additional
            // costs and applicable increases; the printed base is omitted.
        }
        CastMethodV4::Omen => {
            let (cost, types) = if let Some(adventure) = supported_adventure(definition) {
                (adventure.cost, adventure.types)
            } else {
                let omen = supported_omen(definition)?;
                (omen.cost, omen.types)
            };
            selected.costs.push(cost);
            selected.types = types;
        }
        CastMethodV4::Bestow => {
            selected.costs.push(supported_bestow(definition)?.cost);
            selected.types = &[CardType::Enchantment];
        }
    }
    // Spend restrictions inspect the selected spell form, not the creature
    // printed on a different face/form or the source of an activated ability.
    if method == CastMethodV4::Omen
        || (method == CastMethodV4::Alternative
            && definition
                .alt_cost
                .is_some_and(|alt| alt.condition == card_def::AltCostCondition::DisguiseFromHand))
    {
        selected.spell_subtypes.clear();
    } else if method == CastMethodV4::Bestow {
        selected.spell_subtypes.retain(|id| {
            !card_def::Subtype::CREATURE_TYPES
                .iter()
                .any(|subtype| subtype.stable_id() == *id)
        });
        selected
            .spell_subtypes
            .push(card_def::Subtype::Aura.stable_id());
    } else if definition.changeling {
        selected.spell_subtypes.extend(
            card_def::Subtype::CREATURE_TYPES
                .iter()
                .map(|subtype| subtype.stable_id()),
        );
    }
    selected.spell_subtypes.sort_unstable();
    selected.spell_subtypes.dedup();
    // Goblin Anarchomancer and the Familiars: spells of their colors cost
    // {1} less, whichever cost is paid.
    #[cfg(feature = "eternal-fixtures")]
    {
        selected.own_generic_reduction = selected.own_generic_reduction.saturating_add(
            crate::eternal_pauper_e8b_v1::spell_cost_reduction(state, definition, player),
        );
    }
    // Keep surcharges separate from the base so intermediate u8 saturation
    // cannot discard generic mana before the u32 total is determined.
    let surcharge = with_spree_surcharge(definition, mode, Cost::zero());
    if surcharge.generic != 0 {
        selected.costs.push(surcharge);
    }
    if kicked {
        selected.costs.push(definition.kicker_cost?);
    }
    if let Some(additional) = additional_cost_for_method_v1(definition, method) {
        let additional = if let Some(mana) = super::mandatory_cast_cost_v1::mana_branch(definition)
        {
            let sacrifice = state.engine.pending_cast.as_ref().is_some_and(|pending| {
                state.objects.try_get(pending.spell).is_some_and(|object| {
                    card_def::CARD_DEFS
                        .get(usize::from(object.card_def))
                        .is_some_and(|original| std::ptr::eq(definition, original))
                }) && pending
                    .mandatory_additional_cost
                    .is_some_and(|choice| choice.pay_mana == Some(false))
            });
            if sacrifice {
                super::mandatory_cast_cost_v1::SACRIFICE
            } else {
                mana
            }
        } else {
            additional
        };
        selected.components(additional);
    }
    Some(selected)
}

/// Retrace's additional cost: "discard a land card".
const RETRACE_COST: &[CostComponent] = &[CostComponent::DiscardLandCardsV1(1)];

/// A definition's mandatory additional cost for a cast by `method`. Demand
/// Answers' alternative cost replaces its discard
/// (`eternal_pauper_e6b_v1::alternative_skips_additional_cost`).
fn additional_cost_for_method_v1(
    definition: &card_def::CardDef,
    method: CastMethodV4,
) -> Option<&'static [CostComponent]> {
    #[cfg(feature = "eternal-fixtures")]
    if crate::eternal_pauper_e6b_v1::alternative_skips_additional_cost(
        definition,
        method == CastMethodV4::Alternative,
    ) || crate::eternal_pauper_e8b_v1::skips_additional_cost(
        definition,
        method == CastMethodV4::Flashback,
    ) {
        return None;
    }
    let _ = method;
    definition.additional_cost
}

/// A definition's flashback cost: printed, or the mana-cost flashback
/// Snapcaster Mage grants an instant or sorcery card. Whether a card has a
/// granted flashback is checked where the cast is offered.
fn flashback_cost_v1(definition: &card_def::CardDef) -> Option<&'static [CostComponent]> {
    definition
        .flashback
        .as_ref()
        .map(|flashback| flashback.cost)
        .or_else(|| crate::eternal_lurrus_v1::granted_flashback_cost(definition))
}

/// A definition's escape cost: printed, or the one Underworld Breach grants
/// a nonland card. Whether a card has a granted escape is checked where the
/// cast is offered.
fn escape_cost_v1(definition: &card_def::CardDef) -> Option<&'static [CostComponent]> {
    definition
        .escape
        .as_ref()
        .map(|escape| escape.cost)
        .or_else(|| crate::eternal_lurrus_v1::granted_escape_cost(definition))
}

// Keep selected form, payment and source inputs explicit at this shared quote boundary.
pub(super) fn selected_spell_mana_costs_for_source_v1(
    source: ObjectId,
    definition: &card_def::CardDef,
    form: (CastMethodV4, bool, u8),
    targets: &[Target],
    player: PlayerId,
    state: &GameState,
) -> Option<SelectedSpellManaCostsV1> {
    selected_spell_mana_costs_for_source_grant_v1(
        source,
        definition,
        form,
        targets,
        player,
        state,
        crate::effect::sphinx_lore_v1::granted_route(state, source),
    )
}

#[allow(clippy::too_many_arguments)]
fn selected_spell_mana_costs_for_source_grant_v1(
    source: ObjectId,
    definition: &card_def::CardDef,
    form: (CastMethodV4, bool, u8),
    targets: &[Target],
    player: PlayerId,
    state: &GameState,
    grant: Option<u64>,
) -> Option<SelectedSpellManaCostsV1> {
    let (method, kicked, mode) = form;
    let granted = method == CastMethodV4::Flashback && grant.is_some();
    if let Some(index) = grant {
        if state
            .objects
            .try_get(source)
            .and_then(|o| card_def::CARD_DEFS.get(o.card_def as usize))
            .is_none_or(|actual| !std::ptr::eq(actual, definition))
            || method != CastMethodV4::Flashback
            || !crate::effect::sphinx_lore_v1::quote_grant(state, source, player, index)
        {
            return None;
        }
    }
    let mut selected = selected_spell_mana_costs_v1(
        definition,
        if granted {
            CastMethodV4::Normal
        } else {
            method
        },
        kicked,
        mode,
        targets,
        player,
        state,
    )?;
    selected.tinybones_mana_v72 = super::tinybones_v72::mana_grant(state, source, player);
    if selected.no_mana {
        selected.no_mana_source = Some(source);
    }
    // Bilbo, Thief in the Night: "Spells you cast from anywhere other than
    // your hand cost {1} less to cast."
    #[cfg(feature = "eternal-fixtures")]
    {
        selected.own_generic_reduction = selected.own_generic_reduction.saturating_add(
            crate::eternal_cards_v1::outside_hand_cast_reduction(state, source, player),
        );
        // Anointed Peacekeeper: spells with the chosen name cost {2} more.
        selected.own_generic_increase =
            crate::eternal_vintage_v1::spell_tax(state, definition.object_name, player);
        // Retrace (Wrenn and Six's emblem): discard a land card in addition
        // to the other costs.
        if matches!(method, CastMethodV4::Normal | CastMethodV4::Alternative)
            && crate::eternal_vintage_e7b_v1::retrace_quote(state, source, player)
        {
            selected.components(RETRACE_COST);
            selected.retrace = true;
        }
        // Eye of Ugin, It That Heralds the End, Leyline Binding's domain.
        selected.own_generic_reduction = selected.own_generic_reduction.saturating_add(
            crate::eternal_legacy_e5b_v1::spell_cost_reduction(state, definition, player),
        );
    }
    if granted {
        selected.component_groups.insert(0, &[]);
        selected.granted_flashback_v1 = true;
    }
    if super::resolution_cast_v1::free_cast_for(state, source, player) {
        if !super::resolution_cast_v1::form_allowed_for(state, source, method, player) {
            return None;
        }
        selected.costs.remove(0);
    } else if cfg!(feature = "eternal-fixtures")
        && method == CastMethodV4::Normal
        && (crate::eternal_cards_v1::e3::free_hand_cast_selected(state, source, player, mode)
            || crate::eternal_e5a_v1::permissions::citadel_cast(state, source, player))
    {
        // Omniscience, Aluren: "without paying its mana cost". Bolas's
        // Citadel: "pay life equal to its mana value rather than pay its
        // mana cost", paid as the cast completes.
        selected.costs.remove(0);
    }
    Some(selected)
}

/// Quote a selected total before all interactive cost objects are picked.
/// Complete the current pipeline's single supported object family without
/// changing state. Tap picks reserve mana sources; sacrifices may use their
/// mana abilities first, and mandatory exile cannot also pay Delve.
#[allow(clippy::too_many_arguments)]
pub(super) fn selected_spell_quote_v1(
    definition: &card_def::CardDef,
    source: ObjectId,
    origin_zone: crate::state::Zone,
    method: CastMethodV4,
    kicked: bool,
    mode: u8,
    x: u8,
    targets: &[Target],
    chosen_prefix: &[ObjectId],
    player: PlayerId,
    state: &GameState,
    reserved_graveyard: &[ObjectId],
) -> Option<SpellManaPaymentV1> {
    selected_spell_quote_wide_x_v59(
        definition,
        source,
        origin_zone,
        method,
        kicked,
        mode,
        u16::from(x),
        targets,
        chosen_prefix,
        player,
        state,
        reserved_graveyard,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn selected_spell_quote_wide_x_v59(
    definition: &card_def::CardDef,
    source: ObjectId,
    origin_zone: crate::state::Zone,
    method: CastMethodV4,
    kicked: bool,
    mode: u8,
    x: u16,
    targets: &[Target],
    chosen_prefix: &[ObjectId],
    player: PlayerId,
    state: &GameState,
    reserved_graveyard: &[ObjectId],
) -> Option<SpellManaPaymentV1> {
    selected_spell_quote_with_grant_wide_x_v59(
        definition,
        source,
        origin_zone,
        method,
        kicked,
        mode,
        x,
        targets,
        chosen_prefix,
        player,
        state,
        reserved_graveyard,
        crate::effect::sphinx_lore_v1::granted_route(state, source),
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn selected_spell_quote_with_grant_v1(
    definition: &card_def::CardDef,
    source: ObjectId,
    origin_zone: crate::state::Zone,
    method: CastMethodV4,
    kicked: bool,
    mode: u8,
    x: u8,
    targets: &[Target],
    chosen_prefix: &[ObjectId],
    player: PlayerId,
    state: &GameState,
    reserved_graveyard: &[ObjectId],
    grant: Option<u64>,
) -> Option<SpellManaPaymentV1> {
    selected_spell_quote_with_grant_wide_x_v59(
        definition,
        source,
        origin_zone,
        method,
        kicked,
        mode,
        u16::from(x),
        targets,
        chosen_prefix,
        player,
        state,
        reserved_graveyard,
        grant,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn selected_spell_quote_with_grant_wide_x_v59(
    definition: &card_def::CardDef,
    source: ObjectId,
    origin_zone: crate::state::Zone,
    method: CastMethodV4,
    kicked: bool,
    mode: u8,
    x: u16,
    targets: &[Target],
    chosen_prefix: &[ObjectId],
    player: PlayerId,
    state: &GameState,
    reserved_graveyard: &[ObjectId],
    grant: Option<u64>,
) -> Option<SpellManaPaymentV1> {
    if super::mandatory_cast_cost_v1::mana_branch(definition).is_some() {
        let choice = state
            .engine
            .pending_cast
            .as_ref()
            .filter(|pending| pending.spell == source)
            .and_then(|pending| pending.mandatory_additional_cost)
            .and_then(|choice| choice.pay_mana);
        return [false, true]
            .into_iter()
            .filter(|branch| choice.is_none_or(|selected| selected == *branch))
            .find_map(|branch| {
                selected_spell_quote_branch_grant_v1(
                    definition,
                    source,
                    origin_zone,
                    method,
                    kicked,
                    mode,
                    x,
                    targets,
                    chosen_prefix,
                    player,
                    state,
                    reserved_graveyard,
                    Some(branch),
                    grant,
                )
            });
    }
    selected_spell_quote_branch_grant_v1(
        definition,
        source,
        origin_zone,
        method,
        kicked,
        mode,
        x,
        targets,
        chosen_prefix,
        player,
        state,
        reserved_graveyard,
        None,
        grant,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn selected_spell_quote_branch_wide_x_v59(
    definition: &card_def::CardDef,
    source: ObjectId,
    origin_zone: crate::state::Zone,
    method: CastMethodV4,
    kicked: bool,
    mode: u8,
    x: u16,
    targets: &[Target],
    chosen_prefix: &[ObjectId],
    player: PlayerId,
    state: &GameState,
    reserved_graveyard: &[ObjectId],
    forced_branch: Option<bool>,
) -> Option<SpellManaPaymentV1> {
    selected_spell_quote_branch_grant_v1(
        definition,
        source,
        origin_zone,
        method,
        kicked,
        mode,
        x,
        targets,
        chosen_prefix,
        player,
        state,
        reserved_graveyard,
        forced_branch,
        crate::effect::sphinx_lore_v1::granted_route(state, source),
    )
}

#[allow(clippy::too_many_arguments)]
fn selected_spell_quote_branch_grant_v1(
    definition: &card_def::CardDef,
    source: ObjectId,
    origin_zone: crate::state::Zone,
    method: CastMethodV4,
    kicked: bool,
    mode: u8,
    x: u16,
    targets: &[Target],
    chosen_prefix: &[ObjectId],
    player: PlayerId,
    state: &GameState,
    reserved_graveyard: &[ObjectId],
    forced_branch: Option<bool>,
    grant: Option<u64>,
) -> Option<SpellManaPaymentV1> {
    if x != 0
        && (super::resolution_cast_v1::free_cast_for(state, source, player)
            || (cfg!(feature = "eternal-fixtures")
                && method == CastMethodV4::Normal
                && (crate::eternal_cards_v1::e3::free_hand_cast_selected(
                    state, source, player, mode,
                ) || crate::eternal_e5a_v1::permissions::citadel_cast(state, source, player))))
    {
        return None;
    }
    if method == CastMethodV4::Alternative
        && !super::alt_cost_condition_met(
            definition.alt_cost?.condition,
            player,
            origin_zone,
            state,
        )
    {
        return None;
    }
    // Sneak and disturb: the costs each card may be cast for.
    #[cfg(feature = "eternal-fixtures")]
    if !crate::eternal_pauper_e6b_v1::quote_allowed(state, definition, source, origin_zone, method)
    {
        return None;
    }
    // Foretell: a foretold card is cast only for its foretell cost.
    #[cfg(feature = "eternal-fixtures")]
    if !crate::eternal_pauper_e6a_v1::foretell_quote_allowed(
        state,
        definition,
        source,
        origin_zone,
        method,
    ) {
        return None;
    }
    let mut selected = selected_spell_mana_costs_for_source_grant_v1(
        source,
        definition,
        (method, kicked, mode),
        targets,
        player,
        state,
        grant,
    )?;
    if let Some(pay_mana) = forced_branch {
        let mana = super::mandatory_cast_cost_v1::mana_branch(definition)?;
        // Retrace's group follows the mandatory additional cost.
        let retrace = if selected.retrace {
            selected.component_groups.pop()
        } else {
            None
        };
        let original = selected.component_groups.pop()?;
        if let [CostComponent::Mana(_)] = original {
            selected.costs.pop()?;
        }
        selected.components(if pay_mana {
            mana
        } else {
            super::mandatory_cast_cost_v1::SACRIFICE
        });
        selected.component_groups.extend(retrace);
    }
    let modifiers = super::spell_cost_generic_modifiers_v1(state, selected.types, player);
    let mut family = None;
    let mut graveyard_types_needed = None;
    let mut discard_count = 0usize;
    let mut needs_creature = false;
    let mut reveals_hand = false;
    for component in selected
        .component_groups
        .iter()
        .flat_map(|group| group.iter())
    {
        let interactive = match component {
            CostComponent::SacrificeLands(count) => Some((
                usize::from(*count),
                super::sacrificeable_lands(player, state, &[]),
            )),
            CostComponent::SacrificeControlled { count, filter } => Some((
                usize::from(*count),
                super::sacrificeable_controlled_permanents(player, *filter, state, &[]),
            )),
            CostComponent::SacrificeOtherControlledCreatures(count) => Some((
                usize::from(*count),
                super::sacrificeable_other_controlled_creatures(player, source, state, &[]),
            )),
            CostComponent::ExileOtherCardsFromOwnGraveyard(count) => Some((
                usize::from(*count),
                super::checked_graveyard_exile_candidates(player, source, state, &[])?,
            )),
            CostComponent::TapUntappedControlledPermanent(filter) => Some((
                1,
                super::tap_permanent_cost_candidates(player, state, *filter, &[]),
            )),
            CostComponent::ReturnControlledPermanentToOwnersHand(filter) => Some((
                1,
                super::return_permanent_cost_candidates(player, state, *filter, &[])
                    .into_iter()
                    .map(|binding| binding.object)
                    .collect(),
            )),
            CostComponent::ReturnControlledPermanentsToOwnersHandV1 { count, filter } => Some((
                usize::from(*count),
                super::return_permanent_cost_candidates(player, state, *filter, &[])
                    .into_iter()
                    .map(|binding| binding.object)
                    .collect(),
            )),
            CostComponent::TapUntappedControlledPermanentsV1 { count, filter } => Some((
                usize::from(*count),
                super::tap_permanent_cost_candidates(player, state, *filter, &[]),
            )),
            CostComponent::ExileCardFromHandWithColorV1(color) => Some((
                1,
                super::hand_exile_cost_candidates(player, source, *color, state),
            )),
            CostComponent::ExileCardsFromHandWithColorV1 { count, color } => Some((
                usize::from(*count),
                super::hand_exile_cost_candidates(player, source, *color, state),
            )),
            CostComponent::DiscardLandCardsV1(count) => Some((
                usize::from(*count),
                super::hand_land_discard_candidates(player, source, state),
            )),
            CostComponent::ExileOtherGraveyardCardsWithCardTypesV1(minimum) => {
                graveyard_types_needed = Some(*minimum);
                Some((
                    0,
                    super::checked_graveyard_exile_candidates(player, source, state, &[])?,
                ))
            }
            CostComponent::DiscardCards(count) => {
                discard_count += usize::from(*count);
                None
            }
            CostComponent::ChooseControlledCreatureOrRevealCreatureCardFromHand => {
                needs_creature = true;
                None
            }
            CostComponent::RevealHandIfNoCardsWithType(_) => {
                reveals_hand = true;
                None
            }
            _ => None,
        };
        if let Some(interactive) = interactive {
            if family.replace(interactive).is_some() {
                // PendingCast cannot stage two independent object families.
                return None;
            }
        }
    }
    if discard_count != 0 || needs_creature || reveals_hand {
        let mut hand = Vec::new();
        for &object in &state.players[player.index()].hand {
            let card = state.objects.try_get(object)?;
            if card.owner != player
                || card.zone != crate::state::Zone::Hand
                || card_def::CARD_DEFS.get(card.card_def as usize).is_none()
                || hand.contains(&object)
            {
                return None;
            }
            hand.push(object);
        }
        hand.retain(|object| *object != source);
        if hand.len() < discard_count {
            return None;
        }
        if needs_creature
            && super::chosen_creature_cost_candidates(
                player,
                super::ChosenCreatureCostZoneV1::Battlefield,
                state,
            )
            .is_empty()
            && (hand.len() <= discard_count
                || !hand.iter().any(|object| {
                    card_def::CARD_DEFS[state.objects.get(*object).card_def as usize]
                        .has_type(CardType::Creature)
                }))
        {
            return None;
        }
    }
    let quote = |chosen: &[ObjectId]| {
        let choices = selected.component_choices_v1(definition, method, player, state, chosen)?;
        let groups = choices
            .chosen_by_group
            .iter()
            .map(Vec::as_slice)
            .collect::<Vec<_>>();
        selected
            .payment_plan_with_component_choices_v1(
                x,
                player,
                source,
                state,
                modifiers,
                &groups,
                reserved_graveyard,
            )
            .filter(|payment| {
                !definition.records_mana_spent
                    || super::checked_spell_mana_plan_v59(state, player, &payment.mana)
                        .is_some_and(|n| u16::try_from(n).is_ok())
            })
    };
    let Some((needed, candidates)) = family else {
        return chosen_prefix.is_empty().then(|| quote(&[])).flatten();
    };
    if let Some(minimum) = graveyard_types_needed {
        // Any number of cards covering `minimum` card types: complete the
        // chosen prefix with cards that add a new card type. Which cards
        // pay never changes the mana payment.
        if chosen_prefix.iter().enumerate().any(|(index, object)| {
            !candidates.contains(object) || chosen_prefix[..index].contains(object)
        }) {
            return None;
        }
        let mut chosen = chosen_prefix.to_vec();
        for object in candidates {
            if crate::eternal_cards_v1::card_type_count(state, &chosen) >= usize::from(minimum) {
                break;
            }
            if chosen.contains(&object) {
                continue;
            }
            let before = crate::eternal_cards_v1::card_type_count(state, &chosen);
            chosen.push(object);
            if crate::eternal_cards_v1::card_type_count(state, &chosen) == before {
                chosen.pop();
            }
        }
        return quote(&chosen);
    }
    if chosen_prefix.len() > needed
        || chosen_prefix.iter().enumerate().any(|(index, object)| {
            !candidates.contains(object) || chosen_prefix[..index].contains(object)
        })
    {
        return None;
    }
    let candidates = candidates
        .into_iter()
        .filter(|object| !chosen_prefix.contains(object))
        .collect::<Vec<_>>();
    let mut chosen = chosen_prefix.to_vec();
    fn complete(
        candidates: &[ObjectId],
        needed: usize,
        chosen: &mut Vec<ObjectId>,
        quote: &impl Fn(&[ObjectId]) -> Option<SpellManaPaymentV1>,
    ) -> Option<SpellManaPaymentV1> {
        if chosen.len() == needed {
            return quote(chosen);
        }
        if candidates.len() < needed - chosen.len() {
            return None;
        }
        for (index, object) in candidates.iter().enumerate() {
            chosen.push(*object);
            if let Some(payment) = complete(&candidates[index + 1..], needed, chosen, quote) {
                return Some(payment);
            }
            chosen.pop();
        }
        None
    }
    complete(&candidates, needed, &mut chosen, &quote)
}

/// Complete ephemeral payment, prepared before any real discard or mana use.
/// The engine must consume it synchronously with the same player/source/state.
pub(super) struct PreparedSpellPaymentV1 {
    greatwurm: Option<super::greatwurm_cast_v1::PaymentV1>,
    mana: SpellManaPaymentV1,
    base: PreparedSpellNonmanaV1,
    additional: PreparedSpellNonmanaV1,
    chosen_reveal: Option<ObjectId>,
    optional_kind: Option<super::OptionalAdditionalCostDef>,
    optional_objects: Vec<ObjectId>,
    pub(super) ordered_objects: Vec<ObjectId>,
    pub(super) chosen_power_lki: Option<i32>,
}

impl SpellManaPaymentV1 {
    fn commit(&self, state: &mut GameState, player: PlayerId, source: ObjectId) -> u32 {
        for &object in &self.convoke_tapped {
            super::event::propose_and_commit(state, super::ProposedEvent::tap(object));
        }
        if !self.convoke_tapped.is_empty() {
            state.objects.get_mut(source).v4.convoked_creatures_v1 =
                u8::try_from(self.convoke_tapped.len()).unwrap_or(u8::MAX);
        }
        let spent = if super::spell_mana_needs_wide_commit_v59(state, player, &self.mana) {
            super::pay_spell_plan_wide_v59(state, player, &self.mana)
        } else {
            u32::from(super::pay_plan(state, player, &self.mana))
        };
        if !self.delve_exiled.is_empty() {
            super::commit_graveyard_exile(state, &self.delve_exiled);
        }
        spent
    }
}

impl PreparedSpellPaymentV1 {
    /// The graveyard cards Delve will exile, in exile order.
    #[cfg(feature = "eternal-fixtures")]
    pub(super) fn delve_exiled(&self) -> &[ObjectId] {
        &self.mana.delve_exiled
    }

    /// The mana sources this payment taps and the color each adds.
    #[cfg(feature = "eternal-fixtures")]
    pub(super) fn mana_taps(&self) -> &[(ObjectId, crate::mana::ManaColor)] {
        &self.mana.mana.taps
    }

    /// The restricted mana units this payment spends, by pool index.
    #[cfg(feature = "eternal-fixtures")]
    pub(super) fn restricted_pool_used(&self) -> &[usize] {
        &self.mana.mana.restricted_pool_used
    }

    /// Whether this payment spends colored mana (Void Mirror): a source
    /// tapped for a color, or colored mana from the pool.
    #[cfg(feature = "eternal-fixtures")]
    pub(super) fn spends_colored_mana(&self, state: &GameState, player: PlayerId) -> bool {
        let plan = &self.mana.mana;
        plan.taps
            .iter()
            .any(|(_, color)| *color != mana::ManaColor::C)
            || plan.pool_used[..5].iter().any(|&amount| amount > 0)
            || plan.restricted_pool_used.iter().any(|&index| {
                state.players[player.index()]
                    .restricted_mana_pool
                    .0
                    .get(index)
                    .is_some_and(|unit| unit.color != mana::ManaColor::C)
            })
    }

    /// How many colors of mana this payment spends (converge): the colors
    /// sources were tapped for and the colored mana taken from the pool.
    #[cfg(feature = "eternal-fixtures")]
    pub(super) fn colors_spent(&self, state: &GameState, player: PlayerId) -> u8 {
        let plan = &self.mana.mana;
        [
            mana::ManaColor::W,
            mana::ManaColor::U,
            mana::ManaColor::B,
            mana::ManaColor::R,
            mana::ManaColor::G,
        ]
        .into_iter()
        .filter(|color| {
            plan.taps.iter().any(|(_, tapped)| tapped == color)
                || plan.pool_used[color.pool_index()] > 0
                || plan.restricted_pool_used.iter().any(|&index| {
                    state.players[player.index()]
                        .restricted_mana_pool
                        .0
                        .get(index)
                        .is_some_and(|unit| unit.color == *color)
                })
        })
        .count() as u8
    }

    fn commit_optional(&self, state: &mut GameState) {
        match self.optional_kind {
            Some(super::OptionalAdditionalCostDef::CollectEvidence { .. }) => {
                super::commit_graveyard_exile(state, &self.optional_objects);
            }
            Some(
                super::OptionalAdditionalCostDef::Bargain
                | super::OptionalAdditionalCostDef::Casualty(_),
            ) => super::commit_sacrifice(state, &self.optional_objects),
            None => {}
        }
    }

    /// Discards have already committed through the shared Madness-aware path.
    pub(super) fn commit(self, state: &mut GameState, player: PlayerId, source: ObjectId) -> u32 {
        let spent = self.mana.commit(state, player, source);
        self.base.apply(state, player, source);
        if let Some(object) = self.chosen_reveal {
            PreparedNonmanaActionV1::RevealHand(vec![object]).commit(state, player, source);
        }
        self.additional.apply(state, player, source);
        self.commit_optional(state);
        if let Some(plan) = self.greatwurm {
            plan.apply(state);
        }
        spent
    }
}

/// Targets, timing and PendingCast bindings are independently authenticated by
/// the caller. Freeze modifiers before projecting discard, including Madness;
/// prepare all subsequent resource actions before the caller changes real state.
pub(super) fn prepare_final_spell_payment_v1(
    state: &GameState,
    pending: &super::PendingCast,
    method: CastMethodV4,
    discarded: &[ObjectId],
) -> Option<PreparedSpellPaymentV1> {
    if super::sphinx_lore_cast_v1::waiting(pending) {
        return None;
    }
    let definition = &card_def::CARD_DEFS[state.objects.get(pending.spell).card_def as usize];
    let player = pending.controller;
    let source = pending.spell;
    let selected = selected_spell_mana_costs_for_source_v1(
        source,
        definition,
        (
            method,
            pending.kicked == Some(true),
            pending.mode_chosen.unwrap_or(0),
        ),
        &pending.targets_chosen,
        player,
        state,
    )?;
    let modifiers = super::spell_cost_generic_modifiers_v1(state, selected.types, player);
    let choices = selected.component_choices_v1(
        definition,
        method,
        player,
        state,
        &pending.sacrifice_chosen,
    )?;
    let discard_count = selected
        .component_groups
        .iter()
        .flat_map(|group| group.iter())
        .map(|component| match component {
            CostComponent::DiscardCards(amount) => usize::from(*amount),
            _ => 0,
        })
        .sum::<usize>();
    if discarded.len() != discard_count
        || discarded.iter().enumerate().any(|(index, object)| {
            discarded[..index].contains(object)
                || *object == source
                || !state.players[player.index()].hand.contains(object)
                || state.objects.try_get(*object).is_none_or(|card| {
                    card.owner != player
                        || card.zone != crate::state::Zone::Hand
                        || card_def::CARD_DEFS.get(card.card_def as usize).is_none()
                })
        })
    {
        return None;
    }
    let optional_kind = if pending.optional_additional_cost_paid == Some(true) {
        Some(definition.optional_additional_cost?)
    } else {
        None
    };
    let optional_objects = pending
        .optional_additional_cost_chosen
        .iter()
        .map(|binding| binding.object)
        .collect::<Vec<_>>();
    let optional_graveyard = if matches!(
        optional_kind,
        Some(super::OptionalAdditionalCostDef::CollectEvidence { .. })
    ) {
        optional_objects.as_slice()
    } else {
        &[]
    };
    let group_choices = choices
        .chosen_by_group
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let resource_projection = if discarded.is_empty() {
        None
    } else {
        let mut projected = state.clone();
        projected.engine.pending_cast = None;
        projected.engine.pending_discard = None;
        for &object in discarded {
            super::commit_discarded_card(&mut projected, object);
        }
        Some(projected)
    };
    let resources = resource_projection.as_ref().unwrap_or(state);
    let mana = selected.payment_plan_with_component_choices_v1(
        pending.selected_x_v59().unwrap_or(0),
        player,
        source,
        resources,
        modifiers,
        &group_choices,
        optional_graveyard,
    )?;
    let spent = super::checked_spell_mana_plan_v59(resources, player, &mana.mana)?;
    if definition.records_mana_spent && u16::try_from(spent).is_err() {
        return None;
    }
    let mut prepared = PreparedSpellPaymentV1 {
        greatwurm: if pending
            .greatwurm_v1
            .as_ref()
            .is_some_and(|c| c.printed_route == Some(true))
        {
            Some(super::greatwurm_cast_v1::prepare(state, pending)?)
        } else {
            None
        },
        mana,
        base: PreparedSpellNonmanaV1 {
            actions: Vec::new(),
        },
        additional: PreparedSpellNonmanaV1 {
            actions: Vec::new(),
        },
        chosen_reveal: None,
        optional_kind,
        optional_objects,
        ordered_objects: choices.ordered_objects,
        chosen_power_lki: None,
    };
    if selected.convoke || selected.teamwork.is_some() {
        if !pending.convoke_finished {
            return None;
        }
        prepared.mana =
            super::standard_cast_convoke::payment(resources, pending, &pending.convoke_chosen)?;
    }
    // Ordinary mana-only spells require no whole-state projection.
    if prepared.greatwurm.is_none()
        && selected.component_groups.is_empty()
        && discarded.is_empty()
        && optional_kind.is_none()
        && pending.chosen_creature_cost.is_none()
        && !selected.delve
        && !selected.convoke
        && selected.teamwork.is_none()
    {
        return Some(prepared);
    }
    let mut projected = resource_projection.unwrap_or_else(|| state.clone());
    projected.engine.pending_cast = None;
    projected.engine.pending_discard = None;
    // Delve may use an actual graveyard discard, but a Madness card enters exile.
    // Its original frozen total is unchanged by either projection.
    prepared.mana.commit(&mut projected, player, source);
    let base_count = usize::from(matches!(
        method,
        CastMethodV4::Alternative | CastMethodV4::Flashback | CastMethodV4::Escape
    ));
    let groups = selected
        .component_groups
        .iter()
        .zip(&group_choices)
        .map(|(components, chosen)| (*components, *chosen))
        .collect::<Vec<_>>();
    prepared.base = prepare_spell_nonmana_v1(&projected, player, source, &groups[..base_count])?;
    prepared.base.apply(&mut projected, player, source);
    if let Some(binding) = pending.chosen_creature_cost {
        let zone = pending.chosen_creature_cost_zone?;
        if !super::chosen_creature_cost_candidates(player, zone, &projected).contains(&binding) {
            return None;
        }
        if zone == super::ChosenCreatureCostZoneV1::Hand {
            prepared.chosen_reveal = Some(binding.object);
            PreparedNonmanaActionV1::RevealHand(vec![binding.object]).commit(
                &mut projected,
                player,
                source,
            );
        }
    }
    prepared.additional =
        prepare_spell_nonmana_v1(&projected, player, source, &groups[base_count..])?;
    prepared.additional.apply(&mut projected, player, source);
    super::mandatory_cast_cost_v1::validate_paid_creature(&projected, pending).ok()?;
    if let Some(kind) = optional_kind {
        if pending
            .optional_additional_cost_chosen
            .iter()
            .any(|binding| !super::optional_cost_binding_is_live(&projected, *binding))
        {
            return None;
        }
        if !matches!(
            kind,
            super::OptionalAdditionalCostDef::CollectEvidence { .. }
        ) {
            let candidates = super::bargain_candidates(kind, player, &projected).ok()?;
            if !super::selected_bindings_are_unique_subset(
                &pending.optional_additional_cost_chosen,
                &candidates,
            ) {
                return None;
            }
        }
    }
    prepared.commit_optional(&mut projected);
    if let Some(binding) = pending.chosen_creature_cost {
        prepared.chosen_power_lki = Some(match projected.objects.get(binding.object).zone {
            crate::state::Zone::Battlefield => super::effective_power(&projected, binding.object),
            crate::state::Zone::Hand => card_def::CARD_DEFS
                [projected.objects.get(binding.object).card_def as usize]
                .power
                .map(i32::from)
                .unwrap_or(0),
            _ => return None,
        });
    }
    if prepared.greatwurm.is_some() {
        // Counts and exact incarnations were prepared before any real payment.
        // Automatic mana/base costs may not silently spend the same selected
        // counter units. Validate the exact counter-payment input before any
        // real cost. Current Greatwurm has no other printed nonmana cost.
        for unit in &pending.greatwurm_v1.as_ref()?.units {
            if crate::effect::greatwurm_v1::count(&projected, unit.permanent.object, unit.kind)
                .ok()?
                != crate::effect::greatwurm_v1::count(state, unit.permanent.object, unit.kind)
                    .ok()?
            {
                return None;
            }
        }
        super::greatwurm_cast_v1::prepare(&projected, pending)?.apply(&mut projected);
        super::validate_battlefield_pt(&projected).ok()?;
    }
    Some(prepared)
}

#[cfg(all(test, not(feature = "standard-magezero-fixtures")))]
mod tests {
    use super::*;

    fn ready() -> GameState {
        let forest = card_def::card_id_by_name("Forest").unwrap();
        GameState::new_from_libraries(&[forest], &[forest], |_| "Forest".into(), 941)
    }

    fn selected(name: &str, method: CastMethodV4, kicked: bool) -> SelectedSpellManaCostsV1 {
        let definition = &card_def::CARD_DEFS[card_def::card_id_by_name(name).unwrap() as usize];
        selected_spell_mana_costs_v1(definition, method, kicked, 0, &[], PlayerId::P0, &ready())
            .unwrap()
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn selected_kicker_discount_reaches_additional_generic_without_touching_red() {
        let selected = selected("Burst Lightning", CastMethodV4::Normal, true);
        let mut state = ready();
        state.players[0].mana_pool = [0, 0, 0, 1, 0, 3];
        let plan = selected
            .mana_only_plan(0, PlayerId::P0, &state, 0, 1)
            .unwrap();
        assert_eq!(plan.pool_used, state.players[0].mana_pool);
        state.players[0].mana_pool[5] = 2;
        assert!(selected
            .mana_only_plan(0, PlayerId::P0, &state, 0, 1)
            .is_none());
        state.players[0].mana_pool = [0, 0, 0, 0, 0, 255];
        assert!(selected
            .mana_only_plan(0, PlayerId::P0, &state, 0, 255)
            .is_none());
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn selected_flashback_and_escape_keep_their_nonmana_components() {
        for (name, method, has_exile) in [
            ("Think Twice", CastMethodV4::Flashback, false),
            ("Sleep of the Dead", CastMethodV4::Escape, true),
        ] {
            let selected = selected(name, method, false);
            let mut state = ready();
            state.players[0].mana_pool = [0, 1, 0, 0, 0, 1];
            let plan = selected
                .mana_only_plan(0, PlayerId::P0, &state, 0, 1)
                .unwrap();
            assert_eq!(plan.pool_used, state.players[0].mana_pool, "{name}");
            assert_eq!(
                selected
                    .component_groups
                    .iter()
                    .flat_map(|group| group.iter())
                    .any(|component| matches!(
                        component,
                        CostComponent::ExileOtherCardsFromOwnGraveyard(3)
                    )),
                has_exile
            );
        }
    }

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn selected_adventure_uses_spell_face_and_required_additional_discard_survives() {
        let adventure = selected("Fang Dragon", CastMethodV4::Omen, false);
        assert_eq!(adventure.types, &[CardType::Sorcery]);
        let mut state = ready();
        state.players[0].mana_pool[3] = 1;
        assert!(adventure
            .mana_only_plan(0, PlayerId::P0, &state, 0, 1)
            .is_some());
        let thrill = selected("Thrill of Possibility", CastMethodV4::Normal, false);
        assert!(thrill
            .component_groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|component| matches!(component, CostComponent::DiscardCards(1))));
        assert!(thrill
            .mana_only_plan(0, PlayerId::P0, &state, 0, 1)
            .is_some());
    }

    #[test]
    fn selected_alternative_keeps_sacrifices_and_applies_tax_to_zero_base_mana() {
        let selected = selected("Fireblast", CastMethodV4::Alternative, false);
        assert!(selected.costs.is_empty());
        assert!(selected
            .component_groups
            .iter()
            .flat_map(|group| group.iter())
            .any(|component| matches!(component, CostComponent::SacrificeLands(2))));
        let mut state = ready();
        assert!(selected
            .mana_only_plan(0, PlayerId::P0, &state, 1, 0)
            .is_none());
        state.players[0].mana_pool[5] = 1;
        let plan = selected
            .mana_only_plan(0, PlayerId::P0, &state, 1, 0)
            .unwrap();
        assert_eq!(plan.pool_used[5], 1);
    }

    #[test]
    fn selected_x_cost_discount_leaves_announced_x_and_colored_pips_intact() {
        let selected = selected("Nyxborn Hydra", CastMethodV4::Normal, false);
        let mut state = ready();
        state.players[0].mana_pool = [0, 0, 0, 0, 1, 2];
        let plan = selected
            .mana_only_plan(3, PlayerId::P0, &state, 0, 1)
            .unwrap();
        assert_eq!(plan.pool_used, state.players[0].mana_pool);
        assert!(selected
            .mana_only_plan(4, PlayerId::P0, &state, 0, 1)
            .is_none());
    }

    #[test]
    fn selected_component_choices_reject_missing_land_without_paying_mana_or_life() {
        let mut state = ready();
        let source = state.draw_card(PlayerId::P0).unwrap();
        state.players[0].mana_pool[5] = 3;
        let before = serde_json::to_value(&state).unwrap();
        let components = [CostComponent::PayLife(1), CostComponent::SacrificeLands(1)];
        assert!(super::super::validate_cost_component_choices_v1(
            &state,
            PlayerId::P0,
            source,
            &components,
            &[source],
        )
        .is_none());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert!(state.move_hand_to_battlefield(PlayerId::P0, source));
        let before = serde_json::to_value(&state).unwrap();
        assert_eq!(
            super::super::validate_cost_component_choices_v1(
                &state,
                PlayerId::P0,
                source,
                &components,
                &[source],
            ),
            Some(Vec::new())
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn selected_tap_cost_reservation_prevents_one_land_paying_two_components() {
        let mut state = ready();
        let source = state.draw_card(PlayerId::P0).unwrap();
        assert!(state.move_hand_to_battlefield(PlayerId::P0, source));
        let components = [
            CostComponent::Mana(Cost {
                pips: &[mana::Pip::Colored(mana::ManaColor::G)],
                generic: 0,
                x_count: 0,
            }),
            CostComponent::Tap,
        ];
        let reserved = super::super::validate_cost_component_choices_v1(
            &state,
            PlayerId::P0,
            source,
            &components,
            &[],
        )
        .unwrap();
        assert_eq!(reserved, vec![source]);
        let pips = [mana::Pip::Colored(mana::ManaColor::G)];
        assert!(
            mana::plan_spell_mana_total_v1(&pips, 0, PlayerId::P0, &state, false, &[], 0).is_some()
        );
        assert!(mana::plan_spell_mana_total_v1(
            &pips,
            0,
            PlayerId::P0,
            &state,
            false,
            &reserved,
            0
        )
        .is_none());
        assert!(!state.objects.get(source).tapped);
    }

    #[test]
    fn component_preflight_uses_frozen_modifiers_without_rechecking_completed_discard() {
        let mut selected = selected("Fireblast", CastMethodV4::Alternative, false);
        selected.costs = vec![Cost {
            pips: &[],
            generic: 2,
            x_count: 0,
        }];
        selected.component_groups =
            vec![&[CostComponent::DiscardCards(1), CostComponent::PayLife(1)]];
        let mut state = ready();
        let source = state.draw_card(PlayerId::P0).unwrap();
        state.players[0].mana_pool[5] = 1;
        // Only the casting source remains in hand after the validated discard.
        assert_eq!(state.players[0].hand, vec![source]);
        let before = serde_json::to_value(&state).unwrap();
        assert!(selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 1),
                &[&[]],
                &[],
            )
            .is_some());
        assert!(selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 0),
                &[&[]],
                &[],
            )
            .is_none());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        state.players[0].life = 0;
        assert!(selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 1),
                &[&[]],
                &[],
            )
            .is_none());
        assert!(selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 0),
                &[],
                &[],
            )
            .is_none());
    }

    #[test]
    fn component_preflight_reserves_graveyard_picks_from_delve_without_mutation() {
        let mut selected = selected("Gurmag Angler", CastMethodV4::Normal, false);
        selected.component_groups = vec![&[CostComponent::ExileOtherCardsFromOwnGraveyard(1)]];
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state =
            GameState::new_from_libraries(&[forest; 3], &[forest], |_| "Forest".into(), 950);
        let source = state.draw_card(PlayerId::P0).unwrap();
        let mut graveyard = Vec::new();
        for _ in 0..2 {
            let object = state.draw_card(PlayerId::P0).unwrap();
            state.players[0].hand.retain(|id| *id != object);
            state.objects.get_mut(object).zone = crate::state::Zone::Graveyard;
            state.players[0].graveyard.push(object);
            graveyard.push(object);
        }
        state.players[0].mana_pool = [0, 0, 1, 0, 0, 5];
        let before = serde_json::to_value(&state).unwrap();
        let payment = selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 0),
                &[&graveyard[..1]],
                &[],
            )
            .unwrap();
        assert_eq!(payment.delve_exiled, graveyard[1..]);
        assert!(selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 0),
                &[&graveyard[..1]],
                &graveyard[1..],
            )
            .is_none());
        assert!(selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 0),
                &[&[source]],
                &[],
            )
            .is_none());
        assert!(selected
            .payment_plan_with_component_choices_v1(
                0,
                PlayerId::P0,
                source,
                &state,
                (0, 0),
                &[&graveyard[..1]],
                &graveyard[..1],
            )
            .is_none());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn selected_convoke_pays_total_after_x_and_tax_reduction_adjustment() {
        let mut selected = selected("Fireblast", CastMethodV4::Alternative, false);
        selected.costs = vec![Cost {
            pips: &[],
            generic: 1,
            x_count: 1,
        }];
        selected.component_groups.clear();
        selected.convoke = true;
        let elf = card_def::card_id_by_name("Llanowar Elves").unwrap();
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[elf], &[forest], |_| "card".into(), 947);
        let object = state.draw_card(PlayerId::P0).unwrap();
        assert!(state.move_hand_to_battlefield(PlayerId::P0, object));
        state.players[0].mana_pool[5] = 2;
        let payment = selected
            .payment_plan(2, PlayerId::P0, &state, 1, 1, &[])
            .unwrap();
        assert_eq!(payment.convoke_tapped, vec![object]);
        assert_eq!(payment.mana.pool_used[5], 2);
        assert!(payment.mana.taps.is_empty());
        assert!(payment.delve_exiled.is_empty());
        assert!(selected
            .payment_plan(3, PlayerId::P0, &state, 1, 1, &[])
            .is_none());
        assert!(selected
            .payment_plan(2, PlayerId::P0, &state, 1, 1, &[object])
            .is_none());
        assert!(!state.objects.get(object).tapped);
    }

    #[test]
    fn selected_delve_pays_adjusted_total_including_x_and_additional_mana() {
        let mut selected = selected("Gurmag Angler", CastMethodV4::Normal, false);
        selected.costs.push(Cost {
            pips: &[],
            generic: 2,
            x_count: 1,
        });
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state =
            GameState::new_from_libraries(&[forest; 8], &[forest], |_| "Forest".into(), 943);
        for _ in 0..8 {
            let object = state.draw_card(PlayerId::P0).unwrap();
            state.players[0].hand.retain(|id| *id != object);
            state.objects.get_mut(object).zone = crate::state::Zone::Graveyard;
            state.players[0].graveyard.push(object);
        }
        state.players[0].mana_pool = [0, 0, 1, 0, 0, 3];
        let graveyard = state.players[0].graveyard.clone();
        let payment = selected
            .payment_plan(3, PlayerId::P0, &state, 1, 2, &[])
            .unwrap();
        // 6 + 2 + X=3 + tax1 - reduction2 = 10 generic, seven exiled.
        assert_eq!(payment.delve_exiled, graveyard[..7]);
        assert_eq!(payment.mana.pool_used, state.players[0].mana_pool);
        let payment = selected
            .payment_plan(3, PlayerId::P0, &state, 1, 2, &graveyard[..1])
            .unwrap();
        assert_eq!(payment.delve_exiled, graveyard[1..]);
        state.players[0].mana_pool[5] = 10;
        assert!(selected
            .payment_plan(3, PlayerId::P0, &state, 1, 2, &[])
            .unwrap()
            .delve_exiled
            .is_empty());
        state.players[0].mana_pool[2] = 0;
        assert!(selected
            .payment_plan(3, PlayerId::P0, &state, 1, 2, &[])
            .is_none());
    }

    #[test]
    fn selected_multiple_life_components_share_budget_with_phyrexian_mana() {
        let mut selected = selected("Fireblast", CastMethodV4::Alternative, false);
        selected.costs.push(Cost {
            pips: &[mana::Pip::Phyrexian(mana::ManaColor::B)],
            generic: 0,
            x_count: 0,
        });
        selected.component_groups =
            vec![&[CostComponent::PayLife(2)], &[CostComponent::PayLife(3)]];
        let mut state = ready();
        state.players[0].life = 6;
        assert!(selected
            .payment_plan(0, PlayerId::P0, &state, 0, 0, &[])
            .is_none());
        state.players[0].life = 7;
        let payment = selected
            .payment_plan(0, PlayerId::P0, &state, 0, 0, &[])
            .unwrap();
        assert_eq!(payment.mana.life_paid, 2);
        state.players[0].mana_pool[2] = 1;
        state.players[0].life = 5;
        let payment = selected
            .payment_plan(0, PlayerId::P0, &state, 0, 0, &[])
            .unwrap();
        assert_eq!(payment.mana.life_paid, 0);
        assert_eq!(payment.mana.pool_used[2], 1);
    }

    #[test]
    fn selected_madness_requires_its_own_red_pip_even_with_excess_reduction() {
        let selected = selected("Fiery Temper", CastMethodV4::Madness, false);
        let mut state = ready();
        state.players[0].mana_pool[5] = 255;
        assert!(selected
            .mana_only_plan(0, PlayerId::P0, &state, 0, 255)
            .is_none());
        state.players[0].mana_pool[3] = 1;
        let plan = selected
            .mana_only_plan(0, PlayerId::P0, &state, 0, 255)
            .unwrap();
        assert_eq!(plan.pool_used[3], 1);
        assert_eq!(plan.pool_used[5], 0);
    }

    fn staged_payment(names: &[&str], spell_name: &str) -> (GameState, super::super::PendingCast) {
        let definitions = names
            .iter()
            .map(|name| card_def::card_id_by_name(name).unwrap())
            .collect::<Vec<_>>();
        let mut state =
            GameState::new_from_libraries(&definitions, &definitions[..1], |_| "card".into(), 953);
        for _ in names {
            state.draw_card(PlayerId::P0).unwrap();
        }
        let spell_definition = card_def::card_id_by_name(spell_name).unwrap();
        let source = *state.players[0]
            .hand
            .iter()
            .find(|object| state.objects.get(**object).card_def == spell_definition)
            .unwrap();
        super::super::begin_cast(&mut state, PlayerId::P0, source);
        let pending = state.engine.pending_cast.as_ref().unwrap().clone();
        (state, pending)
    }

    fn quote_pending(
        state: &GameState,
        pending: &super::super::PendingCast,
        method: CastMethodV4,
        prefix: &[ObjectId],
        reserved: &[ObjectId],
    ) -> Option<SpellManaPaymentV1> {
        let definition = &card_def::CARD_DEFS[state.objects.get(pending.spell).card_def as usize];
        selected_spell_quote_v1(
            definition,
            pending.spell,
            pending.origin_zone,
            method,
            false,
            0,
            0,
            &[],
            prefix,
            pending.controller,
            state,
            reserved,
        )
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn quote_requires_a_discard_other_than_the_casting_source() {
        let (mut state, pending) =
            staged_payment(&["Thrill of Possibility"], "Thrill of Possibility");
        state.players[0].mana_pool = [0, 0, 0, 1, 0, 1];
        let before = serde_json::to_value(&state).unwrap();
        assert!(quote_pending(&state, &pending, CastMethodV4::Normal, &[], &[]).is_none());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        let (mut state, pending) = staged_payment(
            &["Thrill of Possibility", "Forest"],
            "Thrill of Possibility",
        );
        state.players[0].mana_pool = [0, 0, 0, 1, 0, 1];
        let before = serde_json::to_value(&state).unwrap();
        assert!(quote_pending(&state, &pending, CastMethodV4::Normal, &[], &[]).is_some());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn quote_completes_sacrifice_prefix_without_reserving_land_mana() {
        let (mut state, pending) = staged_payment(&["Raze", "Mountain"], "Raze");
        let land = state.players[0].hand[0];
        assert!(state.move_hand_to_battlefield(PlayerId::P0, land));
        let before = serde_json::to_value(&state).unwrap();
        for prefix in [&[][..], &[land][..]] {
            assert!(quote_pending(&state, &pending, CastMethodV4::Normal, prefix, &[]).is_some());
        }
        assert!(
            quote_pending(&state, &pending, CastMethodV4::Normal, &[land, land], &[]).is_none()
        );
        assert!(quote_pending(
            &state,
            &pending,
            CastMethodV4::Normal,
            &[pending.spell],
            &[]
        )
        .is_none());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn quote_escape_completion_excludes_source_and_separately_reserved_cards() {
        let (mut state, pending) = staged_payment(
            &["Sleep of the Dead", "Forest", "Forest", "Forest"],
            "Sleep of the Dead",
        );
        let graveyard = state.players[0].hand.clone();
        for &object in &graveyard {
            super::super::event::propose_and_commit(
                &mut state,
                super::super::ProposedEvent::zone_change(object, crate::state::Zone::Graveyard),
            );
        }
        state.players[0].mana_pool[1] = 1;
        state.players[0].mana_pool[5] = 2;
        let before = serde_json::to_value(&state).unwrap();
        assert!(quote_pending(&state, &pending, CastMethodV4::Escape, &[], &[]).is_some());
        assert!(
            quote_pending(&state, &pending, CastMethodV4::Escape, &[graveyard[0]], &[]).is_some()
        );
        assert!(quote_pending(
            &state,
            &pending,
            CastMethodV4::Escape,
            &[pending.spell],
            &[]
        )
        .is_none());
        assert!(
            quote_pending(&state, &pending, CastMethodV4::Escape, &[], &[graveyard[0]]).is_none()
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn quote_reveal_only_cost_authenticates_hand_before_legacy_lookups() {
        let (state, pending) = staged_payment(&["Land Grant", "Lightning Bolt"], "Land Grant");
        let hand = state.players[0].hand[0];
        assert!(quote_pending(&state, &pending, CastMethodV4::Alternative, &[], &[]).is_some());
        for invalid in 0..3 {
            let mut forged = state.clone();
            match invalid {
                0 => forged.objects.get_mut(hand).owner = PlayerId::P1,
                1 => forged.players[0].hand.push(ObjectId(u32::MAX)),
                _ => forged.objects.get_mut(hand).card_def = u16::MAX,
            }
            let before = serde_json::to_value(&forged).unwrap();
            assert!(
                quote_pending(&forged, &pending, CastMethodV4::Alternative, &[], &[]).is_none()
            );
            assert_eq!(serde_json::to_value(&forged).unwrap(), before);
        }
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn prepared_full_payment_refuses_before_discard_and_projects_madness_exactly() {
        for (discard_name, expected_zone) in [
            ("Forest", crate::state::Zone::Graveyard),
            ("Fiery Temper", crate::state::Zone::Exile),
        ] {
            let (mut state, pending) = staged_payment(
                &["Thrill of Possibility", discard_name],
                "Thrill of Possibility",
            );
            let discarded = state.players[0].hand[0];
            state.players[0].mana_pool[3] = 1;
            let before = serde_json::to_value(&state).unwrap();
            assert!(prepare_final_spell_payment_v1(
                &state,
                &pending,
                CastMethodV4::Normal,
                &[discarded]
            )
            .is_none());
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
            state.players[0].mana_pool[5] = 1;
            let funded = serde_json::to_value(&state).unwrap();
            let payment = prepare_final_spell_payment_v1(
                &state,
                &pending,
                CastMethodV4::Normal,
                &[discarded],
            )
            .unwrap();
            assert_eq!(serde_json::to_value(&state).unwrap(), funded);
            state.engine.pending_cast = None;
            super::super::commit_discarded_card(&mut state, discarded);
            let spent = payment.commit(&mut state, PlayerId::P0, pending.spell);
            assert_eq!(spent, 2);
            assert_eq!(state.players[0].mana_pool, [0; 6]);
            assert_eq!(state.objects.get(discarded).zone, expected_zone);
            assert_eq!(
                state
                    .engine
                    .pending_triggers
                    .iter()
                    .filter(|trigger| trigger.is_madness_offer)
                    .count(),
                usize::from(discard_name == "Fiery Temper")
            );
        }
    }

    #[test]
    fn prepared_full_payment_can_use_a_sacrificed_land_for_mana_first() {
        let (mut state, mut pending) = staged_payment(&["Raze", "Mountain"], "Raze");
        let land = state.players[0].hand[0];
        assert!(state.move_hand_to_battlefield(PlayerId::P0, land));
        pending.sacrifice_chosen = vec![land];
        let before = serde_json::to_value(&state).unwrap();
        let payment =
            prepare_final_spell_payment_v1(&state, &pending, CastMethodV4::Normal, &[]).unwrap();
        assert_eq!(payment.ordered_objects, vec![land]);
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        state.engine.pending_cast = None;
        let spent = payment.commit(&mut state, PlayerId::P0, pending.spell);
        assert_eq!(spent, 1);
        assert_eq!(state.players[0].mana_pool, [0; 6]);
        assert_eq!(state.objects.get(land).zone, crate::state::Zone::Graveyard);
        assert!(!state.players[0].battlefield.contains(&land));
        assert_eq!(
            state.players[0]
                .graveyard
                .iter()
                .filter(|object| **object == land)
                .count(),
            1
        );
    }

    #[test]
    fn prepared_nonmana_groups_refuse_shared_life_or_counter_exhaustion_without_mutation() {
        let elf = card_def::card_id_by_name("Llanowar Elves").unwrap();
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[elf], &[forest], |_| "card".into(), 950);
        let object = state.draw_card(PlayerId::P0).unwrap();
        assert!(state.move_hand_to_battlefield(PlayerId::P0, object));
        state.objects.get_mut(object).counters.plus1_plus1 = 2;
        state.players[0].life = 3;
        let before = serde_json::to_value(&state).unwrap();
        for component in [
            CostComponent::PayLife(2),
            CostComponent::RemovePlusOneCountersFromControlledCreatures(2),
        ] {
            let chosen = match component {
                CostComponent::RemovePlusOneCountersFromControlledCreatures(_) => {
                    vec![object, object]
                }
                _ => Vec::new(),
            };
            assert!(super::super::can_pay_components(
                &[component],
                PlayerId::P0,
                object,
                &state,
            ));
            assert!(prepare_spell_nonmana_v1(
                &state,
                PlayerId::P0,
                object,
                &[(&[component], &chosen), (&[component], &chosen)],
            )
            .is_none());
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
        }
        let prepared = prepare_spell_nonmana_v1(
            &state,
            PlayerId::P0,
            object,
            &[
                (
                    &[CostComponent::RemovePlusOneCountersFromControlledCreatures(
                        1,
                    )],
                    &[object],
                ),
                (
                    &[CostComponent::RemovePlusOneCountersFromControlledCreatures(
                        1,
                    )],
                    &[object],
                ),
            ],
        )
        .unwrap();
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        prepared.commit(&mut state, PlayerId::P0, object);
        assert_eq!(state.objects.get(object).counters.plus1_plus1, 0);
        assert_eq!(state.players[0].life, 3);
    }

    #[test]
    fn prepared_counter_payment_preserves_the_selected_creature() {
        let elf = card_def::card_id_by_name("Llanowar Elves").unwrap();
        let mut state = GameState::new_from_libraries(&[elf, elf], &[], |_| "elf".into(), 953);
        let first = state.draw_card(PlayerId::P0).unwrap();
        let selected = state.draw_card(PlayerId::P0).unwrap();
        for object in [first, selected] {
            assert!(state.move_hand_to_battlefield(PlayerId::P0, object));
            state.objects.get_mut(object).counters.plus1_plus1 = 1;
        }
        let before = state.clone();
        let payment = prepare_spell_nonmana_v1(
            &state,
            PlayerId::P0,
            first,
            &[(
                &[CostComponent::RemovePlusOneCountersFromControlledCreatures(
                    1,
                )],
                &[selected],
            )],
        )
        .unwrap();
        assert_eq!(state, before);
        payment.commit(&mut state, PlayerId::P0, first);
        assert_eq!(state.objects.get(first).counters.plus1_plus1, 1);
        assert_eq!(state.objects.get(selected).counters.plus1_plus1, 0);
    }

    #[test]
    fn prepared_nonmana_refuses_counters_after_their_only_creature_was_sacrificed() {
        let elf = card_def::card_id_by_name("Llanowar Elves").unwrap();
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state =
            GameState::new_from_libraries(&[elf, forest], &[forest], |_| "card".into(), 951);
        let creature = state.draw_card(PlayerId::P0).unwrap();
        assert!(state.move_hand_to_battlefield(PlayerId::P0, creature));
        let source = state.draw_card(PlayerId::P0).unwrap();
        state.objects.get_mut(creature).counters.plus1_plus1 = 2;
        let before = serde_json::to_value(&state).unwrap();
        assert!(prepare_spell_nonmana_v1(
            &state,
            PlayerId::P0,
            source,
            &[
                (
                    &[CostComponent::SacrificeControlled {
                        count: 1,
                        filter: super::super::PermanentFilter::Creature,
                    }],
                    &[creature]
                ),
                (
                    &[CostComponent::RemovePlusOneCountersFromControlledCreatures(
                        1
                    )],
                    &[creature]
                ),
            ],
        )
        .is_none());
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn prepared_nonmana_reveal_checks_live_hand_before_any_payment() {
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(&[forest], &[forest], |_| "card".into(), 952);
        let source = state.draw_card(PlayerId::P0).unwrap();
        let groups: &[(&[CostComponent], &[ObjectId])] = &[
            (&[CostComponent::PayLife(1)], &[]),
            (
                &[CostComponent::RevealHandIfNoCardsWithType(
                    CardType::Creature,
                )],
                &[],
            ),
        ];
        let before = serde_json::to_value(&state).unwrap();
        let prepared = prepare_spell_nonmana_v1(&state, PlayerId::P0, source, groups).unwrap();
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        prepared.commit(&mut state, PlayerId::P0, source);
        assert_eq!(state.players[0].life, 19);
        assert!(state.hand_knowledge[1][0]
            .iter()
            .any(|entry| entry.object == source));

        for tamper in 0..3 {
            let mut forged = state.clone();
            match tamper {
                0 => forged.objects.get_mut(source).owner = PlayerId::P1,
                1 => forged.players[0].hand.push(ObjectId(u32::MAX)),
                2 => forged.objects.get_mut(source).card_def = u16::MAX,
                _ => unreachable!(),
            }
            let corrupt = serde_json::to_value(&forged).unwrap();
            assert!(prepare_spell_nonmana_v1(&forged, PlayerId::P0, source, groups).is_none());
            assert_eq!(serde_json::to_value(&forged).unwrap(), corrupt);
        }
    }

    #[test]
    fn selected_object_choices_keep_base_and_additional_ownership() {
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state =
            GameState::new_from_libraries(&[forest; 2], &[forest], |_| "card".into(), 948);
        let objects = (0..2)
            .map(|_| {
                let object = state.draw_card(PlayerId::P0).unwrap();
                assert!(state.move_hand_to_battlefield(PlayerId::P0, object));
                object
            })
            .collect::<Vec<_>>();
        for (name, method, picks) in [
            ("Fireblast", CastMethodV4::Alternative, objects.as_slice()),
            ("Raze", CastMethodV4::Normal, &objects[..1]),
        ] {
            let definition =
                &card_def::CARD_DEFS[card_def::card_id_by_name(name).unwrap() as usize];
            let selected = selected_spell_mana_costs_v1(
                definition,
                method,
                false,
                0,
                &[],
                PlayerId::P0,
                &state,
            )
            .unwrap();
            let choices = selected
                .component_choices_v1(definition, method, PlayerId::P0, &state, picks)
                .unwrap();
            assert_eq!(choices.ordered_objects, picks, "{name}");
            assert_eq!(choices.chosen_by_group, vec![picks.to_vec()], "{name}");
            // A mismatch between collection and choice assignment fails closed.
            let mut wrong = selected;
            wrong.component_groups.clear();
            assert!(wrong
                .component_choices_v1(definition, method, PlayerId::P0, &state, picks)
                .is_none());
        }
    }

    #[test]
    #[cfg(feature = "limited-fdn-fixtures")]
    fn selected_escape_choices_canonicalize_without_dropping_invalid_picks() {
        let definition =
            &card_def::CARD_DEFS[card_def::card_id_by_name("Sleep of the Dead").unwrap() as usize];
        let selected = selected("Sleep of the Dead", CastMethodV4::Escape, false);
        let forest = card_def::card_id_by_name("Forest").unwrap();
        let mut state =
            GameState::new_from_libraries(&[forest; 4], &[forest], |_| "card".into(), 949);
        let mut objects = Vec::new();
        for _ in 0..4 {
            let object = state.draw_card(PlayerId::P0).unwrap();
            state.players[0].hand.retain(|id| *id != object);
            state.objects.get_mut(object).zone = crate::state::Zone::Graveyard;
            state.players[0].graveyard.push(object);
            objects.push(object);
        }
        let before = serde_json::to_value(&state).unwrap();
        let choices = selected
            .component_choices_v1(
                definition,
                CastMethodV4::Escape,
                PlayerId::P0,
                &state,
                &[objects[2], objects[0], objects[1]],
            )
            .unwrap();
        assert_eq!(choices.ordered_objects, objects[..3]);
        assert_eq!(choices.chosen_by_group, vec![objects[..3].to_vec()]);
        for invalid in [vec![objects[0], objects[0]], vec![ObjectId(u32::MAX)]] {
            assert!(selected
                .component_choices_v1(
                    definition,
                    CastMethodV4::Escape,
                    PlayerId::P0,
                    &state,
                    &invalid
                )
                .is_none());
        }
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
}
