//! FDN51: native attack-bound, exact-incarnation, mana-cost flashback grants.
//! History is the existing native mutable-world authority, not an external capability.
use super::*;
use crate::state::{AbilitySourceContractV4, SpellCastRouteV4};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV1 {
    Attack {
        declaration: u64,
        source: AbilitySourceContractV4,
        turn: u32,
    },
    Collected {
        attack: u64,
    },
    Published {
        attack: u64,
        item: Box<StackItem>,
    },
    Installed {
        attack: u64,
        stack_item_id: StackItemId,
        target: crate::state::StackTargetContractV4,
        controller: PlayerId,
        turn: u32,
    },
    Cleanup {
        index: u64,
        turn: u32,
        active: PlayerId,
        installations: Vec<u64>,
    },
}
fn journal(state: &GameState, index: u64) -> Result<&JournalV1, String> {
    match usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    {
        Some(event::CommittedEvent::SphinxLoreV1 { journal }) => Ok(journal),
        _ => Err("Sphinx original history receipt absent".into()),
    }
}
fn append(state: &mut GameState, journal: JournalV1) -> u64 {
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(event::CommittedEvent::SphinxLoreV1 { journal });
    index
}
pub(crate) fn marker() -> EffectOp {
    EffectOp::BindSphinxLoreV1
}
pub(crate) fn recipe(op: &EffectOp) -> bool {
    matches!(
        op,
        EffectOp::BindSphinxLoreV1 | EffectOp::SphinxLoreV1 { .. }
    )
}
pub(crate) fn target_spec() -> card_def::TargetSpec {
    card_def::TargetSpec::StandardV1(
        crate::standard_cards_v1::StandardTargetV1::InstantOrSorceryCardInOwnGraveyard,
    )
}
pub(crate) fn record_attack(state: &mut GameState, id: ObjectId) {
    let o = state.objects.get(id);
    if !definition(o.card_def)
        || o.v4.face_index != 0
        || o.v4.face_down_v1.is_some()
        || !crate::continuous_characteristics_v1::printed_abilities_active(state, id)
    {
        return;
    }
    let source = AbilitySourceContractV4::capture(state, id);
    let Some(declaration) = state.engine.event_history.len().checked_sub(1) else {
        return;
    };
    let journal = JournalV1::Attack {
        declaration: declaration as u64,
        source,
        turn: state.turn_key_v1(),
    };
    let event = event::CommittedEvent::SphinxLoreV1 { journal };
    state.engine.event_history.push(event.clone());
    state.engine.event_log.push(event);
}
fn source_at(state: &GameState, index: u64) -> Result<AbilitySourceContractV4, String> {
    let JournalV1::Attack {
        declaration,
        source,
        ..
    } = journal(state, index)?
    else {
        return Err("Sphinx attack receipt changed".into());
    };
    if declaration.checked_add(1) != Some(index)
        || !definition(source.card_def)
        || source.zone != Zone::Battlefield
        || !matches!(state.engine.event_history.get(*declaration as usize),Some(event::CommittedEvent::DeclaredAttacker{source:id,source_zone_change_count,controller})if *id==source.source && *source_zone_change_count==source.zone_change_count && *controller==source.controller)
    {
        return Err("Sphinx original declaration changed".into());
    }
    crate::engine::validate_historical_ability_source_contract(state, *source)?;
    Ok(*source)
}
pub(crate) fn collect(
    state: &mut GameState,
    events: &[event::CommittedEvent],
) -> Result<Vec<crate::trigger::PendingTrigger>, String> {
    let mut out = vec![];
    let mut seen = vec![];
    for e in events {
        if !matches!(
            e,
            event::CommittedEvent::SphinxLoreV1 {
                journal: JournalV1::Attack { .. }
            }
        ) {
            continue;
        }
        let indices = state
            .engine
            .event_history
            .iter()
            .enumerate()
            .filter_map(|(i, r)| (r == e).then_some(i as u64))
            .collect::<Vec<_>>();
        let [attack] = indices.as_slice() else {
            return Err("Sphinx capture erased or duplicated".into());
        };
        let source = source_at(state, *attack)?;
        if seen.contains(attack)||state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Collected{attack:i}}if i==attack)){return Err("Sphinx attack recollected".into());}
        out.push(crate::trigger::PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::SphinxLoreV1 {
                attack_history_index: *attack,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: target_spec(),
            targets: vec![],
            target_contracts: vec![],
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: vec![],
        });
        seen.push(*attack);
    }
    for attack in seen {
        append(state, JournalV1::Collected { attack });
    }
    Ok(out)
}
pub(crate) fn validate_root(
    state: &GameState,
    op: &EffectOp,
    source: Option<AbilitySourceContractV4>,
) -> Result<(), String> {
    let EffectOp::SphinxLoreV1 {
        attack_history_index,
    } = op
    else {
        return if recipe(op) {
            Err("unbound Sphinx template".into())
        } else {
            Ok(())
        };
    };
    if Some(source_at(state,*attack_history_index)?)!=source || state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Collected{attack}}if attack==attack_history_index)).count()!=1{return Err("Sphinx captured source/occurrence changed".into());}
    Ok(())
}
pub(crate) fn selected(state: &GameState, item: &StackItem) -> bool {
    state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Published{item:r,..}}if r.v4.stack_item_id==item.v4.stack_item_id))
        || item.inline_effect.as_ref().is_some_and(recipe)
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state.stack.iter().any(|r| {
        selected(state, r)
            && (r.source == ctx.source
                || r.v4.ability_source_contract.is_some()
                    && r.v4.ability_source_contract == ctx.ability_source_contract)
    })
}
pub(crate) fn validate_item(state: &GameState, item: &StackItem) -> Result<(), String> {
    let op = item.inline_effect.as_ref().ok_or("Sphinx root erased")?;
    validate_root(state, op, item.v4.ability_source_contract)?;
    let source = item
        .v4
        .ability_source_contract
        .ok_or("Sphinx source absent")?;
    if !recipe(op)
        || item.kind != crate::state::StackItemKind::TriggeredAbility
        || item.source != source.source
        || item.controller != source.controller
        || item.is_copy
        || item.kicked
        || item.is_flashback
        || item.madness_offer
        || item.mode_chosen != 0
        || item.v4.x_value != 0
        || item.v4.granted_by.is_some()
        || item.v4.source_contract.is_some()
        || item.v4.activated_ability_index.is_some()
        || item.v4.target_spec != Some(target_spec())
        || item.targets.len() > 1
        || item.v4.target_contracts.len() != item.targets.len()
        || !item.discarded.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
        || item.v4.hidden_ability_source.is_some()
        || item.v4.cauldron_grant.0.is_some()
    {
        return Err("Sphinx row/controller/target recipe changed".into());
    }
    Ok(())
}
pub(crate) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(EffectOp::SphinxLoreV1 {
        attack_history_index,
    }) = item.inline_effect
    else {
        return Ok(());
    };
    validate_item(state, item)?;
    if item.targets.len() != 1 {
        return Err("Sphinx publication requires its announced target".into());
    }
    if state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Published{attack,..}}if *attack==attack_history_index)){return Err("Sphinx attack already published".into());}
    append(
        state,
        JournalV1::Published {
            attack: attack_history_index,
            item: Box::new(item.clone()),
        },
    );
    Ok(())
}
pub(crate) fn validate_published(state: &GameState, item: &StackItem) -> Result<(), String> {
    validate_item(state, item)?;
    let rows = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            event::CommittedEvent::SphinxLoreV1 {
                journal: JournalV1::Published { item: r, .. },
            } if r.v4.stack_item_id == item.v4.stack_item_id => Some(r.as_ref()),
            _ => None,
        })
        .collect::<Vec<_>>();
    if rows != [item] {
        return Err("Sphinx original published row changed".into());
    }
    Ok(())
}
pub(super) fn execute(
    op: &EffectOp,
    ctx: &ExecCtx,
    state: &mut GameState,
    proof: &NativeExecutionProofV1<'_>,
) -> Result<(), String> {
    if proof.ctx != ctx
        || *proof.owner.root != *op
        || proof.owner.item.v4.ability_source_contract != ctx.ability_source_contract
    {
        return Err("Sphinx lexical execution owner mismatch".into());
    }
    let item = &proof.owner.item;
    validate_published(state, item)?;
    if state.engine.event_history.iter().any(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Installed{stack_item_id,..}}if *stack_item_id==item.v4.stack_item_id)){return Err("Sphinx grant replayed".into());}
    let EffectOp::SphinxLoreV1 {
        attack_history_index,
    } = op
    else {
        return Err("Sphinx template execution".into());
    };
    let Some(Target::Object(id)) = item.targets.first() else {
        return Err("Sphinx target is not a card".into());
    };
    if !crate::engine::effect_target_is_legal_from_ability_source(
        state,
        ctx.ability_source_contract.unwrap(),
        ctx.controller,
        target_spec(),
        &item.targets,
        0,
    ) {
        return Ok(());
    }
    let target = item.v4.target_contracts[0];
    if !crate::engine::target_contract_matches_live(state, Target::Object(*id), target) {
        return Ok(());
    }
    append(
        state,
        JournalV1::Installed {
            attack: *attack_history_index,
            stack_item_id: item.v4.stack_item_id,
            target,
            controller: item.controller,
            turn: state.turn_key_v1(),
        },
    );
    Ok(())
}
/// Historical validation is independent of current turn, source survival, and target's later zone.
pub(crate) fn installation(
    state: &GameState,
    index: u64,
) -> Result<(EffectObjectBinding, PlayerId, u32), String> {
    let JournalV1::Installed {
        attack,
        stack_item_id,
        target,
        controller,
        turn,
    } = journal(state, index)?
    else {
        return Err("Sphinx grant installation absent".into());
    };
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            event::CommittedEvent::SphinxLoreV1 {
                journal: JournalV1::Published { attack: a, item },
            } if a == attack && item.v4.stack_item_id == *stack_item_id => Some((i, item)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(published, item)] = rows.as_slice() else {
        return Err("Sphinx original installation publication absent".into());
    };
    validate_published(state, item)?;
    let crate::state::StackTargetContractV4::Object {
        object,
        card_def,
        owner,
        zone,
        zone_change_count,
        ..
    } = *target
    else {
        return Err("Sphinx installation not a physical card".into());
    };
    if *published as u64>=index||item.controller!=*controller||zone!=Zone::Graveyard||item.targets!=[Target::Object(object)]||owner!=*controller
        ||item.v4.target_contracts!=[*target]
        ||state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Installed{stack_item_id:id,..}}if id==stack_item_id)).count()!=1{return Err("Sphinx installation target/source changed".into());}
    let o = state
        .objects
        .try_get(object)
        .ok_or("Sphinx granted card absent")?;
    if o.owner != owner || o.card_def != card_def || o.zone_change_count < zone_change_count {
        return Err("Sphinx granted original card changed".into());
    }
    let d = &card_def::CARD_DEFS[usize::from(card_def)];
    if !d
        .types
        .iter()
        .any(|t| matches!(t, CardType::Instant | CardType::Sorcery))
        || d.is_token
    {
        return Err("Sphinx grant not instant/sorcery card".into());
    }
    Ok((
        EffectObjectBinding {
            object,
            expected_zone: zone,
            expected_zone_change_count: zone_change_count,
        },
        *controller,
        *turn,
    ))
}
pub(crate) fn active_at(state: &GameState, end: usize) -> Result<Vec<u64>, String> {
    let mut active = vec![];
    for (i, e) in state
        .engine
        .event_history
        .get(..end)
        .ok_or("Sphinx history boundary outside stream")?
        .iter()
        .enumerate()
    {
        match e {
            event::CommittedEvent::SphinxLoreV1 {
                journal: JournalV1::Installed { .. },
            } => {
                installation(state, i as u64)?;
                active.push(i as u64);
            }
            event::CommittedEvent::SphinxLoreV1 {
                journal:
                    JournalV1::Cleanup {
                        index,
                        installations,
                        ..
                    },
            } => {
                if *index != i as u64 || installations != &active {
                    return Err("Sphinx Cleanup occurrence/expiry roster changed".into());
                }
                active.clear();
            }
            _ => {}
        }
    }
    Ok(active)
}
pub(crate) fn cleanup(state: &mut GameState) -> Result<(), String> {
    let installations = active_at(state, state.engine.event_history.len())?;
    if installations.is_empty() {
        return Ok(());
    }
    if state.step != crate::state::Step::Cleanup {
        return Err("Sphinx expiry outside actual Cleanup".into());
    }
    let index = state.engine.event_history.len() as u64;
    append(
        state,
        JournalV1::Cleanup {
            index,
            turn: state.turn_key_v1(),
            active: state.active_player,
            installations,
        },
    );
    Ok(())
}
pub(crate) fn active_grants(state: &GameState, id: ObjectId, player: PlayerId) -> Vec<u64> {
    let Some(o) = state.objects.try_get(id) else {
        return vec![];
    };
    if o.zone != Zone::Graveyard
        || o.owner != player
        || o.v4.is_token
        || o.spell_copy_origin.is_some()
    {
        return vec![];
    }
    let Ok(active) = active_at(state, state.engine.event_history.len()) else {
        return vec![];
    };
    active
        .into_iter()
        .filter(|i| {
            installation(state, *i).is_ok_and(|(b, p, t)| {
                b.object == id
                    && b.expected_zone_change_count == o.zone_change_count
                    && p == player
                    && t == state.turn_key_v1()
            })
        })
        .collect()
}
pub(crate) fn granted_route(state: &GameState, id: ObjectId) -> Option<u64> {
    match state.objects.try_get(id)?.v4.spell_cast_origin?.route {
        SpellCastRouteV4::SphinxLoreFlashbackV1 { installation, .. } => Some(installation),
        _ => None,
    }
}
pub(crate) fn validate_cast_grant(
    state: &GameState,
    id: ObjectId,
    player: PlayerId,
    index: u64,
) -> bool {
    installation(state,index).is_ok_and(|(b,p,_)| b.object==id&&p==player && state.objects.try_get(id).is_some_and(|o|o.owner==p&&o.zone==Zone::Stack&&o.zone_change_count==b.expected_zone_change_count.checked_add(1).unwrap_or(0)
        &&o.v4.spell_cast_origin.is_some_and(|origin| origin.origin_zone==Zone::Graveyard&&origin.origin_zone_change_count==b.expected_zone_change_count&&matches!(origin.route,SpellCastRouteV4::SphinxLoreFlashbackV1{installation,holder,graveyard_generation}if installation==index&&holder==p&&graveyard_generation==b.expected_zone_change_count))))
}
pub(crate) fn quote_grant(state: &GameState, id: ObjectId, player: PlayerId, index: u64) -> bool {
    active_grants(state, id, player).contains(&index)
        || validate_cast_grant(state, id, player, index)
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if !state
        .engine
        .event_history
        .iter()
        .any(|e| matches!(e, event::CommittedEvent::SphinxLoreV1 { .. }))
    {
        return Ok(());
    }
    active_at(state, state.engine.event_history.len())?;
    for (i, e) in state.engine.event_history.iter().enumerate() {
        match e {
            event::CommittedEvent::SphinxLoreV1 {
                journal: JournalV1::Attack { .. },
            } => {
                source_at(state, i as u64)?;
            }
            event::CommittedEvent::SphinxLoreV1 {
                journal: JournalV1::Collected { attack },
            } => {
                source_at(state, *attack)?;
                if *attack>=i as u64||state.engine.event_history.iter().filter(|e|matches!(e,event::CommittedEvent::SphinxLoreV1{journal:JournalV1::Collected{attack:a}}if a==attack)).count()!=1{return Err("Sphinx collection order/uniqueness changed".into());}
            }
            event::CommittedEvent::SphinxLoreV1 {
                journal: JournalV1::Published { attack, item },
            } => {
                if *attack >= i as u64 {
                    return Err("Sphinx publication before capture".into());
                }
                validate_published(state, item)?;
            }
            _ => {}
        }
    }
    Ok(())
}
pub(crate) fn refuse_legacy(state: &GameState) -> Result<(), String> {
    if crate::standard_cards_v1::has_battlefield_copy_origin_v1(state, definition)
        || state.objects.iter().any(|(_, o)| {
            matches!(o.zone, Zone::Battlefield | Zone::Stack | Zone::Graveyard)
                && o.v4.face_down_v1.is_none()
                && definition(o.card_def)
        })
        || state
            .engine
            .event_history
            .iter()
            .any(|e| matches!(e, event::CommittedEvent::SphinxLoreV1 { .. }))
    {
        Err("Sphinx of Forgotten Lore requires a complete successor".into())
    } else {
        Ok(())
    }
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 477
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
            d.name == "Sphinx of Forgotten Lore"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                        ],
                        generic: 2,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Sphinx]
                && d.supertypes.is_empty()
                && d.power == Some(3)
                && d.toughness == Some(3)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [crate::mana::ManaColor::U]
                && d.target_spec == card_def::TargetSpec::None
                && d.keywords == (Keywords::FLASH | Keywords::FLYING)
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
                && d.activated_abilities.is_empty()
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
                && d.mana_value == 4
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "sphinx_lore_v1_tests.rs"]
mod tests;
