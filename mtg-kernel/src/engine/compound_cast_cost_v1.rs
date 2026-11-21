//! Ordered object costs for a graveyard cast with a second mandatory object payment.
//! ObjectIds may recur across groups: returning a land or sacrificing a creature
//! creates a new incarnation which can pay the following cost.

use super::*;
use spell_costs_v1::{SelectedSpellManaCostsV1, SpellManaPaymentV1};

#[derive(Clone, Copy)]
pub(super) struct Costs {
    additional: CostComponent,
    route: CostComponent,
}

fn additional_object_cost(def: &card_def::CardDef) -> Option<CostComponent> {
    match def.additional_cost? {
        [cost @ (CostComponent::SacrificeControlled { .. }
        | CostComponent::SacrificeOtherControlledCreatures(_)
        | CostComponent::ReturnControlledPermanentToOwnersHand(_)
        | CostComponent::ExileOtherCardsFromOwnGraveyard(_)
        | CostComponent::ExileXCardsFromOwnGraveyardV1 { blue_only: false })] => Some(*cost),
        _ => None,
    }
}

pub(super) fn supports_escape(def: &card_def::CardDef) -> bool {
    additional_object_cost(def).is_some()
}

impl Costs {
    pub(super) fn new(
        def: &card_def::CardDef,
        method: CastMethodV4,
        retrace: bool,
    ) -> Option<Self> {
        let additional = additional_object_cost(def)?;
        let route = match method {
            CastMethodV4::Escape => spell_costs_v1::escape_cost_v1(def)?
                .iter()
                .copied()
                .find(|part| matches!(part, CostComponent::ExileOtherCardsFromOwnGraveyard(_)))?,
            CastMethodV4::Normal if retrace => CostComponent::DiscardLandCardsV1(1),
            _ => return None,
        };
        Some(Self { additional, route })
    }

    pub(super) fn pending(
        state: &GameState,
        pending: &PendingCast,
        def: &card_def::CardDef,
    ) -> Option<Self> {
        additional_object_cost(def)?;
        let method = finalized_cast_method(pending, pending.source_contract.cast_method, def);
        Self::new(
            def,
            method,
            crate::eternal_vintage_e7b_v1::pending_retrace(state, pending),
        )
    }

    fn groups(self, route_first: bool) -> [CostComponent; 2] {
        if route_first {
            [self.route, self.additional]
        } else {
            [self.additional, self.route]
        }
    }

    pub(super) fn announces_x(self) -> bool {
        matches!(
            self.additional,
            CostComponent::ExileXCardsFromOwnGraveyardV1 { .. }
        )
    }

    fn for_x(self, x: u16) -> Option<Self> {
        if self.announces_x() {
            Some(Self {
                additional: CostComponent::ExileOtherCardsFromOwnGraveyard(u8::try_from(x).ok()?),
                ..self
            })
        } else {
            Some(self)
        }
    }

