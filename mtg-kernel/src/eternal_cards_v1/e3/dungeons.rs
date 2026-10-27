//! Venture into the dungeon (309, 701.49) through the three Adventures in the
//! Forgotten Realms dungeons, and Acererak the Archlich.
//!
//! A player's dungeon and room live in `PlayerState::dungeon`, beside the
//! Undercity (dungeon id 1, which only the initiative ventures into). As for
//! the Undercity, a room's ability is a triggered ability whose source is the
//! card whose ability made the player venture, and a dungeon is completed as
//! its bottommost room is entered.

use super::super::{eternal, EternalOpV1};
use super::{
    bind, binding_live, e3_mut, link, offer, printed, E3ChoiceKindV1, E3ChoiceV1, UntilNextTurnV1,
};
use crate::card_def::{card_id_by_name, CardType, TargetSpec};
use crate::effect::{
    EffectContinuation, EffectFrame, EffectObjectBinding, EffectOp, ExecCtx, PlayerRef,
};
use crate::event::{self, ProposedEvent};
use crate::ids::{ObjectId, PlayerId};
use crate::state::{AbilitySourceContractV4, GameState, ObjectLinkV4, Target, Zone};
use serde::{Deserialize, Serialize};

pub(crate) const ACERERAK: &str = "Acererak the Archlich";
pub(crate) const LOST_MINE_OF_PHANDELVER: u16 = 2;
pub(crate) const DUNGEON_OF_THE_MAD_MAGE: u16 = 3;
pub(crate) const TOMB_OF_ANNIHILATION: u16 = 4;
const DUNGEONS: [u16; 3] = [
    LOST_MINE_OF_PHANDELVER,
    DUNGEON_OF_THE_MAD_MAGE,
    TOMB_OF_ANNIHILATION,
];

/// A room ability waiting for the next trigger collection.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomTriggerV1 {
    pub player: PlayerId,
    pub source: AbilitySourceContractV4,
    pub dungeon: u16,
    pub room: u16,
}

/// The rooms a room leads to, by room id (the topmost room is 1). The
/// bottommost room leads nowhere. `None` for an unknown room.
fn leads_to(dungeon: u16, room: u16) -> Option<&'static [u16]> {
    Some(match (dungeon, room) {
        // Lost Mine of Phandelver: Cave Entrance, Goblin Lair, Mine Tunnels,
        // Storeroom, Dark Pool, Fungi Cavern, Temple of Dumathoin.
        (LOST_MINE_OF_PHANDELVER, 1) => &[2, 3],
        (LOST_MINE_OF_PHANDELVER, 2) => &[4, 5],
        (LOST_MINE_OF_PHANDELVER, 3) => &[5, 6],
        (LOST_MINE_OF_PHANDELVER, 4..=6) => &[7],
        (LOST_MINE_OF_PHANDELVER, 7) => &[],
        // Dungeon of the Mad Mage: Yawning Portal, Dungeon Level, Goblin
        // Bazaar, Twisted Caverns, Lost Level, Runestone Caverns, Muiral's
        // Graveyard, Deep Mines, Mad Wizard's Lair.
        (DUNGEON_OF_THE_MAD_MAGE, 1) => &[2],
        (DUNGEON_OF_THE_MAD_MAGE, 2) => &[3, 4],
        (DUNGEON_OF_THE_MAD_MAGE, 3 | 4) => &[5],
        (DUNGEON_OF_THE_MAD_MAGE, 5) => &[6, 7],
        (DUNGEON_OF_THE_MAD_MAGE, 6 | 7) => &[8],
        (DUNGEON_OF_THE_MAD_MAGE, 8) => &[9],
        (DUNGEON_OF_THE_MAD_MAGE, 9) => &[],
        // Tomb of Annihilation: Trapped Entry, Veils of Fear, Oubliette,
        // Sandfall Cell, Cradle of the Death God.
        (TOMB_OF_ANNIHILATION, 1) => &[2, 3],
        (TOMB_OF_ANNIHILATION, 2) => &[4],
        (TOMB_OF_ANNIHILATION, 3 | 4) => &[5],
        (TOMB_OF_ANNIHILATION, 5) => &[],
        _ => return None,
    })
}

