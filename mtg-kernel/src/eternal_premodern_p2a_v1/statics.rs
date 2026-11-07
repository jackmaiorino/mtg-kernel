//! Static abilities and engine hooks of the Premodern wave P2a cards:
//! Contamination, Stasis, Forsaken City, Root Maze, Future Sight, Eternal
//! Dragon, Krosan Tusker, Black Vise and Bane of the Living.

use super::{
    active_named, active_permanents, definition, definition_name, live_link, p2a, p2a_state,
    queue_trigger, read_state, P2aOpV1, BANE_OF_THE_LIVING, BLACK_VISE, CONTAMINATION,
    ETERNAL_DRAGON, FORSAKEN_CITY, FUTURE_SIGHT, KROSAN_TUSKER, ROOT_MAZE, STASIS, TAINTED_FIELD,
    TAINTED_WOOD,
};
use crate::card_def::{CardType, TargetSpec, CARD_DEFS};
use crate::ids::{ObjectId, PlayerId};
use crate::mana::{Cost, ManaColor, Pip};
use crate::state::{AbilitySourceContractV4, GameState, Step, Zone};

/// The definition ids of the cards whose statics the engine asks about on
/// hot paths, so those checks compare ids before names.
struct StaticIds {
    contamination: Option<u16>,
    root_maze: Option<u16>,
    stasis: Option<u16>,
    future_sight: Option<u16>,
}

fn static_ids() -> &'static StaticIds {
    static IDS: std::sync::OnceLock<StaticIds> = std::sync::OnceLock::new();
    IDS.get_or_init(|| StaticIds {
        contamination: crate::card_def::card_id_by_name(CONTAMINATION),
        root_maze: crate::card_def::card_id_by_name(ROOT_MAZE),
        stasis: crate::card_def::card_id_by_name(STASIS),
        future_sight: crate::card_def::card_id_by_name(FUTURE_SIGHT),
    })
}

/// The battlefield permanents with definition `id`, named `name`, whose
/// printed abilities apply, controlled by `controller` if given. Compares
/// definition ids first.
fn active_with_id<'a>(
    state: &'a GameState,
    id: Option<u16>,
    name: &'a str,
    controller: Option<PlayerId>,
) -> impl Iterator<Item = ObjectId> + 'a {
    [PlayerId::P0, PlayerId::P1]
        .into_iter()
        .filter(move |&player| controller.is_none_or(|wanted| wanted == player))
        .flat_map(move |player| state.players[player.index()].battlefield.iter().copied())
        .filter(move |&object| {
            id.is_some_and(|id| state.objects.get(object).card_def == id)
                && active_named(state, object, name)
                && controller.is_none_or(|player| state.objects.get(object).controller == player)
        })
}

// ---- Untap steps and tapped entries -------------------------------------------------

/// Stasis: "Players skip their untap steps." Forsaken City: "This land
/// does not untap during your untap step." Whether `object` stays tapped
/// through the untap step.
pub(crate) fn prevents_untap_step(state: &GameState, object: ObjectId) -> bool {
    if active_with_id(state, static_ids().stasis, STASIS, None)
        .next()
        .is_some()
    {
        return true;
    }
    state
        .objects
        .try_get(object)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && active_named(state, object, FORSAKEN_CITY)
}

/// Root Maze: "Artifacts and lands enter the battlefield tapped." Whether
/// `object`, entering under `controller`, enters tapped.
pub(crate) fn enters_tapped(state: &GameState, object: ObjectId, _controller: PlayerId) -> bool {
    let Some(live) = state.objects.try_get(object) else {
        return false;
    };
    let Some(def) = CARD_DEFS.get(usize::from(live.card_def)) else {
        return false;
    };
    let types = def.types_for_face(live.v4.face_index);
    (types.contains(&CardType::Artifact) || types.contains(&CardType::Land))
        && active_with_id(state, static_ids().root_maze, ROOT_MAZE, None)
            .next()
            .is_some()
}

// ---- Contamination ------------------------------------------------------------------

/// Contamination: "If a land is tapped for mana, it produces {B} instead of
/// any other type and amount." Whether the land `land` produces only {B}
/// (one mana) for each tap.
pub(crate) fn produces_only_black(state: &GameState, land: ObjectId) -> bool {
    state
        .objects
        .try_get(land)
        .is_some_and(|live| live.zone == Zone::Battlefield)
        && crate::engine::object_has_type(state, land, CardType::Land)
        && active_with_id(state, static_ids().contamination, CONTAMINATION, None)
            .next()
            .is_some()
}

