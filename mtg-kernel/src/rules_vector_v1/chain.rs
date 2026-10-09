//! Backward chaining over rules facets (a diagnostic, not a policy input).
//!
//! Starting from "an opponent loses life", the chainer regresses through the
//! facets of a deck's cards: a damage amount that reads "creature cards in
//! your graveyard" needs something that moves creature cards into your
//! graveyard; a trigger on entering needs something that puts the card onto
//! the battlefield; a graveyard cast needs the card in your graveyard. It is
//! generic goal regression: it uses no card names and nothing a human wrote
//! about the cards, only what their programs do.
//!
//! Costs (sacrifices, mana) are not regressed in v1; the report says so.

use std::collections::BTreeSet;

use serde::Serialize;

use super::facets::*;
use super::sources::{card_rules, CardRulesV1};
use crate::card_def::CARD_DEFS;

/// Something the chainer is trying to make true.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum GoalV1 {
    /// An opponent loses life.
    OpponentLosesLife,
    /// The number of objects of `obj` in `zone` of `owner`'s side goes up.
    CountUp { owner: RelF, zone: ZoneF, obj: ObjF },
    /// A specific deck card is in a zone of yours.
    InZone { card: u16, zone: ZoneF },
    /// A specific deck card enters the battlefield.
    Enters { card: u16 },
}

/// One supporting step: which card's ability achieves which goal, and how
/// large the enabling effect is.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct StepV1 {
    pub goal: GoalV1,
    pub card: u16,
    pub ability: usize,
    pub amount: AmtF,
    pub needs: Vec<GoalV1>,
}

/// A complete line: every goal reached either by casting a card from hand
/// or by a step whose needs are themselves met.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct LineV1 {
    pub steps: Vec<StepV1>,
    /// Distinct cards the line uses.
    pub cards: Vec<u16>,
    /// True when the final damage is state-dependent (scales with the
    /// enabling moves) rather than a fixed number.
    pub scaling: bool,
}

struct Deck {
    ids: Vec<u16>,
    rules: Vec<CardRulesV1>,
}

fn has_target(ability: &AbilityFacts, wanted: TargetAtom) -> bool {
    ability.atoms.contains(&Atom::Target(wanted))
}

/// Whether an effect's player relation can denote `owner`.
fn reaches(player: Option<RelF>, owner: RelF, ability: &AbilityFacts) -> bool {
    match player {
        None => false,
        Some(p) if p == owner => true,
        Some(RelF::EachPlayer) => true,
        Some(RelF::EachOpponent) => owner == RelF::Opponent,
        Some(RelF::ChosenPlayer) => match owner {
            RelF::You => has_target(ability, TargetAtom::PlayerYou),
            RelF::Opponent => has_target(ability, TargetAtom::PlayerOpponent),
            _ => false,
        },
        Some(_) => false,
    }
}

fn card_types(card: u16) -> Vec<CardTypeF> {
    CARD_DEFS[usize::from(card)]
        .types
        .iter()
        .map(|&t| t.into())
        .collect()
}

/// Whether an effect's object class covers a card of these types.
fn covers(obj: Option<ObjF>, types: &[CardTypeF]) -> bool {
    match obj {
        None | Some(ObjF::AnyCard) => true,
        Some(ObjF::Typed(t)) => types.contains(&t),
        Some(ObjF::Permanent) => !types
            .iter()
            .all(|t| matches!(t, CardTypeF::Instant | CardTypeF::Sorcery)),
        Some(ObjF::NonlandPermanent) => types
            .iter()
            .any(|t| !matches!(t, CardTypeF::Land | CardTypeF::Instant | CardTypeF::Sorcery)),
        _ => false,
    }
}

/// Whether an effect's object class covers a read's object class.
fn covers_class(effect_obj: Option<ObjF>, read_obj: Option<ObjF>) -> bool {
    match (effect_obj, read_obj) {
        (_, None) | (None, _) | (Some(ObjF::AnyCard), _) => true,
        (Some(a), Some(b)) => a == b,
    }
}

impl Deck {
    fn new(ids: &[u16]) -> Self {
        let mut ids: Vec<u16> = ids.to_vec();
        ids.sort_unstable();
        ids.dedup();
        let rules = ids.iter().map(|&id| card_rules(id)).collect();
        Deck { ids, rules }
    }

