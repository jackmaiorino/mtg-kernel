//! Rules shared by the Standard legendary creatures. Persistent grants retain
//! exact battlefield incarnations; continuously changing bonuses read the board.
use crate::card_def::{CardType, Keywords, Supertype, CARD_DEFS};
use crate::effect::{EffectObjectBinding, EffectOp, ExecCtx};
use crate::ids::ObjectId;
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};
pub mod jodah;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LegendEffectV1 {
    CountersOnControlledCreatures,
    ProtectControlledLegendaryCreatures,
    SourcePowerCountersAndHaste,
    CounterStackTargetAndDraw,
    DestroyTargetAndDraw,
    SkrelvGrant(crate::mana::ManaColor),
    ProtectTargetFromDeath,
    ReturnBoundPermanent(EffectObjectBinding),
    LagrellaExileTargets,
    CountersOnReturnedPermanent(EffectObjectBinding),
    ShannaPayAndDraw,
    BindJodahCast,
    JodahCastSnapshot(jodah::JodahCastV1),
    GwennaMana(crate::mana::ManaColor, crate::mana::ManaColor),
    GwennaCounterAndUntap,
}

pub(crate) fn has_printed_ability(state: &GameState, object: ObjectId, name: &str) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let def = &CARD_DEFS[live.card_def as usize];
    live.zone == Zone::Battlefield
        && def.is_executable()
        && def.name == name
        && crate::continuous_characteristics_v1::printed_abilities_active(state, object)
}

pub(crate) fn legendary_creature(state: &GameState, object: ObjectId) -> bool {
    crate::engine::object_has_type(state, object, CardType::Creature)
        && crate::engine::effective_supertypes(state, object).contains(&Supertype::Legendary)
}

pub(crate) fn jodah_bonus(state: &GameState, recipient: ObjectId) -> i32 {
    let object = state.objects.get(recipient);
    if object.zone != Zone::Battlefield || !legendary_creature(state, recipient) {
        return 0;
    }
    let battlefield = &state.players[object.controller.index()].battlefield;
    let sources = battlefield
        .iter()
        .filter(|&&id| has_printed_ability(state, id, "Jodah, the Unifier"))
        .count();
    let count = battlefield
        .iter()
        .filter(|&&id| legendary_creature(state, id))
        .count();
    i32::try_from(sources.saturating_mul(count)).unwrap_or(i32::MAX)
}

fn binding(state: &GameState, object: ObjectId) -> EffectObjectBinding {
    EffectObjectBinding {
        object,
        expected_zone: Zone::Battlefield,
        expected_zone_change_count: state.objects.get(object).zone_change_count,
    }
}