/// The rooms `player` may venture into: the topmost room of each dungeon
/// outside one, or the rooms their current room leads to.
fn next_rooms(state: &GameState, player: PlayerId) -> Result<Vec<(u16, u16)>, String> {
    let dungeon = &state.players[player.index()].dungeon;
    match (dungeon.dungeon_id, dungeon.room_id) {
        (None, None) => Ok(DUNGEONS.iter().map(|&dungeon| (dungeon, 1)).collect()),
        (Some(current), Some(room)) if DUNGEONS.contains(&current) => {
            let next = leads_to(current, room).ok_or("dungeon state names an unknown room")?;
            if next.is_empty() {
                return Err("a completed dungeon is still current".to_string());
            }
            Ok(next.iter().map(|&next| (current, next)).collect())
        }
        _ => Err("only the initiative ventures through the Undercity".to_string()),
    }
}

/// "Venture into the dungeon": enter the only next room, or let the
/// controller choose among them.
pub(super) fn venture(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<(), String> {
    let options = next_rooms(state, continuation.ctx.controller)?;
    if let [(dungeon, room)] = options.as_slice() {
        return enter_room(state, &continuation.ctx, *dungeon, *room);
    }
    continuation.frames.push(EffectFrame::Program {
        op: EffectOp::Choice {
            controller: PlayerRef::Controller,
            options: options
                .into_iter()
                .map(|(dungeon, room)| eternal(EternalOpV1::EnterRoomV1 { dungeon, room }))
                .collect(),
        },
        path,
    });
    Ok(())
}

/// Moves the controller's venture marker into `room` of `dungeon` and sets
/// off that room's ability.
pub(super) fn enter_room(
    state: &mut GameState,
    ctx: &ExecCtx,
    dungeon: u16,
    room: u16,
) -> Result<(), String> {
    let player = ctx.controller;
    let source = ctx
        .ability_source_contract
        .ok_or("venture lost the card that made it")?;
    if !next_rooms(state, player)?.contains(&(dungeon, room)) {
        return Err("venture chose a room the marker cannot reach".to_string());
    }
    let bottommost = leads_to(dungeon, room).is_some_and(<[u16]>::is_empty);
    let marker = &mut state.players[player.index()].dungeon;
    if bottommost {
        if let Err(index) = marker.completed_dungeons.binary_search(&dungeon) {
            marker.completed_dungeons.insert(index, dungeon);
        }
        marker.dungeon_id = None;
        marker.room_id = None;
    } else {
        marker.dungeon_id = Some(dungeon);
        marker.room_id = Some(room);
    }
    e3_mut(state).room_triggers.push(RoomTriggerV1 {
        player,
        source,
        dungeon,
        room,
    });
    Ok(())
}

/// Whether `player` has completed Tomb of Annihilation.
fn completed_tomb(state: &GameState, player: PlayerId) -> bool {
    state.players[player.index()]
        .dungeon
        .completed_dungeons
        .contains(&TOMB_OF_ANNIHILATION)
}

pub(super) fn room_ability(dungeon: u16, room: u16) -> EffectOp {
    eternal(EternalOpV1::RoomAbilityV1 { dungeon, room })
}

fn room_of(effect: &EffectOp) -> Option<(u16, u16)> {
    match effect {
        EffectOp::StandardV1(crate::standard_cards_v1::StandardOpV1::EternalV1(
            EternalOpV1::RoomAbilityV1 { dungeon, room },
        )) if leads_to(*dungeon, *room).is_some() => Some((*dungeon, *room)),
        _ => None,
    }
}

/// Storeroom, Fungi Cavern and Twisted Caverns target a creature.
fn room_target_spec(dungeon: u16, room: u16) -> TargetSpec {
    match (dungeon, room) {
        (LOST_MINE_OF_PHANDELVER, 4 | 6) | (DUNGEON_OF_THE_MAD_MAGE, 4) => TargetSpec::Creature,
        _ => TargetSpec::None,
    }
}

/// The room abilities set off since the last collection.
pub(super) fn room_triggers(state: &mut GameState) -> Vec<crate::trigger::PendingTrigger> {
    let Some(rooms) = state
        .eternal_v1
        .as_mut()
        .and_then(|eternal| eternal.e3.as_deref_mut())
        .map(|e3| std::mem::take(&mut e3.room_triggers))
    else {
        return Vec::new();
    };
    rooms
        .into_iter()
        .map(|entered| {
            let mut trigger = super::super::delayed_trigger(
                entered.source,
                entered.player,
                room_ability(entered.dungeon, entered.room),
            );
            trigger.target_spec = room_target_spec(entered.dungeon, entered.room);
            trigger
        })
        .collect()
}

/// Whether `effect` is one of Acererak's triggers created outside its
/// printed table (a room ability).
pub(super) fn delayed_effect_matches(effect: &EffectOp) -> bool {
    room_of(effect).is_some()
}

/// The target specification of an Acererak trigger.
pub(super) fn trigger_target_spec(effect: &EffectOp) -> TargetSpec {
    room_of(effect).map_or(TargetSpec::None, |(dungeon, room)| {
        room_target_spec(dungeon, room)
    })
}

// ---- Acererak the Archlich ---------------------------------------------------------

pub(super) fn acererak_enters() -> EffectOp {
    eternal(EternalOpV1::AcererakEntersV1)
}

pub(super) fn acererak_attacks() -> EffectOp {
    eternal(EternalOpV1::AcererakAttacksV1)
}

/// "When Acererak enters, if you haven't completed Tomb of Annihilation":
/// the trigger's condition.
pub(super) fn acererak_entered(state: &GameState, source: ObjectId) -> bool {
    !completed_tomb(state, state.objects.get(source).controller)
}

/// "... return Acererak to its owner's hand and venture into the dungeon."
/// The condition is checked again on resolution (603.4).
pub(super) fn resolve_acererak_enters(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<(), String> {
    let player = continuation.ctx.controller;
    if completed_tomb(state, player) {
        return Ok(());
    }
    let source = continuation
        .ctx
        .ability_source_contract
        .ok_or("Acererak's trigger lost its source")?;
    let acererak = ObjectLinkV4 {
        object: source.source,
        zone_change_count: source.zone_change_count,
    };
    if super::link_live(state, acererak, Zone::Battlefield) {
        event::propose_and_commit(
            state,
            ProposedEvent::zone_change(acererak.object, Zone::Hand),
        );
    }
    venture(state, continuation, path)
}

/// "Whenever Acererak attacks, for each opponent, you create a 2/2 black
/// Zombie creature token unless that player sacrifices a creature of their
/// choice."
pub(super) fn resolve_acererak_attacks(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<bool, String> {
    let controller = continuation.ctx.controller;
    let opponent = controller.opponent();
    let creatures = creatures_of(state, opponent);
    if creatures.is_empty() {
        create_tokens(state, &continuation.ctx, ZOMBIE_TOKEN, 1);
        return Ok(false);
    }
    offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::ZombieUnlessSacrificeV1 { controller },
            player: opponent,
            candidates: targets(&creatures),
            bindings: creatures,
            min: 0,
            max: 1,
            ordered: false,
            path,
            remaining: continuation.frames.clone(),
        },
    );
    Ok(true)
}

// ---- Tokens ------------------------------------------------------------------------

const GOBLIN_TOKEN: &str = "Goblin Token";
const TREASURE_TOKEN: &str = "Treasure Token";
const SKELETON_TOKEN: &str = "Black Skeleton Token";
const ZOMBIE_TOKEN: &str = "Zombie Token";
const THE_ATROPAL: &str = "The Atropal";

fn create_tokens(state: &mut GameState, ctx: &ExecCtx, token: &str, count: usize) {
    let Some(token_def) = card_id_by_name(token) else {
        super::halt(state, ctx.source);
        return;
    };
    for _ in 0..count {
        crate::effect::execute(
            &EffectOp::CreateToken {
                token_def,
                controller: PlayerRef::Controller,
            },
            ctx,
            state,
        );
    }
}

// ---- Room abilities ----------------------------------------------------------------

fn targets(bindings: &[EffectObjectBinding]) -> Vec<Target> {
    bindings
        .iter()
        .map(|binding| Target::Object(binding.object))
        .collect()
}

fn controlled_matching(
    state: &GameState,
    player: PlayerId,
    keep: impl Fn(ObjectId) -> bool,
) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .battlefield
        .iter()
        .copied()
        .filter(|&object| state.objects.get(object).controller == player && keep(object))
        .map(|object| bind(state, object))
        .collect()
}

