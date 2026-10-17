//! FDN176 native Partial491 with exact black 2/2 Zombie Partial492.
//! Registration follows the actual committed Tinybones490 prerequisite.
use super::*;
use crate::{event::CommittedEvent as E, ids::StackItemId, state::StackItemKind};
use serde::{Deserialize, Serialize};
#[path = "liliana_dreadhorde_public_v73.rs"]
pub(crate) mod public_v73;
pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 491
        && card_def::CARD_DEFS.get(usize::from(id)).is_some_and(|d| {
            let card_def::CardDef {
                name: _,
                capability: _,
                cost: _,
                generic_cost_reduction: _,
                ward_cost: _,
                spell_cannot_be_countered: _,
                records_mana_spent: _,
                equipment: _,
                types: _,
                subtypes: _,
                supertypes: _,
                power: _,
                toughness: _,
                starting_loyalty: _,
                enters_with_plus_one_counters: _,
                controlled_counter_keyword: _,
                game_outcome_protection_v1: _,
                controlled_creature_grant: _,
                is_land: _,
                produces_mana: _,
                colors: _,
                target_spec: _,
                keywords: _,
                spell_effect: _,
                mana_ability: _,
                alt_cost: _,
                kicker_cost: _,
                additional_cost: _,
                flashback: _,
                activated_abilities: _,
                plot_cost: _,
                madness_cost: _,
                mode2: _,
                mode3: _,
                additional_modes: _,
                kicked_target_spec: _,
                is_token: _,
                escape: _,
                mana_ability_choices: _,
                enters_battlefield_tapped: _,
                mana_ability_def: _,
                minimum_blockers: _,
                omen: _,
                mana_value: _,
                mana_ability_includes_chosen_color: _,
                as_enters_choose_color_other_than: _,
                additional_mana_abilities: _,
                object_name: _,
                enters_battlefield_tapped_unless: _,
                attachment: _,
                transform_face: _,
                saga: _,
                optional_additional_cost: _,
                changeling: _,
                bestow: _,
                delve: _,
                adventure: _,
                cant_be_blocked_by_monarchs_creatures: _,
                conditional_tap_yield: _,
                enters_tapped_unless_controller: _,
                additional_mana_ability_conditions: _,
                restricted_mana_abilities: _,
                animation: _,
                activated_ability_generic_reductions: _,
                combat_static_rules: _,
                as_enters_color_choice_v1: _,
                chosen_color_creature_boost_v1: _,
                damage_replacement_v1: _,
                token_counter_replacement_v48: _,
                starting_life_self_bonuses_v1: _,
                source_type_hexproof_v1: _,
                other_controlled_nontoken_death_v1: _,
                creature_type_entry_v1: _,
            } = d;
            d.name == "Liliana, Dreadhorde General"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::B),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::B),
                        ],
                        generic: 4,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Planeswalker]
                && d.subtypes == [card_def::Subtype::Liliana]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power.is_none()
                && d.toughness.is_none()
                && d.starting_loyalty == Some(6)
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::B]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == Keywords::NONE
                && (d.spell_effect)()
                    == Some(EffectOp::MoveObject {
                        object: ObjectRef::ThisSource,
                        to_zone: Zone::Battlefield,
                    })
                && (d.mana_ability)().is_none()
                && d.alt_cost.is_none()
                && d.kicker_cost.is_none()
                && d.additional_cost.is_none()
                && d.flashback.is_none()
                && abilities_exact(d.activated_abilities)
                && d.plot_cost.is_none()
                && d.madness_cost.is_none()
                && d.mode2.is_none()
                && d.mode3.is_none()
                && d.additional_modes.is_empty()
                && d.kicked_target_spec.is_none()
                && !d.is_token
                && d.escape.is_none()
                && d.mana_ability_choices.is_empty()
                && !d.enters_battlefield_tapped
                && d.mana_ability_def.is_none()
                && d.minimum_blockers == 1
                && d.omen.is_none()
                && d.mana_value == 6
                && !d.mana_ability_includes_chosen_color
                && d.as_enters_choose_color_other_than.is_none()
                && d.additional_mana_abilities.is_empty()
                && d.object_name == d.name
                && d.enters_battlefield_tapped_unless.is_none()
                && d.attachment.is_none()
                && d.transform_face.is_none()
                && d.saga.is_none()
                && d.optional_additional_cost.is_none()
                && !d.changeling
                && d.bestow.is_none()
                && !d.delve
                && d.adventure.is_none()
                && !d.cant_be_blocked_by_monarchs_creatures
                && d.conditional_tap_yield.is_none()
                && d.enters_tapped_unless_controller.is_none()
                && d.additional_mana_ability_conditions.is_empty()
                && d.restricted_mana_abilities.is_empty()
                && d.animation.is_none()
                && d.activated_ability_generic_reductions.is_empty()
                && d.combat_static_rules == card_def::CombatStaticRulesDef::NONE
                && d.as_enters_color_choice_v1.is_none()
                && d.chosen_color_creature_boost_v1.is_none()
                && d.damage_replacement_v1.is_none()
                && d.token_counter_replacement_v48.is_none()
                && d.starting_life_self_bonuses_v1.is_empty()
                && d.source_type_hexproof_v1.is_none()
                && d.other_controlled_nontoken_death_v1.is_none()
                && d.creature_type_entry_v1.is_none()
        })
}