// ---- The Tainted lands ----------------------------------------------------------------

/// Tainted Wood: "{T}: Add {B} or {G}. Activate only if you control a
/// Swamp." and Tainted Field: "{T}: Add {W} or {B}. ..." The colors beyond
/// the {C} of the land's free ability that the payment planner may also tap
/// it for: its second mana ability is a free tap, so it is as automatic a
/// source as the first once its condition holds.
pub(crate) fn tainted_colors(state: &GameState, land: ObjectId) -> &'static [ManaColor] {
    let colors: &'static [ManaColor] = match definition_name(state, land) {
        Some(TAINTED_WOOD) => &[ManaColor::B, ManaColor::G],
        Some(TAINTED_FIELD) => &[ManaColor::W, ManaColor::B],
        _ => return &[],
    };
    let controller = state.objects.get(land).controller;
    let swamp = state.players[controller.index()]
        .battlefield
        .iter()
        .any(|&object| {
            state.objects.get(object).zone == Zone::Battlefield
                && crate::engine::has_effective_subtype(
                    state,
                    object,
                    crate::card_def::Subtype::Swamp,
                )
        });
    if swamp {
        colors
    } else {
        &[]
    }
}

// ---- Future Sight -------------------------------------------------------------------

/// Future Sight: whether `player` controls one, so the top card of their
/// library is revealed and may be played.
pub(crate) fn future_sight(state: &GameState, player: PlayerId) -> bool {
    active_with_id(state, static_ids().future_sight, FUTURE_SIGHT, Some(player))
        .next()
        .is_some()
}

// ---- Activations --------------------------------------------------------------------

/// Activation restrictions: Eternal Dragon's return is for its owner's
/// upkeep only.
pub(crate) fn activation_allowed(
    state: &GameState,
    source: ObjectId,
    name: &str,
    ability_index: usize,
) -> bool {
    match (name, ability_index) {
        (ETERNAL_DRAGON, 0) => {
            state.step == Step::Upkeep
                && state
                    .objects
                    .try_get(source)
                    .is_some_and(|live| live.owner == state.active_player)
        }
        _ => true,
    }
}

/// Costs paid as an ability is put on the stack, and the triggers its
/// payment sets off: cycling Krosan Tusker triggers it ("When you cycle
/// this card") wherever the discarded card went (702.29c).
pub(crate) fn activation_paid(state: &mut GameState, source: ObjectId, ability_index: u8) {
    let Some(name) = definition_name(state, source) else {
        return;
    };
    let live = state.objects.get(source);
    if name == KROSAN_TUSKER && ability_index == 0 && live.zone != Zone::Hand {
        let owner = live.owner;
        let contract = AbilitySourceContractV4::capture(state, source);
        queue_trigger(state, contract, owner, super::tusker_cycled(), None);
    }
}

/// Entry bookkeeping: Black Vise records its chosen opponent.
pub(crate) fn apply_entry_counters(state: &mut GameState, object: ObjectId) {
    if definition_name(state, object) != Some(BLACK_VISE) {
        return;
    }
    let live = state.objects.get(object);
    if live.zone != Zone::Battlefield || live.v4.face_index != 0 {
        return;
    }
    let link = live_link(state, object);
    super::record_vise(state, link);
}

// ---- Morph with an X cost: Bane of the Living ------------------------------------------

/// Morph (702.37): Bane of the Living is cast face down through disguise's
/// face-down cast, without disguise's ward.
pub(crate) fn is_morph(name: &str) -> bool {
    name == BANE_OF_THE_LIVING
}

/// Bane of the Living's morph cost {X}{B}{B}: the {B}{B} is the special
/// action's cost; its X is announced and paid right after
/// (`entry_choice_decision`).
const BANE_MORPH: Cost = Cost {
    pips: &[Pip::Colored(ManaColor::B), Pip::Colored(ManaColor::B)],
    generic: 0,
    x_count: 0,
};

/// The cost of turning the face-down morph permanent `id` face up (a
/// special action, 702.37e), or `None` when it is not a Bane of the Living.
pub(crate) fn morph_face_up_cost(state: &GameState, id: ObjectId) -> Option<Cost> {
    let live = state.objects.try_get(id)?;
    if !live.v4.face_down_v1.is_some_and(|face| face.disguised) {
        return None;
    }
    morph_cost(state, id)
}

