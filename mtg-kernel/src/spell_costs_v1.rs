//! Selected spell mana costs before CR 601.2f adjustments.
//!
//! This collector does not pay nonmana components or validate targets/timing.
//! The next casting-path increment uses the same collected total for offers,
//! pending choices and payment, with nonmana costs committed afterward.

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

pub(super) struct SelectedSpellManaCostsV1 {
    pub(super) costs: Vec<Cost>,
    pub(super) component_groups: Vec<&'static [CostComponent]>,
    pub(super) types: &'static [CardType],
    pub(super) own_generic_reduction: u32,
    pub(super) delve: bool,
    pub(super) convoke: bool,
}

impl SelectedSpellManaCostsV1 {
    /// Validate one chosen-object slice per collected component group before
    /// deriving the combined payment. The casting caller independently validates
    /// discard and chosen-creature bindings, and freezes generic modifiers before
    /// any cost changes state. A projected resource state can then include paid
    /// discards without changing that frozen total. Separately selected graveyard
    /// costs cannot reuse mandatory exile picks or supply Delve payment.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn payment_plan_with_component_choices_v1(
        &self,
        x: u8,
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
                matches!(component, CostComponent::ExileOtherCardsFromOwnGraveyard(_))
            }) {
                if chosen.iter().any(|object| reserved.contains(object)) {
                    return None;
                }
                reserved.extend(chosen.iter().copied());
            }
        }
        let (increase, reduction) = generic_modifiers;
        self.payment_plan(x, player, state, increase, reduction, &reserved)
    }

    /// Determine the payment without changing state. Freeze this result before
    /// any nonmana sacrifice can remove a cost reducer. Reserved objects are
    /// unavailable as mana sources or as Delve cards.
    pub(super) fn payment_plan(
        &self,
        x: u8,
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
        let pips = self
            .costs
            .iter()
            .flat_map(|cost| cost.pips.iter().copied())
            .collect::<Vec<_>>();
        let generic = self
            .costs
            .iter()
            .map(|cost| u32::from(cost.generic) + u32::from(cost.x_count) * u32::from(x))
            .sum::<u32>()
            .saturating_add(increase)
            .saturating_sub(reduction.saturating_add(self.own_generic_reduction));
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
            let (mana, convoke_tapped) = mana::plan_spell_mana_total_with_convoke_v1(
                &pips,
                generic,
                player,
                state,
                self.types.contains(&CardType::Creature),
                reserved,
                additional_life,
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
            if let Some(mana) = mana::plan_spell_mana_total_v1(
                &pips,
                generic - exiled as u32,
                player,
                state,
                self.types.contains(&CardType::Creature),
                reserved,
                additional_life,
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
        x: u8,
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
        x: u8,
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
        costs: Vec::new(),
        component_groups: Vec::new(),
        types: definition.types,
        own_generic_reduction: 0,
        delve: false,
        convoke: false,
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
        CastMethodV4::Alternative => selected.components(definition.alt_cost?.components),
        CastMethodV4::Flashback => selected.components(definition.flashback.as_ref()?.cost),
        CastMethodV4::Escape => selected.components(definition.escape.as_ref()?.cost),
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
    // Keep surcharges separate from the base so intermediate u8 saturation
    // cannot discard generic mana before the u32 total is determined.
    let surcharge = with_spree_surcharge(definition, mode, Cost::zero());
    if surcharge.generic != 0 {
        selected.costs.push(surcharge);
    }
    if kicked {
        selected.costs.push(definition.kicker_cost?);
    }
    if let Some(additional) = definition.additional_cost {
        selected.components(additional);
    }
    Some(selected)
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
}
