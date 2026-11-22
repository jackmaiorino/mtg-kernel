//! Ordered object costs for a graveyard cast with an additional object payment.
//! ObjectIds may recur across groups: returning, sacrificing, or discarding a card
//! creates a new incarnation which can pay the following cost.

use super::*;
use spell_costs_v1::{SelectedSpellManaCostsV1, SpellManaPaymentV1};

#[derive(Clone, Copy)]
pub(super) struct Costs {
    printed: Group,
    route: CostComponent,
}

#[derive(Clone, Copy)]
enum Group {
    Component(CostComponent),
    Alternative(&'static [CostComponent]),
    OptionalSacrifice(OptionalAdditionalCostDef),
}

impl Group {
    fn component(self) -> Option<CostComponent> {
        match self {
            Self::Component(cost) => Some(cost),
            Self::Alternative(components) => components.last().copied(),
            Self::OptionalSacrifice(_) => None,
        }
    }

    fn count(self) -> u8 {
        self.component().map_or(1, count)
    }
}

fn printed_cost(
    def: &card_def::CardDef,
    method: CastMethodV4,
    mode: u8,
    x: u16,
) -> Option<CostComponent> {
    let group = spell_costs_v1::additional_cost_for_method_v1(def, method, mode)?;
    match group {
        [CostComponent::DiscardCards(printed)] => {
            let dynamic =
                crate::eternal_premodern_p1b_v1::statics::additional_discard_count(def, Some(x))
                    .or_else(|| {
                        crate::eternal_cube_c1b_v1::additional_discard_count(def, Some(mode))
                    });
            Some(CostComponent::DiscardCards(
                dynamic.map_or(*printed, |amount| amount.unwrap_or(0)),
            ))
        }
        [CostComponent::ExileXCardsFromOwnGraveyardV1 { blue_only: false }] => Some(
            CostComponent::ExileOtherCardsFromOwnGraveyard(u8::try_from(x).ok()?),
        ),
        [cost @ (CostComponent::SacrificeControlled { .. }
        | CostComponent::SacrificeOtherControlledCreatures(_)
        | CostComponent::ReturnControlledPermanentToOwnersHand(_)
        | CostComponent::ExileOtherCardsFromOwnGraveyard(_))] => Some(*cost),
        _ => None,
    }
}

pub(super) fn supports_escape(def: &card_def::CardDef) -> bool {
    has_chosen_creature_additional_cost(def)
        || printed_cost(def, CastMethodV4::Escape, 0, 0).is_some()
}

impl Costs {
    pub(super) fn new(
        def: &card_def::CardDef,
        method: CastMethodV4,
        retrace: bool,
        mode: u8,
        x: u16,
    ) -> Option<Self> {
        if method == CastMethodV4::Alternative && retrace {
            // The alternative replaces the mana cost. Retrace is still an
            // additional cost. Retain the entire alternative, including any life payment.
            let components = def.alt_cost?.components;
            let object_cost = match components {
                [cost] | [CostComponent::PayLife(_), cost] => *cost,
                _ => return None,
            };
            if def.additional_cost.is_some()
                || !matches!(
                    object_cost,
                    CostComponent::ReturnControlledPermanentToOwnersHand(_)
                        | CostComponent::ReturnControlledPermanentsToOwnersHandV1 { .. }
                        | CostComponent::ExileCardFromHandWithColorV1(_)
                        | CostComponent::ExileCardsFromHandWithColorV1 { .. }
                )
            {
                return None;
            }
            return Some(Self {
                printed: Group::Alternative(components),
                route: CostComponent::DiscardLandCardsV1(1),
            });
        }
        let route = match method {
            CastMethodV4::Escape => spell_costs_v1::escape_cost_v1(def)?
                .iter()
                .copied()
                .find(|part| matches!(part, CostComponent::ExileOtherCardsFromOwnGraveyard(_)))?,
            CastMethodV4::Normal if retrace => CostComponent::DiscardLandCardsV1(1),
            _ => return None,
        };
        Some(Self {
            printed: Group::Component(printed_cost(def, method, mode, x)?),
            route,
        })
    }