fn creatures_of(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    controlled_matching(state, player, |object| {
        crate::engine::object_has_type(state, object, CardType::Creature)
    })
}

fn hand_of(state: &GameState, player: PlayerId) -> Vec<EffectObjectBinding> {
    state.players[player.index()]
        .hand
        .iter()
        .map(|&card| bind(state, card))
        .collect()
}

/// The room ability's creature target, if still legal.
fn live_target(state: &GameState, ctx: &ExecCtx) -> Option<ObjectId> {
    let Some(Target::Object(object)) = ctx.targets.first().copied() else {
        return None;
    };
    (ctx.target_incarnation_matches(0, state)
        && state.objects.get(object).zone == Zone::Battlefield
        && crate::engine::object_has_type(state, object, CardType::Creature))
    .then_some(object)
}

/// Runs a room ability. `Ok(true)` when it staged a choice.
pub(super) fn resolve_room(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    dungeon: u16,
    room: u16,
    path: Vec<u16>,
) -> Result<bool, String> {
    let ctx = continuation.ctx.clone();
    let player = ctx.controller;
    let run = |state: &mut GameState, op: EffectOp| crate::effect::execute(&op, &ctx, state);
    match (dungeon, room) {
        // Cave Entrance, Dungeon Level: "Scry 1." Lost Level: "Scry 2."
        // Deep Mines: "Scry 3."
        (LOST_MINE_OF_PHANDELVER, 1) | (DUNGEON_OF_THE_MAD_MAGE, 2) => {
            return Ok(stage_scry(state, continuation, player, 1, path));
        }
        (DUNGEON_OF_THE_MAD_MAGE, 5) => {
            return Ok(stage_scry(state, continuation, player, 2, path));
        }
        (DUNGEON_OF_THE_MAD_MAGE, 8) => {
            return Ok(stage_scry(state, continuation, player, 3, path));
        }
        // Goblin Lair: "Create a 1/1 red Goblin creature token."
        (LOST_MINE_OF_PHANDELVER, 2) => create_tokens(state, &ctx, GOBLIN_TOKEN, 1),
        // Mine Tunnels, Goblin Bazaar: "Create a Treasure token."
        (LOST_MINE_OF_PHANDELVER, 3) | (DUNGEON_OF_THE_MAD_MAGE, 3) => {
            create_tokens(state, &ctx, TREASURE_TOKEN, 1)
        }
        // Storeroom: "Put a +1/+1 counter on target creature."
        (LOST_MINE_OF_PHANDELVER, 4) => {
            if let Some(creature) = live_target(state, &ctx) {
                event::add_plus_one_counters(state, creature, player, 1)?;
            }
        }
        // Dark Pool: "Each opponent loses 1 life and you gain 1 life."
        (LOST_MINE_OF_PHANDELVER, 5) => {
            run(
                state,
                EffectOp::LoseLife {
                    player: PlayerRef::Opponent,
                    amount: 1,
                },
            );
            run(
                state,
                EffectOp::GainLife {
                    player: PlayerRef::Controller,
                    amount: 1,
                },
            );
        }
        // Fungi Cavern: "Target creature gets -4/-0 until your next turn."
        (LOST_MINE_OF_PHANDELVER, 6) => {
            if let Some(creature) = live_target(state, &ctx) {
                let entry = UntilNextTurnV1 {
                    object: link(state, creature),
                    holder: player,
                };
                e3_mut(state).weakened.push(entry);
            }
        }
        // Temple of Dumathoin: "Draw a card."
        (LOST_MINE_OF_PHANDELVER, 7) => run(
            state,
            EffectOp::DrawCards {
                player: PlayerRef::Controller,
                count: 1,
            },
        ),
        // Yawning Portal: "You gain 1 life."
        (DUNGEON_OF_THE_MAD_MAGE, 1) => run(
            state,
            EffectOp::GainLife {
                player: PlayerRef::Controller,
                amount: 1,
            },
        ),
        // Twisted Caverns: "Target creature can't attack until your next
        // turn."
        (DUNGEON_OF_THE_MAD_MAGE, 4) => {
            if let Some(creature) = live_target(state, &ctx) {
                let entry = UntilNextTurnV1 {
                    object: link(state, creature),
                    holder: player,
                };
                e3_mut(state).pacified.push(entry);
            }
        }
        // Runestone Caverns: "Exile the top two cards of your library. You
        // may play them."
        (DUNGEON_OF_THE_MAD_MAGE, 6) => runestone_caverns(state, player),
        // Muiral's Graveyard: "Create two 1/1 black Skeleton creature
        // tokens."
        (DUNGEON_OF_THE_MAD_MAGE, 7) => create_tokens(state, &ctx, SKELETON_TOKEN, 2),
        // Mad Wizard's Lair: "Draw three cards and reveal them. You may cast
        // one of them without paying its mana cost."
        (DUNGEON_OF_THE_MAD_MAGE, 9) => {
            return mad_wizards_lair(state, continuation, path);
        }
        // Trapped Entry: "Each player loses 1 life."
        (TOMB_OF_ANNIHILATION, 1) => {
            for loser in [state.active_player, state.active_player.opponent()] {
                event::propose_and_commit(state, ProposedEvent::life_loss(loser, 1));
            }
        }
        // Veils of Fear: "Each player loses 2 life unless they discard a
        // card." Sandfall Cell: "Each player loses 2 life unless they
        // sacrifice a creature, artifact, or land of their choice."
        (TOMB_OF_ANNIHILATION, 2 | 4) => {
            let first = state.active_player;
            return stage_punisher(
                state,
                continuation,
                room,
                &[first, first.opponent()],
                Vec::new(),
                path,
            );
        }
        // Oubliette: "Discard a card and sacrifice a creature, an artifact,
        // and a land."
        (TOMB_OF_ANNIHILATION, 3) => {
            return stage_oubliette(state, continuation, player, None, Vec::new(), 0, path);
        }
        // Cradle of the Death God: "Create The Atropal, a legendary 4/4 black
        // God Horror creature token with deathtouch."
        (TOMB_OF_ANNIHILATION, 5) => create_tokens(state, &ctx, THE_ATROPAL, 1),
        _ => return Err("unknown dungeon room".to_string()),
    }
    Ok(false)
}