/// The morph cost of the face-down permanent `id` when it would have morph
/// face up (702.37e): not while its abilities are removed. A manifested
/// morph card may also turn face up this way (701.34c).
pub(crate) fn morph_cost(state: &GameState, id: ObjectId) -> Option<Cost> {
    let live = state.objects.try_get(id)?;
    if live.zone != Zone::Battlefield || live.v4.face_down_v1.is_none() {
        return None;
    }
    let def = CARD_DEFS.get(usize::from(live.card_def))?;
    if def.name != BANE_OF_THE_LIVING
        || crate::continuous_characteristics_v1::removal_timestamp(state, id).is_some()
    {
        return None;
    }
    Some(BANE_MORPH)
}

/// The most generic mana `player` can pay for an X.
fn payable_x(state: &GameState, player: PlayerId) -> u8 {
    (1..=30_u8)
        .take_while(|&x| {
            let cost = Cost {
                pips: &[],
                generic: x,
                x_count: 0,
            };
            crate::mana::can_pay(&cost, 0, player, state).is_some()
        })
        .last()
        .unwrap_or(0)
}

/// Called as the face-down permanent `id` is turned face up: a Bane of the
/// Living owes its controller's X before its trigger is queued.
pub(crate) fn turned_face_up(state: &mut GameState, id: ObjectId) {
    if definition_name(state, id) != Some(BANE_OF_THE_LIVING) {
        return;
    }
    let live = state.objects.get(id);
    if live.zone != Zone::Battlefield {
        return;
    }
    if payable_x(state, live.controller) == 0 {
        return;
    }
    let link = live_link(state, id);
    p2a_state(state).bane_x = Some(link);
}

/// The X still owed by a face-up Bane of the Living, as a decision for its
/// controller.
pub(crate) fn entry_choice_decision(state: &GameState) -> Option<crate::engine::Decision> {
    let link = read_state(state)?.bane_x?;
    if !super::link_live_in(state, link, Zone::Battlefield) {
        return None;
    }
    let controller = state.objects.get(link.object).controller;
    let maximum = payable_x(state, controller);
    Some(crate::engine::Decision::ChooseEffectOption {
        player: controller,
        source: link.object,
        option_count: u16::from(maximum) + 1,
    })
}

/// Answers the pending X: pays it and queues "When this creature is turned
/// face up, all creatures get -X/-X until end of turn."
pub(crate) fn answer_entry_choice(state: &mut GameState, option: u16) -> Result<bool, String> {
    let Some(link) = read_state(state).and_then(|p2a| p2a.bane_x) else {
        return Ok(false);
    };
    p2a_state(state).bane_x = None;
    if !super::link_live_in(state, link, Zone::Battlefield) {
        return Ok(true);
    }
    let controller = state.objects.get(link.object).controller;
    let x = u8::try_from(option).map_err(|_| "X is out of range".to_string())?;
    if x > payable_x(state, controller) {
        return Err("X is more than the mana available".to_string());
    }
    if x == 0 {
        return Ok(true);
    }
    let cost = Cost {
        pips: &[],
        generic: x,
        x_count: 0,
    };
    let plan = crate::mana::can_pay(&cost, 0, controller, state)
        .ok_or_else(|| "X is no longer payable".to_string())?;
    crate::engine::pay_plan(state, controller, &plan);
    let contract = AbilitySourceContractV4::capture(state, link.object);
    queue_trigger(
        state,
        contract,
        controller,
        p2a(P2aOpV1::BaneShrinkV1 { x: u16::from(x) }),
        None,
    );
    Ok(true)
}

/// The spec used when a hook asks for a trigger without a target.
#[allow(dead_code)]
fn no_target() -> TargetSpec {
    TargetSpec::None
}

/// The definition of `object`, for hooks that compare printed text.
#[allow(dead_code)]
fn def_of(state: &GameState, object: ObjectId) -> Option<&'static crate::card_def::CardDef> {
    definition(state, object)
}

/// Whether any active permanent named `name` is on the battlefield.
#[allow(dead_code)]
fn any_active(state: &GameState, name: &str) -> bool {
    active_permanents(state, name, None).next().is_some()
}