pub(crate) fn execute(op: LegendEffectV1, ctx: &ExecCtx, state: &mut GameState) {
    match op {
        LegendEffectV1::GwennaMana(_, _) => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
        LegendEffectV1::GwennaCounterAndUntap => {
            if ctx.ability_source_contract.is_some_and(|source| {
                state.objects.try_get(source.source).is_some_and(|live| {
                    live.zone == Zone::Battlefield
                        && live.zone_change_count == source.zone_change_count
                })
            }) {
                crate::event::add_plus_one_counters(state, ctx.source, ctx.controller, 1)
                    .expect("live Gwenna");
                crate::event::propose_and_commit(
                    state,
                    crate::event::ProposedEvent::untap(ctx.source),
                );
            }
        }
        LegendEffectV1::ShannaPayAndDraw => {
            state.engine.halted = Some((
                crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                ctx.source,
            ));
        }
        LegendEffectV1::ProtectTargetFromDeath => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || state.objects.get(target).zone != Zone::Battlefield
            {
                return;
            }
            let Some(source) = ctx.ability_source_contract else {
                return;
            };
            state
                .objects
                .get_mut(target)
                .v4
                .melira_protection_v1
                .get_or_insert_with(Vec::new)
                .push(source);
        }
        LegendEffectV1::ReturnBoundPermanent(object) => {
            if state.objects.try_get(object.object).is_some_and(|live| {
                live.zone == Zone::Graveyard
                    && live.zone_change_count == object.expected_zone_change_count
            }) {
                crate::event::propose_and_commit(
                    state,
                    crate::event::ProposedEvent::zone_change(object.object, Zone::Battlefield),
                );
            }
        }
        LegendEffectV1::CountersOnReturnedPermanent(object) => {
            if state.objects.try_get(object.object).is_some_and(|live| {
                live.zone == Zone::Battlefield
                    && live.zone_change_count == object.expected_zone_change_count
            }) {
                crate::event::add_plus_one_counters(state, object.object, ctx.controller, 2)
                    .expect("live returned permanent");
            }
        }
        LegendEffectV1::LagrellaExileTargets => lagrella_exile(ctx, state),
        LegendEffectV1::SkrelvGrant(color) => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || state.objects.get(target).zone != Zone::Battlefield
            {
                return;
            }
            let timestamp = crate::engine::next_timestamp(state);
            state
                .objects
                .get_mut(target)
                .v4
                .skrelv_grants_v1
                .get_or_insert_with(Vec::new)
                .push(SkrelvGrantV1 { color, timestamp });
        }
        LegendEffectV1::BindJodahCast | LegendEffectV1::JodahCastSnapshot(_) => {}
        LegendEffectV1::CounterStackTargetAndDraw => {
            let Some(crate::state::StackTargetContractV4::StackItem {
                stack_item_id,
                controller,
                ..
            }) = ctx.target_contracts.first().copied()
            else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state) {
                return;
            }
            if let Err(_) = crate::engine::counter_stack_item_by_id(state, stack_item_id) {
                state.engine.halted = Some((
                    crate::engine::UnsupportedMechanic::InvalidEffectContinuation,
                    ctx.source,
                ));
                return;
            }
            crate::event::propose_and_commit(state, crate::event::ProposedEvent::draw(controller));
        }
        LegendEffectV1::DestroyTargetAndDraw => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state) {
                return;
            }
            let controller = state.objects.get(target).controller;
            crate::effect::execute(
                &EffectOp::DestroyObject {
                    object: crate::effect::ObjectRef::Target(0),
                },
                ctx,
                state,
            );
            crate::event::propose_and_commit(state, crate::event::ProposedEvent::draw(controller));
        }
        LegendEffectV1::CountersOnControlledCreatures => {
            let creatures: Vec<_> = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| crate::engine::object_has_type(state, id, CardType::Creature))
                .collect();
            for object in creatures {
                crate::event::add_plus_one_counters(state, object, ctx.controller, 1)
                    .expect("live creature");
            }
        }
        LegendEffectV1::ProtectControlledLegendaryCreatures => {
            let creatures: Vec<_> = state.players[ctx.controller.index()]
                .battlefield
                .iter()
                .copied()
                .filter(|&id| legendary_creature(state, id))
                .map(|id| binding(state, id))
                .collect();
            for object in creatures {
                crate::effect::install_temporary_boost(
                    state,
                    object,
                    1,
                    0,
                    Keywords::INDESTRUCTIBLE,
                );
            }
        }
        LegendEffectV1::SourcePowerCountersAndHaste => {
            let Some(Target::Object(target)) = ctx.targets.first().copied() else {
                return;
            };
            if !ctx.target_incarnation_matches(0, state)
                || state.objects.get(target).zone != Zone::Battlefield
            {
                return;
            }
            let power = ctx
                .ability_source_contract
                .and_then(|contract| {
                    let live = state.objects.try_get(contract.source)?;
                    if live.zone == Zone::Battlefield
                        && live.zone_change_count == contract.zone_change_count
                    {
                        Some(crate::engine::effective_power(state, contract.source))
                    } else {
                        state
                            .engine
                            .event_history
                            .iter()
                            .rev()
                            .find_map(|event| match event {
                                crate::event::CommittedEvent::PowerBeforeLeavingBattlefield {
                                    object,
                                    zone_change_count,
                                    power,
                                } if *object == contract.source
                                    && *zone_change_count == contract.zone_change_count =>
                                {
                                    Some(*power)
                                }
                                _ => None,
                            })
                    }
                })
                .unwrap_or(0)
                .max(0);
            crate::event::add_plus_one_counters(state, target, ctx.controller, power)
                .expect("live target");
            crate::effect::execute(
                &EffectOp::GrantKeywordTargetUntilEndOfTurn {
                    object: crate::effect::ObjectRef::Target(0),
                    keyword: Keywords::HASTE,
                },
                ctx,
                state,
            );
        }
    }
}

