//! Definition-scoped native Tinybones prerequisites. No generic exile permission.
use super::*;

/// A lexical mana-matching grant. It is neither serialized nor constructible by
/// a public caller and does not authorize playing or casting its bound card.
#[derive(Debug, Clone)]
pub(crate) struct ManaGrantV72 {
    _source: AbilitySourceContractV4,
    _card: EffectObjectBinding,
    _controller: PlayerId,
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 490
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
            d.name == "Tinybones, Bauble Burglar"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == Cost {
                        pips: &[mana::Pip::Colored(ManaColor::B)],
                        generic: 1,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Skeleton, card_def::Subtype::Rogue]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power == Some(1)
                && d.toughness == Some(3)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors == [ManaColor::B]
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
                && d.activated_abilities.len() == 1
                && activation_matches(&d.activated_abilities[0])
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
                && d.mana_value == 2
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
fn active_source(state: &GameState, player: PlayerId) -> Option<AbilitySourceContractV4> {
    state.players[player.index()]
        .battlefield
        .iter()
        .find_map(|&id| {
            let o = state.objects.try_get(id)?;
            (o.zone == Zone::Battlefield
                && o.controller == player
                && o.v4.face_index == 0
                && o.v4.face_down_v1.is_none()
                && definition(o.card_def)
                && crate::continuous_characteristics_v1::printed_abilities_active(state, id))
            .then(|| AbilitySourceContractV4::capture(state, id))
        })
}

/// Mana-as-though is distinct from the own-turn permission to play a card.
/// An independently authorized foreign stashed spell may use the current
/// controller's actual Tinybones on another turn as well. The stash issuer is
/// deliberately absent: any current printed source grants the static ability.
pub(crate) fn mana_grant(
    state: &GameState,
    card: ObjectId,
    controller: PlayerId,
) -> Option<ManaGrantV72> {
    let o = state.objects.try_get(card)?;
    if o.owner == controller || o.v4.is_token || o.spell_copy_origin.is_some() {
        return None;
    }
    let stash = match o.zone {
        Zone::Exile if state.exile.contains(&card) => o.counters.stash,
        Zone::Stack if o.controller == controller => {
            let origin = o.v4.spell_cast_origin?;
            if origin.origin_zone != Zone::Exile
                || origin.origin_zone_change_count.checked_add(1) != Some(o.zone_change_count)
            {
                return None;
            }
            state
                .counter_lki_for(card, origin.origin_zone_change_count)?
                .stash
        }
        _ => return None,
    };
    let generation = if o.zone == Zone::Stack {
        o.v4.spell_cast_origin?.origin_zone_change_count
    } else {
        o.zone_change_count
    };
    if stash <= 0 || !stashed_incarnation(state, card, generation, stash) {
        return None;
    }
    Some(ManaGrantV72 {
        _source: active_source(state, controller)?,
        _card: EffectObjectBinding {
            object: card,
            expected_zone: o.zone,
            expected_zone_change_count: o.zone_change_count,
        },
        _controller: controller,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardV72 {
    pub card: AbilitySourceContractV4,
    pub discarder: PlayerId,
    pub observers: Vec<AbilitySourceContractV4>,
    pub move_history_index: u64,
    pub outcome: EffectObjectBinding,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayV72 {
    pub card: AbilitySourceContractV4,
    pub stash: i32,
    pub source: AbilitySourceContractV4,
    pub actor: PlayerId,
    pub turn: u32,
    pub land: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum JournalV72 {
    Discarded(DiscardV72),
    Published {
        discard: u64,
        observer: u32,
        item: Box<StackItem>,
    },
    Stashed {
        publication: u64,
        move_history_index: u64,
        exiled: EffectObjectBinding,
        after: i32,
    },
    Finished {
        publication: u64,
        countered: bool,
    },
    Play(PlayV72),
    Announced {
        play: u64,
        source: StackSourceContractV4,
        stack_item_id: StackItemId,
    },
    Paid {
        play: u64,
        source: StackSourceContractV4,
        stack_item_id: StackItemId,
    },
    Land {
        play: u64,
        source: AbilitySourceContractV4,
    },
    Aborted {
        play: u64,
        returned: EffectObjectBinding,
    },
    ForeignAdventure {
        original: Box<StackItem>,
        move_history_index: u64,
        exiled: EffectObjectBinding,
    },
    AdventureAborted {
        permission: u64,
        original: StackSourceContractV4,
        returned: EffectObjectBinding,
    },
    /// A stash-bearing cast may have an independent exile permission. Its
    /// native reversal restores counters too, without inventing a Tiny play.
    IndependentStashAborted {
        original: StackSourceContractV4,
        returned: EffectObjectBinding,
        stash: i32,
    },
}

/// Adventure grants its actual caster the later permanent cast, including a
/// foreign card. The old owner-only marker remains byte-identical for ordinary
/// owner casts; only the previously unsupported foreign path has this receipt.
pub(super) fn foreign_adventure_finished(state: &mut GameState, item: &StackItem) {
    if item.is_copy || state.objects.get(item.source).owner == item.controller {
        return;
    }
    let exiled = binding(state, item.source);
    let move_history_index = state.engine.event_history.iter().rposition(|e| {
        matches!(e, CommittedEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Exile } if *object == item.source)
    }).expect("actual Adventure departure retained its zone event") as u64;
    push(
        state,
        JournalV72::ForeignAdventure {
            original: Box::new(item.clone()),
            move_history_index,
            exiled,
        },
    );
}

fn adventure_binding(
    state: &GameState,
    at: u64,
) -> Result<(EffectObjectBinding, PlayerId), String> {
    match journal(state, at)? {
        JournalV72::ForeignAdventure {
            original,
            move_history_index,
            exiled,
        } => {
            let source = original
                .v4
                .source_contract
                .ok_or("foreign Adventure source absent")?;
            if original.is_copy
                || original.kind != StackItemKind::Spell
                || original.inline_effect.is_some()
                || original.source != source.source
                || original.controller != source.controller
                || original.v4.cast_method != Some(CastMethodV4::Omen)
                || source.cast_method != CastMethodV4::Omen
                || source.owner == source.controller
                || source.spell_copy_origin.is_some()
                || card_def::CARD_DEFS
                    .get(source.card_def as usize)
                    .and_then(supported_adventure)
                    .is_none()
                || source.zone != Zone::Stack
                || exiled.object != source.source
                || exiled.expected_zone != Zone::Exile
                || source.zone_change_count.checked_add(1)
                    != Some(exiled.expected_zone_change_count)
                || *move_history_index >= at
                || !matches!(state.engine.event_history.get(*move_history_index as usize), Some(CommittedEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Exile }) if *object == source.source)
            {
                return Err("foreign Adventure changed its original cast/departure".into());
            }
            let casts = state.engine.event_history.iter().take(*move_history_index as usize).filter(|e| {
                matches!(e, CommittedEvent::PublishedCastIdentityV1 { receipt } if receipt.spell==source.source && receipt.stack_item_id==original.v4.stack_item_id && receipt.owner==source.owner && receipt.controller==source.controller && receipt.stack_zone_change_count==source.zone_change_count && receipt.public_identity==crate::cast_identity_v1::PublicIdentityV1::Known {card_def:source.card_def})
            }).count();
            if casts != 1 {
                return Err("foreign Adventure lost its actual finalized cast".into());
            }
            Ok((*exiled, source.controller))
        }
        JournalV72::AdventureAborted {
            permission,
            original,
            returned,
        } => {
            if *permission >= at {
                return Err("foreign Adventure reversal order".into());
            }
            let (before, holder) = adventure_binding(state, *permission)?;
            let origin = original
                .spell_cast_origin
                .ok_or("foreign Adventure reversal origin")?;
            if original.source != before.object
                || original.controller != holder
                || origin.origin_zone != Zone::Exile
                || origin.origin_zone_change_count != before.expected_zone_change_count
                || origin.route != SpellCastRouteV4::AdventureExile
                || original.cast_method != CastMethodV4::Normal
                || before.expected_zone_change_count.checked_add(1)
                    != Some(original.zone_change_count)
                || returned.object != before.object
                || returned.expected_zone != Zone::Exile
                || original.zone_change_count.checked_add(1)
                    != Some(returned.expected_zone_change_count)
            {
                return Err("foreign Adventure reversal changed card/controller/generation".into());
            }
            Ok((*returned, holder))
        }
        _ => Err("foreign Adventure permission absent".into()),
    }
}
fn adventure_permission(
    state: &GameState,
    card: ObjectId,
    generation: u32,
) -> Option<(u64, PlayerId)> {
    if !state
        .engine
        .event_history
        .has_any(&[EventKindV1::TinybonesV72])
    {
        return None;
    }
    state.engine.event_history.iter().enumerate().rev().find_map(|(at,e)| {
        if !matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::ForeignAdventure{..}|JournalV72::AdventureAborted{..})) {return None;}
        let (b,holder)=adventure_binding(state,at as u64).ok()?;
        (b.object==card && b.expected_zone_change_count==generation).then_some((at as u64,holder))
    })
}
pub(super) fn adventure_allowed(state: &GameState, card: ObjectId, actor: PlayerId) -> bool {
    let o = state.objects.get(card);
    o.v4.on_adventure
        && adventure_permission(state, card, o.zone_change_count)
            .map_or(o.owner == actor, |(_, holder)| holder == actor)
}
pub(super) fn adventure_contract(state: &GameState, source: StackSourceContractV4) -> bool {
    source.spell_cast_origin.is_some_and(|origin| {
        adventure_permission(state, source.source, origin.origin_zone_change_count)
            .map_or(source.owner == source.controller, |(_, holder)| {
                holder == source.controller
            })
    })
}
fn push(state: &mut GameState, journal: JournalV72) -> u64 {
    let at = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::TinybonesV72 {
            journal: Box::new(journal),
        });
    at
}
fn journal(state: &GameState, index: u64) -> Result<&JournalV72, String> {
    match usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    {
        Some(CommittedEvent::TinybonesV72 { journal }) => Ok(journal),
        _ => Err("Tinybones original occurrence absent".into()),
    }
}
fn binding(state: &GameState, id: ObjectId) -> EffectObjectBinding {
    let o = state.objects.get(id);
    EffectObjectBinding {
        object: id,
        expected_zone: o.zone,
        expected_zone_change_count: o.zone_change_count,
    }
}
fn matches_binding(state: &GameState, b: EffectObjectBinding) -> bool {
    state.objects.try_get(b.object).is_some_and(|o| {
        o.zone == b.expected_zone && o.zone_change_count == b.expected_zone_change_count
    })
}
/// Captured before the actual shared discard, including Madness replacement.
/// No capture exists in an ordinary world without an active printed observer.
pub(super) fn before_discard(
    state: &GameState,
    id: ObjectId,
) -> Option<(AbilitySourceContractV4, Vec<AbilitySourceContractV4>, usize)> {
    let o = state.objects.try_get(id)?;
    if o.zone != Zone::Hand
        || o.v4.is_token
        || o.spell_copy_origin.is_some()
        || !state.players[o.owner.index()].hand.contains(&id)
    {
        return None;
    }
    let observers = state.players[o.owner.opponent().index()]
        .battlefield
        .iter()
        .filter_map(|&source| {
            let c = AbilitySourceContractV4::capture(state, source);
            (c.controller == o.owner.opponent() && active_source_is(state, c)).then_some(c)
        })
        .collect::<Vec<_>>();
    (!observers.is_empty()).then(|| {
        (
            AbilitySourceContractV4::capture(state, id),
            observers,
            state.engine.event_history.len(),
        )
    })
}
fn active_source_is(state: &GameState, c: AbilitySourceContractV4) -> bool {
    c.zone == Zone::Battlefield
        && definition(c.card_def)
        && state.players[c.controller.index()]
            .battlefield
            .contains(&c.source)
        && state.objects.try_get(c.source).is_some_and(|o| {
            o.zone == Zone::Battlefield
                && o.controller == c.controller
                && o.zone_change_count == c.zone_change_count
                && o.v4.face_index == 0
                && o.v4.face_down_v1.is_none()
        })
        && AbilitySourceContractV4::capture(state, c.source) == c
        && crate::continuous_characteristics_v1::printed_abilities_active(state, c.source)
}
pub(super) fn after_discard(
    state: &mut GameState,
    input: Option<(AbilitySourceContractV4, Vec<AbilitySourceContractV4>, usize)>,
) {
    let Some((card, observers, start)) = input else {
        return;
    };
    if state.engine.halted.is_some() {
        return;
    }
    let moves=state.engine.event_history.iter().enumerate().skip(start).filter_map(|(at,e)|matches!(e,CommittedEvent::ZoneChange{object,from:Zone::Hand,..} if *object==card.source).then_some(at)).collect::<Vec<_>>();
    let [at] = moves.as_slice() else {
        state.engine.halted = Some((UnsupportedMechanic::InvalidEffectContinuation, card.source));
        return;
    };
    let outcome = binding(state, card.source);
    let record = DiscardV72 {
        card,
        discarder: card.owner,
        observers: observers.clone(),
        move_history_index: *at as u64,
        outcome,
    };
    let index = push(state, JournalV72::Discarded(record));
    for (observer, source) in observers.into_iter().enumerate() {
        state.engine.pending_triggers.push(PendingTrigger {
            controller: source.controller,
            source: source.source,
            source_contract: Some(source),
            granted_by: None,
            effect: EffectOp::TinybonesStashV72 {
                discard: index,
                observer: observer as u32,
            },
            is_madness_offer: false,
            kicked: false,
            target_spec: TargetSpec::None,
            targets: vec![],
            target_contracts: vec![],
            placement_ordered: false,
            optional_additional_cost_paid: None,
            paid_cost_refs: vec![],
        });
    }
}
fn discard(state: &GameState, index: u64) -> Result<&DiscardV72, String> {
    let JournalV72::Discarded(d) = journal(state, index)? else {
        return Err("Tinybones discard kind changed".into());
    };
    if d.card.zone != Zone::Hand
        || d.card.owner != d.discarder
        || d.card.controller != d.discarder
        || d.card.attached_to.is_some()
        || d.card.source != d.outcome.object
        || d.card.zone_change_count.checked_add(1) != Some(d.outcome.expected_zone_change_count)
        || d.move_history_index >= index
        || d.observers.is_empty()
        || !matches!(state.engine.event_history.get(d.move_history_index as usize),Some(CommittedEvent::ZoneChange{object,from:Zone::Hand,to,..}) if *object==d.card.source && *to==d.outcome.expected_zone)
        || d.observers.iter().enumerate().any(|(i, o)| {
            o.controller != d.discarder.opponent()
                || o.zone != Zone::Battlefield
                || !definition(o.card_def)
                || d.observers[..i].iter().any(|old| old.source == o.source)
        })
    {
        return Err("Tinybones original discard/source/incarnation changed".into());
    }
    if state.engine.event_history.iter().take(index as usize).any(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Discarded(old) if old.move_history_index==d.move_history_index || old.card==d.card))) { return Err("Tinybones original discard captured twice".into()); }
    let live = state
        .objects
        .try_get(d.card.source)
        .ok_or("Tinybones discarded card absent")?;
    if live.owner != d.card.owner || live.zone_change_count < d.outcome.expected_zone_change_count {
        return Err("Tinybones discarded physical owner/generation changed".into());
    }
    validate_historical_ability_source_contract(state, d.card)?;
    for &source in &d.observers {
        validate_historical_ability_source_contract(state, source)?;
    }
    Ok(d)
}
pub(crate) fn trigger_identity(state: &GameState, item: &StackItem) -> Result<(), String> {
    let Some(EffectOp::TinybonesStashV72 {
        discard: index,
        observer,
    }) = item.inline_effect.as_ref()
    else {
        return Ok(());
    };
    let d = discard(state, *index)?;
    let source = *d
        .observers
        .get(*observer as usize)
        .ok_or("Tinybones observer index outside original capture")?;
    if item.kind != StackItemKind::TriggeredAbility
        || item.source != source.source
        || item.controller != source.controller
        || item.v4.ability_source_contract != Some(source)
        || item.is_copy
        || item.madness_offer
        || item.is_flashback
        || item.kicked
        || !item.targets.is_empty()
        || item.v4.granted_by.is_some()
        || item.v4.target_spec.is_some()
        || !item.v4.target_contracts.is_empty()
        || !item.v4.paid_cost_refs.is_empty()
        || item.v4.optional_additional_cost_paid.is_some()
    {
        return Err("Tinybones original trigger row/source/controller changed".into());
    }
    Ok(())
}
pub(super) fn publish(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    let Some(EffectOp::TinybonesStashV72 { discard, observer }) = item.inline_effect.as_ref()
    else {
        return Ok(());
    };
    trigger_identity(state, item)?;
    if state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Published{discard:d,observer:o,..} if d==discard && o==observer))) {return Err("Tinybones discard observer published twice".into());}
    push(
        state,
        JournalV72::Published {
            discard: *discard,
            observer: *observer,
            item: Box::new(item.clone()),
        },
    );
    Ok(())
}
fn publication(state: &GameState, item: &StackItem) -> Result<Option<u64>, String> {
    if !matches!(item.inline_effect, Some(EffectOp::TinybonesStashV72 { .. })) {
        return Ok(None);
    }
    trigger_identity(state, item)?;
    let rows = state
        .engine
        .event_history
        .iter()
        .enumerate()
        .filter_map(|(at, e)| match e {
            CommittedEvent::TinybonesV72 { journal } => match journal.as_ref() {
                JournalV72::Published { item: original, .. }
                    if original.v4.stack_item_id == item.v4.stack_item_id =>
                {
                    Some((at, original))
                }
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(at, original)] = rows.as_slice() else {
        return Err("Tinybones unique original publication absent".into());
    };
    if original.as_ref() != item {
        return Err("Tinybones full published row changed".into());
    }
    Ok(Some(*at as u64))
}
pub(crate) fn waiting(state: &GameState, ctx: &ExecCtx) -> bool {
    state
        .stack
        .iter()
        .chain(
            state
                .engine
                .pending_effect
                .iter()
                .map(|p| &p.resolving_item),
        )
        .any(|row| {
            row.source == ctx.source
                && row
                    .v4
                    .ability_source_contract
                    .is_some_and(|c| definition(c.card_def))
        })
}
pub(crate) fn resolve_stash(
    state: &mut GameState,
    ctx: &ExecCtx,
    item: &StackItem,
) -> Result<(), String> {
    let p = publication(state, item)?.ok_or("Tinybones native row absent")?;
    let Some(EffectOp::TinybonesStashV72 {
        discard: index,
        observer,
    }) = item.inline_effect.as_ref()
    else {
        return Err("Tinybones native root changed".into());
    };
    let d = discard(state, *index)?.clone();
    let source = *d
        .observers
        .get(*observer as usize)
        .ok_or("Tinybones source index")?;
    if ctx.source != source.source
        || ctx.controller != source.controller
        || ctx.ability_source_contract != Some(source)
        || ctx.stack_item_id != Some(item.v4.stack_item_id)
        || state
            .stack
            .iter()
            .any(|r| r.v4.stack_item_id == item.v4.stack_item_id)
    {
        return Err("Tinybones lost lexical resolving row context".into());
    }
    if state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Finished{publication,..} if *publication==p))){return Err("Tinybones trigger already finished".into());}
    // Untargeted original-incarnation lookup. Madness, replacement and later
    // returns do not turn a different graveyard incarnation into this card.
    if d.outcome.expected_zone == Zone::Graveyard
        && matches_binding(state, d.outcome)
        && state.players[d.card.owner.index()]
            .graveyard
            .contains(&d.card.source)
    {
        let start = state.engine.event_history.len();
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change(d.card.source, Zone::Exile),
        );
        if state.engine.halted.is_some() {
            return Err("Tinybones exile halted".into());
        }
        let exiled = binding(state, d.card.source);
        if exiled.expected_zone == Zone::Exile
            && d.outcome.expected_zone_change_count.checked_add(1)
                == Some(exiled.expected_zone_change_count)
        {
            let moves=state.engine.event_history.iter().enumerate().skip(start).filter_map(|(i,e)|matches!(e,CommittedEvent::ZoneChange{object,from:Zone::Graveyard,to:Zone::Exile,..} if *object==d.card.source).then_some(i)).collect::<Vec<_>>();
            let [at] = moves.as_slice() else {
                return Err("Tinybones exact exile event absent".into());
            };
            let after = state
                .objects
                .get(d.card.source)
                .counters
                .stash
                .checked_add(1)
                .ok_or("stash counter overflow")?;
            state.objects.get_mut(d.card.source).counters.stash = after;
            push(
                state,
                JournalV72::Stashed {
                    publication: p,
                    move_history_index: *at as u64,
                    exiled,
                    after,
                },
            );
        }
    }
    push(
        state,
        JournalV72::Finished {
            publication: p,
            countered: false,
        },
    );
    Ok(())
}
pub(super) fn countered(state: &mut GameState, item: &StackItem) -> Result<(), String> {
    if let Some(p) = publication(state, item)? {
        push(
            state,
            JournalV72::Finished {
                publication: p,
                countered: true,
            },
        );
    }
    Ok(())
}
pub(crate) fn play_allowed(state: &GameState, card: ObjectId, actor: PlayerId) -> bool {
    actor == state.active_player
        && state.objects.try_get(card).is_some_and(|o| {
            o.zone == Zone::Exile
                && o.owner != actor
                && !o.v4.is_token
                && o.spell_copy_origin.is_none()
                && o.counters.stash > 0
                && stashed_incarnation(state, card, o.zone_change_count, o.counters.stash)
                && state.exile.contains(&card)
        })
        && active_source(state, actor).is_some()
}
pub(super) fn begin_play(
    state: &mut GameState,
    card: ObjectId,
    actor: PlayerId,
    land: bool,
) -> Option<u64> {
    if !play_allowed(state, card, actor) {
        return None;
    }
    let o = state.objects.get(card);
    let input = PlayV72 {
        card: AbilitySourceContractV4::capture(state, card),
        stash: o.counters.stash,
        source: active_source(state, actor)?,
        actor,
        turn: state.turn_key_v1(),
        land,
    };
    Some(push(state, JournalV72::Play(input)))
}
fn play(state: &GameState, index: u64) -> Result<PlayV72, String> {
    let JournalV72::Play(p) = journal(state, index)? else {
        return Err("Tinybones original play kind changed".into());
    };
    if p.card.zone != Zone::Exile
        || p.card.owner == p.actor
        || p.source.zone != Zone::Battlefield
        || p.source.controller != p.actor
        || !definition(p.source.card_def)
        || p.stash <= 0
        || card_def::CARD_DEFS
            .get(p.card.card_def as usize)
            .is_none_or(|d| d.is_playable_land() != p.land || !d.is_executable())
    {
        return Err("Tinybones original play/source/physical owner changed".into());
    }
    if !stashed_incarnation(state, p.card.source, p.card.zone_change_count, p.stash) {
        return Err("Tinybones play lost actual stash exile receipt".into());
    }
    let actual_turn = state
        .engine
        .event_history
        .iter()
        .take(index as usize)
        .rev()
        .find_map(|e| match e {
            CommittedEvent::LifeLossTurnBeganV1 {
                turn,
                active_player,
            } => Some((*turn, *active_player)),
            _ => None,
        })
        .ok_or("Tinybones play lost original native turn boundary")?;
    if actual_turn != (p.turn, p.actor) {
        return Err("Tinybones play was not the source controller's original turn".into());
    }
    validate_historical_ability_source_contract(state, p.card)?;
    validate_historical_ability_source_contract(state, p.source)?;
    Ok(*p)
}
fn route(source: StackSourceContractV4) -> Option<u64> {
    match source.spell_cast_origin?.route {
        SpellCastRouteV4::TinybonesExileV72 { play_history_index } => Some(play_history_index),
        _ => None,
    }
}
pub(super) fn announced(state: &mut GameState, source: StackSourceContractV4, id: StackItemId) {
    if let Some(play) = route(source) {
        push(
            state,
            JournalV72::Announced {
                play,
                source,
                stack_item_id: id,
            },
        );
    }
}
pub(super) fn paid(state: &mut GameState, source: StackSourceContractV4, id: StackItemId) {
    if let Some(play) = route(source) {
        push(
            state,
            JournalV72::Paid {
                play,
                source,
                stack_item_id: id,
            },
        );
    }
}
pub(super) fn record_land(state: &mut GameState, input: Option<u64>, id: ObjectId) {
    if let Some(play) = input {
        let source = AbilitySourceContractV4::capture(state, id);
        push(state, JournalV72::Land { play, source });
    }
}
pub(super) fn aborted(state: &mut GameState, p: &PendingCast) {
    if let Some(play) = route(p.source_contract) {
        let returned = binding(state, p.spell);
        push(state, JournalV72::Aborted { play, returned });
    } else if let Some(origin) = p
        .source_contract
        .spell_cast_origin
        .filter(|o| o.origin_zone == Zone::Exile)
    {
        let returned = binding(state, p.spell);
        let stash = state.objects.get(p.spell).counters.stash;
        if returned.expected_zone == Zone::Exile
            && stash > 0
            && state
                .counter_lki_for(p.spell, origin.origin_zone_change_count)
                .is_some_and(|c| c.stash == stash)
        {
            push(
                state,
                JournalV72::IndependentStashAborted {
                    original: p.source_contract,
                    returned,
                    stash,
                },
            );
        }
    }
    if let Some(origin) = p
        .source_contract
        .spell_cast_origin
        .filter(|o| o.route == SpellCastRouteV4::AdventureExile)
    {
        if let Some((permission, _)) =
            adventure_permission(state, p.spell, origin.origin_zone_change_count)
        {
            let returned = binding(state, p.spell);
            push(
                state,
                JournalV72::AdventureAborted {
                    permission,
                    original: p.source_contract,
                    returned,
                },
            );
        }
    }
}
pub(super) fn validate_contract(
    state: &GameState,
    source: StackSourceContractV4,
) -> Result<(), String> {
    let Some(index) = route(source) else {
        return Ok(());
    };
    let p = play(state, index)?;
    let origin = source.spell_cast_origin.ok_or("Tinybones origin absent")?;
    if p.land
        || source.source != p.card.source
        || source.owner != p.card.owner
        || source.card_def != p.card.card_def
        || source.controller != p.actor
        || source.zone != Zone::Stack
        || p.card.zone_change_count.checked_add(1) != Some(source.zone_change_count)
        || origin.origin_zone != Zone::Exile
        || origin.origin_zone_change_count != p.card.zone_change_count
        || !matches!(
            source.cast_method,
            CastMethodV4::Normal
                | CastMethodV4::Alternative
                | CastMethodV4::Omen
                | CastMethodV4::Bestow
        )
        || source.spell_copy_origin.is_some()
    {
        return Err("Tinybones cast changed original exile card/form/controller".into());
    }
    let lki = state
        .counter_lki_for(source.source, p.card.zone_change_count)
        .ok_or("Tinybones original exile stash LKI absent")?;
    if lki.stash != p.stash {
        return Err("Tinybones original exile stash changed".into());
    }
    Ok(())
}
pub(super) fn validate_pending(state: &GameState, p: &PendingCast) -> Result<(), String> {
    if route(p.source_contract).is_none() {
        return Ok(());
    }
    validate_contract(state, p.source_contract)?;
    let index = route(p.source_contract).unwrap();
    let announcements = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::TinybonesV72 { journal } => match journal.as_ref() {
                JournalV72::Announced {
                    play,
                    source,
                    stack_item_id,
                } if *play == index => Some((*source, *stack_item_id)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let [(source, id)] = announcements.as_slice() else {
        return Err("Tinybones pending announcement absent or duplicated".into());
    };
    if *source != p.source_contract
        || p.controller != source.controller
        || p.spell != source.source
        || !state
            .stack
            .iter()
            .any(|r| r.v4.stack_item_id == *id && r.v4.source_contract == Some(*source))
    {
        return Err("Tinybones original pending cast changed".into());
    }
    Ok(())
}
pub(super) fn validate_finalized(
    state: &GameState,
    item: &StackItem,
    pending: bool,
) -> Result<(), String> {
    if item.is_copy {
        return Ok(());
    }
    let Some(source) = item.v4.source_contract else {
        return Ok(());
    };
    let Some(index) = route(source) else {
        return Ok(());
    };
    validate_contract(state, source)?;
    if pending {
        return Ok(());
    }
    let paid = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::TinybonesV72 { journal } => match journal.as_ref() {
                JournalV72::Paid {
                    play,
                    source,
                    stack_item_id,
                } if *play == index => Some((*source, *stack_item_id)),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    if paid != [(source, item.v4.stack_item_id)] {
        return Err("Tinybones finalized cast lost exact paid receipt".into());
    }
    Ok(())
}
fn stashed_incarnation(state: &GameState, id: ObjectId, generation: u32, count: i32) -> bool {
    // Native illegal-cast reversal restores the original counters, under a new
    // physical generation. Derive those rebinding links in actual history order.
    let mut known = Vec::<(EffectObjectBinding, i32)>::new();
    let mut plays = Vec::<(u64, PlayV72)>::new();
    for (at, e) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::TinybonesV72 { journal } = e else {
            continue;
        };
        match journal.as_ref() {
            JournalV72::Stashed { exiled, after, .. } => known.push((*exiled, *after)),
            JournalV72::Play(p)
                if known.iter().any(|(b, n)| {
                    b.object == p.card.source
                        && b.expected_zone == Zone::Exile
                        && b.expected_zone_change_count == p.card.zone_change_count
                        && *n == p.stash
                }) =>
            {
                plays.push((at as u64, *p))
            }
            JournalV72::IndependentStashAborted {
                original,
                returned,
                stash,
            } => {
                if let Some(origin) = original.spell_cast_origin {
                    if known.iter().any(|(b, n)| {
                        b.object == original.source
                            && b.expected_zone == Zone::Exile
                            && b.expected_zone_change_count == origin.origin_zone_change_count
                            && n == stash
                    }) && original.zone_change_count.checked_add(1)
                        == Some(returned.expected_zone_change_count)
                        && returned.object == original.source
                        && returned.expected_zone == Zone::Exile
                    {
                        known.push((*returned, *stash));
                    }
                }
            }
            JournalV72::Aborted { play, returned } => {
                if let Some((_, p)) = plays.iter().find(|(at, _)| at == play) {
                    if returned.object == p.card.source
                        && returned.expected_zone == Zone::Exile
                        && p.card.zone_change_count.checked_add(2)
                            == Some(returned.expected_zone_change_count)
                    {
                        known.push((*returned, p.stash));
                    }
                }
            }
            _ => {}
        }
    }
    known.iter().any(|(b, n)| {
        b.object == id
            && b.expected_zone == Zone::Exile
            && b.expected_zone_change_count == generation
            && *n == count
    })
}

pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.objects.iter().any(|(id, o)| {
        o.counters.stash < 0
            || o.zone == Zone::Exile
                && o.counters.stash > 0
                && !stashed_incarnation(state, id, o.zone_change_count, o.counters.stash)
    }) {
        return Err("Tinybones stash has no actual native exile occurrence".into());
    }
    for (at, entry) in state.counter_lki_v1.iter().flatten().enumerate() {
        if entry.counters.stash != 0
            && (entry.counters.stash < 0
                || state
                    .counter_lki_v1
                    .iter()
                    .flatten()
                    .take(at)
                    .any(|old| old.source == entry.source))
        {
            return Err("Tinybones original stash LKI missing, duplicated or forged".into());
        }
    }
    if state
        .engine
        .event_log
        .iter()
        .any(|e| matches!(e, CommittedEvent::TinybonesV72 { .. }))
    {
        return Err("Tinybones private custody entered trigger log".into());
    }
    for (at, e) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::TinybonesV72 { journal: j } = e else {
            continue;
        };
        match j.as_ref() {
            JournalV72::ForeignAdventure { .. } | JournalV72::AdventureAborted { .. } => {
                let (b, _) = adventure_binding(state, at as u64)?;
                if state.engine.event_history.iter().take(at).enumerate().any(|(i,e)|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::ForeignAdventure{..}|JournalV72::AdventureAborted{..})) && adventure_binding(state,i as u64).is_ok_and(|(old,_)|old==b)) {
                    return Err("foreign Adventure permission duplicated".into());
                }
                let live = state.objects.get(b.object);
                if live.zone_change_count < b.expected_zone_change_count
                    || (live.zone_change_count == b.expected_zone_change_count
                        && (live.zone != Zone::Exile || !live.v4.on_adventure))
                {
                    return Err("foreign Adventure exile incarnation changed".into());
                }
            }
            JournalV72::Discarded(_) => {
                let d = discard(state, at as u64)?;
                for observer in 0..d.observers.len() {
                    let pending = state
                        .engine
                        .pending_triggers
                        .iter()
                        .filter(|p| {
                            p.effect
                                == EffectOp::TinybonesStashV72 {
                                    discard: at as u64,
                                    observer: observer as u32,
                                }
                        })
                        .count();
                    let published=state.engine.event_history.iter().filter(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Published{discard,observer:o,..}if *discard==at as u64 && *o==observer as u32))).count();
                    if pending + published != 1 {
                        return Err(
                            "Tinybones original pending/publication inventory changed".into()
                        );
                    }
                }
            }
            JournalV72::Published {
                discard: d,
                observer,
                item,
            } => {
                if *d >= at as u64
                    || item.v4.stack_item_id == StackItemId::default()
                    || item.inline_effect
                        != Some(EffectOp::TinybonesStashV72 {
                            discard: *d,
                            observer: *observer,
                        })
                {
                    return Err("Tinybones publication original index changed".into());
                }
                trigger_identity(state, item)?;
                publication(state, item)?;
                let finished = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| match e {
                        CommittedEvent::TinybonesV72 { journal } => match journal.as_ref() {
                            JournalV72::Finished { publication, .. }
                                if *publication == at as u64 =>
                            {
                                Some(i)
                            }
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if finished.len() > 1 || finished.first().is_some_and(|i| *i <= at) {
                    return Err("Tinybones terminal duplicated/reordered".into());
                }
                let live = state
                    .stack
                    .iter()
                    .chain(
                        state
                            .engine
                            .pending_effect
                            .iter()
                            .map(|p| &p.resolving_item),
                    )
                    .filter(|r| r.v4.stack_item_id == item.v4.stack_item_id)
                    .collect::<Vec<_>>();
                if finished.is_empty() {
                    if live.len() != 1 || *live[0] != **item {
                        if !time_stop_v1::disposed(state, item)? {
                            return Err("Tinybones original publication erased or altered".into());
                        }
                    }
                } else if !live.is_empty() {
                    return Err("Tinybones finished publication remains live".into());
                }
            }
            JournalV72::Finished {
                publication: p,
                countered,
            } => {
                if !countered {
                    let JournalV72::Published { discard: d, .. } = journal(state, *p)? else {
                        return Err("Tinybones finish lost original publication".into());
                    };
                    let d = discard(state, *d)?;
                    let stashed=state.engine.event_history.iter().take(at).any(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Stashed{publication,..}if publication==p)));
                    if !stashed
                        && d.outcome.expected_zone == Zone::Graveyard
                        && matches_binding(state, d.outcome)
                    {
                        return Err(
                            "Tinybones finish omitted its still-eligible discarded card".into()
                        );
                    }
                }
                if *p >= at as u64 || !matches!(journal(state, *p)?, JournalV72::Published { .. }) {
                    return Err("Tinybones terminal has no original row".into());
                }
            }
            JournalV72::Stashed {
                publication: p,
                move_history_index,
                exiled,
                after,
            } => {
                let JournalV72::Published { discard: d, .. } = journal(state, *p)? else {
                    return Err("Tinybones stash publication absent".into());
                };
                let original = discard(state, *d)?;
                if state.engine.event_history.iter().take(at).any(|e|matches!(e,CommittedEvent::TinybonesV72{journal} if matches!(journal.as_ref(),JournalV72::Stashed{publication,..}if publication==p))){return Err("Tinybones stash output duplicated".into());}
                if *after != 1
                    || (matches_binding(state, *exiled)
                        && state.objects.get(exiled.object).counters.stash != *after)
                    || *p >= *move_history_index
                    || *move_history_index >= at as u64
                    || exiled.object != original.card.source
                    || exiled.expected_zone != Zone::Exile
                    || original.outcome.expected_zone != Zone::Graveyard
                    || original.outcome.expected_zone_change_count.checked_add(1)
                        != Some(exiled.expected_zone_change_count)
                    || !matches!(state.engine.event_history.get(*move_history_index as usize),Some(CommittedEvent::ZoneChange{object,from:Zone::Graveyard,to:Zone::Exile,..})if *object==exiled.object)
                {
                    return Err("Tinybones stash lost exact native exile move".into());
                }
            }
            JournalV72::Play(_) => {
                let input = play(state, at as u64)?;
                let mut announced = Vec::new();
                let mut paid = 0usize;
                let mut aborted = 0usize;
                let mut lands = 0usize;
                for (after, event) in state.engine.event_history.iter().enumerate() {
                    let CommittedEvent::TinybonesV72 { journal } = event else {
                        continue;
                    };
                    match journal.as_ref() {
                        JournalV72::Announced {
                            play,
                            source,
                            stack_item_id,
                        } if *play == at as u64 => {
                            if after <= at {
                                return Err("Tinybones announcement precedes permission".into());
                            }
                            announced.push((*source, *stack_item_id));
                        }
                        JournalV72::Paid { play, .. } if *play == at as u64 => {
                            if after <= at {
                                return Err("Tinybones payment precedes permission".into());
                            }
                            paid += 1;
                        }
                        JournalV72::Aborted { play, .. } if *play == at as u64 => aborted += 1,
                        JournalV72::Land { play, .. } if *play == at as u64 => lands += 1,
                        _ => {}
                    }
                }
                if input.land {
                    if lands != 1 || !announced.is_empty() || paid != 0 || aborted != 0 {
                        return Err("Tinybones original land permission inventory changed".into());
                    }
                } else {
                    if announced.len() != 1 || lands != 0 || paid + aborted > 1 {
                        return Err("Tinybones original cast permission inventory changed".into());
                    }
                    let (source, id) = announced[0];
                    if paid + aborted == 0
                        && state.engine.pending_cast.as_ref().is_none_or(|p| {
                            p.source_contract != source
                                || !state.stack.iter().any(|r| {
                                    r.v4.stack_item_id == id && r.v4.source_contract == Some(source)
                                })
                        })
                    {
                        return Err("Tinybones pending cast owner erased".into());
                    }
                    if paid + aborted != 0
                        && state
                            .engine
                            .pending_cast
                            .as_ref()
                            .is_some_and(|p| route(p.source_contract) == Some(at as u64))
                    {
                        return Err("Tinybones completed cast regained pending owner".into());
                    }
                }
            }
            JournalV72::Announced {
                play: p, source, ..
            }
            | JournalV72::Paid {
                play: p, source, ..
            } => {
                if *p >= at as u64 || route(*source) != Some(*p) {
                    return Err("Tinybones cast receipt reordered/foreign".into());
                }
                validate_contract(state, *source)?;
            }
            JournalV72::Land { play: p, source } => {
                let before = play(state, *p)?;
                if *p >= at as u64
                    || !before.land
                    || before.card.source != source.source
                    || before.card.owner != source.owner
                    || before.card.card_def != source.card_def
                    || source.controller != before.actor
                    || source.zone != Zone::Battlefield
                    || before.card.zone_change_count.checked_add(1)
                        != Some(source.zone_change_count)
                {
                    return Err("Tinybones land changed actual owner/controller/incarnation".into());
                }
            }
            JournalV72::IndependentStashAborted {
                original,
                returned,
                stash,
            } => {
                let origin = original
                    .spell_cast_origin
                    .ok_or("independent stash reversal origin absent")?;
                let live = state
                    .objects
                    .try_get(original.source)
                    .ok_or("independent stash reversal card absent")?;
                let original_route = match origin.route {
                    SpellCastRouteV4::ExilePermission {
                        holder,
                        permission_zone_change_count,
                    }
                    | SpellCastRouteV4::ExileFreePermission {
                        holder,
                        permission_zone_change_count,
                    } => {
                        holder == original.controller
                            && permission_zone_change_count == origin.origin_zone_change_count
                    }
                    SpellCastRouteV4::ResolvingEffectV1 { holder, .. } => {
                        holder == original.controller
                    }
                    SpellCastRouteV4::AdventureExile => adventure_contract(state, *original),
                    SpellCastRouteV4::Plotted { .. } => {
                        original.owner == original.controller
                            && original.cast_method == CastMethodV4::Plotted
                    }
                    _ => false,
                };
                if !original_route || origin.origin_zone!=Zone::Exile || original.zone!=Zone::Stack
                    || original.spell_copy_origin.is_some() || original.owner!=live.owner || original.card_def!=live.card_def
                    || origin.origin_zone_change_count.checked_add(1)!=Some(original.zone_change_count)
                    || original.zone_change_count.checked_add(1)!=Some(returned.expected_zone_change_count)
                    || returned.object!=original.source || returned.expected_zone!=Zone::Exile || *stash<=0
                    || !stashed_incarnation(state,original.source,origin.origin_zone_change_count,*stash)
                    || state.counter_lki_for(original.source,origin.origin_zone_change_count).is_none_or(|c|c.stash!=*stash)
                    || state.engine.event_history.iter().take(at).any(|e| matches!(e,CommittedEvent::TinybonesV72{journal}if matches!(journal.as_ref(),JournalV72::IndependentStashAborted{original:old,..} if old.source==original.source && old.zone_change_count==original.zone_change_count)))
                {return Err("independent stash reversal changed original owner/route/incarnation/counters".into());}
            }
            JournalV72::Aborted { play: p, returned } => {
                let before = play(state, *p)?;
                if *p >= at as u64
                    || returned.object != before.card.source
                    || returned.expected_zone != Zone::Exile
                    || before.card.zone_change_count.checked_add(2)
                        != Some(returned.expected_zone_change_count)
                {
                    return Err("Tinybones native abort changed original exile card".into());
                }
            }
        }
    }
    for p in &state.engine.pending_triggers {
        if let EffectOp::TinybonesStashV72 {
            discard: index,
            observer,
        } = &p.effect
        {
            let d = discard(state, *index)?;
            let source = *d
                .observers
                .get(*observer as usize)
                .ok_or("Tinybones pending observer index")?;
            if p.source_contract != Some(source)
                || p.source != source.source
                || p.controller != source.controller
                || p.is_madness_offer
                || p.granted_by.is_some()
                || !p.targets.is_empty()
                || p.target_spec != TargetSpec::None
                || p.kicked
                || p.optional_additional_cost_paid.is_some()
                || !p.paid_cost_refs.is_empty()
            {
                return Err("Tinybones pending native trigger changed".into());
            }
        }
    }
    if let Some(p) = &state.engine.pending_cast {
        validate_pending(state, p)?;
    }
    for row in &state.stack {
        if matches!(row.inline_effect, Some(EffectOp::TinybonesStashV72 { .. })) {
            publication(state, row)?;
        }
    }
    Ok(())
}
pub(crate) fn require_legacy(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    if state.engine.event_history.has_any(&[EventKindV1::TinybonesV72]) || state.objects.iter().any(|(id,o)|crate::optional_trigger_public_v1::reference(state,id,viewer).is_some_and(|r|matches!(r.identity,crate::optional_trigger_public_v1::IdentityV1::Known{card_id}if definition(card_id))) || viewer.is_some_and(|v|o.v4.face_down_v1.is_some_and(|f|f.lookers&(1<<v.index())!=0)&&definition(o.card_def))) {Err("Tinybones requires a distinct complete V72 consumer".into())}else{Ok(())}
}