fn abilities_exact(abilities: &[card_def::ActivatedAbilityDef]) -> bool {
    abilities.len() == 3
        && abilities.iter().enumerate().all(|(i, a)| {
            a.cost == [card_def::CostComponent::Loyalty([1, -4, -9][i])]
                && a.target_spec == card_def::TargetSpec::None
                && (a.effect)() == program(i)
                && a.activation_zone == Zone::Battlefield
                && a.sorcery_speed_only
                && a.activation_target_filter == card_def::ActivationTargetFilter::TargetSpecOnly
                && a.max_activations_per_turn.is_none()
                && a.face.is_none()
        })
}
pub(crate) fn zombie() -> EffectOp {
    EffectOp::LilianaZombieV1
}
pub(crate) fn sacrifice_two() -> EffectOp {
    EffectOp::LilianaSacrificeV1 { ultimate: false }
}
pub(crate) fn ultimate() -> EffectOp {
    EffectOp::LilianaSacrificeV1 { ultimate: true }
}
fn program(i: usize) -> EffectOp {
    match i {
        0 => zombie(),
        1 => sacrifice_two(),
        2 => ultimate(),
        _ => EffectOp::Sequence(vec![]),
    }
}
pub(crate) fn death_draw() -> EffectOp {
    EffectOp::DrawCards {
        player: PlayerRef::Controller,
        count: 1,
    }
}
pub(crate) const DEATH_TRIGGERS: [crate::trigger::TriggeredAbilityDef; 1] =
    [crate::trigger::TriggeredAbilityDef {
        condition: crate::trigger::TriggerCondition::ThisOrControlledCreatureDies,
        effect: death_draw,
        home_zone: Zone::Battlefield,
        face_index: 0,
        intervening_if_kicked: false,
        intervening_if_controls_another_source_card: false,
    }];
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PaymentV1 {
    index: u64,
    turn: u32,
    source: AbilitySourceContractV4,
    pending: Box<crate::engine::PendingActivation>,
    before: u32,
    after: u32,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum JournalV1 {
    Payment {
        input: Box<PaymentV1>,
    },
    Published {
        input: Box<PaymentV1>,
        item: Box<StackItem>,
    },
    Started {
        item: Box<StackItem>,
        choice: Box<ChoiceV1>,
    },
    Issued {
        start: u64,
        choice: Box<ChoiceV1>,
    },
    CommitInput {
        start: u64,
        choice: Box<ChoiceV1>,
        sacrificed: Vec<EffectObjectBinding>,
    },
    Finished {
        input: u64,
        history_end: u64,
    },
    TokenInput {
        publication: u64,
        item: Box<StackItem>,
    },
    TokenFinished {
        input: u64,
        history_end: u64,
    },
}
fn rows(state: &GameState) -> impl DoubleEndedIterator<Item = (usize, &JournalV1)> {
    state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            E::LilianaDreadhordeV1 { journal } => Some((at, journal.as_ref())),
            _ => None,
        })
}
fn append(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::LilianaDreadhordeV1 {
        journal: Box::new(journal),
    });
}
fn at(state: &GameState, index: u64) -> Result<&JournalV1, String> {
    match usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    {
        Some(E::LilianaDreadhordeV1 { journal }) => Ok(journal),
        _ => Err("Liliana original journal absent".into()),
    }
}
pub(crate) fn record_input(
    state: &mut GameState,
    p: &crate::engine::PendingActivation,
) -> Result<Option<u64>, String> {
    let source = AbilitySourceContractV4::capture(state, p.source);
    if !definition(source.card_def)
        || p.cauldron_grant.0.is_some()
        || usize::from(p.ability_index) >= 3
    {
        return Ok(None);
    }
    let before =
        crate::planeswalker_v1::loyalty(state, p.source).ok_or("Liliana loyalty absent")?;
    let after = crate::planeswalker_v1::quote_loyalty_change_v48(
        state,
        p.source,
        [1, -4, -9][usize::from(p.ability_index)],
        crate::doubling_season_v48::CounterOriginV48::Cost,
    )?;
    let index = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV1::Payment {
            input: Box::new(PaymentV1 {
                index,
                turn: state.turn_key_v1(),
                source,
                pending: Box::new(p.clone()),
                before,
                after,
            }),
        },
    );
    Ok(Some(index))
}
pub(crate) fn record_paid(
    state: &mut GameState,
    id: StackItemId,
    input: Option<u64>,
) -> Result<(), String> {
    let Some(input) = input else { return Ok(()) };
    let JournalV1::Payment { input } = at(state, input)? else {
        return Err("Liliana paid input kind changed".into());
    };
    let input = input.clone();
    let item = state
        .stack
        .iter()
        .find(|r| r.v4.stack_item_id == id)
        .ok_or("Liliana paid row absent")?
        .clone();
    if item.source != input.source.source
        || item.controller != input.source.controller
        || item.v4.ability_source_contract != Some(input.source)
        || crate::planeswalker_v1::loyalty(state, item.source) != Some(input.after)
    {
        return Err("Liliana paid row did not follow actual loyalty cost".into());
    }
    append(
        state,
        JournalV1::Published {
            input,
            item: Box::new(item),
        },
    );
    Ok(())
}
fn validate_payment(state: &GameState, p: &PaymentV1) -> Result<(), String> {
    if !definition(p.source.card_def)
        || p.source.zone != Zone::Battlefield
        || p.pending.source != p.source.source
        || p.pending.controller != p.source.controller
        || p.pending.source_zone_change_count != p.source.zone_change_count
        || p.pending.cauldron_grant.0.is_some()
        || p.pending.ability_index >= 3
        || p.pending.target_spec != card_def::TargetSpec::None
        || p.pending.targets_chosen.len()
            != usize::from(crate::engine::target_count(p.pending.target_spec))
        || p.pending.target_contracts.len() != p.pending.targets_chosen.len()
        || p.pending.loyalty_x.is_some()
        || p.pending.mana_x_v1.is_some()
        || !p.pending.object_cost_chosen.is_empty()
        || p.pending.crew_finished
        || p.pending
            .cost_discard_paid
            .as_ref()
            .is_none_or(|v| !v.is_empty())
    {
        return Err("Liliana original paid source/choice changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, p.source)?;
    let delta = [1, -4, -9][p.pending.ability_index as usize];
    if delta < 0 && p.before.checked_sub((-delta) as u32) != Some(p.after)
        || delta > 0 && p.after <= p.before
    {
        return Err("Liliana original loyalty subtraction changed".into());
    }
    Ok(())
}
fn validate_published(state: &GameState, p: &PaymentV1, item: &StackItem) -> Result<(), String> {
    validate_payment(state, p)?;
    if !matches!(at(state,p.index)?,JournalV1::Payment{input} if input.as_ref()==p)
        || item.kind != StackItemKind::ActivatedAbility
        || item.source != p.source.source
        || item.controller != p.source.controller
        || item.v4.ability_source_contract != Some(p.source)
        || item.inline_effect != Some(program(p.pending.ability_index as usize))
        || item.targets != p.pending.targets_chosen
        || item.v4.target_contracts != p.pending.target_contracts
        || item.v4.target_spec != Some(p.pending.target_spec)
        || item.v4.activated_ability_index != Some(p.pending.ability_index)
        || item.v4.source_contract.is_some()
        || item.v4.granted_by.is_some()
        || item.is_copy
        || item.is_flashback
        || item.kicked
        || item.madness_offer
        || item.mode_chosen != 0
        || !item.discarded.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
    {
        return Err("Liliana publication lost exact original payment/program/targets".into());
    }
    Ok(())
}

pub(crate) fn publication(state: &GameState, item: &StackItem) -> Result<u64, String> {
    let mut found = rows(state).filter_map(|(at, j)| match j {
        JournalV1::Published { item: old, .. } if old.v4.stack_item_id == item.v4.stack_item_id => {
            Some((at, old))
        }
        _ => None,
    });
    let (at, old) = found
        .next()
        .ok_or("Liliana original paid publication absent")?;
    if found.next().is_some() || old.as_ref() != item {
        return Err("Liliana original full paid row changed or duplicated".into());
    }
    Ok(at as u64)
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    item.kind==StackItemKind::ActivatedAbility && (item.inline_effect.as_ref().is_some_and(|op|matches!(op,
        EffectOp::LilianaZombieV1 | EffectOp::LilianaSacrificeV1 { .. }))
        || item.v4.ability_source_contract.is_some_and(|s|definition(s.card_def))
        || rows(state).any(|(_,j)|matches!(j,JournalV1::Published{item:old,..} if old.v4.stack_item_id==item.v4.stack_item_id)))
}
pub(crate) fn sacrifice_selected(state: &GameState, item: &StackItem) -> bool {
    selected(state, item)
        && item
            .v4
            .activated_ability_index
            .is_some_and(|i| i == 1 || i == 2)
        || matches!(
            item.inline_effect,
            Some(EffectOp::LilianaSacrificeV1 { .. })
        )
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|item| {
        selected(state, item)
            && (Some(item.v4.stack_item_id) == ctx.stack_item_id || item.source == ctx.source)
    })
}
pub(crate) fn token_definition(id: u16) -> bool {
    id == 492
        && card_def::CARD_DEFS.get(id as usize).is_some_and(|d| {
            d.name == "Zombie Token"
                && d.is_executable()
                && d.is_token
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Zombie]
                && d.colors == [crate::mana::ManaColor::B]
                && d.power == Some(2)
                && d.toughness == Some(2)
                && d.keywords == Keywords::NONE
                && d.activated_abilities.is_empty()
                && (d.spell_effect)().is_none()
                && d.starting_loyalty.is_none()
        })
}
pub(crate) struct TokenPermitV1<'a, 'b> {
    proof: &'a NativeExecutionProofV1<'b>,
    token_def: u16,
}
impl TokenPermitV1<'_, '_> {
    pub(crate) fn events(&self) -> Vec<event::ProposedEvent> {
        vec![event::ProposedEvent::create_token(
            self.token_def,
            self.proof.ctx.controller,
        )]
    }
    pub(crate) fn allows(&self, state: &GameState, events: &[event::ProposedEvent]) -> bool {
        token_definition(self.token_def)
            && *self.proof.owner.root == zombie()
            && self.proof.owner.ctx == *self.proof.ctx
            && events == self.events()
            && publication(state, &self.proof.owner.item).is_ok()
            && self
                .proof
                .owner
                .validate_entry(state, &self.proof.owner.item, self.proof.ctx, &zombie())
                .is_ok()
    }
}
pub(super) fn create_zombie(
    state: &mut GameState,
    proof: &NativeExecutionProofV1<'_>,
) -> Result<(), String> {
    let publication = publication(state, &proof.owner.item)?;
    let token_def = card_def::card_id_by_name("Zombie Token")
        .filter(|id| token_definition(*id))
        .ok_or("Liliana exact Zombie token absent")?;
    if rows(state)
        .any(|(_, j)| matches!(j,JournalV1::TokenInput{publication:p,..}if *p==publication))
    {
        return Err("Liliana Zombie occurrence already executed".into());
    }
    let input = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV1::TokenInput {
            publication,
            item: Box::new(proof.owner.item.clone()),
        },
    );
    event::commit_liliana_zombie_owned_v1(state, TokenPermitV1 { proof, token_def })?;
    let history_end = state.engine.event_history.len() as u64;
    append(state, JournalV1::TokenFinished { input, history_end });
    Ok(())
}