/// Katilda grants a separate mana ability even to Humans whose own printed
/// abilities have been removed, provided her grant has the later timestamp.
pub(crate) fn katilda_mana_colors(
    state: &GameState,
    object: ObjectId,
) -> Vec<crate::mana::ManaColor> {
    use crate::mana::ManaColor;
    let Some(live) = state.objects.try_get(object) else {
        return Vec::new();
    };
    if live.zone != Zone::Battlefield
        || !crate::engine::object_has_type(state, object, CardType::Creature)
        || !crate::engine::has_effective_subtype(state, object, crate::card_def::Subtype::Human)
    {
        return Vec::new();
    }
    let granted = state.players[live.controller.index()]
        .battlefield
        .iter()
        .any(|&source| {
            has_printed_ability(state, source, "Katilda, Dawnhart Prime")
                && crate::continuous_characteristics_v1::grant_survives(
                    state,
                    object,
                    state.objects.get(source).v4.layer_timestamp.unwrap_or(0),
                )
        });
    if !granted {
        return Vec::new();
    }
    let mask = crate::engine::object_color_mask(state, object);
    [
        ManaColor::W,
        ManaColor::U,
        ManaColor::B,
        ManaColor::R,
        ManaColor::G,
    ]
    .into_iter()
    .filter(|&color| mask & crate::card_def::mana_color_mask(color) != 0)
    .collect()
}