pub(crate) fn trigger_recipe() -> EffectOp {
    EffectOp::TinybonesStashV72 {
        discard: u64::MAX,
        observer: u32::MAX,
    }
}
pub(crate) static TRIGGERS: [trigger::TriggeredAbilityDef; 1] = [trigger::TriggeredAbilityDef {
    condition: trigger::TriggerCondition::OpponentDiscardsV72,
    home_zone: Zone::Battlefield,
    intervening_if_kicked: false,
    intervening_if_controls_another_source_card: false,
    face_index: 0,
    effect: trigger_recipe,
}];

pub(crate) fn activation() -> EffectOp {
    EffectOp::DiscardCards {
        player: effect::PlayerRef::Opponent,
        count: 1,
    }
}
fn activation_matches(a: &ActivatedAbilityDef) -> bool {
    a.cost
        == [
            CostComponent::Mana(Cost {
                pips: &[mana::Pip::Colored(ManaColor::B)],
                generic: 3,
                x_count: 0,
            }),
            CostComponent::Tap,
        ]
        && (a.effect)() == activation()
        && a.activation_zone == Zone::Battlefield
        && a.sorcery_speed_only
        && a.target_spec == TargetSpec::None
        && a.activation_target_filter == ActivationTargetFilter::TargetSpecOnly
        && a.max_activations_per_turn.is_none()
        && a.face.is_none()
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "tinybones_v72_tests.rs"]
mod tests;