    pub(super) fn optional_escape(def: &card_def::CardDef, method: CastMethodV4) -> Option<Self> {
        if method != CastMethodV4::Escape || def.additional_cost.is_some() {
            return None;
        }
        let kind = def.optional_additional_cost?;
        if !matches!(
            kind,
            OptionalAdditionalCostDef::Bargain | OptionalAdditionalCostDef::Casualty(_)
        ) {
            return None;
        }
        let route = spell_costs_v1::escape_cost_v1(def)?
            .iter()
            .copied()
            .find(|cost| matches!(cost, CostComponent::ExileOtherCardsFromOwnGraveyard(_)))?;
        Some(Self {
            printed: Group::OptionalSacrifice(kind),
            route,
        })
    }

    /// Earlier snapshots staged the route and then the sacrifice in separate fields.
    pub(super) fn ordered_optional(pending: &PendingCast) -> bool {
        pending.optional_additional_cost_paid == Some(true)
            && (pending.graveyard_cost_first_v1.is_some()
                || (pending.sacrifice_chosen.is_empty()
                    && pending.optional_additional_cost_chosen.is_empty()))
    }

    pub(super) fn pays_optional(self) -> bool {
        matches!(self.printed, Group::OptionalSacrifice(_))
    }

    pub(super) fn optional_pick_index(self, route_first: bool) -> Option<usize> {
        self.pays_optional().then(|| {
            if route_first {
                usize::from(count(self.route))
            } else {
                0
            }
        })
    }

    pub(super) fn pending(
        state: &GameState,
        pending: &PendingCast,
        def: &card_def::CardDef,
    ) -> Option<Self> {
        let method = finalized_cast_method(pending, pending.source_contract.cast_method, def);
        if Self::ordered_optional(pending) {
            if let Some(costs) = Self::optional_escape(def, method) {
                return Some(costs);
            }
        }
        if method != CastMethodV4::Alternative {
            def.additional_cost?;
        }
        if pending.cast_mode == Some(CastMode::Alternative)
            && crate::eternal_legacy_e8a_v1::graveyard_life_choice(def, pending.origin_zone, method)
        {
            return None;
        }
        Self::new(
            def,
            method,
            crate::eternal_vintage_e7b_v1::pending_retrace(state, pending),
            pending.mode_chosen?,
            pending.selected_x_v59().unwrap_or(0),
        )
    }

    /// Both printed X costs must be fixed before either payment group is chosen.
    pub(super) fn waiting_for_x(pending: &PendingCast, def: &card_def::CardDef) -> bool {
        pending.selected_x_v59().is_none()
            && (matches!(
                def.additional_cost,
                Some([CostComponent::ExileXCardsFromOwnGraveyardV1 { blue_only: false }])
            ) || crate::eternal_premodern_p1b_v1::statics::additional_discard_count(def, None)
                .is_some())
    }

    pub(super) fn printed_discard(self) -> bool {
        matches!(
            self.printed.component(),
            Some(CostComponent::DiscardCards(_))
        )
    }

    /// Only the escape group's cards can require a positive first X.
    /// Any payable set of discards can supply that many with at most one
    /// discarded card per exile. The payment quote checks replacements.
    pub(super) fn escape_discard_x_limit(
        def: &card_def::CardDef,
        method: CastMethodV4,
    ) -> Option<u8> {
        if method != CastMethodV4::Escape
            || crate::eternal_premodern_p1b_v1::statics::additional_discard_count(def, None)
                .is_none()
        {
            return None;
        }
        Some(count(Self::new(def, method, false, 0, 0)?.route))
    }

    fn groups(self, route_first: bool) -> [Group; 2] {
        if route_first {
            [Group::Component(self.route), self.printed]
        } else {
            [self.printed, Group::Component(self.route)]
        }
    }

