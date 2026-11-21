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
    Snapcaster,
    BeamMeUp,
    Breach,
    Retrace,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CostGrant {
    Sphinx(u64),
    Snapcaster,
    Retrace,
}
impl CostGrant {
    pub(super) fn selected(state: &GameState, id: ObjectId) -> Option<Self> {
        match state.objects.try_get(id)?.v4.spell_cast_origin?.route {
            SpellCastRouteV4::SnapcasterFlashbackV1 => Some(Self::Snapcaster),
            SpellCastRouteV4::SphinxLoreFlashbackV1 { installation, .. } => {
                Some(Self::Sphinx(installation))
            }
            SpellCastRouteV4::RetraceV1 => Some(Self::Retrace),
            _ => None,
        }
    }
    pub(super) fn permits(
        self,
        state: &GameState,
        id: ObjectId,
        player: PlayerId,
        method: CastMethodV4,
    ) -> bool {
        let Some(o) = state.objects.try_get(id) else {
            return false;
        };
        if o.owner != player {
            return false;
        }
        let generation = match o.zone {
            Zone::Graveyard => o.zone_change_count,
            Zone::Stack if Self::selected(state, id) == Some(self) => {
                let Some(origin) = o.v4.spell_cast_origin else {
                    return false;
                };
                if origin.origin_zone != Zone::Graveyard
                    || origin.origin_zone_change_count.checked_add(1) != Some(o.zone_change_count)
                {
                    return false;
                }
                origin.origin_zone_change_count
            }
            _ => return false,
        };
        match self {
            Self::Sphinx(i) => {
                method == CastMethodV4::Flashback && lore::quote_grant(state, id, player, i)
            }
            Self::Snapcaster => {
                method == CastMethodV4::Flashback
                    && crate::eternal_lurrus_v1::flashback_granted(state, id, generation)
            }
            Self::Retrace => {
                matches!(
                    method,
                    CastMethodV4::Normal | CastMethodV4::Alternative | CastMethodV4::Omen
                ) && crate::eternal_vintage_e7b_v1::retrace_permission(state, id, player)
            }
        }
    }
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
pub(crate) fn options_record(state: &GameState, index: u64) -> Result<&OptionsV1, String> {
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
                || (card.zone_change_count == options.graveyard_generation
                    && card.card_def != options.card_def)
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
                || !(options
                    .routes
                    .iter()
                    .any(|r| matches!(r, RouteV1::Granted { .. } | RouteV1::Snapcaster))
                    || cfg!(feature = "eternal-fixtures")
                        && (crate::eternal_pauper_pf1c_v1::beam_me_up_cost(d).is_some()
                            || d.flashback.is_some()
                            || d.escape.is_some()))
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
                    RouteV1::Snapcaster
                        if crate::eternal_lurrus_v1::flashback_granted(
                            state,
                            options.spell,
                            options.graveyard_generation,
                        ) => {}
                    RouteV1::BeamMeUp
                        if crate::eternal_pauper_pf1c_v1::beam_me_up_cost(d).is_some() => {}
                    RouteV1::Breach
                        if crate::eternal_lurrus_v1::escape_granted(
                            state,
                            options.spell,
                            options.graveyard_generation,
                        ) => {}
                    RouteV1::Retrace
                        if crate::eternal_vintage_e7b_v1::retrace_permission(
                            state,
                            options.spell,
                            options.controller,
                        ) => {}
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
        RouteV1::Normal | RouteV1::Retrace => CastMethodV4::Normal,
        RouteV1::BeamMeUp => CastMethodV4::Alternative,
        RouteV1::Escape | RouteV1::Breach => CastMethodV4::Escape,
        _ => CastMethodV4::Flashback,
    }
}
fn route_origin(route: RouteV1, player: PlayerId, generation: u32) -> SpellCastRouteV4 {
    match route {
        RouteV1::Normal => SpellCastRouteV4::GraveyardPermissionV1 {
            holder: player,
            permission_zone_change_count: generation,
        },
        RouteV1::Escape | RouteV1::Breach => SpellCastRouteV4::GraveyardEscape,
        RouteV1::BeamMeUp => SpellCastRouteV4::BeamMeUpV1,
        RouteV1::Snapcaster => SpellCastRouteV4::SnapcasterFlashbackV1,
        RouteV1::Retrace => SpellCastRouteV4::RetraceV1,
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
        Some(
            SpellCastRouteV4::SphinxLoreFlashbackV1 { .. }
                | SpellCastRouteV4::SnapcasterFlashbackV1
        )
    )
}
fn route_quote(state: &GameState, id: ObjectId, player: PlayerId, route: RouteV1) -> bool {
    let grant = match route {
        RouteV1::Granted { installation } => Some(CostGrant::Sphinx(installation)),
        RouteV1::Snapcaster => Some(CostGrant::Snapcaster),
        RouteV1::Retrace => Some(CostGrant::Retrace),
        _ => None,
    };
    is_castable_with_cost_grant(player, id, method(route), state, grant)
}
pub(super) fn routes(state: &GameState, id: ObjectId, player: PlayerId) -> Vec<RouteV1> {
    let o = state.objects.get(id);
    if o.zone != Zone::Graveyard || o.owner != player {
        return vec![];
    }
    let d = &card_def::CARD_DEFS[usize::from(o.card_def)];
    let grants = lore::active_grants(state, id, player);
    let snapcaster = cfg!(feature = "eternal-fixtures")
        && crate::eternal_lurrus_v1::flashback_granted(state, id, o.zone_change_count);
    let beam = crate::eternal_pauper_pf1c_v1::beam_me_up_cost(d).is_some();
    if grants.is_empty() && !snapcaster && !beam {
        return vec![];
    }
    let mut routes = vec![];
    if beam {
        routes.push(RouteV1::BeamMeUp);
    }
    if d.flashback.is_some() && d.escape.is_none() {
        routes.push(RouteV1::PrintedFlashback);
    }
    if d.escape.is_some() && d.flashback.is_none() {
        routes.push(RouteV1::Escape);
    }
    if cfg!(feature = "eternal-fixtures")
        && d.escape.is_none()
        && crate::eternal_lurrus_v1::escape_granted(state, id, o.zone_change_count)
    {
        routes.push(RouteV1::Breach);
    }
    if crate::standard_cards_v1::graveyard_cast_granted_without_retrace(
        state,
        id,
        player,
        o.zone_change_count,
    ) {
        routes.push(RouteV1::Normal);
    }
    if cfg!(feature = "eternal-fixtures")
        && crate::eternal_vintage_e7b_v1::retrace_permission(state, id, player)
    {
        routes.push(RouteV1::Retrace);
    }
    if snapcaster {
        routes.push(RouteV1::Snapcaster);
    }
    routes.extend(
        grants
            .into_iter()
            .map(|installation| RouteV1::Granted { installation }),
    );
    // Keep legacy single-route announcements when no competing permission exists.
    if grants_are_absent(&routes) && routes.len() <= 1 {
        return vec![];
    }
    routes.retain(|r| route_quote(state, id, player, *r));
    // An unaffordable new alternative must not replace an independently legal
    // printed/granted predecessor route or manufacture an empty choice stage.
    if !cfg!(feature = "eternal-fixtures") && grants_are_absent(&routes) {
        return vec![];
    }
    if routes.len() > usize::from(u16::MAX) {
        return vec![];
    }
    routes
}
fn grants_are_absent(routes: &[RouteV1]) -> bool {
    !routes.iter().any(|r| matches!(r, RouteV1::Granted { .. }))
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
    if options.routes.contains(&RouteV1::Breach) {
        crate::eternal_lurrus_v1::note_breach_escape(state, id, options.graveyard_generation);
    }
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
            RouteV1::Snapcaster | RouteV1::BeamMeUp | RouteV1::Breach | RouteV1::Retrace => {} // options_record validates the grant.
            RouteV1::Normal
                if crate::standard_cards_v1::graveyard_cast_granted_without_retrace(
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
        && crate::eternal_pauper_pf1c_v1::beam_me_up_cost(
            &card_def::CARD_DEFS[source.card_def as usize],
        )
        .is_none()
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
            && !(matches!(route, RouteV1::Normal | RouteV1::Retrace)
                && source.cast_method == CastMethodV4::Alternative))
        || source.spell_cast_origin.is_none_or(|o| {
            o.origin_zone_change_count != record.graveyard_generation
                || o.route != route_origin(route, item.controller, record.graveyard_generation)
        })
    {
        return Err("Sphinx finalized route/source changed".into());
    }
    Ok(())
}

/// Expose the original paid selection, without treating a cast placeholder or
/// a virtual copy as a new graveyard permission.
pub(crate) fn published_choice(
    state: &GameState,
    item: &StackItem,
) -> Result<Option<ChoiceV1>, String> {
    if item.is_copy
        || state.engine.pending_cast.as_ref().is_some_and(|p| {
            p.spell == item.source
                && state
                    .stack
                    .last()
                    .is_some_and(|last| last.v4.stack_item_id == item.v4.stack_item_id)
        })
    {
        return Ok(None);
    }
    validate_finalized(state, item, false)?;
    let Some(index) = item
        .v4
        .source_contract
        .and_then(|s| s.finalized_cast_binding)
        .and_then(|binding| binding.sphinx_lore_cast_v1)
    else {
        return Ok(None);
    };
    match state
        .engine
        .event_history
        .get(usize::try_from(index).map_err(|_| "Sphinx paid receipt width")?)
    {
        Some(CommittedEvent::SphinxLoreCastSelectedV1 { choice, .. }) => Ok(Some(*choice)),
        _ => Err("Sphinx paid receipt absent".into()),
    }
}
