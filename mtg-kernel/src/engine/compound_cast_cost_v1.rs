//! Ordered object costs for a graveyard cast with a mandatory permanent payment.
//! ObjectIds may recur across groups: returning a land or sacrificing a creature
//! creates a new incarnation which can pay the following cost.

use super::*;
use spell_costs_v1::{SelectedSpellManaCostsV1, SpellManaPaymentV1};

#[derive(Clone, Copy)]
pub(super) struct Costs {
    permanent: CostComponent,
    route: CostComponent,
}

fn permanent_cost(def: &card_def::CardDef) -> Option<CostComponent> {
    match def.additional_cost? {
        [cost @ (CostComponent::SacrificeControlled { .. }
        | CostComponent::SacrificeOtherControlledCreatures(_)
        | CostComponent::ReturnControlledPermanentToOwnersHand(_))] => Some(*cost),
        _ => None,
    }
}

pub(super) fn supports_escape(def: &card_def::CardDef) -> bool {
    permanent_cost(def).is_some()
}

impl Costs {
    pub(super) fn new(
        def: &card_def::CardDef,
        method: CastMethodV4,
        retrace: bool,
    ) -> Option<Self> {
        let permanent = permanent_cost(def)?;
        let route = match method {
            CastMethodV4::Escape => spell_costs_v1::escape_cost_v1(def)?
                .iter()
                .copied()
                .find(|part| matches!(part, CostComponent::ExileOtherCardsFromOwnGraveyard(_)))?,
            CastMethodV4::Normal if retrace => CostComponent::DiscardLandCardsV1(1),
            _ => return None,
        };
        Some(Self { permanent, route })
    }

    pub(super) fn pending(
        state: &GameState,
        pending: &PendingCast,
        def: &card_def::CardDef,
    ) -> Option<Self> {
        permanent_cost(def)?;
        let method = finalized_cast_method(pending, pending.source_contract.cast_method, def);
        Self::new(
            def,
            method,
            crate::eternal_vintage_e7b_v1::pending_retrace(state, pending),
        )
    }

    fn groups(self, route_first: bool) -> [CostComponent; 2] {
        if route_first {
            [self.route, self.permanent]
        } else {
            [self.permanent, self.route]
        }
    }

    pub(super) fn count(self) -> usize {
        usize::from(count(self.route)) + usize::from(count(self.permanent))
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
        if prefix.len() > self.count()
            || state.players[player.index()].battlefield.len() < usize::from(count(self.permanent))
        {
            return None;
        }
        if matches!(
            self.route,
            CostComponent::ExileOtherCardsFromOwnGraveyard(_)
        ) {
            checked_graveyard_exile_candidates(player, source, state, &[])?;
        }
        let ctx = Context {
            player,
            source,
            reserved,
        };
        for route_first in [false, true] {
            if order.is_some_and(|wanted| wanted != route_first) {
                continue;
            }
            let groups = self.groups(route_first);
            let split = prefix.len().min(usize::from(count(groups[0])));
            let (first, second) = prefix.split_at(split);
            let permanent_picks = if route_first { second } else { first };
            let leaves_for_mana = |id| {
                state.objects.try_get(id).is_some()
                    && crate::eternal_premodern_p1a_v1::statics::leaves_when_tapped_for_mana(
                        state, id,
                    )
            };
            let quote = |permanents: &[ObjectId]| {
                let mut excluded = reserved.to_vec();
                excluded.extend(permanents.iter().copied().filter(|&id| leaves_for_mana(id)));
                let mana =
                    selected.payment_plan(x, player, state, modifiers.0, modifiers.1, &excluded)?;
                checked_spell_mana_plan_v59(state, player, &mana.mana)?;
                let mut projected = state.clone();
                projected.engine.pending_cast = None;
                mana.commit(&mut projected, player, source);
                let plan = ctx.complete(&projected, groups[0], first, |mut first_picks| {
                    let mut after = projected.clone();
                    ctx.pay(&mut after, groups[0], &mut first_picks);
                    ctx.complete(&after, groups[1], second, |mut second_picks| {
                        ctx.canonicalize(&after, groups[1], &mut second_picks);
                        Some(Payment {
                            groups: [(groups[0], first_picks.clone()), (groups[1], second_picks)],
                        })
                    })
                })?;
                Some((mana, plan))
            };
            if let Some(plan) = quote(permanent_picks) {
                return Some(plan);
            }
            // An unrestricted mana plan may use the last mining counter of a
            // permanent that must survive for payment. Reserve that permanent
            // and retry with the other available mana sources.
            if permanent_picks.len() < usize::from(count(self.permanent)) {
                if let Some(plan) = ctx.complete(state, self.permanent, permanent_picks, |picks| {
                    picks
                        .iter()
                        .any(|id| !permanent_picks.contains(id) && leaves_for_mana(*id))
                        .then(|| quote(&picks))
                        .flatten()
                }) {
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
