//! Explicit public Eternal facts missing from ObservationV5. This is a partial
//! additive supplement, not a complete observation or model-admission permit.
//! Never serialize EternalStateV1: it also contains unrevealed private choices.
use crate::{
    card_def::Subtype,
    rl::{CardStableRefV1, RlContractError},
    state::{GameState, ObjectLinkV4, Zone},
};
use serde::{Deserialize, Serialize};

pub const SCHEMA_V1: &str = "mtg-kernel/eternal-actor-public-choices/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChosenTypeV1 {
    pub source: CardStableRefV1,
    pub chosen_type: Subtype,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChosenNameV1 {
    pub source: CardStableRefV1,
    /// Publicly named printing, not a reference to any object with that name.
    pub named_card_id: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CastManaV1 {
    pub spell: CardStableRefV1,
    pub total: u16,
    pub colored: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SagaAbilitiesV1 {
    pub source: CardStableRefV1,
    pub highest_gained_chapter: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicLinkV1 {
    pub source: CardStableRefV1,
    pub target: CardStableRefV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GrantedLeaveDrawV1 {
    pub source: CardStableRefV1,
    /// The cast trigger can grant this to the spell's next battlefield
    /// incarnation. It is not yet an active battlefield ability in that case.
    pub pending_battlefield_entry: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntryOptionV1 {
    CreatureType(Subtype),
    CardName { printing: u16 },
    PayLifeToEnterUntapped { life: u8, pay: bool },
}

/// Meanings of the finite entry-choice indices. The caller supplies the fresh
/// backend semantic; this helper conveys no executable action authority.
pub fn entry_option_v1(
    state: &GameState,
    action: &crate::rl::ActionSemanticV1,
    known_cavern_types: Option<&[Subtype]>,
) -> Result<Option<EntryOptionV1>, RlContractError> {
    let crate::rl::ActionSemanticV1::ChooseEffectOption {
        actor,
        source,
        option_index,
        option_count,
    } = action
    else {
        return Ok(None);
    };
    let Some(def) = crate::card_def::CARD_DEFS.get(usize::from(source.card_db_id)) else {
        return Err(RlContractError("invalid public entry printing".into()));
    };
    if !matches!(
        def.name,
        "Cavern of Souls" | "Pithing Needle" | "Bridgeworks Battle"
    ) {
        return Ok(None);
    }
    let link = ObjectLinkV4 {
        object: crate::ids::ObjectId(source.arena_id),
        zone_change_count: source.zone_change_count,
    };
    if current_public(state, link, Zone::Battlefield)?.as_ref() != Some(source)
        || source.controller != *actor
        || option_index >= option_count
    {
        return Err(RlContractError("stale public entry option".into()));
    }
    let player = match actor {
        crate::rl::PlayerSeatV1::P0 => crate::ids::PlayerId::P0,
        crate::rl::PlayerSeatV1::P1 => crate::ids::PlayerId::P1,
    };
    let index = usize::from(*option_index);
    Ok(Some(match def.name {
        "Cavern of Souls" => {
            // The sealed owner derives this public menu from its exact own
            // reset mainboard. Do not inspect physical hidden library copies.
            let options = known_cavern_types.ok_or_else(|| {
                RlContractError("Cavern needs authenticated own-deck creature types".into())
            })?;
            if options.len() != usize::from(*option_count) {
                return Err(RlContractError("stale public creature-type menu".into()));
            }
            EntryOptionV1::CreatureType(
                *options
                    .get(index)
                    .ok_or_else(|| RlContractError("missing public creature type".into()))?,
            )
        }
        "Pithing Needle" => {
            require_continuous_needle_sources(state)?;
            let options = crate::eternal_lurrus_v1::needle_name_options(state, player);
            if options.len() != usize::from(*option_count) {
                return Err(RlContractError("stale public card-name menu".into()));
            }
            EntryOptionV1::CardName {
                printing: *options
                    .get(index)
                    .ok_or_else(|| RlContractError("missing public card name".into()))?,
            }
        }
        "Bridgeworks Battle" => {
            if state.objects.get(link.object).v4.face_index != 1 || *option_count > 2 {
                return Err(RlContractError("unexpected public land-face choice".into()));
            }
            EntryOptionV1::PayLifeToEnterUntapped {
                life: 3,
                pay: *option_index == 1,
            }
        }
        _ => unreachable!(),
    }))
}

/// The legacy native Needle menu follows physical stack sources. Until a new
/// native menu uses frozen identities, refuse every departed stack source
/// before calling that helper. A later hidden reappearance never repairs the
/// frozen incarnation comparison, and no current definition controls refusal.
pub(crate) fn require_continuous_needle_sources(state: &GameState) -> Result<(), RlContractError> {
    let refusal = || RlContractError("Needle menu needs continuous public stack sources".into());
    for item in &state.stack {
        let (source, zone, incarnation) = if let Some(frozen) = item.v4.source_contract {
            (frozen.source, frozen.zone, frozen.zone_change_count)
        } else if let Some(frozen) = item.v4.ability_source_contract {
            (frozen.source, frozen.zone, frozen.zone_change_count)
        } else {
            return Err(refusal());
        };
        if source != item.source
            || !matches!(
                zone,
                Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
            )
        {
            return Err(refusal());
        }
        let live = state.objects.try_get(source).ok_or_else(refusal)?;
        if live.zone_change_count != incarnation
            || live.zone != zone
            || live.v4.face_down_v1.is_some()
        {
            return Err(refusal());
        }
    }
    Ok(())
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicChoicesV1 {
    pub schema: String,
    pub energy: [u32; 2],
    pub city_blessing: [bool; 2],
    /// Derived from public cast events, never the card-conditional E8a ledger.
    pub has_cast_spell: [bool; 2],
    pub cavern_types: Vec<ChosenTypeV1>,
    pub needle_names: Vec<ChosenNameV1>,
    pub uncounterable_spells: Vec<CardStableRefV1>,
    pub cast_mana: Vec<CastManaV1>,
    pub saga_abilities: Vec<SagaAbilitiesV1>,
    pub prepare_copies: Vec<PublicLinkV1>,
    pub animate_dead_waiting: Vec<PublicLinkV1>,
    pub animate_dead_attached: Vec<PublicLinkV1>,
    pub granted_opponent_draw_on_leave: Vec<GrantedLeaveDrawV1>,
}

/// Check public zone and exact incarnation before asking for a projected
/// reference. A departed or recycled physical source never reveals its current
/// hidden definition through an old public choice. A concealed current source
/// refuses until public correspondence continuity has its own validated path.
/// Facts retained across a face-up transform are historical choices, not a
/// claim that the source's old printed ability is currently active.
fn current_public(
    state: &GameState,
    link: ObjectLinkV4,
    zone: Zone,
) -> Result<Option<CardStableRefV1>, RlContractError> {
    if !matches!(
        zone,
        Zone::Battlefield | Zone::Stack | Zone::Graveyard | Zone::Exile
    ) {
        return Err(RlContractError("nonpublic Eternal link zone".into()));
    }
    let Some(object) = state.objects.try_get(link.object) else {
        return Ok(None);
    };
    if object.zone != zone || object.zone_change_count != link.zone_change_count {
        return Ok(None);
    }
    if object.v4.face_down_v1.is_some() {
        return Err(RlContractError(
            "concealed Eternal choice source requires public continuity".into(),
        ));
    }
    crate::rl::card_ref(state, link.object).map(Some)
}

/// Public choice operands only. Other Eternal dynamic mechanisms remain
/// unrepresented and must refuse in any consumer seeking complete admission.
pub fn capture_v1(state: &GameState) -> Result<PublicChoicesV1, RlContractError> {
    let mut out = PublicChoicesV1 {
        schema: SCHEMA_V1.into(),
        energy: [0; 2],
        city_blessing: [false; 2],
        has_cast_spell: [false; 2],
        cavern_types: Vec::new(),
        needle_names: Vec::new(),
        uncounterable_spells: Vec::new(),
        cast_mana: Vec::new(),
        saga_abilities: Vec::new(),
        prepare_copies: Vec::new(),
        animate_dead_waiting: Vec::new(),
        animate_dead_attached: Vec::new(),
        granted_opponent_draw_on_leave: Vec::new(),
    };
    for event in state.engine.event_history.iter() {
        if let crate::event::CommittedEvent::SpellCast { controller, .. } = event {
            out.has_cast_spell[controller.index()] = true;
        }
    }
    let Some(eternal) = &state.eternal_v1 else {
        return Ok(out);
    };
    out.energy = eternal.energy;
    out.city_blessing = eternal.e5b.city_blessing;
    for &grant in &eternal.e9b.thief_grants {
        let Some(object) = state.objects.try_get(grant.object) else {
            continue;
        };
        let pending = object.zone == Zone::Stack
            && object.zone_change_count.checked_add(1) == Some(grant.zone_change_count);
        let live =
            object.zone == Zone::Battlefield && object.zone_change_count == grant.zone_change_count;
        if !pending && !live {
            continue;
        }
        let current = ObjectLinkV4 {
            object: grant.object,
            zone_change_count: object.zone_change_count,
        };
        let source = current_public(state, current, object.zone)?
            .ok_or_else(|| RlContractError("missing public leave-draw grant source".into()))?;
        out.granted_opponent_draw_on_leave.push(GrantedLeaveDrawV1 {
            source,
            pending_battlefield_entry: pending,
        });
    }
    for &(source, copy) in &eternal.e7b.prepared {
        let Some(object) = state.objects.try_get(copy) else {
            return Err(RlContractError("missing public prepare copy".into()));
        };
        if object.zone != Zone::Exile {
            return Err(RlContractError(
                "unrepresented prepare copy lifecycle".into(),
            ));
        }
        let target = ObjectLinkV4 {
            object: copy,
            zone_change_count: object.zone_change_count,
        };
        out.prepare_copies
            .push(public_link(state, source, target, Zone::Exile)?);
    }
    for &(source, target) in &eternal.e7a.animating {
        out.animate_dead_waiting
            .push(public_link(state, source, target, Zone::Graveyard)?);
    }
    for &(source, target) in &eternal.e7a.animated {
        out.animate_dead_attached
            .push(public_link(state, source, target, Zone::Battlefield)?);
    }
    if let Some(e3) = &eternal.e3 {
        for &(link, chosen_type) in &e3.cavern_types {
            if let Some(source) = current_public(state, link, Zone::Battlefield)? {
                out.cavern_types.push(ChosenTypeV1 {
                    source,
                    chosen_type,
                });
            }
        }
        for &link in &e3.uncounterable_spells {
            if let Some(spell) = current_public(state, link, Zone::Stack)? {
                out.uncounterable_spells.push(spell);
            }
        }
    }
    for &(link, named_card_id) in &eternal.lurrus.needle_names {
        if let Some(source) = current_public(state, link, Zone::Battlefield)? {
            if crate::card_def::CARD_DEFS
                .get(usize::from(named_card_id))
                .is_none()
            {
                return Err(RlContractError("invalid public named printing".into()));
            }
            out.needle_names.push(ChosenNameV1 {
                source,
                named_card_id,
            });
        }
    }
    for cast in &eternal.vintage.cast_mana {
        if let Some(spell) = current_public(state, cast.spell, Zone::Stack)? {
            out.cast_mana.push(CastManaV1 {
                spell,
                total: cast.total,
                colored: cast.colored,
            });
        }
    }
    for &(link, highest_gained_chapter) in &eternal.vintage.saga_abilities {
        if let Some(source) = current_public(state, link, Zone::Battlefield)? {
            out.saga_abilities.push(SagaAbilitiesV1 {
                source,
                highest_gained_chapter,
            });
        }
    }
    Ok(out)
}

fn public_link(
    state: &GameState,
    source: ObjectLinkV4,
    target: ObjectLinkV4,
    target_zone: Zone,
) -> Result<PublicLinkV1, RlContractError> {
    // A departed source can leave a delayed active effect. Do not silently
    // omit it or reuse a hidden new incarnation as its historical identity.
    let source = current_public(state, source, Zone::Battlefield)?.ok_or_else(|| {
        RlContractError("departed Eternal link needs public historical source".into())
    })?;
    let target = current_public(state, target, target_zone)?.ok_or_else(|| {
        RlContractError("departed Eternal link needs lifecycle projection".into())
    })?;
    Ok(PublicLinkV1 { source, target })
}

#[cfg(all(test, feature = "eternal-fixtures"))]
mod tests {
    use super::*;
    use crate::{
        ids::PlayerId,
        policy_observation_v6::tests::{put, ready_state_for},
    };

    #[test]
    fn issue454_needle_refuses_departed_stack_source_before_reading_hidden_definition() {
        use crate::state::{AbilitySourceContractV4, StackItem, StackItemKind, StackStateV4};
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(viewer);
            let needle = put(&mut state, viewer, "Pithing Needle", Zone::Battlefield);
            let source = put(
                &mut state,
                viewer.opponent(),
                "Wasteland",
                Zone::Battlefield,
            );
            let frozen = AbilitySourceContractV4::capture(&state, source);
            state.stack.push(StackItem {
                kind: StackItemKind::ActivatedAbility,
                source,
                controller: viewer.opponent(),
                targets: Vec::new(),
                is_copy: false,
                inline_effect: None,
                discarded: Vec::new(),
                is_flashback: false,
                mode_chosen: 0,
                madness_offer: false,
                kicked: false,
                v4: StackStateV4 {
                    ability_source_contract: Some(frozen),
                    ..Default::default()
                },
            });
            let action = crate::rl::ActionSemanticV1::ChooseEffectOption {
                actor: viewer.into(),
                source: crate::rl::card_ref(&state, needle).unwrap(),
                option_index: 0,
                option_count: 1,
            };
            assert!(require_continuous_needle_sources(&state).is_ok());
            for (zone, incarnation) in [(Zone::Hand, 1), (Zone::Library, 1), (Zone::Battlefield, 2)]
            {
                for name in ["Island", "Forest", "Force of Will"] {
                    let live = state.objects.get_mut(source);
                    live.zone = zone;
                    live.zone_change_count = incarnation;
                    live.card_def = crate::card_def::card_id_by_name(name).unwrap();
                    assert_eq!(
                        entry_option_v1(&state, &action, None).unwrap_err().0,
                        "Needle menu needs continuous public stack sources"
                    );
                }
            }
        }
    }

    fn stack_source(state: &mut GameState, player: PlayerId, name: &str) -> crate::ids::ObjectId {
        let source = put(state, player, name, Zone::Hand);
        state.players[player.index()]
            .hand
            .retain(|id| *id != source);
        state.objects.get_mut(source).zone = Zone::Stack;
        source
    }

    #[test]
    fn issue454_leave_draw_grant_distinguishes_pending_entry_from_active_permanent() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(player);
            let source = stack_source(&mut state, player, "Thief of Existence");
            state
                .eternal_v1
                .get_or_insert_with(Default::default)
                .e9b
                .thief_grants
                .push(ObjectLinkV4 {
                    object: source,
                    zone_change_count: 1,
                });
            let pending = capture_v1(&state).unwrap();
            assert!(pending.granted_opponent_draw_on_leave[0].pending_battlefield_entry);
            state.objects.get_mut(source).zone = Zone::Battlefield;
            state.objects.get_mut(source).zone_change_count = 1;
            let active = capture_v1(&state).unwrap();
            assert!(!active.granted_opponent_draw_on_leave[0].pending_battlefield_entry);
            assert_ne!(pending, active);
            state.objects.get_mut(source).zone = Zone::Hand;
            state.objects.get_mut(source).zone_change_count = 2;
            let departed = capture_v1(&state).unwrap();
            assert!(departed.granted_opponent_draw_on_leave.is_empty());
            state.objects.get_mut(source).card_def =
                crate::card_def::card_id_by_name("Island").unwrap();
            assert_eq!(capture_v1(&state).unwrap(), departed);
        }
    }

    #[test]
    fn issue454_public_cast_fact_ignores_hidden_conditional_ledgers() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(player);
            let spell = stack_source(&mut state, player, "Ponder");
            crate::event::log_spell_cast(&mut state, spell, player);
            let baseline = capture_v1(&state).unwrap();
            assert!(baseline.has_cast_spell[player.index()]);
            let hidden = put(&mut state, player.opponent(), "Island", Zone::Library);
            for hidden_name in ["Island", "Once Upon a Time"] {
                state.objects.get_mut(hidden).card_def =
                    crate::card_def::card_id_by_name(hidden_name).unwrap();
                for ledger in [[false, false], [true, true]] {
                    state
                        .eternal_v1
                        .get_or_insert_with(Default::default)
                        .e8a
                        .spells_cast = ledger;
                    assert_eq!(capture_v1(&state).unwrap(), baseline);
                }
            }
        }
    }

    #[test]
    fn issue454_public_attachment_links_preserve_operands_and_refuse_departure() {
        let mut state = ready_state_for(PlayerId::P0);
        let aura = put(&mut state, PlayerId::P0, "Animate Dead", Zone::Battlefield);
        let creature = put(
            &mut state,
            PlayerId::P1,
            "Thief of Existence",
            Zone::Battlefield,
        );
        let link = |object| ObjectLinkV4 {
            object,
            zone_change_count: 0,
        };
        state
            .eternal_v1
            .get_or_insert_with(Default::default)
            .e7a
            .animated
            .push((link(aura), link(creature)));
        let public = capture_v1(&state).unwrap();
        assert_eq!(public.animate_dead_attached[0].source.arena_id, aura.0);
        assert_eq!(public.animate_dead_attached[0].target.arena_id, creature.0);
        state.objects.get_mut(aura).zone = Zone::Hand;
        state.objects.get_mut(aura).zone_change_count += 1;
        for hidden_name in ["Island", "Forest"] {
            state.objects.get_mut(aura).card_def =
                crate::card_def::card_id_by_name(hidden_name).unwrap();
            assert_eq!(
                capture_v1(&state).unwrap_err().0,
                "departed Eternal link needs public historical source"
            );
        }
    }

    #[test]
    fn issue454_entry_handlers_publish_choices_and_concealed_sources_refuse_uniformly() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(player);
            let cavern = put(&mut state, player, "Cavern of Souls", Zone::Battlefield);
            put(&mut state, player, "Wastescape Battlemage", Zone::Hand);
            let options = crate::eternal_cards_v1::e3::cavern_type_options(&state, player);
            let selected = options.iter().position(|t| *t == Subtype::Eldrazi).unwrap();
            let action = crate::rl::ActionSemanticV1::ChooseEffectOption {
                actor: player.into(),
                source: crate::rl::card_ref(&state, cavern).unwrap(),
                option_index: selected as u16,
                option_count: options.len() as u16,
            };
            assert_eq!(
                entry_option_v1(&state, &action, Some(&options)).unwrap(),
                Some(EntryOptionV1::CreatureType(Subtype::Eldrazi))
            );
            crate::eternal_cards_v1::e3::answer_entry_choice(&mut state, selected as u16).unwrap();
            let needle = put(&mut state, player, "Pithing Needle", Zone::Battlefield);
            let options = crate::eternal_lurrus_v1::needle_name_options(&state, player);
            let named = crate::card_def::card_id_by_name("Cavern of Souls").unwrap();
            let selected = options.iter().position(|id| *id == named).unwrap();
            let action = crate::rl::ActionSemanticV1::ChooseEffectOption {
                actor: player.into(),
                source: crate::rl::card_ref(&state, needle).unwrap(),
                option_index: selected as u16,
                option_count: options.len() as u16,
            };
            assert_eq!(
                entry_option_v1(&state, &action, None).unwrap(),
                Some(EntryOptionV1::CardName { printing: named })
            );
            assert!(
                crate::eternal_lurrus_v1::answer_entry_choice(&mut state, selected as u16).unwrap()
            );
            let public = capture_v1(&state).unwrap();
            assert_eq!(public.cavern_types[0].chosen_type, Subtype::Eldrazi);
            assert_eq!(public.needle_names[0].named_card_id, named);
            // No inspection of the concealed source's underlying printing may
            // decide whether this incomplete continuity contract accepts it.
            for source in [cavern, needle] {
                for hidden_name in ["Island", "Forest", "Pithing Needle"] {
                    let mut concealed = state.clone();
                    let object = concealed.objects.get_mut(source);
                    object.v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                        disguised: true,
                        lookers: 0,
                        hidden_by: None,
                        hidden_by_source: None,
                    });
                    object.card_def = crate::card_def::card_id_by_name(hidden_name).unwrap();
                    assert_eq!(
                        capture_v1(&concealed).unwrap_err().0,
                        "concealed Eternal choice source requires public continuity"
                    );
                }
            }
        }
    }

    #[test]
    fn issue454_public_choices_distinguish_names_and_types_without_hidden_definitions() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(player);
            let cavern = put(&mut state, player, "Cavern of Souls", Zone::Battlefield);
            let needle = put(&mut state, player, "Pithing Needle", Zone::Battlefield);
            let hidden = put(&mut state, player.opponent(), "Island", Zone::Hand);
            let named = crate::card_def::card_id_by_name("Forest").unwrap();
            let link = |object| ObjectLinkV4 {
                object,
                zone_change_count: 0,
            };
            let eternal = state.eternal_v1.get_or_insert_with(Default::default);
            eternal
                .e3
                .get_or_insert_with(Default::default)
                .cavern_types
                .push((link(cavern), Subtype::Goblin));
            eternal.lurrus.needle_names.push((link(needle), named));
            let baseline = capture_v1(&state).unwrap();
            assert_eq!(baseline.cavern_types[0].source.arena_id, cavern.0);
            assert_eq!(baseline.needle_names[0].named_card_id, named);
            let mut hidden_variant = state.clone();
            hidden_variant.objects.get_mut(hidden).card_def = named;
            assert_eq!(capture_v1(&hidden_variant).unwrap(), baseline);
            // A public named printing does not assert that a hidden copy exists.
            let mut changed = state.clone();
            changed.eternal_v1.as_mut().unwrap().lurrus.needle_names[0].1 =
                crate::card_def::card_id_by_name("Island").unwrap();
            assert_ne!(capture_v1(&changed).unwrap(), baseline);
            changed = state.clone();
            changed
                .eternal_v1
                .as_mut()
                .unwrap()
                .e3
                .as_mut()
                .unwrap()
                .cavern_types[0]
                .1 = Subtype::Human;
            assert_ne!(capture_v1(&changed).unwrap(), baseline);
            // Old choices never acquire the identity of a hidden new incarnation.
            for source in [cavern, needle] {
                state.objects.get_mut(source).zone = Zone::Hand;
                state.objects.get_mut(source).zone_change_count += 1;
            }
            let departed = capture_v1(&state).unwrap();
            assert!(departed.cavern_types.is_empty() && departed.needle_names.is_empty());
            state.objects.get_mut(cavern).card_def = named;
            state.objects.get_mut(needle).card_def = named;
            assert_eq!(capture_v1(&state).unwrap(), departed);
        }
    }
}
