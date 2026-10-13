//! Scoped Crystal static facts and affected-controller prevention choices.
//! No frozen observation or model schema is relabeled.
use crate::ids::PlayerId;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::{GameState, Target, Zone};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/twinflame-observation/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticSourceV1 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionV1 {
    pub kind: String,
    pub source: CardStableRefV1,
    pub shield_remaining: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MenuV1 {
    pub chooser: PlayerSeatV1,
    /// Present only for publicly announced, receipt-authenticated sacrifice
    /// damage. In that case source describes the captured incarnation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_history: Option<SavedDamageSourceV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fixed_source_history: Option<SavedFixedDamageSourceV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raid_source_history: Option<SavedRaidDamageSourceV4>,
    /// Includes the current affected recipient. No hidden roster or raw IDs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub simultaneous_choices_remaining: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creature_power_at_resolution: Option<i32>,
    pub source: CardStableRefV1,
    pub resolving_source: CardStableRefV1,
    pub recipient: crate::rl::TargetRefV1,
    pub is_combat: bool,
    pub original_damage: i32,
    pub applied_multipliers: Vec<CardStableRefV1>,
    pub remaining_damage: i32,
    pub options: Vec<OptionV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiscardMenuV2 {
    pub chooser: PlayerSeatV1,
    pub source: CardStableRefV1,
    pub maximum_cards: u16,
    pub draws_if_discarded: u16,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramMenuV3 {
    pub chooser: PlayerSeatV1,
    pub source: CardStableRefV1,
    pub decline_index: u16,
    pub fight_index: u16,
    pub requires_original_source_creature: bool,
}
pub type SavedDamageSourceV1 = crate::crystal_public_v1::SavedDamageSourceV1;
pub type SavedFixedDamageSourceV3 = crate::crystal_public_v1::SavedFixedDamageSourceV3;
pub type SavedRaidDamageSourceV4 = crate::crystal_public_v1::SavedRaidDamageSourceV4;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TwinflameObservationV1 {
    pub schema: String,
    pub producer_sha256: String,
    pub kernel_version: String,
    pub surface_version: u32,
    pub policy_surface_version: u32,
    pub card_db_hash: u64,
    pub acting_player: PlayerSeatV1,
    pub step_index: u64,
    pub physical_decision_id: u64,
    pub substep_index: u32,
    pub substep_count: u32,
    pub projection: PublicObservationProjectionV5,
    pub own_hand: Vec<CardPrivateV1>,
    pub known_library_cards: [Vec<KnownLibraryCardV4>; 2],
    pub known_hand_cards: [Vec<CardPrivateV1>; 2],
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_face_down_cards: Vec<crate::rl::FaceDownCardKnowledgeV1>,
    pub extensions: crate::policy_observation_v8::PolicyObservationExtensionsV8,
    pub attacked_this_turn: [bool; 2],
    pub static_sources: Vec<StaticSourceV1>,
    pub multiplier_sources: Vec<StaticSourceV1>,
    pub meanings: Vec<crate::twinflame_tyrant_v1::PrintedMeaningV1>,
    pub combat_allocation: Option<AllocationMenuV38>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub equipment_damage_sources: Vec<crate::equipment_damage_source_v4::PublicSourceV4>,
    pub prevention_menu: Option<MenuV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional_discard_menu: Option<DiscardMenuV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optional_fight_menu: Option<ProgramMenuV3>,
    pub projection_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    let mut h = Sha256::new();
    for source in [
        SCHEMA,
        include_str!("twinflame_observation_v1.rs"),
        include_str!("twinflame_tyrant_v1.rs"),
        include_str!("twinflame_tyrant_v1/ordering.rs"),
        include_str!("twinflame_tyrant_v1/sampling.rs"),
        include_str!("twinflame_tyrant_v1/combat.rs"),
        include_str!("twinflame_tyrant_v1/fiery.rs"),
        include_str!("fiery_annihilation_v1.rs"),
        include_str!("rules_meaning_fiery_v1.rs"),
        include_str!("crystal_public_v1.rs"),
        include_str!("crystal_barricade_v1.rs"),
        include_str!("effect/crystal_prevention_v1.rs"),
        include_str!("effect.rs"),
        include_str!("event.rs"),
        include_str!("engine.rs"),
        include_str!("state.rs"),
        include_str!("rl.rs"),
        include_str!("rl_session.rs"),
        include_str!("rl_session/twinflame_v38.rs"),
        include_str!("human_bo3_v1/twinflame_v38.rs"),
        include_str!("human_bo3_v1/visible.rs"),
        include_str!("human_bo3_v1/labels.rs"),
        include_str!("human_bo3_v1/life_ward_v8.rs"),
        include_str!("continuous_characteristics_v1.rs"),
        include_str!("sower_restriction_capture_v1.rs"),
        include_str!("standard_cards_v1.rs"),
        include_str!("source_power_activation_v1.rs"),
        include_str!("fixed_source_damage_v3.rs"),
        include_str!("equipment_damage_source_v4.rs"),
        include_str!("trigger.rs"),
        include_str!("legend_rule_v1.rs"),
        include_str!("gorehorn_damage_v1.rs"),
        include_str!("gorehorn_crystal_v1.rs"),
        include_str!("raid_entry_custody_v1.rs"),
        include_str!("rules_meaning_heartfire_v1.rs"),
        include_str!("creature_stats_lki_v1.rs"),
        include_str!("standard_legends_v1.rs"),
        include_str!("rules_meaning_crystal_ordering_v1.rs"),
        include_str!("card_def.rs"),
        include_str!("../build.rs"),
        include_str!("card_def.rs"),
        include_str!("policy_surface_v5.rs"),
        include_str!("surface_v2.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
        include_str!("policy_observation_v8.rs"),
        include_str!("policy_observation_v7.rs"),
        include_str!("rl_session/flat_action_v3.rs"),
        include_str!("rl_session/flat_action_v4.rs"),
    ] {
        h.update([0]);
        h.update(source.as_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
fn commitment(o: &TwinflameObservationV1) -> Result<String, RlContractError> {
    let mut v = serde_json::to_value(o)?;
    v.as_object_mut()
        .ok_or_else(|| RlContractError("Twinflame observation is not an object".into()))?
        .remove("projection_sha256");
    Ok(digest(&serde_json::to_vec(&v)?))
}
impl TwinflameObservationV1 {
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.producer_sha256 != producer_sha256_v1()
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError("wrong Crystal artifact identity".into()));
        }
        Ok(())
    }
}
pub(crate) fn validate_projection_authority(state: &GameState) -> Result<(), RlContractError> {
    crate::crystal_public_v1::validate_state(state).map_err(RlContractError)?;
    crate::twinflame_tyrant_v1::combat::validate(state).map_err(RlContractError)?;
    meanings()?;
    Ok(())
}
fn public_ref(
    state: &GameState,
    id: crate::ids::ObjectId,
) -> Result<CardStableRefV1, RlContractError> {
    crate::crystal_public_v1::public_ref(state, id)
}
fn meanings() -> Result<Vec<crate::twinflame_tyrant_v1::PrintedMeaningV1>, RlContractError> {
    if !cfg!(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    )) {
        return Ok(Vec::new());
    }
    crate::card_def::card_id_by_name("Twinflame Tyrant")
        .map(|id| {
            crate::twinflame_tyrant_v1::meaning(id)
                .map(|meaning| vec![meaning])
                .map_err(RlContractError)
        })
        .unwrap_or_else(|| Ok(Vec::new()))
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IncomingPacketV38 {
    pub source: CardStableRefV1,
    pub recipient: crate::rl::TargetRefV1,
    pub amount: i32,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AllocationMenuV38 {
    pub chooser: crate::rl::PlayerSeatV1,
    pub action_source: CardStableRefV1,
    pub packets: Vec<IncomingPacketV38>,
}
fn recipient(state: &GameState, target: Target) -> Result<crate::rl::TargetRefV1, RlContractError> {
    match target {
        Target::Player(player) => Ok(crate::rl::TargetRefV1::Player {
            player: player.into(),
        }),
        Target::Object(id) => Ok(crate::rl::TargetRefV1::Object {
            object: public_ref(state, id)?,
        }),
        Target::StackItem(_) => Err(RlContractError(
            "damage recipient cannot be a stack item".into(),
        )),
    }
}
fn options(
    state: &GameState,
    choice: &crate::effect::crystal_prevention_v1::ChoiceV1,
) -> Result<Vec<OptionV1>, RlContractError> {
    use crate::effect::crystal_prevention_v1::PreventionV1 as P;
    choice.options.iter().map(|option| {
        let (kind,id,remaining)=match option {
            P::Multiplier(source)=>("multiplier",source.source,None),
            P::Barricade(source)=>("barricade",source.source,None),
            P::Protection(target)=>("protection",target.object,None),
            P::Replacement(r)=>match r.kind {
                crate::event::ReplacementEffectKind::PreventNextDamage {remaining,..}=>("shield",r.source,Some(remaining)),
                crate::event::ReplacementEffectKind::PreventDamageFromColorUntilEndOfTurn {..}=>("color",r.source,None),
                crate::event::ReplacementEffectKind::PreventCombatDamageToObjectUntilEndOfTurn {..}=>("combat-prevention",r.source,None),
                _=>return Err(RlContractError("damage menu contains a non-damage replacement".into())),
            }
        };
        Ok(OptionV1 {kind:kind.into(),source:public_ref(state,id)?,shield_remaining:remaining})
    }).collect()
}
fn allocation(state: &GameState) -> Result<Option<AllocationMenuV38>, RlContractError> {
    let Some(pending) = state.engine.twinflame_combat_v38.0.as_ref() else {
        return Ok(None);
    };
    if pending.selected.is_some() {
        return Ok(None);
    }
    let slots = crate::twinflame_tyrant_v1::combat::allocation_choices(pending);
    let Some(first) = slots.first() else {
        return Err(RlContractError(
            "waiting combat has no actual allocation menu".into(),
        ));
    };
    let packet = &pending.packets[*first];
    Ok(Some(AllocationMenuV38 {
        chooser: packet.chooser.into(),
        action_source: public_ref(state, packet.source)?,
        packets: slots
            .into_iter()
            .map(|slot| {
                let p = &pending.packets[slot];
                Ok(IncomingPacketV38 {
                    source: public_ref(state, p.source)?,
                    recipient: recipient(state, p.target)?,
                    amount: p.remaining_amount,
                })
            })
            .collect::<Result<Vec<_>, RlContractError>>()?,
    }))
}
fn menu(state: &GameState) -> Result<Option<MenuV1>, RlContractError> {
    use crate::effect::{EffectOptionChoicePurpose, PendingEffectChoice};
    let (choice, is_combat) = if let Some(pending) = state.engine.twinflame_combat_v38.0.as_ref() {
        let Some(slot) = pending.selected else {
            return Ok(None);
        };
        (&pending.packets[slot], true)
    } else {
        let Some(PendingEffectChoice::ChooseOption {
            purpose: EffectOptionChoicePurpose::CrystalPreventionV1 { choice },
            ..
        }) = state
            .engine
            .pending_effect
            .as_ref()
            .and_then(|p| p.choice.as_ref())
        else {
            return Ok(None);
        };
        (choice.as_ref(), false)
    };
    let (source, source_history, fixed_source_history, raid_source_history) = if is_combat {
        (public_ref(state, choice.source)?, None, None, None)
    } else {
        crate::crystal_public_v1::damage_origin(state, choice)?
    };
    let resolving_source = if is_combat {
        source.clone()
    } else {
        crate::rl::crystal_resolving_source_ref(
            state,
            &state.engine.pending_effect.as_ref().unwrap().resolving_item,
        )?
    };
    Ok(Some(MenuV1 {
        chooser: choice.chooser.into(),
        source,
        source_history,
        fixed_source_history,
        raid_source_history,
        resolving_source,
        simultaneous_choices_remaining: if is_combat {
            state
                .engine
                .twinflame_combat_v38
                .0
                .as_ref()
                .map(|p| p.remaining.len())
        } else {
            choice.batch.as_ref().map(|p| {
                p.outcomes
                    .iter()
                    .filter(|o| {
                        matches!(
                            o,
                            crate::effect::crystal_prevention_v1::BatchOutcomeV2::Waiting
                        )
                    })
                    .count()
            })
        },
        creature_power_at_resolution: crate::effect::crystal_prevention_v1::creature_power_signed(
            state, choice,
        )
        .map_err(RlContractError)?,
        recipient: recipient(state, choice.target)?,
        is_combat,
        original_damage: choice.original_amount,
        applied_multipliers: choice
            .applied_multipliers
            .0
            .iter()
            .map(|source| public_ref(state, source.source))
            .collect::<Result<Vec<_>, RlContractError>>()?,
        remaining_damage: choice.remaining_amount,
        options: options(state, choice)?,
    }))
}

#[allow(clippy::too_many_arguments)]
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step_index: u64,
    physical_decision_id: u64,
    substep_index: u32,
    substep_count: u32,
) -> Result<TwinflameObservationV1, RlContractError> {
    validate_projection_authority(state)?;
    let optional_discard_menu = crate::crystal_public_v1::discard_menu(state, actor)?;
    let optional_fight_menu = if crate::effect::crystal_prevention_v1::is_program_choice(state) {
        let pending = state
            .engine
            .pending_effect
            .as_ref()
            .ok_or_else(|| RlContractError("Crystal optional fight absent".into()))?;
        Some(ProgramMenuV3 {
            chooser: pending.ctx.controller.into(),
            source: crate::rl::crystal_resolving_source_ref(state, &pending.resolving_item)?,
            decline_index: 0,
            fight_index: 1,
            requires_original_source_creature: true,
        })
    } else {
        None
    };
    let mut static_sources = Vec::new();
    for (id, o) in state.objects.iter() {
        if crate::crystal_barricade_v1::is_active_source(state, id) {
            static_sources.push(StaticSourceV1 {
                source: public_ref(state, id)?,
                controller: o.controller.into(),
            });
        }
    }
    let base = crate::rl::twinflame_observation_storage_v38(
        state,
        surface,
        actor,
        step_index,
        physical_decision_id,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let base = storage.base;
    let mut o = TwinflameObservationV1 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256_v1(),
        kernel_version: base.kernel_version,
        surface_version: base.surface_version,
        policy_surface_version: base.policy_surface_version,
        card_db_hash: base.card_db_hash,
        acting_player: base.acting_player,
        step_index: base.step_index,
        physical_decision_id: base.physical_decision_id,
        substep_index: base.substep_index,
        substep_count: base.substep_count,
        projection: base.projection,
        own_hand: base.own_hand,
        known_library_cards: base.known_library_cards,
        known_hand_cards: base.known_hand_cards,
        known_face_down_cards: base.known_face_down_cards,
        extensions: storage.extensions,
        attacked_this_turn: [
            state.player_attacked_this_turn_v1(PlayerId::P0),
            state.player_attacked_this_turn_v1(PlayerId::P1),
        ],
        static_sources,
        multiplier_sources: state
            .objects
            .iter()
            .filter_map(|(id, _)| {
                crate::twinflame_tyrant_v1::controller(state, id).map(|controller| (id, controller))
            })
            .map(|(id, controller)| {
                Ok(StaticSourceV1 {
                    source: public_ref(state, id)?,
                    controller: controller.into(),
                })
            })
            .collect::<Result<Vec<_>, RlContractError>>()?,
        meanings: meanings()?,
        combat_allocation: allocation(state)?,
        prevention_menu: menu(state)?,
        optional_discard_menu,
        optional_fight_menu,
        equipment_damage_sources: crate::equipment_damage_source_v4::public_sources(state)
            .map_err(RlContractError)?,
        projection_sha256: String::new(),
    };
    o.projection_sha256 = commitment(&o)?;
    Ok(o)
}
#[allow(clippy::too_many_arguments)]
pub fn validate_observation_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    identity: (u64, u64, u32, u32),
    supplied: &TwinflameObservationV1,
) -> Result<(), RlContractError> {
    let expected = observe_v1(
        state, surface, actor, identity.0, identity.1, identity.2, identity.3,
    )?;
    if &expected != supplied {
        return Err(RlContractError(
            "Twinflame observation changed complete reconstructed input".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    #[test]
    fn real_predecessor_parts_and_complete_public_identity_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            let surface = PolicySurfaceV5::new_for_session();
            let before = serde_json::to_vec(&state).unwrap();
            let actual = observe_v1(&state, &surface, actor, 0, 11, 0, 1).unwrap();
            let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 11, 0, 1).unwrap();
            assert_eq!(actual.projection, old.projection);
            assert_eq!(actual.own_hand, old.own_hand);
            assert_eq!(actual.extensions, old.extensions);
            assert!(
                actual.multiplier_sources.is_empty()
                    && actual.combat_allocation.is_none()
                    && actual.prevention_menu.is_none()
            );
            validate_observation_v1(&state, &surface, actor, (0, 11, 0, 1), &actual).unwrap();
            for mutation in 0..4 {
                let mut forged = actual.clone();
                match mutation {
                    0 => forged.schema.clear(),
                    1 => forged.producer_sha256.clear(),
                    2 => forged.acting_player = actor.opponent().into(),
                    _ => forged.substep_count += 1,
                };
                assert!(
                    validate_observation_v1(&state, &surface, actor, (0, 11, 0, 1), &forged)
                        .is_err()
                );
            }
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn at_scalar(actor: PlayerId, player_target: bool) -> GameState {
        use crate::engine::{self, Action, Decision};
        use crate::mana::ManaColor;
        use crate::state::Zone;
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            for _ in 0..16 {
                put(&mut state, player, "Forest", Zone::Library);
            }
        }
        let tyrant = put(&mut state, actor, "Twinflame Tyrant", Zone::Hand);
        assert!(crate::twinflame_tyrant_v1::owns_definition(
            state.objects.get(tyrant).card_def
        ));
        let before = serde_json::to_vec(&state).unwrap();
        assert!(engine::step(&mut state, Action::CastSpell(tyrant)).is_err());
        assert_eq!(serde_json::to_vec(&state).unwrap(), before);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(tyrant)).unwrap();
        for _ in 0..16 {
            if state.objects.get(tyrant).zone == Zone::Battlefield && state.stack.is_empty() {
                break;
            }
            let d = engine::advance_until_decision(&mut state);
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            if state.objects.get(tyrant).zone == Zone::Battlefield && state.stack.is_empty() {
                break;
            }
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.objects.get(tyrant).zone, Zone::Battlefield);
        let recipient = put(
            &mut state,
            actor.opponent(),
            "Shivan Dragon",
            Zone::Battlefield,
        );
        let target = if player_target {
            Target::Player(actor.opponent())
        } else {
            Target::Object(recipient)
        };
        state
            .engine
            .active_replacements
            .push(crate::event::ActiveReplacement {
                id: 90_060,
                source: recipient,
                kind: crate::event::ReplacementEffectKind::PreventNextDamage {
                    target,
                    remaining: 3,
                },
            });
        let bolt = put(&mut state, actor, "Lightning Bolt", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 1;
        engine::step(&mut state, Action::CastSpell(bolt)).unwrap();
        engine::step(&mut state, Action::ChooseTarget(target)).unwrap();
        for _ in 0..16 {
            if crate::effect::crystal_prevention_v1::is_choice(&state) {
                break;
            }
            let d = engine::advance_until_decision(&mut state);
            assert!(matches!(d, Decision::CastSpellOrPass { .. }), "{d:?}");
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert!(crate::effect::crystal_prevention_v1::is_choice(&state));
        state
    }
    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[ignore = "requires actual Twinflame registration"]
    fn genuine_scalar_public_player_permanent_bound_options_and_old_refusal_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for player_target in [false, true] {
                let mut state = at_scalar(actor, player_target);
                let surface = PolicySurfaceV5::new_for_session();
                let before = serde_json::to_vec(&state).unwrap();
                let actual = observe_v1(&state, &surface, actor.opponent(), 0, 12, 0, 1).unwrap();
                let menu = actual.prevention_menu.as_ref().unwrap();
                assert_eq!(menu.chooser, actor.opponent().into());
                assert_eq!(menu.original_damage, 3);
                assert_eq!(menu.remaining_damage, 3);
                assert!(!menu.is_combat);
                assert_eq!(actual.multiplier_sources.len(), 1);
                assert!(menu.options.iter().any(|o| o.kind == "multiplier"));
                assert!(menu.options.iter().any(|o| o.kind == "shield"));
                assert_eq!(
                    matches!(menu.recipient, crate::rl::TargetRefV1::Player { .. }),
                    player_target
                );
                assert!(crate::rl::observe_v1(&state, actor.opponent(), 0).is_err());
                assert!(crate::crystal_public_v1::observe_v1(
                    &state,
                    &surface,
                    actor.opponent(),
                    0,
                    12,
                    0,
                    1
                )
                .is_err());
                for mutation in 0..5 {
                    let mut bad = actual.clone();
                    match mutation {
                        0 => bad.prevention_menu.as_mut().unwrap().options.reverse(),
                        1 => bad.prevention_menu.as_mut().unwrap().remaining_damage += 1,
                        2 => bad.multiplier_sources[0].controller = actor.opponent().into(),
                        3 => bad.meanings[0].each_live_source_once_per_event = false,
                        _ => bad.projection_sha256.clear(),
                    };
                    assert!(validate_observation_v1(
                        &state,
                        &surface,
                        actor.opponent(),
                        (0, 12, 0, 1),
                        &bad
                    )
                    .is_err());
                }
                assert_eq!(serde_json::to_vec(&state).unwrap(), before);
                let saved = serde_json::to_vec(&state).unwrap();
                let mut replay: GameState = serde_json::from_slice(&saved).unwrap();
                for branch in [&mut state, &mut replay] {
                    let index = actual
                        .prevention_menu
                        .as_ref()
                        .unwrap()
                        .options
                        .iter()
                        .position(|o| o.kind == "multiplier")
                        .unwrap();
                    crate::engine::step(
                        branch,
                        crate::engine::Action::ChooseEffectOption(index as u16),
                    )
                    .unwrap();
                    let next = observe_v1(branch, &surface, actor.opponent(), 1, 13, 0, 1).unwrap();
                    let menu = next.prevention_menu.unwrap();
                    assert_eq!(menu.remaining_damage, 6);
                    assert_eq!(menu.applied_multipliers.len(), 1);
                    assert_eq!(menu.options.len(), 1);
                }
                assert_eq!(state, replay);
            }
        }
    }
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    pub(crate) fn at_combat(actor: PlayerId) -> GameState {
        use crate::engine::{self, Action, Decision};
        use crate::mana::ManaColor;
        let mut state = ready_state_for(actor);
        for player in [actor, actor.opponent()] {
            for _ in 0..16 {
                put(&mut state, player, "Forest", Zone::Library);
            }
            // Genuine mana decisions prevent the backend from advancing across Cleanup after completion.
            put(&mut state, player, "Forest", Zone::Battlefield);
        }
        let tyrant = put(&mut state, actor, "Twinflame Tyrant", Zone::Hand);
        state.players[actor.index()].mana_pool[ManaColor::C.pool_index()] = 3;
        state.players[actor.index()].mana_pool[ManaColor::R.pool_index()] = 2;
        engine::step(&mut state, Action::CastSpell(tyrant)).unwrap();
        for _ in 0..24 {
            let decision = engine::advance_until_decision(&mut state);
            if state.objects.get(tyrant).zone == Zone::Battlefield && state.stack.is_empty() {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        assert_eq!(state.objects.get(tyrant).zone, Zone::Battlefield);
        let dragon = put(&mut state, actor, "Shivan Dragon", Zone::Battlefield);
        for source in [tyrant, dragon] {
            state.objects.get_mut(source).summoning_sick = false;
            state.objects.get_mut(source).v4.lifelink_keyword_counters = 1;
        }
        state
            .engine
            .active_replacements
            .push(crate::event::ActiveReplacement {
                id: 90_065,
                source: dragon,
                kind: crate::event::ReplacementEffectKind::PreventNextDamage {
                    target: Target::Player(actor.opponent()),
                    remaining: 3,
                },
            });
        for _ in 0..24 {
            let decision = engine::advance_until_decision(&mut state);
            if matches!(decision, Decision::DeclareAttackers { .. }) {
                break;
            }
            assert!(
                matches!(decision, Decision::CastSpellOrPass { .. }),
                "{decision:?}"
            );
            engine::step(&mut state, Action::Pass).unwrap();
        }
        engine::step(&mut state, Action::DeclareAttackers(vec![tyrant, dragon])).unwrap();
        for _ in 0..24 {
            let decision = engine::advance_until_decision(&mut state);
            if crate::twinflame_tyrant_v1::combat::has_pending(&state) {
                break;
            }
            match decision {
                Decision::DeclareBlockers { .. } => {
                    engine::step(&mut state, Action::DeclareBlockers(vec![])).unwrap()
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("actual combat allocation entry {other:?}"),
            }
        }
        assert!(crate::twinflame_tyrant_v1::combat::has_pending(&state));
        state
    }
}