pub(crate) fn katilda_protected_from(
    state: &GameState,
    target: ObjectId,
    source: ObjectId,
) -> bool {
    has_printed_ability(state, target, "Katilda, Dawnhart Prime")
        && crate::engine::has_effective_subtype(state, source, crate::card_def::Subtype::Werewolf)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SkrelvGrantV1 {
    pub color: crate::mana::ManaColor,
    pub timestamp: u64,
}

/// All of Skrelv's grants are public. Each resolution adds a separate toxic
/// instance; colored hexproof only restricts opponents' sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LegendCharacteristicsV1 {
    pub toxic: u16,
    pub hexproof_color_mask: u8,
    pub cant_be_blocked_by_color_mask: u8,
    pub protection_from_werewolves: bool,
}
pub fn characteristics(state: &GameState, id: ObjectId) -> Option<LegendCharacteristicsV1> {
    let live = state.objects.try_get(id)?;
    if live.zone != Zone::Battlefield {
        return None;
    }
    let mut toxic = u16::from(crate::engine::has_effective_keyword(
        state,
        id,
        Keywords::TOXIC_1,
    ));
    let mut colors = 0;
    for grant in live.v4.skrelv_grants_v1.iter().flatten() {
        // Losing abilities removes toxic and hexproof; the blocking
        // restriction changes combat rules and survives ability removal.
        if crate::continuous_characteristics_v1::grant_survives(state, id, grant.timestamp) {
            toxic = toxic.saturating_add(1);
            colors |= crate::card_def::mana_color_mask(grant.color);
        }
    }
    let cant_be_blocked_by_color_mask = live
        .v4
        .skrelv_grants_v1
        .iter()
        .flatten()
        .fold(0, |mask, grant| {
            mask | crate::card_def::mana_color_mask(grant.color)
        });
    let result = LegendCharacteristicsV1 {
        toxic,
        hexproof_color_mask: colors,
        cant_be_blocked_by_color_mask,
        protection_from_werewolves: has_printed_ability(state, id, "Katilda, Dawnhart Prime"),
    };
    (toxic != 0
        || colors != 0
        || cant_be_blocked_by_color_mask != 0
        || result.protection_from_werewolves)
        .then_some(result)
}
pub fn skrelv_choice() -> EffectOp {
    EffectOp::Choice {
        controller: crate::effect::PlayerRef::Controller,
        options: crate::mana::ManaColor::ALL[..5]
            .iter()
            .copied()
            .map(|color| EffectOp::StandardLegendV1(LegendEffectV1::SkrelvGrant(color)))
            .collect(),
    }
}
pub(crate) fn hexproof_from_color(
    state: &GameState,
    target: ObjectId,
    source: ObjectId,
    source_card_def: u16,
    source_generation: u32,
    targeter: crate::ids::PlayerId,
) -> bool {
    if state.objects.get(target).controller == targeter {
        return false;
    }
    let Some(characteristics) = characteristics(state, target) else {
        return false;
    };
    let source_colors = state
        .objects
        .try_get(source)
        .filter(|live| live.zone_change_count == source_generation)
        .map_or_else(
            || crate::card_def::mana_colors_mask(CARD_DEFS[source_card_def as usize].colors),
            |_| crate::engine::object_color_mask(state, source),
        );
    characteristics.hexproof_color_mask & source_colors != 0
}
pub(crate) fn skrelv_blocks(state: &GameState, attacker: ObjectId, blocker: ObjectId) -> bool {
    characteristics(state, attacker).is_some_and(|c| {
        c.cant_be_blocked_by_color_mask & crate::engine::object_color_mask(state, blocker) != 0
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PoisonPreventionV1(pub bool);
impl PoisonPreventionV1 {
    pub fn is_false(&self) -> bool {
        !self.0
    }
}
impl std::hash::Hash for PoisonPreventionV1 {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if self.0 {
            std::hash::Hash::hash(&"poison_prevention/v1", state);
        }
    }
}
pub(crate) fn give_poison(
    state: &mut GameState,
    placer: crate::ids::PlayerId,
    player: crate::ids::PlayerId,
    amount: u16,
) {
    if amount == 0 || state.players[player.index()].poison_prevention_v1.0 {
        return;
    }
    let amount = crate::standard_cards_v1::scale_counters(state, placer, i32::from(amount))
        .clamp(0, i32::from(u16::MAX)) as u16;
    let melira = state.players[player.index()]
        .battlefield
        .iter()
        .any(|&id| has_printed_ability(state, id, "Melira, the Living Cure"));
    let amount = if melira {
        state.players[player.index()].poison_prevention_v1.0 = true;
        1
    } else {
        amount
    };
    state.players[player.index()].poison_counters.0 = state.players[player.index()]
        .poison_counters
        .0
        .saturating_add(amount);
}
fn queue_delayed(
    state: &mut GameState,
    source: crate::state::AbilitySourceContractV4,
    effect: LegendEffectV1,
) {
    state
        .legend_pending_v1
        .get_or_insert_with(Vec::new)
        .push(crate::trigger::PendingTrigger {
            controller: source.controller,
            source: source.source,
            effect: EffectOp::StandardLegendV1(effect),
            is_madness_offer: false,
            kicked: false,
            target_spec: crate::card_def::TargetSpec::None,
            targets: vec![],
            target_contracts: vec![],
            placement_ordered: false,
            source_contract: Some(source),
            granted_by: None,
            optional_additional_cost_paid: None,
            paid_cost_refs: vec![],
        });
}
/// Snapshot the protected incarnation before its zone-change reset. Replacement
/// effects have already chosen the destination at this point.
pub(crate) fn before_zone_change(state: &mut GameState, object: ObjectId, to: Zone) {
    if state.objects.get(object).zone != Zone::Battlefield || to != Zone::Graveyard {
        return;
    }
    let live = state.objects.get(object);
    let target = EffectObjectBinding {
        object,
        expected_zone: Zone::Graveyard,
        expected_zone_change_count: live.zone_change_count + 1,
    };
    let grants = live.v4.melira_protection_v1.clone().unwrap_or_default();
    for source in grants {
        queue_delayed(state, source, LegendEffectV1::ReturnBoundPermanent(target));
    }
}
fn lagrella_exile(ctx: &ExecCtx, state: &mut GameState) {
    let Some(source) = ctx.ability_source_contract else {
        return;
    };
    if !state.objects.try_get(source.source).is_some_and(|live| {
        live.zone == Zone::Battlefield && live.zone_change_count == source.zone_change_count
    }) {
        return;
    }
    let mut selected = Vec::new();
    for (index, target) in ctx.targets.iter().enumerate() {
        let Target::Object(id) = *target else {
            continue;
        };
        if !ctx.target_incarnation_matches(index, state)
            || state.objects.get(id).zone != Zone::Battlefield
            || !crate::engine::object_has_type(state, id, CardType::Creature)
        {
            continue;
        }
        let controller = state.objects.get(id).controller;
        // Each target's distinct-controller restriction is rechecked against
        // every other still-existing announced target, independently.
        if ctx.targets.iter().enumerate().any(|(other,target)| other!=index && matches!(target,Target::Object(other_id)
            if ctx.target_incarnation_matches(other,state) && state.objects.get(*other_id).zone==Zone::Battlefield
                && state.objects.get(*other_id).controller==controller)) { continue }
        selected.push((
            id,
            state.objects.get(id).card_def,
            state.objects.get(id).owner,
        ));
    }
    crate::event::propose_and_commit_batch(
        state,
        selected
            .iter()
            .map(|(id, _, _)| crate::event::ProposedEvent::zone_change(*id, Zone::Exile))
            .collect(),
    );
    for (id, card_def, owner) in selected {
        let live = state.objects.get(id);
        if live.zone != Zone::Exile {
            continue;
        }
        let generation = live.zone_change_count;
        state
            .engine
            .linked_exile_records
            .push(crate::state::LinkedExileRecordV4 {
                source,
                exiled: id,
                exiled_card_def: card_def,
                exiled_owner: owner,
                exiled_zone_change_count: generation,
            });
        state.objects.get_mut(id).v4.exiled_by = Some(crate::state::ObjectLinkV4 {
            object: source.source,
            zone_change_count: source.zone_change_count,
        });
    }
}
/// The duration ends immediately, even if Lagrella's own abilities were lost.
/// Both exiled cards return simultaneously, then counter triggers are queued.
pub(crate) fn return_lagrella_exiles(state: &mut GameState, source_id: ObjectId, generation: u32) {
    let mut records = Vec::new();
    state.engine.linked_exile_records.retain(|record| {
        if record.source.source == source_id
            && record.source.zone_change_count == generation
            && CARD_DEFS[record.source.card_def as usize].name == "Lagrella, the Magpie"
        {
            records.push(*record);
            false
        } else {
            true
        }
    });
    records.retain(|record| {
        state.objects.try_get(record.exiled).is_some_and(|live| {
            live.zone == Zone::Exile && live.zone_change_count == record.exiled_zone_change_count
        })
    });
    crate::event::propose_and_commit_batch(
        state,
        records
            .iter()
            .map(|record| {
                crate::event::ProposedEvent::zone_change(record.exiled, Zone::Battlefield)
            })
            .collect(),
    );
    for record in records {
        let live = state.objects.get(record.exiled);
        if live.zone == Zone::Battlefield
            && live.controller == record.source.controller
            && live.zone_change_count == record.exiled_zone_change_count + 1
        {
            let target = binding(state, record.exiled);
            queue_delayed(
                state,
                record.source,
                LegendEffectV1::CountersOnReturnedPermanent(target),
            );
        }
    }
}

#[cfg(all(test, feature = "standard-magezero-fixtures"))]
mod tests {
    use super::*;
    #[test]
    fn melira_limits_first_poison_event_and_prevention_survives_her_departure() {
        use crate::ids::PlayerId;
        use crate::state::{GameObject, ObjectStateV4};
        let forest = crate::card_def::card_id_by_name("Forest").unwrap();
        let mut state = GameState::new_from_libraries(
            &[forest; 20],
            &[forest; 20],
            |id| CARD_DEFS[id as usize].object_name.into(),
            991,
        );
        let def = crate::card_def::card_id_by_name("Melira, the Living Cure").unwrap();
        let melira = state.objects.push(GameObject {
            card_def: def,
            name: CARD_DEFS[def as usize].object_name.into(),
            owner: PlayerId::P0,
            controller: PlayerId::P0,
            zone: Zone::Battlefield,
            tapped: false,
            summoning_sick: false,
            damage: 0,
            counters: Default::default(),
            attachments: vec![],
            v4: ObjectStateV4::from_card_def(def),
            spell_copy_origin: None,
            plotted_turn: None,
            zone_change_count: 0,
        });
        state.players[0].battlefield.push(melira);
        give_poison(&mut state, PlayerId::P1, PlayerId::P0, 3);
        assert_eq!(state.players[0].poison_counters.0, 1);
        assert!(state.players[0].poison_prevention_v1.0);
        crate::event::propose_and_commit(
            &mut state,
            crate::event::ProposedEvent::zone_change(melira, Zone::Graveyard),
        );
        give_poison(&mut state, PlayerId::P1, PlayerId::P0, 4);
        assert_eq!(state.players[0].poison_counters.0, 1);
        state.players[0].poison_prevention_v1.0 = false;
        give_poison(&mut state, PlayerId::P1, PlayerId::P0, 2);
        assert_eq!(state.players[0].poison_counters.0, 3);
    }
}

/// Fifteen canonical unordered color pairs are definition-owned choices for
/// Gwenna's one mana ability. They resolve immediately, without using stack.
pub(crate) fn activate_gwenna_mana(
    state: &mut GameState,
    player: crate::ids::PlayerId,
    source: ObjectId,
    index: u8,
) -> Result<bool, String> {
    if !has_printed_ability(state, source, "Gwenna, Eyes of Gaea") {
        return Ok(false);
    }
    let live = state.objects.get(source);
    let def = &CARD_DEFS[live.card_def as usize];
    let ability = def
        .activated_abilities
        .get(usize::from(index))
        .ok_or("Gwenna color pair is missing")?;
    let EffectOp::StandardLegendV1(LegendEffectV1::GwennaMana(first, second)) = (ability.effect)()
    else {
        return Err("Gwenna mana choice changed definition".into());
    };
    let link = crate::state::ObjectLinkV4 {
        object: source,
        zone_change_count: live.zone_change_count,
    };
    let card_def = live.card_def;
    crate::event::propose_and_commit(state, crate::event::ProposedEvent::tap(source));
    crate::event::propose_and_commit(
        state,
        crate::event::ProposedEvent::mana_add(player, vec![first, second]),
    );
    for color in [first, second] {
        state.players[player.index()].mana_pool[color.pool_index()] -= 1;
        state.players[player.index()].restricted_mana_pool.0.push(
            crate::mana::RestrictedManaUnitV1 {
                color,
                restriction:
                    crate::card_def::ManaSpendRestrictionDef::CreatureSpellOrCreatureAbility,
                source: link,
                source_card_def: card_def,
            },
        );
    }
    state.engine.priority_passes = [false, false];
    state.engine.mana_ability_activations += 1;
    state.engine.last_mana_ability_activator = Some(player);
    Ok(true)
}
