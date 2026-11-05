//! Native Vivien loyalty and persistent timestamped emblem. Original committed
//! payment/publication inputs are provenance, not a whole-history attestation.
use super::*;
use crate::{card_def, event::CommittedEvent as E, ids::StackItemId, state::StackItemKind};
use serde::{Deserialize, Serialize};

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 488
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
            d.name == "Vivien Reid"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                        ],
                        generic: 3,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Planeswalker]
                && d.subtypes == [card_def::Subtype::Vivien]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power.is_none()
                && d.toughness.is_none()
                && d.starting_loyalty == Some(5)
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::G]
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
                && d.mana_value == 5
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
            a.cost == [card_def::CostComponent::Loyalty([1, -3, -8][i])]
                && a.target_spec == target(i)
                && (a.effect)() == program(i)
                && a.activation_zone == Zone::Battlefield
                && a.sorcery_speed_only
                && a.activation_target_filter == card_def::ActivationTargetFilter::TargetSpecOnly
                && a.max_activations_per_turn.is_none()
                && a.face.is_none()
        })
}
pub(crate) fn look() -> EffectOp {
    EffectOp::LookTopSelectV1 {
        player: PlayerRef::Controller,
        count: LibraryLookCount::Fixed(4),
        pick_x: false,
        rule: LibraryPickRule {
            pick: 1,
            choose_rest_order: false,
            selection: Some(LibraryPickSelectionV1 {
                without_mana_cost: crate::engine::FreeCastV1(false),
                filter: LibraryPickFilterV1::CreatureOrLandV1,
                optional: true,
                destination: Zone::Hand,
                reveal_selected: true,
            }),
        },
    }
}
pub(crate) fn destroy() -> EffectOp {
    EffectOp::DestroyObject {
        object: ObjectRef::Target(0),
    }
}
pub(crate) fn emblem() -> EffectOp {
    EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::VivienReidEmblemV1)
}
fn program(i: usize) -> EffectOp {
    match i {
        0 => look(),
        1 => destroy(),
        2 => emblem(),
        _ => EffectOp::Sequence(vec![]),
    }
}
fn target(i: usize) -> card_def::TargetSpec {
    if i == 1 {
        card_def::TargetSpec::StandardV1(
            crate::standard_cards_v1::StandardTargetV1::ArtifactEnchantmentOrFlyingCreatureV1,
        )
    } else {
        card_def::TargetSpec::None
    }
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::VivienReidEmblemV1)
    ) || matches!(op,EffectOp::LookTopSelectV1 { rule, .. } if rule.selection.is_some_and(|s|s.filter==LibraryPickFilterV1::CreatureOrLandV1))
}
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
    EmblemInput {
        publication: u64,
        item: Box<StackItem>,
        controller: PlayerId,
        timestamp: u64,
    },
    EmblemCreated {
        input: u64,
        publication: u64,
        controller: PlayerId,
        timestamp: u64,
    },
}
fn rows(state: &GameState) -> impl DoubleEndedIterator<Item = (usize, &JournalV1)> {
    state
        .engine
        .event_history
        .scan_for_v1(&[crate::event_history_v1::EventKindV1::VivienReidV1])
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            E::VivienReidV1 { journal } => Some((at, journal.as_ref())),
            _ => None,
        })
}
fn append(state: &mut GameState, journal: JournalV1) {
    state.engine.event_history.push(E::VivienReidV1 {
        journal: Box::new(journal),
    });
}
fn at(state: &GameState, index: u64) -> Result<&JournalV1, String> {
    match usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    {
        Some(E::VivienReidV1 { journal }) => Ok(journal),
        _ => Err("Vivien original journal absent".into()),
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
    let before = crate::planeswalker_v1::loyalty(state, p.source).ok_or("Vivien loyalty absent")?;
    let after = crate::planeswalker_v1::quote_loyalty_change_v48(
        state,
        p.source,
        [1, -3, -8][usize::from(p.ability_index)],
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
        return Err("Vivien paid input kind changed".into());
    };
    let input = input.clone();
    let item = state
        .stack
        .iter()
        .find(|r| r.v4.stack_item_id == id)
        .ok_or("Vivien paid row absent")?
        .clone();
    if item.source != input.source.source
        || item.controller != input.source.controller
        || item.v4.ability_source_contract != Some(input.source)
        || crate::planeswalker_v1::loyalty(state, item.source) != Some(input.after)
    {
        return Err("Vivien paid row did not follow actual loyalty cost".into());
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
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    item.inline_effect.as_ref().is_some_and(recipe)
        || rows(state).any(|(_,j)|matches!(j,JournalV1::Published{item:old,..} if old.v4.stack_item_id==item.v4.stack_item_id))
}
pub(crate) fn context_selected(state: &GameState, ctx: &ExecCtx) -> bool {
    // A physical arena slot is not an occurrence: an old paid activation must
    // not select a later spell after the card leaves and is cast again.
    // Retain exact historical occurrence/whole captured source-contract
    // selection so erasing or substituting only the context stack ID cannot
    // turn the actual old native program into an ownerless generic effect.
    waiting(state,ctx) || rows(state).any(|(_,j)|matches!(j,JournalV1::Published{item,..}
        if Some(item.v4.stack_item_id)==ctx.stack_item_id
            || ctx.ability_source_contract.is_some() && ctx.ability_source_contract==item.v4.ability_source_contract))
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|row| {
        (selected(state, row)
            || row
                .v4
                .ability_source_contract
                .is_some_and(|s| definition(s.card_def)))
            && (row.source == ctx.source
                || Some(row.v4.stack_item_id) == ctx.stack_item_id
                || row.v4.ability_source_contract.is_some()
                    && row.v4.ability_source_contract == ctx.ability_source_contract)
    })
}

// Only the existing native look owner may stamp its already-public row.
// The independent first receipt binds the entire stamped row, original root,
// context and start stage. Every other field still equals our pre-cost row.
fn unstarted_row(state: &GameState, item: &StackItem) -> Result<StackItem, String> {
    let mut original = item.clone();
    if let Some(started) = item.v4.squad_rallier_started_v1.0 {
        let first = state
            .engine
            .event_history
            .get(started.first_receipt_history_index as usize);
        let Some(E::SquadRallierResolutionBoundaryV1 { receipt }) = first else {
            return Err("Vivien look lost its independent native start".into());
        };
        let Some(p) = receipt.pending.as_deref() else {
            return Err("Vivien look start already closed".into());
        };
        if item.inline_effect != Some(look())
            || item.v4.activated_ability_index != Some(0)
            || !definition(started.source.card_def)
            || item.v4.ability_source_contract != Some(started.source)
            || started.stack_item_id != item.v4.stack_item_id
            || receipt.history_index != started.first_receipt_history_index
            || receipt.ordinal != 0
            || receipt.previous_history_index.is_some()
            || receipt.stack_item_id != item.v4.stack_item_id
            || receipt.source != started.source
            || p.resolving_item != *item
            || p.ctx != super::crystal_prevention_v1::context(item)
            || p.choice.is_some()
            || p.answered_choice_guard.is_some()
            || p.frames
                != [EffectFrame::Program {
                    op: look(),
                    path: vec![],
                }]
        {
            return Err("Vivien look changed its exact native start/row".into());
        }
        original.v4.squad_rallier_started_v1 = Default::default();
    }
    Ok(original)
}
fn publication(state: &GameState, item: &StackItem) -> Result<u64, String> {
    let mut found = rows(state).filter_map(|(at, j)| match j {
        JournalV1::Published { item: old, .. } if old.v4.stack_item_id == item.v4.stack_item_id => {
            Some((at, old))
        }
        _ => None,
    });
    let (at, old) = found.next().ok_or("Vivien original publication absent")?;
    if found.next().is_some() || old.as_ref() != &unstarted_row(state, item)? {
        return Err("Vivien full publication changed or duplicated".into());
    }
    Ok(at as u64)
}
fn validate_payment(state: &GameState, p: &PaymentV1) -> Result<(), String> {
    if !definition(p.source.card_def)
        || p.source.zone != Zone::Battlefield
        || p.pending.source != p.source.source
        || p.pending.controller != p.source.controller
        || p.pending.source_zone_change_count != p.source.zone_change_count
        || p.pending.cauldron_grant.0.is_some()
        || p.pending.ability_index >= 3
        || p.pending.target_spec != target(p.pending.ability_index as usize)
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
        return Err("Vivien original paid source/choice changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, p.source)?;
    let delta = [1, -3, -8][p.pending.ability_index as usize];
    if delta < 0 && p.before.checked_sub((-delta) as u32) != Some(p.after)
        || delta > 0 && p.after <= p.before
    {
        return Err("Vivien original loyalty subtraction changed".into());
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
        return Err("Vivien publication lost exact original payment/program/targets".into());
    }
    Ok(())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, E::VivienReidV1 { .. }))
    {
        return Err("Vivien private journal entered event dispatch".into());
    }
    let mut paid = std::collections::BTreeSet::new();
    let mut emblem_inputs = std::collections::BTreeSet::new();
    let mut timestamps = std::collections::BTreeSet::new();
    let mut publication_ids = std::collections::BTreeSet::new();
    let mut inputs = std::collections::BTreeSet::new();
    let mut created_inputs = std::collections::BTreeSet::new();
    for (position, j) in rows(state) {
        match j {
            JournalV1::Payment { input } => {
                if input.index != position as u64 {
                    return Err("Vivien original input moved".into());
                }
                validate_payment(state, input)?;
            }
            JournalV1::Published { input, item } => {
                if input.index >= position as u64 || !paid.insert(input.index) {
                    return Err("Vivien payment reused".into());
                }
                if !publication_ids.insert(item.v4.stack_item_id) {
                    return Err("Vivien stack publication duplicated".into());
                }
                validate_published(state, input, item)?;
                if crate::engine::time_stop_v1::disposed(state,item)?
                    && rows(state).any(|(_,j)|matches!(j,JournalV1::EmblemCreated{publication,..}if *publication==position as u64)) {
                    return Err("Vivien disposed ultimate also claimed emblem creation".into());
                }
                for row in state
                    .stack
                    .iter()
                    .filter(|row| row.v4.stack_item_id == item.v4.stack_item_id)
                {
                    if unstarted_row(state, row)? != **item {
                        return Err("Vivien current row changed from its paid publication".into());
                    }
                }
            }
            JournalV1::EmblemInput {
                publication: pub_at,
                item,
                controller,
                timestamp,
            } => {
                inputs.insert(position as u64);
                if *pub_at >= position as u64
                    || publication(state, item)? != *pub_at
                    || item.inline_effect != Some(emblem())
                    || *controller != item.controller
                    || *timestamp >= state.engine.next_effect_timestamp
                {
                    return Err(
                        "Vivien emblem input lost native original row/controller/timestamp".into(),
                    );
                }
            }
            JournalV1::EmblemCreated {
                input,
                publication: pub_at,
                controller,
                timestamp,
            } => {
                let JournalV1::Published { item, .. } = at(state, *pub_at)? else {
                    return Err("Vivien emblem publication kind changed".into());
                };
                if state
                    .stack
                    .iter()
                    .any(|r| r.v4.stack_item_id == item.v4.stack_item_id)
                    || state
                        .engine
                        .pending_effect
                        .as_ref()
                        .is_some_and(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
                {
                    return Err("Vivien completed emblem publication was reintroduced".into());
                }
                created_inputs.insert(*input);
                if input.checked_add(1) != Some(position as u64)
                    || *timestamp >= state.engine.next_effect_timestamp
                    || !timestamps.insert(*timestamp)
                    || !emblem_inputs.insert(*pub_at)
                    || !matches!(at(state,*input)?,JournalV1::EmblemInput{publication:p,controller:c,timestamp:t,..} if p==pub_at&&c==controller&&t==timestamp)
                {
                    return Err("Vivien emblem creation lost independent native input".into());
                }
            }
        }
    }
    if inputs != created_inputs {
        return Err("Vivien emblem input has no exact creation".into());
    }
    Ok(())
}
pub(crate) fn validate_pending(state: &GameState) -> Result<(), String> {
    let Some(p) = state
        .engine
        .pending_effect
        .as_ref()
        .filter(|p| selected(state, &p.resolving_item))
    else {
        return Ok(());
    };
    validate_continuation(state, p)
}
pub(crate) fn validate_continuation(
    state: &GameState,
    p: &EffectContinuation,
) -> Result<(), String> {
    publication(state, &p.resolving_item)?;
    let mut expected = super::crystal_prevention_v1::context(&p.resolving_item);
    expected.niv_damage_origin_v1 =
        event::niv_damage_origin_for_saved_item_v1(state, &p.resolving_item);
    let root = p
        .resolving_item
        .inline_effect
        .as_ref()
        .ok_or("Vivien root absent")?;
    if p.ctx!=expected || p.frames.iter().any(|f|matches!(f,EffectFrame::Program{op,path} if effect_op_at_structural_path(root,path)!=Some(op))) {
        return Err("Vivien pending original full context/program changed".into())
    }
    Ok(())
}
pub(super) fn install(
    state: &mut GameState,
    owner: &NativeExecutionOwnerV1,
    ctx: &ExecCtx,
) -> Result<(), String> {
    owner.validate_entry(state, &owner.item, ctx, &emblem())?;
    validate_state(state)?;
    let pub_at = publication(state, &owner.item)?;
    if rows(state)
        .any(|(_, j)| matches!(j,JournalV1::EmblemCreated{publication,..}if *publication==pub_at))
    {
        return Err("Vivien emblem already created by this occurrence".into());
    }
    let mut staged = state.clone();
    let timestamp = staged.engine.next_effect_timestamp;
    staged.engine.next_effect_timestamp = timestamp
        .checked_add(1)
        .ok_or("Vivien timestamp exhausted")?;
    let input = staged.engine.event_history.len() as u64;
    append(
        &mut staged,
        JournalV1::EmblemInput {
            publication: pub_at,
            item: Box::new(owner.item.clone()),
            controller: ctx.controller,
            timestamp,
        },
    );
    append(
        &mut staged,
        JournalV1::EmblemCreated {
            input,
            publication: pub_at,
            controller: ctx.controller,
            timestamp,
        },
    );
    validate_state(&staged)?;
    crate::engine::validate_battlefield_pt(&staged)
        .map_err(|_| "Vivien emblem exceeds native PT envelope")?;
    *state = staged;
    Ok(())
}
fn emblems(state: &GameState) -> impl Iterator<Item = (PlayerId, u64)> + '_ {
    rows(state).filter_map(|(_, j)| match j {
        JournalV1::EmblemCreated {
            controller,
            timestamp,
            ..
        } => Some((*controller, *timestamp)),
        _ => None,
    })
}
pub(crate) fn bonus(state: &GameState, id: ObjectId) -> i128 {
    let Some(o) = state
        .objects
        .try_get(id)
        .filter(|o| o.zone == Zone::Battlefield)
    else {
        return 0;
    };
    if !crate::engine::object_has_type(state, id, CardType::Creature) {
        return 0;
    }
    emblems(state)
        .filter(|(p, _)| *p == o.controller)
        .fold(0_i128, |n, _| n + 2)
}
pub(crate) fn grants_keyword(state: &GameState, id: ObjectId, kw: Keywords) -> bool {
    [
        Keywords::VIGILANCE,
        Keywords::TRAMPLE,
        Keywords::INDESTRUCTIBLE,
    ]
    .contains(&kw)
        && granted_keywords(state, id).has(kw)
}
pub(crate) fn granted_keywords(state: &GameState, id: ObjectId) -> Keywords {
    let Some(o) = state
        .objects
        .try_get(id)
        .filter(|o| o.zone == Zone::Battlefield)
    else {
        return Keywords::NONE;
    };
    if !crate::engine::object_has_type(state, id, CardType::Creature) {
        return Keywords::NONE;
    }
    if emblems(state).any(|(p, t)| {
        p == o.controller && crate::continuous_characteristics_v1::grant_survives(state, id, t)
    }) {
        Keywords::VIGILANCE | Keywords::TRAMPLE | Keywords::INDESTRUCTIBLE
    } else {
        Keywords::NONE
    }
}
pub(crate) fn require_legacy(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    validate_state(state)?;
    let visible = state.objects.iter().any(|(id, o)| {
        crate::optional_trigger_public_v1::visible_known(state, id, viewer, |card_id| {
            definition(card_id)
        }) || viewer.is_some_and(|v| {
            o.v4.face_down_v1
                .is_some_and(|f| f.lookers & (1 << v.index()) != 0)
                && definition(o.card_def)
        })
    });
    if rows(state).next().is_some() || visible {
        Err("Vivien and its persistent emblem require a complete V70 consumer".into())
    } else {
        Ok(())
    }
}

