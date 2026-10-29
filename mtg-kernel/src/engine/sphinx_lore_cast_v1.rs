//! Native selection of printed versus installed flashback costs.
use super::*;
use crate::effect::sphinx_lore_v1 as lore;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RouteV1 {
    Normal,
    PrintedFlashback,
    Escape,
    Granted { installation: u64 },
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsV1 {
    pub spell: ObjectId,
    pub card_def: u16,
    pub controller: PlayerId,
    pub graveyard_generation: u32,
    pub turn: u32,
    pub routes: Vec<RouteV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceV1 {
    pub options_history_index: u64,
    pub selected: Option<u16>,
}
fn options_record(state: &GameState, index: u64) -> Result<&OptionsV1, String> {
    match state
        .engine
        .event_history
        .get(usize::try_from(index).map_err(|_| "Sphinx route index width")?)
    {
        Some(CommittedEvent::SphinxLoreCastOptionsV1 { options }) => {
            let card = state
                .objects
                .try_get(options.spell)
                .ok_or("Sphinx cast-choice physical card absent")?;
            let d = card_def::CARD_DEFS
                .get(options.card_def as usize)
                .ok_or("Sphinx cast-choice definition absent")?;
            if card.owner != options.controller
                || card.card_def != options.card_def
                || card.zone_change_count < options.graveyard_generation
                || !d.is_castable()
                || !d.is_executable()
                || !d
                    .types
                    .iter()
                    .any(|t| matches!(t, CardType::Instant | CardType::Sorcery))
                || options.routes.is_empty()
                || options.routes.len() > usize::from(u16::MAX)
                || options
                    .routes
                    .iter()
                    .enumerate()
                    .any(|(i, r)| options.routes[..i].contains(r))
                || !options
                    .routes
                    .iter()
                    .any(|r| matches!(r, RouteV1::Granted { .. }))
            {
                return Err("Sphinx original cast-choice roster changed".into());
            }
            let active = lore::active_at(state, index as usize)?;
            for route in &options.routes {
                match *route {
                    RouteV1::Granted { installation } => {
                        let (b, holder, turn) = lore::installation(state, installation)?;
                        if installation >= index
                            || b.object != options.spell
                            || b.expected_zone_change_count != options.graveyard_generation
                            || holder != options.controller
                            || turn != options.turn
                            || !active.contains(&installation)
                        {
                            return Err(
                                "Sphinx original cast choice names a foreign or expired grant"
                                    .into(),
                            );
                        }
                    }
                    RouteV1::PrintedFlashback if d.flashback.is_some() && d.escape.is_none() => {}
                    RouteV1::Escape if d.escape.is_some() && d.flashback.is_none() => {}
                    RouteV1::Normal => {}
                    _ => return Err("Sphinx original printed cost route changed".into()),
                }
            }
            Ok(options)
        }
        _ => Err("Sphinx original cast choices missing".into()),
    }
}
fn method(route: RouteV1) -> CastMethodV4 {
    match route {
        RouteV1::Normal => CastMethodV4::Normal,
        RouteV1::Escape => CastMethodV4::Escape,
        _ => CastMethodV4::Flashback,
    }
}
fn route_origin(route: RouteV1, player: PlayerId, generation: u32) -> SpellCastRouteV4 {
    match route {
        RouteV1::Normal => SpellCastRouteV4::GraveyardPermissionV1 {
            holder: player,
            permission_zone_change_count: generation,
        },
        RouteV1::Escape => SpellCastRouteV4::GraveyardEscape,
        RouteV1::PrintedFlashback => SpellCastRouteV4::GraveyardFlashback,
        RouteV1::Granted { installation } => SpellCastRouteV4::SphinxLoreFlashbackV1 {
            installation,
            holder: player,
            graveyard_generation: generation,
        },
    }
}
pub(super) fn granted(p: &PendingCast) -> bool {
    matches!(
        p.source_contract.spell_cast_origin.map(|o| o.route),
        Some(SpellCastRouteV4::SphinxLoreFlashbackV1 { .. })
    )
}
fn route_quote(state: &GameState, id: ObjectId, player: PlayerId, route: RouteV1) -> bool {
    let d = &card_def::CARD_DEFS[usize::from(state.objects.get(id).card_def)];
    let m = method(route);
    if !matches!(route, RouteV1::Granted { .. }) {
        return is_castable_now(player, id, m, state);
    }
    if !d.is_castable()
        || !d.is_executable()
        || !cast_form_timing_ok(d.types, d.keywords, player, state)
    {
        return false;
    }
    let modes = viable_printed_spell_modes(d, id, player, state);
    modes.into_iter().any(|mode| {
        [false, true]
            .into_iter()
            .filter(|k| !*k || d.kicker_cost.is_some())
            .any(|kicked| {
                spell_costs_v1::selected_spell_quote_with_grant_v1(
                    d,
                    id,
                    Zone::Graveyard,
                    m,
                    kicked,
                    mode,
                    0,
                    &[],
                    &[],
                    player,
                    state,
                    &[],
                    match route {
                        RouteV1::Granted { installation } => Some(installation),
                        _ => None,
                    },
                )
                .is_some()
            })
    })
}
pub(super) fn routes(state: &GameState, id: ObjectId, player: PlayerId) -> Vec<RouteV1> {
    let grants = lore::active_grants(state, id, player);
    if grants.is_empty() {
        return vec![];
    }
    let o = state.objects.get(id);
    let d = &card_def::CARD_DEFS[usize::from(o.card_def)];
    let mut routes = vec![];
    if d.flashback.is_some() && d.escape.is_none() {
        routes.push(RouteV1::PrintedFlashback);
    }
    if d.escape.is_some() && d.flashback.is_none() {
        routes.push(RouteV1::Escape);
    }
    if graveyard_permission_zone_change_count(player, id, state).is_some() {
        routes.push(RouteV1::Normal);
    }
    routes.extend(
        grants
            .into_iter()
            .map(|installation| RouteV1::Granted { installation }),
    );
    routes.retain(|r| route_quote(state, id, player, *r));
    // An unaffordable new alternative must not replace an independently legal
    // printed/granted predecessor route or manufacture an empty choice stage.
    if !routes.iter().any(|r| matches!(r, RouteV1::Granted { .. })) {
        return vec![];
    }
    if routes.len() > usize::from(u16::MAX) {
        return vec![];
    }
    routes
}
pub(super) fn seed(
    state: &mut GameState,
    id: ObjectId,
    player: PlayerId,
) -> Option<(ChoiceV1, RouteV1)> {
    let routes = routes(state, id, player);
    let first = *routes.first()?;
    let o = state.objects.get(id);
    let options = OptionsV1 {
        spell: id,
        card_def: o.card_def,
        controller: player,
        graveyard_generation: o.zone_change_count,
        turn: state.turn_key_v1(),
        routes,
    };
    let selected = (options.routes.len() == 1).then_some(0);
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::SphinxLoreCastOptionsV1 { options });
    Some((
        ChoiceV1 {
            options_history_index: index,
            selected,
        },
        first,
    ))
}
pub(super) fn selected_method(route: RouteV1) -> CastMethodV4 {
    method(route)
}
pub(super) fn selected_origin(
    route: RouteV1,
    player: PlayerId,
    generation: u32,
) -> SpellCastRouteV4 {
    route_origin(route, player, generation)
}
pub(super) fn waiting(p: &PendingCast) -> bool {
    p.sphinx_lore_v1.is_some_and(|c| c.selected.is_none())
}
pub(super) fn count(state: &GameState, p: &PendingCast) -> Result<u16, String> {
    let c = p.sphinx_lore_v1.ok_or("Sphinx route stage absent")?;
    u16::try_from(options_record(state, c.options_history_index)?.routes.len())
        .map_err(|_| "Sphinx route menu width".into())
}
pub(super) fn validate(state: &GameState, p: &PendingCast) -> Result<(), String> {
    let Some(c) = p.sphinx_lore_v1 else {
        return if granted(p) || p.source_contract.spell_cast_origin.is_some_and(|origin| state.engine.event_history.iter().any(|e|matches!(e,CommittedEvent::SphinxLoreCastOptionsV1{options} if options.spell==p.spell && options.controller==p.controller && options.card_def==p.source_contract.card_def && options.graveyard_generation==origin.origin_zone_change_count))) {
            Err("Sphinx selected route stage erased".into())
        } else {
            Ok(())
        };
    };
    let record = options_record(state, c.options_history_index)?;
    let origin = p
        .source_contract
        .spell_cast_origin
        .ok_or("Sphinx origin erased")?;
    if record.spell != p.spell
        || record.card_def != p.source_contract.card_def
        || record.controller != p.controller
        || record.graveyard_generation != origin.origin_zone_change_count
        || p.origin_zone != Zone::Graveyard
        || record.routes.is_empty()
        || record.routes.len() > usize::from(u16::MAX)
        || record
            .routes
            .iter()
            .enumerate()
            .any(|(i, r)| record.routes[..i].contains(r))
    {
        return Err("Sphinx route source/menu changed".into());
    }
    for route in &record.routes {
        match *route {
            RouteV1::Granted { installation } => {
                let (b, holder, turn) = lore::installation(state, installation)?;
                if installation >= c.options_history_index
                    || b.object != p.spell
                    || b.expected_zone_change_count != record.graveyard_generation
                    || holder != p.controller
                    || turn != record.turn
                    || !lore::active_at(state, c.options_history_index as usize)?
                        .contains(&installation)
                {
                    return Err(
                        "Sphinx original route grant changed or expired before announcement".into(),
                    );
                }
            }
            RouteV1::PrintedFlashback
                if card_def::CARD_DEFS[record.card_def as usize]
                    .flashback
                    .is_some() => {}
            RouteV1::Escape
                if card_def::CARD_DEFS[record.card_def as usize]
                    .escape
                    .is_some() => {}
            RouteV1::Normal
                if crate::standard_cards_v1::graveyard_cast_granted(
                    state,
                    p.spell,
                    p.controller,
                    record.graveyard_generation,
                ) => {}
            _ => return Err("Sphinx foreign cast route".into()),
        }
    }
    let selected = record
        .routes
        .get(usize::from(c.selected.unwrap_or(0)))
        .ok_or("Sphinx selected route outside menu")?;
    if method(*selected) != p.source_contract.cast_method
        || route_origin(*selected, p.controller, record.graveyard_generation) != origin.route
        || p.is_flashback != (method(*selected) == CastMethodV4::Flashback)
    {
        return Err("Sphinx selected method/origin changed".into());
    }
    Ok(())
}
pub(super) fn answer(state: &mut GameState, p: &PendingCast, index: u16) -> Result<(), String> {
    validate(state, p)?;
    let c = p.sphinx_lore_v1.ok_or("Sphinx route stage absent")?;
    if c.selected.is_some() {
        return Err("Sphinx route already selected".into());
    }
    let record = options_record(state, c.options_history_index)?;
    let route = *record
        .routes
        .get(index as usize)
        .ok_or("Sphinx route index outside menu")?;
    let mut source = p.source_contract;
    source.cast_method = method(route);
    source
        .spell_cast_origin
        .as_mut()
        .ok_or("Sphinx origin absent")?
        .route = route_origin(route, p.controller, record.graveyard_generation);
    let is_flashback = source.cast_method == CastMethodV4::Flashback;
    state.objects.get_mut(p.spell).v4.spell_cast_origin = source.spell_cast_origin;
    let row = state.stack.last_mut().ok_or("Sphinx placeholder absent")?;
    row.v4.source_contract = Some(source);
    row.v4.cast_method = Some(source.cast_method);
    row.is_flashback = is_flashback;
    let live = state
        .engine
        .pending_cast
        .as_mut()
        .ok_or("Sphinx pending absent")?;
    live.source_contract = source;
    live.is_flashback = is_flashback;
    live.cast_mode = if source.cast_method == CastMethodV4::Normal
        && card_def::CARD_DEFS[source.card_def as usize]
            .alt_cost
            .is_some()
    {
        None
    } else {
        Some(CastMode::Normal)
    };
    live.sphinx_lore_v1.as_mut().unwrap().selected = Some(index);
    Ok(())
}

pub(super) fn record_finalized(
    state: &mut GameState,
    p: &PendingCast,
) -> Result<Option<u64>, String> {
    let Some(choice) = p.sphinx_lore_v1 else {
        return Ok(None);
    };
    if choice.selected.is_none() {
        return Err("Sphinx cast paid before selecting its route".into());
    }
    let stack_item_id = state
        .stack
        .last()
        .ok_or("Sphinx paid placeholder absent")?
        .v4
        .stack_item_id;
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::SphinxLoreCastSelectedV1 {
            choice,
            source: p.source_contract,
            stack_item_id,
        });
    Ok(Some(index))
}
pub(super) fn validate_finalized(
    state: &GameState,
    item: &StackItem,
    pending: bool,
) -> Result<(), String> {
    if pending || item.is_copy {
        return Ok(());
    }
    let Some(source) = item.v4.source_contract else {
        return Ok(());
    };
    let selected = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::SphinxLoreCastSelectedV1 {
                choice,
                source: s,
                stack_item_id,
            } if *stack_item_id == item.v4.stack_item_id => Some((*choice, *s)),
            _ => None,
        })
        .collect::<Vec<_>>();
    let granted = source
        .spell_cast_origin
        .is_some_and(|o| matches!(o.route, SpellCastRouteV4::SphinxLoreFlashbackV1 { .. }));
    let bound = source
        .finalized_cast_binding
        .and_then(|b| b.sphinx_lore_cast_v1);
    if selected.is_empty() && !granted && bound.is_none() {
        return Ok(());
    }
    let [(choice, original)] = selected.as_slice() else {
        return Err("Sphinx original paid route receipt absent or duplicated".into());
    };
    let bound = bound.ok_or("Sphinx paid receipt reference erased")?;
    if !matches!(state.engine.event_history.get(usize::try_from(bound).map_err(|_|"Sphinx paid receipt width")?),Some(CommittedEvent::SphinxLoreCastSelectedV1{choice:c,source:s,stack_item_id})if c==choice&&s==original&&*stack_item_id==item.v4.stack_item_id)
    {
        return Err("Sphinx paid receipt reference changed".into());
    }
    let mut unfinalized = source;
    unfinalized.finalized_cast_binding = None;
    unfinalized.cast_method = original.cast_method;
    unfinalized
        .spell_cast_origin
        .as_mut()
        .ok_or("Sphinx finalized origin erased")?
        .finalized_method = None;
    let record = options_record(state, choice.options_history_index)?;
    let route = *record
        .routes
        .get(usize::from(
            choice.selected.ok_or("Sphinx paid route unselected")?,
        ))
        .ok_or("Sphinx paid route index")?;
    if *original != unfinalized
        || record.spell != item.source
        || record.controller != item.controller
        || record.card_def != source.card_def
        || (method(route) != source.cast_method
            && !(route == RouteV1::Normal && source.cast_method == CastMethodV4::Alternative))
        || source.spell_cast_origin.is_none_or(|o| {
            o.origin_zone_change_count != record.graveyard_generation
                || o.route != route_origin(route, item.controller, record.graveyard_generation)
        })
    {
        return Err("Sphinx finalized route/source changed".into());
    }
    Ok(())
}