    pub(super) fn count(self, pending: &PendingCast) -> usize {
        usize::from(count(self.route))
            + if self.announces_x() {
                usize::from(pending.selected_x_v59().unwrap_or(0))
            } else {
                usize::from(count(self.additional))
            }
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
        let costs = self.for_x(x)?;
        let total = usize::from(count(costs.route)) + usize::from(count(costs.additional));
        let additional_exiles = matches!(
            costs.additional,
            CostComponent::ExileOtherCardsFromOwnGraveyard(_)
        );
        let route_exiles = matches!(
            costs.route,
            CostComponent::ExileOtherCardsFromOwnGraveyard(_)
        );
        if prefix.len() > total
            || (!additional_exiles
                && state.players[player.index()].battlefield.len()
                    < usize::from(count(costs.additional)))
        {
            return None;
        }
        if additional_exiles || route_exiles {
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
        let survivors = ctx.candidates(&projected, costs.additional)?;
        let lost: Vec<_> = ctx
            .candidates(state, costs.additional)?
            .into_iter()
            .filter(|id| {
                !survivors.contains(id)
                    || state.objects.get(*id).zone_change_count
                        != projected.objects.get(*id).zone_change_count
            })
            .collect();
        for route_first in [false, true] {
            if order.is_some_and(|wanted| wanted != route_first)
                || (route_first && count(costs.additional) == 0)
            {
                continue;
            }
            let groups = costs.groups(route_first);
            let split = prefix.len().min(usize::from(count(groups[0])));
            let (first, second) = prefix.split_at(split);
            let additional_picks = if route_first { second } else { first };
            let complete = |projected: &GameState| {
                let candidates = ctx.candidates(projected, costs.additional)?;
                if additional_exiles && route_exiles && candidates.len() < total {
                    // Both groups draw from the same graveyard and must be disjoint.
                    return None;
                }
                if additional_exiles
                    && !route_exiles
                    && ctx.candidates(projected, costs.route)?.is_empty()
                {
                    // Exiling cards cannot put a land in hand to pay for retrace.
                    return None;
                }
                let discard_can_supply_exile = additional_exiles && !route_exiles && route_first;
                if !discard_can_supply_exile
                    && (candidates.len() < usize::from(count(costs.additional))
                        || additional_picks.iter().any(|id| !candidates.contains(id)))
                {
                    return None;
                }
                ctx.complete(projected, groups[0], first, |mut first_picks| {
                    let mut after = projected.clone();
                    ctx.pay(&mut after, groups[0], &mut first_picks);
                    ctx.complete(&after, groups[1], second, |mut second_picks| {
                        ctx.canonicalize(&after, groups[1], &mut second_picks);
                        Some(Payment {
                            groups: [(groups[0], first_picks.clone()), (groups[1], second_picks)],
                        })
                    })
                })
            };
            if let Some(plan) = complete(&projected) {
                return Some((mana, plan));
            }
            if !lost.is_empty() {
                // Only protect permanents actually lost to the first mana
                // plan. Other selected lands may still tap before returning.
                if let Some(plan) =
                    ctx.complete(state, costs.additional, additional_picks, |picks| {
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
        let groups = self
            .for_x(pending.selected_x_v59().unwrap_or(0))
            .expect("validated compound X")
            .groups(first);
        let len = pending.sacrifice_chosen.len();
        let first_count = usize::from(count(groups[0]));
        let (group, picked) = if len < first_count {
            (groups[0], len)
        } else {
            (groups[1], len - first_count)
        };
        // The preceding sacrifice, return or mana ability may supply a card
        // that is still on the battlefield in the uncommitted game.
        let seat = &state.players[pending.controller.index()];
        let mut candidates = match group {
            CostComponent::ExileOtherCardsFromOwnGraveyard(_) => seat.graveyard.clone(),
            CostComponent::DiscardLandCardsV1(_) => seat.hand.clone(),
            _ => Vec::new(),
        };
        if len >= first_count
            && matches!(groups[0], CostComponent::DiscardLandCardsV1(_))
            && matches!(group, CostComponent::ExileOtherCardsFromOwnGraveyard(_))
        {
            // The first payment can move a land from hand into the graveyard.
            candidates.extend_from_slice(&seat.hand);
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
            cost_kind: kind(group),
            remaining: count(group) - picked as u8,
            candidates,
        }
    }
}

fn count(group: CostComponent) -> u8 {
    match group {
        CostComponent::SacrificeControlled { count, .. }
        | CostComponent::SacrificeOtherControlledCreatures(count)
        | CostComponent::ExileOtherCardsFromOwnGraveyard(count)
        | CostComponent::DiscardLandCardsV1(count) => count,
        CostComponent::ReturnControlledPermanentToOwnersHand(_) => 1,
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
        CostComponent::ReturnControlledPermanentToOwnersHand(_) => CostKind::ReturnPermanentsToHand,
        CostComponent::ExileOtherCardsFromOwnGraveyard(_) => CostKind::ExileFromGraveyard,
        CostComponent::DiscardLandCardsV1(_) => CostKind::DiscardCards,
        _ => unreachable!(),
    }
}

struct Context<'a> {
    player: PlayerId,
    source: ObjectId,
    reserved: &'a [ObjectId],
}

impl Context<'_> {
    fn candidates(&self, state: &GameState, group: CostComponent) -> Option<Vec<ObjectId>> {
        let player = self.player;
        Some(match group {
            CostComponent::SacrificeControlled { filter, .. } => {
                sacrificeable_controlled_permanents(player, filter, state, &[])
            }
            CostComponent::SacrificeOtherControlledCreatures(_) => {
                sacrificeable_other_controlled_creatures(player, self.source, state, &[])
            }
            CostComponent::ReturnControlledPermanentToOwnersHand(filter) => {
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
        group: CostComponent,
        prefix: &[ObjectId],
        mut finish: impl FnMut(Vec<ObjectId>) -> Option<T>,
    ) -> Option<T> {
        let candidates = self.candidates(state, group)?;
        let needed = usize::from(count(group));
        if prefix.len() > needed
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
        fill(&candidates, needed, &mut prefix.to_vec(), &mut finish)
    }

    fn canonicalize(&self, state: &GameState, group: CostComponent, picks: &mut Vec<ObjectId>) {
        if matches!(group, CostComponent::ExileOtherCardsFromOwnGraveyard(_)) {
            *picks = state.players[self.player.index()]
                .graveyard
                .iter()
                .copied()
                .filter(|id| picks.contains(id))
                .collect();
        }
    }

    fn pay(&self, state: &mut GameState, group: CostComponent, picks: &mut Vec<ObjectId>) {
        self.canonicalize(state, group, picks);
        if matches!(
            group,
            CostComponent::SacrificeControlled { .. }
                | CostComponent::SacrificeOtherControlledCreatures(_)
        ) {
            #[cfg(feature = "eternal-fixtures")]
            crate::eternal_pauper_pf1c_v1::note_sacrifice_power(state, self.source, picks);
        }
        let paid = commit_cost_components_from_plan_v1(
            state,
            self.player,
            self.source,
            &[group],
            picks,
            None,
        );
        debug_assert_eq!(paid, Some(0));
    }
}

pub(super) struct Payment {
    groups: [(CostComponent, Vec<ObjectId>); 2],
}

impl Payment {
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
        for (group, picks) in &self.groups {
            let mut picks = picks.clone();
            ctx.pay(state, *group, &mut picks);
            receipts.extend(
                picks
                    .into_iter()
                    .map(|id| PaidCostRefV4::capture(state, id)),
            );
        }
        receipts
    }
}