// ---- Scry --------------------------------------------------------------------------

/// "Scry N": the player looks at the top N cards, chooses those to keep on
/// top in order (first chosen on top), then orders the rest on the bottom.
pub(crate) fn stage_scry(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    player: PlayerId,
    count: usize,
    path: Vec<u16>,
) -> bool {
    let library = state.players[player.index()].library.clone();
    let looked = count.min(library.len());
    if looked == 0 {
        return false;
    }
    state.reveal_library_top(player, player, looked);
    let prefix = library[..looked]
        .iter()
        .map(|&card| bind(state, card))
        .collect::<Vec<_>>();
    offer_scry(continuation, player, prefix, None, path);
    true
}

fn offer_scry(
    continuation: &mut EffectContinuation,
    player: PlayerId,
    prefix: Vec<EffectObjectBinding>,
    top: Option<Vec<EffectObjectBinding>>,
    path: Vec<u16>,
) {
    let candidates = match &top {
        None => prefix.clone(),
        Some(top) => prefix
            .iter()
            .copied()
            .filter(|card| !top.contains(card))
            .collect(),
    };
    let count = u16::try_from(candidates.len()).unwrap_or(u16::MAX);
    let min = if top.is_some() { count } else { 0 };
    offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::ScryV1 { prefix, top },
            player,
            candidates: targets(&candidates),
            bindings: candidates,
            min,
            max: count,
            ordered: true,
            path,
            remaining: continuation.frames.clone(),
        },
    );
}

