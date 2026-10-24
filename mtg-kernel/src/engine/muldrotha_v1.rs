//! FDN243. An explicit source/type permission, never a blanket graveyard grant.
use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PermissionV1 {
    pub source: AbilitySourceContractV4,
    pub permanent_type: CardType,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionsV1 {
    pub card: EffectObjectBinding,
    pub card_def: u16,
    pub controller: PlayerId,
    pub turn: u32,
    pub land: bool,
    pub ordinary: bool,
    pub permissions: Vec<PermissionV1>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingV1 {
    pub options_history_index: u64,
    pub selected: Option<u16>,
}
/// Absent state adds no bytes or hashes to any predecessor world.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct StateV1(pub Option<PendingV1>);
impl StateV1 {
    pub fn is_empty(&self) -> bool {
        self.0.is_none()
    }
}
impl std::hash::Hash for StateV1 {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        if let Some(p) = self.0 {
            "muldrotha-original-permission/v1".hash(h);
            p.hash(h);
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayedV1 {
    pub options_history_index: u64,
    pub permission: PermissionV1,
    pub card: EffectObjectBinding,
    pub controller: PlayerId,
    pub turn: u32,
    /// The pre-finalization physical cast contract, or None for a land play.
    pub spell: Option<(StackSourceContractV4, StackItemId)>,
}

pub(crate) fn definition(id: u16) -> bool {
    cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) && id == 489
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
            d.name == "Muldrotha, the Gravetide"
                && d.capability == card_def::CardCapability::Partial
                && d.cost
                    == crate::mana::Cost {
                        pips: &[
                            crate::mana::Pip::Colored(crate::mana::ManaColor::B),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::G),
                            crate::mana::Pip::Colored(crate::mana::ManaColor::U),
                        ],
                        generic: 3,
                        x_count: 0,
                    }
                && d.generic_cost_reduction.is_none()
                && d.ward_cost.is_none()
                && !d.spell_cannot_be_countered
                && !d.records_mana_spent
                && d.equipment.is_none()
                && d.types == [CardType::Creature]
                && d.subtypes == [card_def::Subtype::Elemental, card_def::Subtype::Avatar]
                && d.supertypes == [card_def::Supertype::Legendary]
                && d.power == Some(6)
                && d.toughness == Some(6)
                && d.starting_loyalty.is_none()
                && d.enters_with_plus_one_counters.is_none()
                && d.controlled_counter_keyword.is_none()
                && d.game_outcome_protection_v1.is_none()
                && d.controlled_creature_grant.is_none()
                && !d.is_land
                && d.produces_mana.is_empty()
                && d.colors
                    == [
                        crate::mana::ManaColor::B,
                        crate::mana::ManaColor::G,
                        crate::mana::ManaColor::U,
                    ]
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
fn active(state: &GameState, source: AbilitySourceContractV4, player: PlayerId) -> bool {
    state.objects.try_get(source.source).is_some_and(|o| {
        o.zone == Zone::Battlefield
            && o.controller == player
            && state.players[player.index()]
                .battlefield
                .contains(&source.source)
            && o.zone_change_count == source.zone_change_count
            && o.v4.face_index == 0
            && o.v4.face_down_v1.is_none()
            && definition(o.card_def)
            && AbilitySourceContractV4::capture(state, source.source) == source
            && crate::continuous_characteristics_v1::printed_abilities_active(state, source.source)
    })
}
fn spent(state: &GameState, permission: PermissionV1, turn: u32) -> bool {
    state.engine.event_history.iter().any(|event| {
        matches!(event,
        CommittedEvent::MuldrothaPlayedV1 { played }
        if played.turn == turn && played.permission.source.source == permission.source.source
            && played.permission.source.zone_change_count == permission.source.zone_change_count
            && played.permission.permanent_type == permission.permanent_type)
    })
}
pub(super) fn permissions(
    state: &GameState,
    card: ObjectId,
    player: PlayerId,
) -> Vec<PermissionV1> {
    let Some(o) = state.objects.try_get(card) else {
        return vec![];
    };
    let Some(d) = card_def::CARD_DEFS.get(o.card_def as usize) else {
        return vec![];
    };
    if player != state.active_player
        || o.zone != Zone::Graveyard
        || o.owner != player
        || !state.players[player.index()].graveyard.contains(&card)
        || o.v4.is_token
        || o.spell_copy_origin.is_some()
        || !d.is_executable()
    {
        return vec![];
    }
    let mut out = vec![];
    for &id in &state.players[player.index()].battlefield {
        let source = AbilitySourceContractV4::capture(state, id);
        if !active(state, source, player) {
            continue;
        }
        for permanent_type in [
            CardType::Land,
            CardType::Creature,
            CardType::Artifact,
            CardType::Enchantment,
            CardType::Planeswalker,
        ] {
            let p = PermissionV1 {
                source,
                permanent_type,
            };
            if d.has_type(permanent_type)
                && (if d.is_playable_land() {
                    permanent_type == CardType::Land
                } else {
                    permanent_type != CardType::Land
                })
                && !spent(state, p, state.turn_key_v1())
            {
                out.push(p);
            }
        }
    }
    out
}
pub(super) fn allowed(state: &GameState, card: ObjectId, player: PlayerId) -> bool {
    !permissions(state, card, player).is_empty()
}
fn options(state: &GameState, index: u64) -> Result<&OptionsV1, String> {
    let Some(CommittedEvent::MuldrothaOptionsV1 { options }) = usize::try_from(index)
        .ok()
        .and_then(|i| state.engine.event_history.get(i))
    else {
        return Err("Muldrotha original option record absent".into());
    };
    let d = card_def::CARD_DEFS
        .get(options.card_def as usize)
        .ok_or("Muldrotha original printing absent")?;
    if options.card.expected_zone != Zone::Graveyard
        || !d.is_executable()
        || options.land != d.is_playable_land()
        || options.permissions.is_empty()
        || options.permissions.iter().any(|p| {
            !definition(p.source.card_def)
                || p.source.zone != Zone::Battlefield
                || p.source.controller != options.controller
                || !d.has_type(p.permanent_type)
                || (if options.land {
                    p.permanent_type != CardType::Land
                } else {
                    p.permanent_type == CardType::Land
                })
                || ![
                    CardType::Land,
                    CardType::Creature,
                    CardType::Artifact,
                    CardType::Enchantment,
                    CardType::Planeswalker,
                ]
                .contains(&p.permanent_type)
        })
    {
        return Err("Muldrotha original source/type/printing changed".into());
    }
    for p in &options.permissions {
        validate_historical_ability_source_contract(state, p.source)?;
    }
    Ok(options)
}
fn selected(state: &GameState, pending: PendingV1) -> Result<Option<PermissionV1>, String> {
    let o = options(state, pending.options_history_index)?;
    let i = usize::from(
        pending
            .selected
            .ok_or("Muldrotha permission not selected")?,
    );
    if o.ordinary && i == 0 {
        return Ok(None);
    }
    o.permissions
        .get(
            i.checked_sub(usize::from(o.ordinary))
                .ok_or("Muldrotha option index")?,
        )
        .copied()
        .map(Some)
        .ok_or("Muldrotha option index outside menu".into())
}
pub(super) fn begin(
    state: &mut GameState,
    card: ObjectId,
    player: PlayerId,
    land: bool,
) -> Result<(), String> {
    let ps = permissions(state, card, player);
    if ps.is_empty() {
        return Err("Muldrotha has no unused permission".into());
    }
    let o = state.objects.get(card);
    let ordinary = !land
        && (unambiguous_graveyard_cast_method(&card_def::CARD_DEFS[o.card_def as usize])
            .is_some_and(|m| is_castable_now(player, card, m, state))
            || graveyard_permission_zone_change_count(player, card, state).is_some()
            || greatwurm_cast_v1::allowed(state, card, player)
            || crate::standard_creatures_v1::graveyard_adventure_allowed(state, card, player)
            || !sphinx_lore_cast_v1::routes(state, card, player).is_empty());
    let options = OptionsV1 {
        card: EffectObjectBinding {
            object: card,
            expected_zone: o.zone,
            expected_zone_change_count: o.zone_change_count,
        },
        card_def: o.card_def,
        controller: player,
        turn: state.turn_key_v1(),
        land,
        ordinary,
        permissions: ps,
    };
    let index = state.engine.event_history.len() as u64;
    state
        .engine
        .event_history
        .push(CommittedEvent::MuldrothaOptionsV1 { options });
    state.engine.muldrotha_v1.0 = Some(PendingV1 {
        options_history_index: index,
        selected: None,
    });
    Ok(())
}
pub(super) fn decision(state: &GameState) -> Result<Option<Decision>, String> {
    let Some(p) = state.engine.muldrotha_v1.0 else {
        return Ok(None);
    };
    validate_choice(state, p)?;
    if p.selected.is_some() {
        return Ok(None);
    }
    let o = options(state, p.options_history_index)?;
    Ok(Some(Decision::ChooseEffectOption {
        player: o.controller,
        source: o.card.object,
        option_count: u16::try_from(o.permissions.len() + usize::from(o.ordinary))
            .map_err(|_| "Muldrotha menu exceeds native u16 width")?,
    }))
}
fn validate_choice(state: &GameState, p: PendingV1) -> Result<(), String> {
    let o = options(state, p.options_history_index)?;
    let live = state
        .objects
        .try_get(o.card.object)
        .ok_or("Muldrotha graveyard card absent")?;
    if live.zone != o.card.expected_zone
        || live.zone_change_count != o.card.expected_zone_change_count
        || live.card_def != o.card_def
        || live.owner != o.controller
        || o.turn != state.turn_key_v1()
        || o.controller != state.active_player
        || state.priority_player != o.controller
        || permissions(state, o.card.object, o.controller) != o.permissions
    {
        return Err("Muldrotha original card/source/budget/turn changed".into());
    }
    if o.land && !land_drop_candidates(o.controller, state).contains(&o.card.object)
        || !o.land && !is_castable_now(o.controller, o.card.object, CastMethodV4::Normal, state)
    {
        return Err("Muldrotha play timing or costs changed".into());
    }
    Ok(())
}
pub(super) fn answer(state: &mut GameState, index: u16) -> Result<(), String> {
    let mut p = state
        .engine
        .muldrotha_v1
        .0
        .ok_or("Muldrotha choice absent")?;
    validate_choice(state, p)?;
    if p.selected.is_some() {
        return Err("Muldrotha permission already selected".into());
    }
    p.selected = Some(index);
    let permission = selected(state, p)?;
    let o = options(state, p.options_history_index)?.clone();
    state
        .engine
        .event_history
        .push(CommittedEvent::MuldrothaSelectedV1 { pending: p });
    state.engine.muldrotha_v1.0 = if permission.is_some() { Some(p) } else { None };
    if o.land {
        let d = &card_def::CARD_DEFS[o.card_def as usize];
        if let Some(excluded_color) = d.as_enters_choose_color_other_than {
            state.engine.pending_land_play = Some(PendingLandPlay {
                source: o.card.object,
                source_zone_change_count: o.card.expected_zone_change_count,
                controller: o.controller,
                origin_zone: Zone::Graveyard,
                excluded_color,
            });
        } else {
            play_land(state, o.controller, o.card.object, None, false)?;
        }
    } else {
        begin_cast(state, o.controller, o.card.object);
        state.engine.muldrotha_v1.0 = None;
    }
    Ok(())
}
pub(super) fn announcement(state: &GameState, card: ObjectId, player: PlayerId) -> Option<u64> {
    let p = state.engine.muldrotha_v1.0?;
    let o = options(state, p.options_history_index).ok()?;
    (o.card.object == card
        && o.controller == player
        && !o.land
        && selected(state, p).ok().flatten().is_some())
    .then_some(p.options_history_index)
}
fn route_index(source: StackSourceContractV4) -> Option<u64> {
    match source.spell_cast_origin?.route {
        SpellCastRouteV4::MuldrothaGraveyardV1 {
            options_history_index,
        } => Some(options_history_index),
        _ => None,
    }
}
fn selected_permission(
    state: &GameState,
    source: StackSourceContractV4,
) -> Result<PermissionV1, String> {
    let index = route_index(source).ok_or("Muldrotha original cast route absent")?;
    let selections: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::MuldrothaSelectedV1 { pending }
                if pending.options_history_index == index =>
            {
                Some(*pending)
            }
            _ => None,
        })
        .collect();
    let [choice] = selections.as_slice() else {
        return Err("Muldrotha original selection absent or duplicated".into());
    };
    selected(state, *choice)?.ok_or("Muldrotha route used ordinary permission".into())
}
fn form_matches(def: &card_def::CardDef, permission: PermissionV1, method: CastMethodV4) -> bool {
    permission.permanent_type != CardType::Land
        && !def.is_playable_land()
        && matches!(
            method,
            CastMethodV4::Normal | CastMethodV4::Alternative | CastMethodV4::Bestow
        )
        && (method != CastMethodV4::Bestow || supported_bestow(def).is_some())
        && crate::source_type_hexproof_v1::spell_types(def, method)
            .is_ok_and(|types| types.contains(permission.permanent_type))
}
/// Only the authenticated Muldrotha route constrains these forms. Other
/// independently owned graveyard, hand and exile permissions keep their menus.
pub(super) fn form_allowed(state: &GameState, pending: &PendingCast, method: CastMethodV4) -> bool {
    route_index(pending.source_contract).is_none()
        || selected_permission(state, pending.source_contract).is_ok_and(|permission| {
            card_def::CARD_DEFS
                .get(pending.source_contract.card_def as usize)
                .is_some_and(|def| form_matches(def, permission, method))
        })
}
pub(super) fn grants_permanent_form(state: &GameState, pending: &PendingCast) -> bool {
    route_index(pending.source_contract).is_some()
        && validate_contract(state, pending.source_contract).is_ok()
        && form_allowed(state, pending, CastMethodV4::Normal)
}
pub(super) fn validate_contract(
    state: &GameState,
    source: StackSourceContractV4,
) -> Result<(), String> {
    let index = route_index(source).ok_or("Muldrotha original cast route absent")?;
    let o = options(state, index)?;
    let origin = source.spell_cast_origin.ok_or("Muldrotha origin absent")?;
    if o.land
        || source.source != o.card.object
        || source.card_def != o.card_def
        || source.owner != o.controller
        || source.controller != o.controller
        || source.zone != Zone::Stack
        || o.card.expected_zone_change_count.checked_add(1) != Some(source.zone_change_count)
        || origin.origin_zone != Zone::Graveyard
        || origin.origin_zone_change_count != o.card.expected_zone_change_count
    {
        return Err("Muldrotha cast physical source/form/incarnation changed".into());
    }
    // The actual choice receipt is independent of mutable pending/source copies.
    let permission = selected_permission(state, source)?;
    let def = &card_def::CARD_DEFS[source.card_def as usize];
    if !form_matches(def, permission, source.cast_method) {
        return Err("Muldrotha chosen type does not authorize the permanent spell form".into());
    }
    Ok(())
}
pub(super) fn validate_pending(state: &GameState, p: &PendingCast) -> Result<(), String> {
    if route_index(p.source_contract).is_none() {
        return Ok(());
    }
    validate_contract(state, p.source_contract)?;
    let index = route_index(p.source_contract).unwrap();
    let o = options(state, index)?;
    let permission = state
        .engine
        .event_history
        .iter()
        .find_map(|e| match e {
            CommittedEvent::MuldrothaSelectedV1 { pending }
                if pending.options_history_index == index =>
            {
                selected(state, *pending).ok().flatten()
            }
            _ => None,
        })
        .ok_or("Muldrotha original selected source absent")?;
    if o.turn != state.turn_key_v1()
        || state.active_player != p.controller
        || p.origin_zone != Zone::Graveyard
        || p.is_flashback
        || p.sphinx_lore_v1.is_some()
        || p.greatwurm_v1.is_some()
        || !active(state, permission.source, p.controller)
        || spent(state, permission, o.turn)
        || p.mode_chosen.is_some()
            && !form_allowed(
                state,
                p,
                finalized_cast_method(
                    p,
                    p.source_contract.cast_method,
                    &card_def::CARD_DEFS[p.source_contract.card_def as usize],
                ),
            )
    {
        return Err("Muldrotha pending route changed".into());
    }
    Ok(())
}
pub(super) fn record_cast(state: &mut GameState, p: &PendingCast) -> Result<(), String> {
    let Some(index) = route_index(p.source_contract) else {
        return Ok(());
    };
    // The ordinary payment plan has just committed atomically. Its legitimate
    // additional costs may sacrifice the permission source itself.
    validate_contract(state, p.source_contract)?;
    if !form_allowed(
        state,
        p,
        finalized_cast_method(
            p,
            p.source_contract.cast_method,
            &card_def::CARD_DEFS[p.source_contract.card_def as usize],
        ),
    ) {
        return Err("Muldrotha payment changed the chosen permanent type or spell form".into());
    }
    let o = options(state, index)?.clone();
    let choice = state
        .engine
        .event_history
        .iter()
        .find_map(|e| match e {
            CommittedEvent::MuldrothaSelectedV1 { pending }
                if pending.options_history_index == index =>
            {
                Some(*pending)
            }
            _ => None,
        })
        .ok_or("Muldrotha selection absent")?;
    let permission = selected(state, choice)?.ok_or("Muldrotha permission absent")?;
    if spent(state, permission, o.turn) {
        return Err("Muldrotha type already consumed".into());
    }
    let stack_item_id = state
        .stack
        .last()
        .ok_or("Muldrotha cast placeholder absent")?
        .v4
        .stack_item_id;
    state
        .engine
        .event_history
        .push(CommittedEvent::MuldrothaPlayedV1 {
            played: PlayedV1 {
                options_history_index: index,
                permission,
                card: o.card,
                controller: o.controller,
                turn: o.turn,
                spell: Some((p.source_contract, stack_item_id)),
            },
        });
    Ok(())
}
pub(super) fn land_input(
    state: &GameState,
    id: ObjectId,
    player: PlayerId,
) -> Result<Option<PlayedV1>, String> {
    if state.objects.get(id).zone != Zone::Graveyard {
        return Ok(None);
    }
    let p = state
        .engine
        .muldrotha_v1
        .0
        .ok_or("graveyard land lost original Muldrotha choice")?;
    validate_choice(state, p)?;
    let o = options(state, p.options_history_index)?;
    if !o.land || o.card.object != id || o.controller != player {
        return Err("Muldrotha land changed".into());
    }
    Ok(Some(PlayedV1 {
        options_history_index: p.options_history_index,
        permission: selected(state, p)?.ok_or("Muldrotha land permission absent")?,
        card: o.card,
        controller: player,
        turn: o.turn,
        spell: None,
    }))
}
pub(super) fn record_land(state: &mut GameState, input: Option<PlayedV1>) {
    if let Some(played) = input {
        state
            .engine
            .event_history
            .push(CommittedEvent::MuldrothaPlayedV1 { played });
        state.engine.muldrotha_v1.0 = None;
    }
}
pub(super) fn validate_finalized(
    state: &GameState,
    item: &StackItem,
    pending: bool,
) -> Result<(), String> {
    if pending || item.is_copy {
        return Ok(());
    }
    let receipts: Vec<_> = state
        .engine
        .event_history
        .iter()
        .filter_map(|e| match e {
            CommittedEvent::MuldrothaPlayedV1 { played }
                if played
                    .spell
                    .is_some_and(|(_, id)| id == item.v4.stack_item_id) =>
            {
                Some(*played)
            }
            _ => None,
        })
        .collect();
    if receipts.len() > 1 {
        return Err("Muldrotha physical stack incarnation has duplicate paid receipts".into());
    }
    let own = receipts.first().copied();
    let source = item.v4.source_contract;
    if own.is_none() && source.is_none_or(|s| route_index(s).is_none()) {
        return Ok(());
    }
    let source = source.ok_or("Muldrotha finalized source erased")?;
    validate_contract(state, source)?;
    let played = own.ok_or("Muldrotha finalized payment receipt absent")?;
    let (before, _) = played.spell.ok_or("Muldrotha paid cast absent")?;
    let mut restored = source;
    restored.finalized_cast_binding = None;
    restored.cast_method = before.cast_method;
    restored
        .spell_cast_origin
        .as_mut()
        .ok_or("Muldrotha origin erased")?
        .finalized_method = None;
    if before != restored || route_index(source) != Some(played.options_history_index) {
        return Err("Muldrotha finalized cast changed original paid contract".into());
    }
    Ok(())
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    if state.engine.event_log.iter().any(|e| {
        matches!(
            e,
            CommittedEvent::MuldrothaOptionsV1 { .. }
                | CommittedEvent::MuldrothaSelectedV1 { .. }
                | CommittedEvent::MuldrothaPlayedV1 { .. }
        )
    }) {
        return Err("Muldrotha owning receipts cannot enter trigger log".into());
    }
    if state.engine.muldrotha_v1.0.is_none()
        && !state.engine.event_history.has_any(&[
            EventKindV1::MuldrothaOptionsV1,
            EventKindV1::MuldrothaSelectedV1,
            EventKindV1::MuldrothaPlayedV1,
        ])
    {
        return Ok(());
    }
    for (at, event) in state.engine.event_history.iter().enumerate() {
        match event {
            CommittedEvent::MuldrothaOptionsV1 { .. } => {
                let o = options(state, at as u64)?;
                let choices: Vec<_> = state
                    .engine
                    .event_history
                    .iter()
                    .enumerate()
                    .filter_map(|(i, e)| match e {
                        CommittedEvent::MuldrothaSelectedV1 { pending }
                            if pending.options_history_index == at as u64 =>
                        {
                            Some((i, *pending))
                        }
                        _ => None,
                    })
                    .collect();
                if choices.len() > 1 || choices.first().is_some_and(|(i, _)| *i <= at) {
                    return Err("Muldrotha original selection duplicated or reordered".into());
                }
                if choices.is_empty()
                    && state.engine.muldrotha_v1.0
                        != Some(PendingV1 {
                            options_history_index: at as u64,
                            selected: None,
                        })
                {
                    return Err("Muldrotha issued permission choice erased".into());
                }
                if choices.is_empty() && o.turn != state.turn_key_v1() {
                    return Err("Muldrotha pending permission crossed turn".into());
                }
            }
            CommittedEvent::MuldrothaSelectedV1 { pending } => {
                if usize::try_from(pending.options_history_index).map_or(true, |i| i >= at) {
                    return Err("Muldrotha selection precedes original options".into());
                }
                if selected(state, *pending)?.is_some() {
                    let o = options(state, pending.options_history_index)?;
                    let played = state.engine.event_history.iter().any(|e| {
                        matches!(e,
                        CommittedEvent::MuldrothaPlayedV1 { played }
                        if played.options_history_index == pending.options_history_index)
                    });
                    let completed = state.engine.event_history[at + 1..]
                        .iter()
                        .any(|e| match e {
                            CommittedEvent::PublishedCastIdentityV1 { receipt } => {
                                !o.land
                                    && receipt.spell == o.card.object
                                    && receipt.origin_zone == Zone::Graveyard
                                    && receipt.origin_zone_change_count
                                        == o.card.expected_zone_change_count
                            }
                            CommittedEvent::ZoneChange {
                                object,
                                from: Zone::Graveyard,
                                ..
                            } => o.land && *object == o.card.object,
                            _ => false,
                        });
                    if completed && !played {
                        return Err("Muldrotha completed play lost consumed type receipt".into());
                    }
                    let live = state
                        .objects
                        .try_get(o.card.object)
                        .ok_or("Muldrotha original physical card absent")?;
                    if !played
                        && live.zone == Zone::Graveyard
                        && live.zone_change_count == o.card.expected_zone_change_count
                        && state.engine.muldrotha_v1.0 != Some(*pending)
                    {
                        return Err("Muldrotha selected original play owner erased".into());
                    }
                }
            }
            CommittedEvent::MuldrothaPlayedV1 { played } => {
                let o = options(state, played.options_history_index)?;
                let choice = state.engine.event_history[..at]
                    .iter()
                    .find_map(|e| match e {
                        CommittedEvent::MuldrothaSelectedV1 { pending }
                            if pending.options_history_index == played.options_history_index =>
                        {
                            Some(*pending)
                        }
                        _ => None,
                    })
                    .ok_or("Muldrotha play lacks preceding selection")?;
                if selected(state, choice)? != Some(played.permission)
                    || o.card != played.card || o.turn != played.turn || o.controller != played.controller
                    || o.land != played.spell.is_none()
                    || state.engine.event_history[..at].iter().any(|e| matches!(e,
                        CommittedEvent::MuldrothaPlayedV1 { played: old }
                            if old.options_history_index == played.options_history_index
                            || old.turn == played.turn
                                && old.permission.source.source == played.permission.source.source
                                && old.permission.source.zone_change_count == played.permission.source.zone_change_count
                                && old.permission.permanent_type == played.permission.permanent_type))
                { return Err("Muldrotha play changed original selection or reused type budget".into()); }
                if let Some((source, _)) = played.spell {
                    validate_contract(state, source)?;
                }
            }
            _ => {}
        }
    }
    if let Some(p) = state.engine.muldrotha_v1.0 {
        validate_choice(state, p)?;
    }
    Ok(())
}
pub(crate) fn require_legacy(state: &GameState, viewer: Option<PlayerId>) -> Result<(), String> {
    let visible = state.objects.iter().any(|(id,o)| {
        crate::optional_trigger_public_v1::reference(state,id,viewer).is_some_and(|r|
            matches!(r.identity,crate::optional_trigger_public_v1::IdentityV1::Known{card_id} if definition(card_id)))
            || viewer.is_some_and(|v|o.v4.face_down_v1.is_some_and(|f|f.lookers&(1<<v.index())!=0)&&definition(o.card_def))
    });
    if visible
        || state.engine.muldrotha_v1.0.is_some()
        || state
            .engine
            .event_history
            .scan_for_v1(&[
                EventKindV1::MuldrothaOptionsV1,
                EventKindV1::MuldrothaSelectedV1,
                EventKindV1::MuldrothaPlayedV1,
            ])
            .iter()
            .any(|event| {
                matches!(
                    event,
                    crate::event::CommittedEvent::MuldrothaOptionsV1 { .. }
                        | crate::event::CommittedEvent::MuldrothaSelectedV1 { .. }
                        | crate::event::CommittedEvent::MuldrothaPlayedV1 { .. }
                )
            })
    {
        Err("Muldrotha source/type permission requires a complete V71 consumer".into())
    } else {
        Ok(())
    }
}

/// Current-slot sampling keeps every factual source/card/type decision intact.
/// Whole-object pools cannot contain any physical object named by this history,
/// even after that original incarnation has departed to a hidden zone.
pub(crate) fn sampler_conflicts_v71(state: &GameState, pool: &[ObjectId]) -> Result<bool, String> {
    validate_state(state)?;
    let ability = |source: AbilitySourceContractV4| {
        pool.contains(&source.source)
            || source.attached_to.is_some_and(|r| pool.contains(&r.object))
    };
    Ok(state.engine.event_history.iter().any(|event| match event {
        CommittedEvent::MuldrothaOptionsV1 { options } => {
            pool.contains(&options.card.object)
                || options.permissions.iter().any(|p| ability(p.source))
        }
        CommittedEvent::MuldrothaPlayedV1 { played } => {
            pool.contains(&played.card.object)
                || ability(played.permission.source)
                || played.spell.is_some_and(|(source, _)| {
                    pool.contains(&source.source)
                        || source.spell_copy_origin.is_some()
                        || source.finalized_cast_binding.is_some_and(|b| {
                            b.chosen_creature_cost
                                .is_some_and(|r| pool.contains(&r.object))
                                || b.mandatory_additional_cost.is_some()
                        })
                })
        }
        // Selection records only refer back to the validated original options.
        _ => false,
    }))
}

pub(crate) fn public_facts_v71(
    state: &GameState,
    viewer: PlayerId,
) -> Result<crate::muldrotha_public_v71::FactsV71, String> {
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
) -> Result<crate::muldrotha_public_v71::FactsV71, String> {
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
) -> Result<crate::muldrotha_public_v71::FactsV71, String> {
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
) -> Result<crate::muldrotha_public_v71::FactsV71, String> {
    use crate::{muldrotha_public_v71::*, optional_trigger_public_v1 as public};
    validate_state(state)?;
    let visible_printings = state
        .objects
        .iter()
        .filter_map(|(id, _)| public::reference(state, id, Some(viewer)))
        .filter(|r| matches!(r.identity, public::IdentityV1::Known { card_id: 489 }))
        .collect();
    let current =
        crate::valkyries_call_public_v62::capture_for_consumer(state, viewer, scope, consumer)?
            .battlefield
            .into_iter()
            .flatten()
            .filter(|r| matches!(r.card.identity, public::IdentityV1::Known { card_id: 489 }))
            .collect();
    let mut budgets = Vec::new();
    for (id, o) in state.objects.iter() {
        if o.zone != Zone::Battlefield || o.v4.face_down_v1.is_some() || !definition(o.card_def) {
            continue;
        }
        let source = AbilitySourceContractV4::capture(state, id);
        let mut unused = Vec::new();
        let mut used = Vec::new();
        for permanent_type in TYPES {
            if spent(
                state,
                PermissionV1 {
                    source,
                    permanent_type,
                },
                state.turn_key_v1(),
            ) {
                used.push(permanent_type)
            } else {
                unused.push(permanent_type)
            }
        }
        budgets.push(BudgetV71 {
            source: public::root(state, source, Some(viewer))?,
            turn: state.turn_key_v1(),
            active_own_turn: state.active_player == o.controller,
            printed_abilities_active:
                crate::continuous_characteristics_v1::printed_abilities_active(state, id),
            unused,
            spent: used,
        });
    }
    let mut plays = Vec::new();
    for (at, e) in state.engine.event_history.iter().enumerate() {
        let CommittedEvent::MuldrothaOptionsV1 { options: o } = e else {
            continue;
        };
        options(state, at as u64)?;
        let chosen = state.engine.event_history.iter().find_map(|e| match e {
            CommittedEvent::MuldrothaSelectedV1 { pending }
                if pending.options_history_index == at as u64 =>
            {
                Some(*pending)
            }
            _ => None,
        });
        let played = state.engine.event_history.iter().find_map(|e| match e {
            CommittedEvent::MuldrothaPlayedV1 { played }
                if played.options_history_index == at as u64 =>
            {
                Some(played)
            }
            _ => None,
        });
        let stage = if chosen.is_some_and(|p| p.selected == Some(0) && o.ordinary) {
            StageV71::OrdinaryRoute
        } else if let Some(p) = played {
            match p.spell {
                None => StageV71::PlayedLand,
                Some((_, id)) if state.stack.iter().any(|r| r.v4.stack_item_id == id) => {
                    StageV71::CastPublished
                }
                Some(_) => StageV71::NoLongerPublished,
            }
        } else if chosen.is_some() {
            StageV71::SelectedPayment
        } else {
            StageV71::PermissionChoice
        };
        let original = public::ReferenceV1 {
            object: o.card.object.0,
            generation: o.card.expected_zone_change_count,
            owner: o.controller.into(),
            controller: o.controller.into(),
            zone: Zone::Graveyard,
            identity: public::IdentityV1::Known {
                card_id: o.card_def,
            },
        };
        let current_original = public::reference(state, o.card.object, Some(viewer))
            .filter(|r| r.generation == o.card.expected_zone_change_count);
        plays.push(PlayV71 {
            occurrence: u32::try_from(plays.len()).map_err(|_| "V71 play count exceeds u32")?,
            card: original,
            current_original,
            controller: o.controller.into(),
            turn: o.turn,
            land: o.land,
            ordinary_available: o.ordinary,
            permissions: o
                .permissions
                .iter()
                .map(|p| {
                    Ok(PermissionV71 {
                        source: public::root(state, p.source, Some(viewer))?,
                        permanent_type: p.permanent_type,
                    })
                })
                .collect::<Result<_, String>>()?,
            selected_menu_index: chosen.and_then(|p| p.selected),
            stage,
        });
    }
    let facts = FactsV71 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        viewer: viewer.into(),
        meaning: crate::rules_meaning_muldrotha_v71::printed_meaning()?,
        visible_printings,
        current,
        budgets,
        plays,
    };
    crate::muldrotha_public_v71::validate(&facts)?;
    Ok(facts)
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
#[path = "muldrotha_v1_tests.rs"]
pub(crate) mod tests;