/// Public projection only after the unchanged whole native journal validation.
pub(crate) fn public_facts_v70(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::vivien_public_v70::FactsV70, String> {
    public_facts_counter_scope_v72(
        state,
        viewer,
        crate::optional_trigger_public_v1::CounterScopeV61::CallV62,
        crate::optional_trigger_public_v1::ConsumerV1::Legacy,
    )
}
pub(crate) fn public_facts_tinybones_compat_v72(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::vivien_public_v70::FactsV70, String> {
    public_facts_tinybones_for_consumer_v72(
        state,
        viewer,
        crate::optional_trigger_public_v1::ConsumerV1::Legacy,
    )
}
pub(crate) fn public_facts_tinybones_for_consumer_v72(
    state: &GameState,
    viewer: PlayerId,
    consumer: crate::optional_trigger_public_v1::ConsumerV1,
) -> Result<crate::vivien_public_v70::FactsV70, String> {
    public_facts_counter_scope_v72(
        state,
        viewer,
        crate::optional_trigger_public_v1::CounterScopeV61::TinybonesV72,
        consumer,
    )
}
fn public_facts_counter_scope_v72(
    state: &GameState,
    viewer: PlayerId,
    scope: crate::optional_trigger_public_v1::CounterScopeV61,
    consumer: crate::optional_trigger_public_v1::ConsumerV1,
) -> Result<crate::vivien_public_v70::FactsV70, String> {
    use crate::{optional_trigger_public_v1 as public, vivien_public_v70::*};
    validate_state(state)?;
    validate_pending(state)?;
    super::squad_rallier_resolution_v1::validate(state)?;
    let visible_printings = state
        .objects
        .iter()
        .filter_map(|(id, _)| public::reference(state, id, Some(viewer)))
        .filter(|r| matches!(r.identity, public::IdentityV1::Known { card_id: 488 }))
        .collect();
    let battlefield =
        crate::valkyries_call_public_v62::capture_for_consumer(state, viewer, scope, consumer)?
            .battlefield;
    let current = battlefield
        .into_iter()
        .flatten()
        .filter(|r| matches!(r.card.identity, public::IdentityV1::Known { card_id: 488 }))
        .collect();
    let mut activations = Vec::new();
    let mut emblems = Vec::new();
    for (pub_at, j) in rows(state) {
        let JournalV1::Published { input, item } = j else {
            continue;
        };
        let occurrence =
            u32::try_from(activations.len()).map_err(|_| "V70 activation count exceeds u32")?;
        let current = state
            .engine
            .pending_effect
            .as_ref()
            .filter(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id);
        let look_receipts = state
            .engine
            .event_history
            .iter()
            .filter_map(|e| match e {
                E::SquadRallierResolutionBoundaryV1 { receipt }
                    if receipt.stack_item_id == item.v4.stack_item_id =>
                {
                    Some(receipt)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        let created = rows(state).find_map(|(_, j)| match j {
            JournalV1::EmblemCreated {
                publication,
                controller,
                timestamp,
                ..
            } if *publication == pub_at as u64 => Some((*controller, *timestamp)),
            _ => None,
        });
        let stage = if crate::engine::time_stop_v1::disposed(state, item)? {
            StageV70::Disposed
        } else if created.is_some() {
            StageV70::EmblemCreated
        } else if let Some(p) = current {
            if p.choice.is_some() {
                StageV70::Looking
            } else {
                StageV70::AnsweredLook
            }
        } else if state
            .stack
            .iter()
            .any(|r| r.v4.stack_item_id == item.v4.stack_item_id)
        {
            StageV70::Published
        } else if look_receipts.last().is_some_and(|r| r.pending.is_none()) {
            StageV70::LookFinished
        } else {
            StageV70::NoLongerPublished
        };
        let mut original_targets = Vec::new();
        for c in &item.v4.target_contracts {
            let crate::state::StackTargetContractV4::Object {
                object,
                card_def,
                owner,
                controller,
                zone,
                zone_change_count,
                ..
            } = *c
            else {
                return Err("V70 destroy target is not an object".into());
            };
            let original = public::ReferenceV1 {
                object: object.0,
                generation: zone_change_count,
                owner: owner.into(),
                controller: controller.into(),
                zone,
                identity: public::IdentityV1::Known {
                    card_id: crate::rl::projected_captured_card_def(
                        state,
                        object,
                        zone_change_count,
                        zone,
                        card_def,
                    ),
                },
            };
            let current_original = state
                .objects
                .try_get(object)
                .filter(|o| o.zone_change_count == zone_change_count && o.zone == zone)
                .and_then(|_| public::reference(state, object, Some(viewer)));
            let legal_now = crate::engine::target_contract_matches_live(state, c.target(), *c)
                && crate::engine::effect_target_is_legal_from_ability_source(
                    state,
                    input.source,
                    item.controller,
                    input.pending.target_spec,
                    &item.targets,
                    0,
                );
            original_targets.push(TargetV70 {
                original,
                current_original,
                legal_now,
            });
        }
        activations.push(ActivationV70 {
            occurrence,
            source: public::root(state, input.source, Some(viewer))?,
            ability_index: input.pending.ability_index,
            loyalty_before: input.before,
            loyalty_after: input.after,
            original_targets,
            stage,
        });
        if let Some((controller, timestamp)) = created {
            emblems.push(EmblemV70 {
                occurrence: u32::try_from(emblems.len())
                    .map_err(|_| "V70 emblem count exceeds u32")?,
                activation: occurrence,
                controller: controller.into(),
                timestamp,
            });
        }
    }
    let value = FactsV70 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning: crate::rules_meaning_vivien_v70::printed_meaning()?,
        visible_printings,
        current,
        activations,
        emblems,
    };
    crate::vivien_public_v70::validate(&value)?;
    Ok(value)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "vivien_reid_v1_tests.rs"]
pub(crate) mod tests;