/// Applies a scry answer: the kept cards first, then (`top` given) the
/// bottom order.
fn finish_scry(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    prefix: &[EffectObjectBinding],
    top: Option<&[EffectObjectBinding]>,
    chosen: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    let (top, bottom) = match top {
        None => {
            let rest = prefix
                .iter()
                .copied()
                .filter(|card| !chosen.contains(card))
                .collect::<Vec<_>>();
            if rest.len() >= 2 {
                offer_scry(
                    continuation,
                    choice.player,
                    prefix.to_vec(),
                    Some(chosen),
                    choice.path.clone(),
                );
                return Ok(true);
            }
            (chosen, rest)
        }
        Some(top) => (top.to_vec(), chosen),
    };
    let expected = prefix
        .iter()
        .map(|card| ObjectLinkV4 {
            object: card.object,
            zone_change_count: card.expected_zone_change_count,
        })
        .collect::<Vec<_>>();
    let objects =
        |cards: &[EffectObjectBinding]| cards.iter().map(|card| card.object).collect::<Vec<_>>();
    state.apply_scry_result(choice.player, &expected, &objects(&top), &objects(&bottom))?;
    Ok(false)
}

// ---- Runestone Caverns -------------------------------------------------------------

/// Exiles the top two cards; their owner may play them for as long as they
/// remain exiled. A permission that never expires is a `LaterTurn` grant
/// for a turn that never comes.
fn runestone_caverns(state: &mut GameState, player: PlayerId) {
    for _ in 0..2 {
        let Some(&top) = state.players[player.index()].library.first() else {
            break;
        };
        event::propose_and_commit(state, ProposedEvent::zone_change(top, Zone::Exile));
        let exiled = state.objects.get(top);
        if exiled.zone != Zone::Exile {
            continue;
        }
        let def = printed(state, top);
        let play_or_cast = if def.is_playable_land() {
            crate::engine::PlayOrCast::Play
        } else if def.is_castable() {
            crate::engine::PlayOrCast::Cast
        } else {
            continue;
        };
        let permission = crate::engine::PlayPermission {
            without_mana_cost: crate::engine::FreeCastV1::default(),
            object: top,
            holder: player,
            zone_change_generation: exiled.zone_change_count,
            play_or_cast,
            expiry: crate::engine::PlayPermissionExpiry::LaterTurn {
                granted_turn: u32::MAX,
                granted_active_player: player,
            },
        };
        state.engine.exile_play_permissions.push(permission);
    }
}