const TYPES: [CardType; 5] = [
    CardType::Land,
    CardType::Creature,
    CardType::Artifact,
    CardType::Enchantment,
    CardType::Planeswalker,
];
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChoiceV1 {
    start: u64,
    issued: Option<u64>,
    ultimate: bool,
    active: PlayerId,
    controller: PlayerId,
    phase: u8,
    rosters: [Vec<(EffectObjectBinding, Vec<CardType>)>; 2],
    sacrificed: [Vec<EffectObjectBinding>; 2],
    kept: Vec<(CardType, EffectObjectBinding)>,
    graveyard: [Vec<EffectObjectBinding>; 2],
    orders: [Vec<EffectObjectBinding>; 2],
}
#[derive(Clone, Copy)]
enum Stage {
    Sacrifice(PlayerId),
    Keep(PlayerId, CardType),
    Order(PlayerId),
}
fn stages(c: &ChoiceV1) -> Vec<Stage> {
    let mut v = if c.ultimate {
        TYPES
            .into_iter()
            .map(|t| Stage::Keep(c.controller.opponent(), t))
            .collect()
    } else {
        vec![
            Stage::Sacrifice(c.active),
            Stage::Sacrifice(c.active.opponent()),
        ]
    };
    v.extend([Stage::Order(c.active), Stage::Order(c.active.opponent())]);
    v
}
fn scene(state: &GameState) -> Result<[Vec<(EffectObjectBinding, Vec<CardType>)>; 2], String> {
    let mut result = [vec![], vec![]];
    for player in [PlayerId::P0, PlayerId::P1] {
        let mut seen = std::collections::BTreeSet::new();
        for &id in &state.players[player.index()].battlefield {
            let o = state
                .objects
                .try_get(id)
                .ok_or("Liliana battlefield object absent")?;
            if o.zone != Zone::Battlefield || o.controller != player || !seen.insert(id) {
                return Err("Liliana battlefield roster changed".into());
            }
            let b = EffectObjectBinding {
                object: id,
                expected_zone: o.zone,
                expected_zone_change_count: o.zone_change_count,
            };
            let types = TYPES
                .into_iter()
                .filter(|t| crate::engine::object_has_type(state, id, *t))
                .collect();
            result[player.index()].push((b, types));
        }
    }
    Ok(result)
}
fn stage_data(c: &ChoiceV1) -> Result<(PlayerId, Vec<EffectObjectBinding>, usize, bool), String> {
    Ok(
        match *stages(c)
            .get(c.phase as usize)
            .ok_or("Liliana selection stage absent")?
        {
            Stage::Sacrifice(p) => {
                let v = c.rosters[p.index()]
                    .iter()
                    .filter(|(_, ts)| ts.contains(&CardType::Creature))
                    .map(|(b, _)| *b)
                    .collect::<Vec<_>>();
                let n = v.len().min(2);
                (p, v, n, false)
            }
            Stage::Keep(p, t) => {
                let v = c.rosters[p.index()]
                    .iter()
                    .filter(|(_, ts)| ts.contains(&t))
                    .map(|(b, _)| *b)
                    .collect::<Vec<_>>();
                let n = usize::from(!v.is_empty());
                (p, v, n, false)
            }
            Stage::Order(p) => {
                let v = c.graveyard[p.index()].clone();
                let n = v.len();
                (p, v, n, true)
            }
        },
    )
}
fn check_selection(
    original: &[EffectObjectBinding],
    chosen: &[EffectObjectBinding],
    count: usize,
) -> Result<(), String> {
    if chosen.len() != count
        || chosen.iter().any(|b| !original.contains(b))
        || chosen
            .iter()
            .map(|b| b.object)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != chosen.len()
    {
        Err("Liliana selection changed exact cardinality, object or incarnation".into())
    } else {
        Ok(())
    }
}
fn all_sacrifices(c: &ChoiceV1) -> Vec<EffectObjectBinding> {
    if c.ultimate {
        c.rosters[c.controller.opponent().index()]
            .iter()
            .map(|(b, _)| *b)
            .filter(|b| !c.kept.iter().any(|(_, k)| k == b))
            .collect()
    } else {
        c.sacrificed[c.active.index()]
            .iter()
            .chain(&c.sacrificed[c.active.opponent().index()])
            .copied()
            .collect()
    }
}
fn record_answer(c: &mut ChoiceV1, selected: Vec<EffectObjectBinding>) -> Result<(), String> {
    let (_, original, count, _) = stage_data(c)?;
    check_selection(&original, &selected, count)?;
    match stages(c)[c.phase as usize] {
        Stage::Sacrifice(p) => c.sacrificed[p.index()] = selected,
        Stage::Keep(_, t) => {
            if let Some(b) = selected.first() {
                c.kept.push((t, *b));
            }
        }
        Stage::Order(p) => c.orders[p.index()] = selected,
    }
    c.phase += 1;
    c.issued = None;
    Ok(())
}
fn initial<'a>(
    state: &'a GameState,
    c: &ChoiceV1,
) -> Result<(&'a StackItem, &'a ChoiceV1), String> {
    match at(state, c.start)? {
        JournalV1::Started { item, choice } => Ok((item, choice)),
        _ => Err("Liliana original resolution start absent".into()),
    }
}
fn validate_choice(state: &GameState, c: &ChoiceV1, live: bool) -> Result<(), String> {
    let (item, original) = initial(state, c)?;
    publication(state, item)?;
    if c.start != original.start
        || c.active != original.active
        || c.controller != original.controller
        || c.ultimate != original.ultimate
        || c.rosters != original.rosters
        || c.phase as usize > stages(c).len()
        || live && (c.active != state.active_player || scene(state)? != c.rosters)
    {
        return Err("Liliana original resolution scene, mode or APNAP order changed".into());
    }
    // Reconstruct every completed exact choice against the immutable start.
    let mut replay = original.clone();
    for stage in stages(c).into_iter().take(c.phase as usize) {
        if matches!(stage, Stage::Order(_)) && replay.graveyard.iter().all(Vec::is_empty) {
            replay.graveyard = if live {
                blasphemous_edict_v1::actual_graveyard_destinations(
                    state,
                    &all_sacrifices(&replay),
                )?
            } else {
                c.graveyard.clone()
            };
        }
        let selected = match stage {
            Stage::Sacrifice(p) => c.sacrificed[p.index()].clone(),
            Stage::Keep(p, t) => {
                let required = replay.rosters[p.index()]
                    .iter()
                    .any(|(_, ts)| ts.contains(&t));
                let matches = c
                    .kept
                    .iter()
                    .filter(|(kind, _)| *kind == t)
                    .map(|(_, b)| *b)
                    .collect::<Vec<_>>();
                if matches.len() != usize::from(required) {
                    return Err("Liliana keep type omitted or duplicated".into());
                }
                matches
            }
            Stage::Order(p) => c.orders[p.index()].clone(),
        };
        record_answer(&mut replay, selected)?;
    }
    if c.phase as usize >= if c.ultimate { 5 } else { 2 } {
        replay.graveyard = if live {
            blasphemous_edict_v1::actual_graveyard_destinations(state, &all_sacrifices(c))?
        } else {
            c.graveyard.clone()
        };
    }
    replay.issued = c.issued;
    if replay != *c {
        return Err("Liliana future or completed choice state changed".into());
    }
    if let Some(index) = c.issued {
        if !matches!(at(state,index)?,JournalV1::Issued{start,choice}if *start==c.start && choice.as_ref()==c)
        {
            return Err("Liliana exact issued choice changed".into());
        }
    }
    Ok(())
}
fn candidate(b: EffectObjectBinding) -> EffectTargetCandidate {
    EffectTargetCandidate {
        target: Target::Object(b.object),
        expected_object: Some(b),
    }
}
pub(super) fn validate_continuation(
    state: &GameState,
    p: &EffectContinuation,
) -> Result<(), String> {
    if !sacrifice_selected(state, &p.resolving_item)
        && !matches!(
            p.choice,
            Some(PendingEffectChoice::SelectTargets {
                purpose: EffectTargetSelectionPurpose::LilianaV1 { .. },
                ..
            })
        )
        && !p
            .frames
            .iter()
            .any(|f| matches!(f, EffectFrame::LilianaV1 { .. }))
    {
        return Ok(());
    }
    publication(state, &p.resolving_item)?;
    let mut ctx = super::crystal_prevention_v1::context(&p.resolving_item);
    ctx.niv_damage_origin_v1 = event::niv_damage_origin_for_saved_item_v1(state, &p.resolving_item);
    if p.ctx != ctx {
        return Err("Liliana original complete context changed".into());
    }
    match (&p.choice, p.frames.as_slice(), &p.answered_choice_guard) {
        (
            Some(PendingEffectChoice::SelectTargets {
                player,
                path,
                selected,
                legal,
                min_targets,
                max_targets,
                ordered,
                purpose: EffectTargetSelectionPurpose::LilianaV1 { choice },
            }),
            [],
            None,
        ) => {
            validate_choice(state, choice, true)?;
            let (chooser, original, count, order) = stage_data(choice)?;
            if *player != chooser
                || !path.is_empty()
                || usize::from(*min_targets) != count
                || usize::from(*max_targets) != count
                || *ordered != order
                || selected.len() > count
                || choice.issued.is_none()
            {
                return Err("Liliana prompt chooser, bounds or order changed".into());
            }
            let mut remaining = original.into_iter().map(candidate).collect::<Vec<_>>();
            for c in selected {
                let i = remaining
                    .iter()
                    .position(|b| b == c)
                    .ok_or("Liliana selected prefix changed")?;
                remaining.remove(i);
            }
            if remaining != *legal {
                return Err("Liliana remaining public candidates changed".into());
            }
            Ok(())
        }
        (
            None,
            [frame @ EffectFrame::LilianaV1 { choice, selected }],
            Some(EffectAnsweredChoiceGuard::StandardSelection { frame: guard }),
        ) if **guard == *frame => {
            validate_choice(state, choice, true)?;
            let (_, original, count, _) = stage_data(choice)?;
            check_selection(&original, selected, count)
        }
        _ => Err("Liliana lost original sacrifice choice stage".into()),
    }
}
pub(super) fn answer(
    p: &mut EffectContinuation,
    choice: ChoiceV1,
    selected: Vec<EffectObjectBinding>,
    path: Vec<u16>,
) -> Result<(), String> {
    if !path.is_empty() || !p.frames.is_empty() {
        return Err("Liliana answer changed root path".into());
    }
    let (_, original, count, _) = stage_data(&choice)?;
    check_selection(&original, &selected, count)?;
    let frame = EffectFrame::LilianaV1 { choice, selected };
    p.answered_choice_guard = Some(EffectAnsweredChoiceGuard::StandardSelection {
        frame: Box::new(frame.clone()),
    });
    p.frames.push(frame);
    Ok(())
}
fn stage(
    state: &mut GameState,
    p: &mut EffectContinuation,
    mut c: ChoiceV1,
) -> Result<bool, String> {
    let selection_end = if c.ultimate { 5 } else { 2 };
    while (c.phase as usize) < stages(&c).len() {
        if c.phase == selection_end {
            c.graveyard =
                blasphemous_edict_v1::actual_graveyard_destinations(state, &all_sacrifices(&c))?;
        }
        let (player, original, count, ordered) = stage_data(&c)?;
        if original.len() <= if ordered { 1 } else { count } {
            record_answer(&mut c, original)?;
            continue;
        }
        let index = state.engine.event_history.len() as u64;
        c.issued = Some(index);
        append(
            state,
            JournalV1::Issued {
                start: c.start,
                choice: Box::new(c.clone()),
            },
        );
        let count = u16::try_from(count).map_err(|_| "Liliana native menu cardinality overflow")?;
        p.choice = Some(PendingEffectChoice::SelectTargets {
            player,
            path: vec![],
            selected: vec![],
            legal: original.into_iter().map(candidate).collect(),
            min_targets: count,
            max_targets: count,
            ordered,
            purpose: EffectTargetSelectionPurpose::LilianaV1 { choice: c },
        });
        return Ok(true);
    }
    validate_choice(state, &c, true)?;
    let sacrificed = all_sacrifices(&c);
    let ordered = c.orders[c.active.index()]
        .iter()
        .chain(&c.orders[c.active.opponent().index()])
        .copied()
        .collect::<Vec<_>>();
    let destinations = sacrificed
        .iter()
        .filter(|b| !ordered.contains(b))
        .copied()
        .chain(ordered.iter().copied())
        .collect::<Vec<_>>();
    let input = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV1::CommitInput {
            start: c.start,
            choice: Box::new(c),
            sacrificed: sacrificed.clone(),
        },
    );
    for b in &sacrificed {
        event::log_sacrifice(state, b.object);
    }
    event::propose_and_commit_batch(
        state,
        destinations
            .iter()
            .map(|b| event::ProposedEvent::zone_change(b.object, Zone::Graveyard))
            .collect(),
    );
    if state.engine.halted.is_some() {
        return Err("Liliana simultaneous native sacrifice batch halted".into());
    }
    let history_end = state.engine.event_history.len() as u64;
    append(state, JournalV1::Finished { input, history_end });
    Ok(false)
}
pub(super) fn start(
    state: &mut GameState,
    p: &mut EffectContinuation,
    path: Vec<u16>,
    ultimate: bool,
) -> Result<bool, String> {
    if !path.is_empty()
        || !p.frames.is_empty()
        || p.resolving_item.inline_effect != Some(EffectOp::LilianaSacrificeV1 { ultimate })
    {
        return Err("Liliana sacrifice lost its original closed root".into());
    }
    publication(state, &p.resolving_item)?;
    if rows(state).any(|(_,j)|matches!(j,JournalV1::Started{item,..}if item.v4.stack_item_id==p.resolving_item.v4.stack_item_id)) {
        return Err("Liliana actual resolution occurrence already started".into());
    }
    let start = state.engine.event_history.len() as u64;
    let c = ChoiceV1 {
        start,
        issued: None,
        ultimate,
        active: state.active_player,
        controller: p.ctx.controller,
        phase: 0,
        rosters: scene(state)?,
        sacrificed: [vec![], vec![]],
        kept: vec![],
        graveyard: [vec![], vec![]],
        orders: [vec![], vec![]],
    };
    append(
        state,
        JournalV1::Started {
            item: Box::new(p.resolving_item.clone()),
            choice: Box::new(c.clone()),
        },
    );
    stage(state, p, c)
}
pub(super) fn advance(
    state: &mut GameState,
    p: &mut EffectContinuation,
    mut choice: ChoiceV1,
    selected: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    validate_choice(state, &choice, true)?;
    record_answer(&mut choice, selected)?;
    stage(state, p, choice)
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, E::LilianaDreadhordeV1 { .. }))
    {
        return Err("Liliana owning journal entered trigger dispatch".into());
    }
    if !state
        .engine
        .event_history
        .has_any(&[crate::event_history_v1::EventKindV1::LilianaDreadhordeV1])
        && !state.stack.iter().any(|item| selected(state, item))
    {
        return Ok(());
    }
    let mut paid = std::collections::BTreeSet::new();
    let mut published = std::collections::BTreeSet::new();
    let mut started = std::collections::BTreeSet::new();
    let mut completed = std::collections::BTreeSet::new();
    let mut inputs = std::collections::BTreeSet::new();
    let mut finished_inputs = std::collections::BTreeSet::new();
    let mut token_inputs = std::collections::BTreeSet::new();
    let mut token_finished = std::collections::BTreeSet::new();
    let mut token_publications = std::collections::BTreeSet::new();
    for (position, j) in rows(state) {
        match j {
            JournalV1::Payment { input } => {
                if input.index != position as u64 {
                    return Err("Liliana original payment input moved".into());
                }
                validate_payment(state, input)?;
            }
            JournalV1::Published { input, item } => {
                if input.index >= position as u64
                    || !paid.insert(input.index)
                    || !published.insert(item.v4.stack_item_id)
                {
                    return Err("Liliana loyalty payment/publication reused".into());
                }
                validate_published(state, input, item)?;
                if crate::engine::time_stop_v1::disposed(state, item)?
                    && rows(state).any(|(_, j)| {
                        matches!(j,
                        JournalV1::Started{item:old,..}|JournalV1::TokenInput{item:old,..}
                        if old.v4.stack_item_id==item.v4.stack_item_id)
                    })
                {
                    return Err("Liliana disposed occurrence also claims native resolution".into());
                }
                if state.stack.iter().any(|row| {
                    row.v4.stack_item_id == item.v4.stack_item_id && row != item.as_ref()
                }) {
                    return Err("Liliana current full row changed from original publication".into());
                }
            }
            JournalV1::Started { item, choice } => {
                publication(state, item)?;
                if choice.start != position as u64
                    || choice.phase != 0
                    || choice.issued.is_some()
                    || !sacrifice_selected(state, item)
                    || choice.controller != item.controller
                    || item.inline_effect
                        != Some(EffectOp::LilianaSacrificeV1 {
                            ultimate: choice.ultimate,
                        })
                    || choice.sacrificed.iter().any(|v| !v.is_empty())
                    || !choice.kept.is_empty()
                    || choice.graveyard.iter().any(|v| !v.is_empty())
                    || choice.orders.iter().any(|v| !v.is_empty())
                    || !started.insert(item.v4.stack_item_id)
                {
                    return Err("Liliana original native start changed or reused".into());
                }
            }
            JournalV1::Issued { start, choice } => {
                if *start >= position as u64
                    || *start != choice.start
                    || choice.issued != Some(position as u64)
                    || choice.phase as usize >= stages(choice).len()
                {
                    return Err("Liliana issued choice moved or changed stage".into());
                }
                validate_choice(state, choice, false)?;
            }
            JournalV1::CommitInput {
                start,
                choice,
                sacrificed,
            } => {
                if *start >= position as u64
                    || choice.start != *start
                    || choice.phase as usize != stages(choice).len()
                    || choice.issued.is_some()
                    || all_sacrifices(choice) != *sacrificed
                {
                    return Err("Liliana actual final sacrifice input changed".into());
                }
                validate_choice(state, choice, false)?;
                inputs.insert(position as u64);
            }
            JournalV1::Finished { input, history_end } => {
                let JournalV1::CommitInput {
                    start, sacrificed, ..
                } = at(state, *input)?
                else {
                    return Err("Liliana completion lost original commit input".into());
                };
                let (item, _) = match at(state, *start)? {
                    JournalV1::Started { item, choice } => (item, choice),
                    _ => return Err("Liliana completion lost native start".into()),
                };
                if *input >= position as u64
                    || *history_end != position as u64
                    || !finished_inputs.insert(*input)
                    || !completed.insert(item.v4.stack_item_id)
                {
                    return Err("Liliana sacrifice completion moved or reused".into());
                }
                let events = &state.engine.event_history[*input as usize + 1..position];
                let logged = events
                    .iter()
                    .filter_map(|e| match e {
                        E::Sacrificed { object, .. } => Some(*object),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if logged!=sacrificed.iter().map(|b|b.object).collect::<Vec<_>>() || sacrificed.iter().any(|b|
                    events.iter().filter(|e|matches!(e,E::ZoneChange{object,from:Zone::Battlefield,..}if *object==b.object)).count()!=1) {
                    return Err("Liliana completion lost actual ordered sacrifice and departure batch".into());
                }
                if state
                    .stack
                    .iter()
                    .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                    || state
                        .engine
                        .pending_effect
                        .as_ref()
                        .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
                {
                    return Err("Liliana finished sacrifice occurrence was reintroduced".into());
                }
            }
            JournalV1::TokenInput {
                publication: pub_at,
                item,
            } => {
                if *pub_at >= position as u64
                    || publication(state, item)? != *pub_at
                    || item.inline_effect != Some(zombie())
                    || !token_publications.insert(*pub_at)
                {
                    return Err("Liliana token input changed original paid row".into());
                }
                token_inputs.insert(position as u64);
            }
            JournalV1::TokenFinished { input, history_end } => {
                let JournalV1::TokenInput { item, .. } = at(state, *input)? else {
                    return Err("Liliana token completion lost native input".into());
                };
                if *input >= position as u64
                    || *history_end != position as u64
                    || !token_finished.insert(*input)
                {
                    return Err("Liliana token completion moved or reused".into());
                }
                let created = state.engine.event_history[*input as usize + 1..position]
                    .iter()
                    .filter_map(|e| match e {
                        E::CreateToken {
                            token_def,
                            controller,
                            ..
                        } => Some((*token_def, *controller)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if created.is_empty()
                    || created
                        .iter()
                        .any(|(id, p)| !token_definition(*id) || *p != item.controller)
                    || state
                        .stack
                        .iter()
                        .any(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                {
                    return Err(
                        "Liliana token completion changed original native token batch".into(),
                    );
                }
            }
        }
    }
    if inputs != finished_inputs || token_inputs != token_finished {
        return Err("Liliana native transaction lost terminal receipt".into());
    }
    for (_, j) in rows(state) {
        if let JournalV1::Started { item, .. } = j {
            if !completed.contains(&item.v4.stack_item_id) {
                let p = state
                    .engine
                    .pending_effect
                    .as_ref()
                    .ok_or("Liliana unfinished original continuation erased")?;
                if p.resolving_item != **item {
                    return Err("Liliana unfinished native row changed".into());
                }
                validate_continuation(state, p)?;
            }
        }
    }
    for item in &state.stack {
        if selected(state, item) {
            publication(state, item)?;
        }
    }
    Ok(())
}
pub(crate) fn require_legacy(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    let visible=state.objects.iter().any(|(id,o)|
        crate::optional_trigger_public_v1::reference(state,id,viewer).is_some_and(|r|matches!(r.identity,
            crate::optional_trigger_public_v1::IdentityV1::Known{card_id}if definition(card_id)||token_definition(card_id)))
        || viewer.is_some_and(|p|o.v4.face_down_v1.is_some_and(|f|f.lookers&(1<<p.index())!=0)&&definition(o.card_def)));
    if visible || rows(state).next().is_some() {
        Err("Liliana native source requires a complete V73 consumer".into())
    } else {
        Ok(())
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "liliana_dreadhorde_v1_tests.rs"]
mod tests;