    pub(super) fn count(self) -> usize {
        usize::from(count(self.route)) + usize::from(self.printed.count())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn quote(
        self,
        selected: &SelectedSpellManaCostsV1,
        state: &GameState,
        source: ObjectId,
        player: PlayerId,
        x: u16,
        modifiers: (u32, u32),
        prefix: &[ObjectId],
        order: Option<bool>,
        reserved: &[ObjectId],
    ) -> Option<(SpellManaPaymentV1, Payment)> {
        let printed_exiles = matches!(
            self.printed.component(),
            Some(CostComponent::ExileOtherCardsFromOwnGraveyard(_))
        );
        let route_exiles = matches!(
            self.route,
            CostComponent::ExileOtherCardsFromOwnGraveyard(_)
        );
        let needs_permanents = self.pays_optional()
            || matches!(
                self.printed.component(),
                Some(
                    CostComponent::SacrificeControlled { .. }
                        | CostComponent::SacrificeOtherControlledCreatures(_)
                        | CostComponent::ReturnControlledPermanentToOwnersHand(_)
                        | CostComponent::ReturnControlledPermanentsToOwnersHandV1 { .. }
                )
            );
        if prefix.len() > self.count()
            || (needs_permanents
                && state.players[player.index()].battlefield.len()
                    < usize::from(self.printed.count()))
        {
            return None;
        }
        if printed_exiles || route_exiles {
            checked_graveyard_exile_candidates(player, source, state, &[])?;
        }
        let ctx = Context {
            player,
            source,
            reserved,
        };
        let project_mana = |protected: &[ObjectId]| {
            let mut excluded = reserved.to_vec();
            excluded.extend_from_slice(protected);
            let mana =
                selected.payment_plan(x, player, state, modifiers.0, modifiers.1, &excluded)?;
            checked_spell_mana_plan_v59(state, player, &mana.mana)?;
            let mut projected = state.clone();
            projected.engine.pending_cast = None;
            projected.engine.pending_discard = None;
            mana.commit(&mut projected, player, source);
            projected
                .engine
                .halted
                .is_none()
                .then_some((mana, projected))
        };
        let (mana, projected) = project_mana(&[])?;
        let survivors = ctx.candidates(&projected, self.printed)?;
        let lost: Vec<_> = ctx
            .candidates(state, self.printed)?
            .into_iter()
            .filter(|id| {
                !survivors.contains(id)
                    || state.objects.get(*id).zone_change_count
                        != projected.objects.get(*id).zone_change_count
            })
            .collect();
        for route_first in [false, true] {
            if order.is_some_and(|wanted| wanted != route_first)
                || (route_first && self.printed.count() == 0)
            {
                continue;
            }
            let groups = self.groups(route_first);
            let split = prefix.len().min(usize::from(groups[0].count()));
            let (first, second) = prefix.split_at(split);
            let printed_picks = if route_first { second } else { first };
            let complete = |projected: &GameState| {
                let candidates = ctx.candidates(projected, self.printed)?;
                if printed_exiles && route_exiles && candidates.len() < self.count() {
                    // The escape and printed costs each need distinct graveyard cards.
                    return None;
                }
                if printed_exiles
                    && !route_exiles
                    && ctx
                        .candidates(projected, Group::Component(self.route))?
                        .is_empty()
                {
                    return None;
                }
                // A land discarded to retrace can subsequently pay a printed exile.
                // Other route payments cannot supply a printed discard or permanent.
                let discard_can_supply_exile = printed_exiles && !route_exiles && route_first;
                // Exiling cards can change a creature's effective power for casualty.
                let eligibility_after_escape = route_first
                    && matches!(
                        self.printed,
                        Group::OptionalSacrifice(OptionalAdditionalCostDef::Casualty(_))
                    )
                    && projected.players[player.index()]
                        .battlefield
                        .iter()
                        .any(|id| {
                            printed_picks.first().is_none_or(|selected| selected == id)
                                && object_has_type(projected, *id, CardType::Creature)
                        });
                if !discard_can_supply_exile
                    && !eligibility_after_escape
                    && (candidates.len() < usize::from(self.printed.count())
                        || printed_picks.iter().any(|id| !candidates.contains(id)))
                {
                    return None;
                }
                let reserved_land = if matches!(
                    groups,
                    [
                        Group::Component(CostComponent::DiscardCards(_)),
                        Group::Component(CostComponent::DiscardLandCardsV1(1)),
                    ]
                ) {
                    // A printed discard cannot supply retrace's land. Keep one
                    // unchosen land while completing this prefix, so trying the
                    // last land never enumerates the remaining hand subsets.
                    Some(
                        ctx.candidates(projected, Group::Component(self.route))?
                            .into_iter()
                            .find(|id| !first.contains(id))?,
                    )
                } else {
                    None
                };
                ctx.complete(
                    projected,
                    groups[0],
                    first,
                    reserved_land,
                    |mut first_picks| {
                        let mut after = projected.clone();
                        ctx.pay(&mut after, groups[0], &mut first_picks);
                        ctx.complete(&after, groups[1], second, None, |mut second_picks| {
                            ctx.canonicalize(&after, groups[1], &mut second_picks);
                            Some(Payment {
                                groups: [
                                    (groups[0], first_picks.clone()),
                                    (groups[1], second_picks),
                                ],
                            })
                        })
                    },
                )
            };
            if let Some(plan) = complete(&projected) {
                return Some((mana, plan));
            }
            if !lost.is_empty() {
                // Only protect permanents actually lost to the first mana
                // plan. Other selected lands may still tap before returning.
                if let Some(plan) =
                    ctx.complete(state, self.printed, printed_picks, None, |picks| {
                        let protect: Vec<_> =
                            picks.into_iter().filter(|id| lost.contains(id)).collect();
                        if protect.is_empty() {
                            return None;
                        }
                        let (mana, projected) = project_mana(&protect)?;
                        Some((mana, complete(&projected)?))
                    })
                {
                    return Some(plan);
                }
            }
        }
        None
    }

    pub(super) fn can_finish(
        self,
        state: &GameState,
        pending: &PendingCast,
        order: bool,
        picks: &[ObjectId],
    ) -> bool {
        let def = &card_def::CARD_DEFS[state.objects.get(pending.spell).card_def as usize];
        let method = finalized_cast_method(pending, pending.source_contract.cast_method, def);
        let Some(selected) = spell_costs_v1::selected_spell_mana_costs_for_source_v1(
            pending.spell,
            def,
            (
                method,
                pending.kicked == Some(true),
                pending.mode_chosen.unwrap_or(0),
            ),
            &pending.targets_chosen,
            pending.controller,
            state,
        ) else {
            return false;
        };
        let modifiers = spell_cost_generic_modifiers_v1(state, selected.types, pending.controller);
        self.quote(
            &selected,
            state,
            pending.spell,
            pending.controller,
            pending.selected_x_v59().unwrap_or(0),
            modifiers,
            picks,
            Some(order),
            &[],
        )
        .is_some()
    }

    pub(super) fn next(self, state: &GameState, pending: &PendingCast, first: bool) -> Decision {
        let groups = self.groups(first);
        let len = pending.sacrifice_chosen.len();
        let first_count = usize::from(groups[0].count());
        let (group, picked) = if len < first_count {
            (groups[0], len)
        } else {
            (groups[1], len - first_count)
        };
        // The preceding sacrifice, return or mana ability may supply a card
        // that is still on the battlefield in the uncommitted game.
        let seat = &state.players[pending.controller.index()];
        let mut candidates = match group.component() {
            Some(CostComponent::ExileOtherCardsFromOwnGraveyard(_)) => seat.graveyard.clone(),
            Some(
                CostComponent::DiscardLandCardsV1(_)
                | CostComponent::DiscardCards(_)
                | CostComponent::ExileCardFromHandWithColorV1(_)
                | CostComponent::ExileCardsFromHandWithColorV1 { .. },
            ) => seat.hand.clone(),
            _ => Vec::new(),
        };
        if len >= first_count
            && matches!(
                group.component(),
                Some(CostComponent::ExileOtherCardsFromOwnGraveyard(_))
            )
            && matches!(
                groups[0].component(),
                Some(CostComponent::DiscardCards(_) | CostComponent::DiscardLandCardsV1(_))
            )
        {
            candidates.extend(pending.sacrifice_chosen[..first_count].iter().copied());
        }
        candidates.extend_from_slice(&seat.battlefield);
        let mut picks = pending.sacrifice_chosen.clone();
        candidates.retain(|&id| {
            picks.push(id);
            let payable = self.can_finish(state, pending, first, &picks);
            picks.pop();
            payable
        });
        Decision::ChooseCostTargets {
            player: pending.controller,
            source: pending.spell,
            cost_kind: group
                .component()
                .map_or(CostKind::SacrificePermanents, kind),
            remaining: group.count() - picked as u8,
            candidates,
        }
    }
}

fn count(group: CostComponent) -> u8 {
    match group {
        CostComponent::SacrificeControlled { count, .. }
        | CostComponent::ReturnControlledPermanentsToOwnersHandV1 { count, .. }
        | CostComponent::ExileCardsFromHandWithColorV1 { count, .. }
        | CostComponent::SacrificeOtherControlledCreatures(count)
        | CostComponent::ExileOtherCardsFromOwnGraveyard(count)
        | CostComponent::DiscardCards(count)
        | CostComponent::DiscardLandCardsV1(count) => count,
        CostComponent::ReturnControlledPermanentToOwnersHand(_)
        | CostComponent::ExileCardFromHandWithColorV1(_) => 1,
        _ => unreachable!(),
    }
}

fn kind(group: CostComponent) -> CostKind {
    match group {
        CostComponent::SacrificeOtherControlledCreatures(_)
        | CostComponent::SacrificeControlled {
            filter: PermanentFilter::Creature,
            ..
        } => CostKind::SacrificeCreatures,
        CostComponent::SacrificeControlled { filter, .. } => cost_kind_for_permanent_filter(filter),
        CostComponent::ReturnControlledPermanentToOwnersHand(_)
        | CostComponent::ReturnControlledPermanentsToOwnersHandV1 { .. } => {
            CostKind::ReturnPermanentsToHand
        }
        CostComponent::ExileOtherCardsFromOwnGraveyard(_) => CostKind::ExileFromGraveyard,
        CostComponent::ExileCardFromHandWithColorV1(_)
        | CostComponent::ExileCardsFromHandWithColorV1 { .. } => CostKind::ExileFromHandV1,
        CostComponent::DiscardLandCardsV1(_) | CostComponent::DiscardCards(_) => {
            CostKind::DiscardCards
        }
        _ => unreachable!(),
    }
}

struct Context<'a> {
    player: PlayerId,
    source: ObjectId,
    reserved: &'a [ObjectId],
}

impl Context<'_> {
    fn candidates(&self, state: &GameState, group: Group) -> Option<Vec<ObjectId>> {
        let player = self.player;
        if let Group::OptionalSacrifice(kind) = group {
            return Some(
                bargain_candidates(kind, player, state)
                    .ok()?
                    .into_iter()
                    .map(|binding| binding.object)
                    .collect(),
            );
        }
        let group = group.component()?;
        Some(match group {
            CostComponent::SacrificeControlled { filter, .. } => {
                sacrificeable_controlled_permanents(player, filter, state, &[])
            }
            CostComponent::SacrificeOtherControlledCreatures(_) => {
                sacrificeable_other_controlled_creatures(player, self.source, state, &[])
            }
            CostComponent::ReturnControlledPermanentToOwnersHand(filter)
            | CostComponent::ReturnControlledPermanentsToOwnersHandV1 { filter, .. } => {
                return_permanent_cost_candidates(player, state, filter, &[])
                    .into_iter()
                    .map(|b| b.object)
                    .collect()
            }
            CostComponent::ExileOtherCardsFromOwnGraveyard(_) => {
                // Between payments, a token can still be indexed in a graveyard
                // until the next SBA. Only cards may pay escape.
                state.players[player.index()]
                    .graveyard
                    .iter()
                    .copied()
                    .filter(|id| {
                        *id != self.source
                            && !self.reserved.contains(id)
                            && state.objects.try_get(*id).is_some_and(|o| {
                                o.owner == player && o.zone == Zone::Graveyard && !o.v4.is_token
                            })
                    })
                    .collect()
            }
            CostComponent::ExileCardFromHandWithColorV1(color)
            | CostComponent::ExileCardsFromHandWithColorV1 { color, .. } => {
                hand_exile_cost_candidates(player, self.source, color, state)
            }
            CostComponent::DiscardCards(_) => state.players[player.index()]
                .hand
                .iter()
                .copied()
                .filter(|&id| {
                    id != self.source
                        && state.objects.try_get(id).is_some_and(|o| {
                            o.owner == player
                                && o.zone == Zone::Hand
                                && !o.v4.is_token
                                && o.spell_copy_origin.is_none()
                                && card_def::CARD_DEFS.get(usize::from(o.card_def)).is_some()
                        })
                })
                .collect(),
            CostComponent::DiscardLandCardsV1(_) => {
                hand_land_discard_candidates(player, self.source, state)
                    .into_iter()
                    .filter(|id| !state.objects.get(*id).v4.is_token)
                    .collect()
            }
            _ => return None,
        })
    }