    /// Prerequisites of using ability `k` of card index `i`.
    fn needs(&self, i: usize, k: usize) -> Option<Vec<GoalV1>> {
        let card = self.ids[i];
        let ability = &self.rules[i].abilities[k];
        let mut needs = Vec::new();
        for atom in &ability.atoms {
            match atom {
                Atom::Trigger(TrigF::SelfEnters) => needs.push(GoalV1::Enters { card }),
                Atom::Trigger(_) => return None, // Other events are not regressed in v1.
                Atom::CastFrom(zone) => needs.push(GoalV1::InZone { card, zone: *zone }),
                Atom::Read(read) if matches!(read.agg, AggF::Count) => {
                    needs.push(GoalV1::CountUp {
                        owner: read.player,
                        zone: read.zone.unwrap_or(ZoneF::Battlefield),
                        obj: read.obj.unwrap_or(ObjF::AnyCard),
                    })
                }
                _ => {}
            }
        }
        if matches!(ability.ctx, CtxF::Static | CtxF::Mana) {
            return None;
        }
        needs.sort();
        needs.dedup();
        Some(needs)
    }

    /// Steps that achieve `goal`.
    fn supports(&self, goal: GoalV1) -> Vec<StepV1> {
        let mut steps = Vec::new();
        for (i, rules) in self.rules.iter().enumerate() {
            for (k, ability) in rules.abilities.iter().enumerate() {
                for atom in &ability.atoms {
                    let Atom::Effect(e) = atom else { continue };
                    let achieved = match goal {
                        GoalV1::OpponentLosesLife => {
                            matches!(e.ev, EvF::Damage | EvF::LifeLoss)
                                && (reaches(e.player, RelF::Opponent, ability)
                                    || (e.player.is_none()
                                        && matches!(
                                            e.obj,
                                            Some(ObjF::Player | ObjF::PlayerOrPermanent)
                                        )))
                        }
                        GoalV1::CountUp { owner, zone, obj } => {
                            e.ev == EvF::Move
                                && e.to == Some(zone)
                                && e.from != Some(zone)
                                && covers_class(e.obj, Some(obj))
                                && reaches(e.player, owner, ability)
                        }
                        GoalV1::InZone { card, zone } => {
                            e.ev == EvF::Move
                                && e.to == Some(zone)
                                && covers(e.obj, &card_types(card))
                                && reaches(e.player, RelF::You, ability)
                        }
                        GoalV1::Enters { card } => {
                            e.ev == EvF::Move
                                && e.to == Some(ZoneF::Battlefield)
                                && e.from.is_some()
                                && e.from != Some(ZoneF::Hand)
                                && covers(e.obj, &card_types(card))
                                && self.ids[i] != card
                        }
                    };
                    if !achieved {
                        continue;
                    }
                    let Some(mut needs) = self.needs(i, k) else {
                        continue;
                    };
                    if let GoalV1::Enters { card } = goal {
                        if e.from == Some(ZoneF::Graveyard) {
                            needs.push(GoalV1::InZone {
                                card,
                                zone: ZoneF::Graveyard,
                            });
                        }
                    }
                    if needs.contains(&goal) {
                        continue;
                    }
                    steps.push(StepV1 {
                        goal,
                        card: self.ids[i],
                        ability: k,
                        amount: e.amount,
                        needs,
                    });
                }
            }
        }
        steps.sort();
        steps.dedup();
        steps
    }

    /// Goals met without any step: casting a deck card from hand.
    fn base(&self, goal: GoalV1) -> bool {
        match goal {
            GoalV1::Enters { card } => {
                let def = &CARD_DEFS[usize::from(card)];
                !def.types.iter().all(|t| {
                    matches!(
                        t,
                        crate::card_def::CardType::Instant | crate::card_def::CardType::Sorcery
                    )
                })
            }
            _ => false,
        }
    }
}

/// Every line to `OpponentLosesLife` within `max_depth` regression steps.
pub fn lines_for_deck(ids: &[u16], max_depth: usize) -> Vec<LineV1> {
    let deck = Deck::new(ids);
    let mut lines = BTreeSet::new();
    let mut stack = vec![(vec![GoalV1::OpponentLosesLife], Vec::<StepV1>::new())];
    while let Some((mut open, steps)) = stack.pop() {
        let Some(goal) = open.pop() else {
            let mut cards: Vec<u16> = steps.iter().map(|s| s.card).collect();
            cards.sort_unstable();
            cards.dedup();
            let scaling = steps
                .iter()
                .any(|s| s.goal == GoalV1::OpponentLosesLife && s.amount == AmtF::Dynamic);
            lines.insert(LineV1 {
                steps,
                cards,
                scaling,
            });
            continue;
        };
        if steps.iter().any(|s| s.goal == goal) {
            stack.push((open, steps));
            continue;
        }
        let mut alternatives = Vec::new();
        if deck.base(goal) {
            alternatives.push(None);
        }
        if steps.len() < max_depth {
            alternatives.extend(deck.supports(goal).into_iter().map(Some));
        }
        for alt in alternatives {
            let mut open = open.clone();
            let mut steps = steps.clone();
            if let Some(step) = alt {
                open.extend(step.needs.iter().copied());
                steps.push(step);
            }
            stack.push((open, steps));
        }
    }
    lines.into_iter().collect()
}