// ---- Mad Wizard's Lair -------------------------------------------------------------

fn mad_wizards_lair(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    path: Vec<u16>,
) -> Result<bool, String> {
    let player = continuation.ctx.controller;
    let before = state.players[player.index()].hand.clone();
    crate::effect::execute(
        &EffectOp::DrawCards {
            player: PlayerRef::Controller,
            count: 3,
        },
        &continuation.ctx,
        state,
    );
    let drawn = state.players[player.index()]
        .hand
        .iter()
        .copied()
        .filter(|card| !before.contains(card))
        .collect::<Vec<_>>();
    for &card in &drawn {
        state.reveal_hand_card(player.opponent(), player, card)?;
    }
    let drawn = drawn
        .into_iter()
        .map(|card| bind(state, card))
        .collect::<Vec<_>>();
    let castable = drawn
        .iter()
        .copied()
        .filter(|&card| {
            crate::engine::resolution_cast_v1::can_cast_exiled_without_mana(
                state, player, card, None,
            )
        })
        .collect::<Vec<_>>();
    if castable.is_empty() {
        return Ok(false);
    }
    offer(
        continuation,
        E3ChoiceV1 {
            kind: E3ChoiceKindV1::MadWizardV1 { drawn },
            player,
            candidates: targets(&castable),
            bindings: castable,
            min: 0,
            max: 1,
            ordered: false,
            path,
            remaining: continuation.frames.clone(),
        },
    );
    Ok(true)
}