    fn complete<T>(
        &self,
        state: &GameState,
        group: Group,
        prefix: &[ObjectId],
        reserved_land: Option<ObjectId>,
        mut finish: impl FnMut(Vec<ObjectId>) -> Option<T>,
    ) -> Option<T> {
        let mut candidates = self.candidates(state, group)?;
        if let Some(land) = reserved_land {
            candidates.retain(|id| *id != land);
        }
        let needed = usize::from(group.count());
        if candidates.len() < needed
            || prefix.len() > needed
            || prefix
                .iter()
                .enumerate()
                .any(|(i, id)| !candidates.contains(id) || prefix[..i].contains(id))
        {
            return None;
        }
        fn fill<T>(
            choices: &[ObjectId],
            needed: usize,
            picks: &mut Vec<ObjectId>,
            finish: &mut impl FnMut(Vec<ObjectId>) -> Option<T>,
        ) -> Option<T> {
            if picks.len() == needed {
                return finish(picks.clone());
            }
            if choices.len() < needed - picks.len() {
                return None;
            }
            for (i, &id) in choices.iter().enumerate() {
                if picks.contains(&id) {
                    continue;
                }
                picks.push(id);
                if let Some(output) = fill(&choices[i + 1..], needed, picks, finish) {
                    return Some(output);
                }
                picks.pop();
            }
            None
        }
        fill(&candidates, needed, &mut prefix.to_vec(), &mut |picks| {
            if let Group::Alternative(components) = group {
                validate_cost_component_choices_v1(
                    state,
                    self.player,
                    self.source,
                    components,
                    &picks,
                )?;
            }
            finish(picks)
        })
    }