/// The card a Mad Wizard's Lair answer casts, if any.
pub(crate) fn resolution_cast_card(
    choice: &E3ChoiceV1,
    selected: &[Target],
) -> Option<EffectObjectBinding> {
    match &choice.kind {
        E3ChoiceKindV1::MadWizardV1 { .. } => {}
        // Beseech the Mirror casts the found card from its owner's hand.
        E3ChoiceKindV1::E5aV1(kind) => {
            return crate::eternal_e5a_v1::choices::resolution_cast_card(kind, choice, selected)
        }
        _ => return None,
    }
    selected
        .first()
        .and_then(|&target| choice.binding_for(target))
}

/// Authenticates the cast a Mad Wizard's Lair answer authorized.
pub(crate) fn validate_resolution_cast(
    suspended: &EffectContinuation,
    choice: &E3ChoiceV1,
    selected: &[Target],
    permission: crate::engine::resolution_cast_v1::ResolutionPermissionV1,
) -> Result<(), String> {
    if resolution_cast_card(choice, selected) != Some(permission.card)
        || selected.len() != 1
        || permission.card.expected_zone != Zone::Hand
        || permission.land
        || permission.maximum_mana_value.is_some()
        || permission.controller != choice.player
        || suspended.ctx.controller != choice.player
        || suspended.frames != choice.remaining
    {
        return Err("Mad Wizard's Lair answer does not authorize this cast".to_string());
    }
    Ok(())
}

// ---- Veils of Fear, Sandfall Cell --------------------------------------------------

fn punisher_candidates(state: &GameState, room: u16, player: PlayerId) -> Vec<EffectObjectBinding> {
    if room == 2 {
        hand_of(state, player)
    } else {
        controlled_matching(state, player, |object| {
            [CardType::Creature, CardType::Artifact, CardType::Land]
                .into_iter()
                .any(|card_type| crate::engine::object_has_type(state, object, card_type))
        })
    }
}

/// Offers the next player's "unless" choice, or applies them all: each
/// player who chose a card discards or sacrifices it, the others lose 2
/// life.
fn stage_punisher(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    room: u16,
    players: &[PlayerId],
    chosen: Vec<(PlayerId, Option<EffectObjectBinding>)>,
    path: Vec<u16>,
) -> Result<bool, String> {
    let mut chosen = chosen;
    for (index, &player) in players.iter().enumerate() {
        let cards = punisher_candidates(state, room, player);
        if cards.is_empty() {
            chosen.push((player, None));
            continue;
        }
        offer(
            continuation,
            E3ChoiceV1 {
                kind: E3ChoiceKindV1::PunisherV1 {
                    room,
                    chosen,
                    later: players[index + 1..].to_vec(),
                },
                player,
                candidates: targets(&cards),
                bindings: cards,
                min: 0,
                max: 1,
                ordered: false,
                path,
                remaining: continuation.frames.clone(),
            },
        );
        return Ok(true);
    }
    for (player, card) in &chosen {
        match card {
            Some(card) if binding_live(state, *card) => {
                if room == 2 {
                    crate::engine::commit_discarded_card(state, card.object);
                } else {
                    event::log_sacrifice(state, card.object);
                    event::propose_and_commit(
                        state,
                        ProposedEvent::zone_change(card.object, Zone::Graveyard),
                    );
                }
            }
            Some(_) => return Err("a punisher choice left its zone".to_string()),
            None => event::propose_and_commit(state, ProposedEvent::life_loss(*player, 2)),
        }
    }
    Ok(false)
}

// ---- Oubliette ---------------------------------------------------------------------

const OUBLIETTE_TYPES: [CardType; 3] = [CardType::Creature, CardType::Artifact, CardType::Land];

/// Oubliette's choices in order: the card to discard (stage 0), then a
/// creature, an artifact and a land to sacrifice (stages 1 to 3). A
/// permanent of two of those types may be chosen for both.
fn stage_oubliette(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    player: PlayerId,
    discard: Option<EffectObjectBinding>,
    chosen: Vec<EffectObjectBinding>,
    stage: u8,
    path: Vec<u16>,
) -> Result<bool, String> {
    let mut stage = stage;
    while stage <= 3 {
        let cards = if stage == 0 {
            hand_of(state, player)
        } else {
            let card_type = OUBLIETTE_TYPES[usize::from(stage - 1)];
            controlled_matching(state, player, |object| {
                crate::engine::object_has_type(state, object, card_type)
            })
        };
        if cards.is_empty() {
            stage += 1;
            continue;
        }
        offer(
            continuation,
            E3ChoiceV1 {
                kind: E3ChoiceKindV1::OublietteV1 {
                    discard,
                    chosen,
                    stage,
                },
                player,
                candidates: targets(&cards),
                bindings: cards,
                min: 1,
                max: 1,
                ordered: false,
                path,
                remaining: continuation.frames.clone(),
            },
        );
        return Ok(true);
    }
    if let Some(card) = discard {
        if !binding_live(state, card) {
            return Err("Oubliette's discard left the hand".to_string());
        }
        crate::engine::commit_discarded_card(state, card.object);
    }
    let mut sacrificed = Vec::new();
    for permanent in chosen {
        if !sacrificed.contains(&permanent) {
            sacrificed.push(permanent);
        }
    }
    super::sacrifice_all(state, player, &sacrificed);
    Ok(false)
}

// ---- Answers -----------------------------------------------------------------------

/// Applies an answered dungeon choice. `Ok(true)` when it staged another.
pub(super) fn finish(
    state: &mut GameState,
    continuation: &mut EffectContinuation,
    choice: &E3ChoiceV1,
    bindings: Vec<EffectObjectBinding>,
) -> Result<bool, String> {
    match &choice.kind {
        E3ChoiceKindV1::ScryV1 { prefix, top } => finish_scry(
            state,
            continuation,
            choice,
            prefix,
            top.as_deref(),
            bindings,
        ),
        E3ChoiceKindV1::PunisherV1 {
            room,
            chosen,
            later,
        } => {
            let mut chosen = chosen.clone();
            if let Some(card) = bindings.first() {
                if *room == 2 {
                    state.reveal_hand_card(choice.player.opponent(), choice.player, card.object)?;
                }
            }
            chosen.push((choice.player, bindings.first().copied()));
            stage_punisher(
                state,
                continuation,
                *room,
                later,
                chosen,
                choice.path.clone(),
            )
        }
        E3ChoiceKindV1::OublietteV1 {
            discard,
            chosen,
            stage,
        } => {
            let picked = *bindings.first().ok_or("Oubliette needs a choice")?;
            let (discard, mut chosen) = (*discard, chosen.clone());
            let discard = if *stage == 0 {
                Some(picked)
            } else {
                chosen.push(picked);
                discard
            };
            stage_oubliette(
                state,
                continuation,
                choice.player,
                discard,
                chosen,
                stage + 1,
                choice.path.clone(),
            )
        }
        E3ChoiceKindV1::ZombieUnlessSacrificeV1 { .. } => {
            if bindings.is_empty() {
                create_tokens(state, &continuation.ctx.clone(), ZOMBIE_TOKEN, 1);
            } else {
                super::sacrifice_all(state, choice.player, &bindings);
            }
            Ok(false)
        }
        // A chosen card is cast by the interpreter before this runs.
        E3ChoiceKindV1::MadWizardV1 { .. } => Ok(false),
        _ => Err("not a dungeon choice".to_string()),
    }
}