    fn canonicalize(&self, state: &GameState, group: Group, picks: &mut Vec<ObjectId>) {
        if matches!(
            group.component(),
            Some(CostComponent::ExileOtherCardsFromOwnGraveyard(_))
        ) {
            *picks = state.players[self.player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|id| picks.contains(id))
                .collect();
        }
    }

    fn pay(&self, state: &mut GameState, group: Group, picks: &mut Vec<ObjectId>) {
        self.canonicalize(state, group, picks);
        let Some(cost) = group.component() else {
            commit_sacrifice(state, picks);
            return;
        };
        if matches!(cost, CostComponent::DiscardCards(_)) {
            commit_discarded_cards(state, picks);
            return;
        }
        if matches!(
            cost,
            CostComponent::SacrificeControlled { .. }
                | CostComponent::SacrificeOtherControlledCreatures(_)
        ) {
            #[cfg(feature = "eternal-fixtures")]
            crate::eternal_pauper_pf1c_v1::note_sacrifice_power(state, self.source, picks);
        }
        let components = match group {
            Group::Alternative(components) => components,
            _ => std::slice::from_ref(&cost),
        };
        let paid = commit_cost_components_from_plan_v1(
            state,
            self.player,
            self.source,
            components,
            picks,
            None,
        );
        debug_assert_eq!(paid, Some(0));
    }
}

pub(super) struct Payment {
    groups: [(Group, Vec<ObjectId>); 2],
}

impl Payment {
    pub(super) fn printed_discards(&self) -> &[ObjectId] {
        self.groups
            .iter()
            .find_map(|(group, picks)| {
                matches!(group.component(), Some(CostComponent::DiscardCards(_)))
                    .then_some(picks.as_slice())
            })
            .unwrap_or_default()
    }

    pub(super) fn commit(
        &self,
        state: &mut GameState,
        player: PlayerId,
        source: ObjectId,
    ) -> Vec<PaidCostRefV4> {
        let ctx = Context {
            player,
            source,
            reserved: &[],
        };
        let mut receipts = Vec::new();
        let mut optional = None;
        for (group, picks) in &self.groups {
            let mut picks = picks.clone();
            ctx.pay(state, *group, &mut picks);
            if matches!(group, Group::OptionalSacrifice(_)) {
                optional = Some(PaidCostRefV4::capture(state, picks[0]));
                continue;
            }
            receipts.extend(
                picks
                    .into_iter()
                    .map(|id| PaidCostRefV4::capture(state, id)),
            );
        }
        // Retain the established route-then-optional receipt layout in either payment order.
        receipts.extend(optional);
        receipts
    }
}
