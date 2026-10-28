//! Current public Phoenix form and live trigger/payment stages only.
//! Existing generic original-context validation precedes every projection.
use crate::{
    card_def::{CardType, Keywords},
    effect::{
        EffectAnsweredChoiceGuard, EffectBooleanChoicePurpose, EffectFrame, PendingEffectChoice,
    },
    ids::ObjectId,
    rl::{PlayerSeatV1, RlContractError},
    rules_meaning_phoenix_v56::{self as meaning, MeaningV56},
    state::{AbilitySourceContractV4, GameState, StackItemKind, Zone},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/phoenix-public/v56";
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum IdentityV56 {
    Known { card_id: u16 },
    FaceDown { disguised: bool },
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceV56 {
    pub object: u32,
    pub generation: u32,
    pub owner: PlayerSeatV1,
    pub controller: PlayerSeatV1,
    pub zone: Zone,
    pub identity: IdentityV56,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentV56 {
    pub source: ReferenceV56,
    pub token: bool,
    pub effective_names: Vec<String>,
    pub effective_types: Vec<CardType>,
    pub effective_subtype_ids: Vec<u16>,
    pub color_mask: u8,
    pub printed_abilities_active: bool,
    pub power: Option<i32>,
    pub toughness: Option<i32>,
    pub flying: bool,
    pub haste: bool,
    pub attacks_each_combat_if_able: bool,
    pub graveyard_ability_functions_here: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StageV56 {
    Held { placement_ordered: bool },
    Published,
    PaymentOffered { can_pay: bool },
    AcceptedAwaitingPayment,
    DeclinedAwaitingCompletion,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TriggerV56 {
    pub original_source: ReferenceV56,
    pub current_original_source: Option<ReferenceV56>,
    pub original_actor: PlayerSeatV1,
    pub stage: StageV56,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpellV56 {
    pub source: ReferenceV56,
    pub is_copy: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactsV56 {
    pub schema: String,
    pub producer_sha256: String,
    pub card_db_hash: u64,
    pub meaning: MeaningV56,
    pub ferocious_now: [bool; 2],
    pub current: Vec<CurrentV56>,
    pub spells: Vec<SpellV56>,
    pub triggers: Vec<TriggerV56>,
}
fn error(message: impl Into<String>) -> RlContractError {
    RlContractError(message.into())
}
pub fn producer_sha256() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [SCHEMA, include_str!("phoenix_public_v56.rs")] {
            h.update((source.len() as u64).to_le_bytes());
            h.update(source.as_bytes());
        }
        h.update(meaning::producer_sha256().as_bytes());
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
fn public_zone(zone: Zone) -> bool {
    matches!(
        zone,
        Zone::Battlefield | Zone::Graveyard | Zone::Exile | Zone::Stack
    )
}
fn reference(state: &GameState, original: AbilitySourceContractV4) -> ReferenceV56 {
    let concealed = state
        .objects
        .try_get(original.source)
        .filter(|o| o.zone_change_count == original.zone_change_count)
        .and_then(|o| o.v4.face_down_v1);
    ReferenceV56 {
        object: original.source.0,
        generation: original.zone_change_count,
        owner: original.owner.into(),
        controller: original.controller.into(),
        zone: original.zone,
        identity: concealed.map_or(
            IdentityV56::Known {
                card_id: original.card_def,
            },
            |face| IdentityV56::FaceDown {
                disguised: face.disguised,
            },
        ),
    }
}
fn current_original(state: &GameState, original: AbilitySourceContractV4) -> Option<ReferenceV56> {
    state
        .objects
        .try_get(original.source)
        .filter(|o| o.zone_change_count == original.zone_change_count && public_zone(o.zone))
        .map(|_| {
            reference(
                state,
                AbilitySourceContractV4::capture(state, original.source),
            )
        })
}
// Refuse the known unsafe native base/counter prefix before calling the shared
// effective query. Later modifier arithmetic remains the engine's contract.
fn checked_native_prefix(state: &GameState, id: ObjectId) -> Result<(), RlContractError> {
    let o = state.objects.get(id);
    for (base, minus) in [
        (crate::engine::effective_base_power(state, id), 0),
        (
            crate::engine::effective_base_toughness(state, id),
            o.counters.minus0_minus1,
        ),
    ] {
        if let Some(base) = base {
            base.checked_add(o.counters.plus1_plus1)
                .and_then(|n| n.checked_sub(i32::from(o.counters.minus1_minus1)))
                .and_then(|n| n.checked_sub(i32::from(minus)))
                .ok_or_else(|| {
                    error("Phoenix public native PT prefix overflow; shared checked query required")
                })?;
        }
    }
    Ok(())
}
fn current(state: &GameState, id: ObjectId) -> CurrentV56 {
    let o = state.objects.get(id);
    let active = crate::continuous_characteristics_v1::printed_abilities_active(state, id);
    let types = [
        CardType::Land,
        CardType::Creature,
        CardType::Instant,
        CardType::Sorcery,
        CardType::Artifact,
        CardType::Enchantment,
        CardType::Planeswalker,
    ]
    .into_iter()
    .filter(|t| crate::engine::object_has_type(state, id, *t))
    .collect::<Vec<_>>();
    let creature = types.contains(&CardType::Creature);
    CurrentV56 {
        source: reference(state, AbilitySourceContractV4::capture(state, id)),
        token: o.v4.is_token,
        effective_names: crate::engine::effective_names(state, id)
            .into_iter()
            .map(str::to_owned)
            .collect(),
        effective_types: types,
        effective_subtype_ids: crate::engine::effective_subtype_ids(state, id),
        color_mask: crate::engine::object_color_mask(state, id),
        printed_abilities_active: active,
        power: creature.then(|| crate::engine::effective_power(state, id)),
        toughness: creature.then(|| crate::engine::effective_toughness(state, id)),
        flying: crate::engine::has_effective_keyword(state, id, Keywords::FLYING),
        haste: crate::engine::has_effective_keyword(state, id, Keywords::HASTE),
        attacks_each_combat_if_able: crate::combat_static_rules_v1::active(state, id)
            .must_attack_each_combat,
        graveyard_ability_functions_here: o.zone == Zone::Graveyard
            && active
            && o.v4.face_index == 0,
    }
}
fn trigger(
    state: &GameState,
    source: AbilitySourceContractV4,
    stage: StageV56,
) -> Result<TriggerV56, RlContractError> {
    if source.zone != Zone::Graveyard || source.owner != source.controller {
        return Err(error("Phoenix original graveyard actor/zone changed"));
    }
    Ok(TriggerV56 {
        original_source: reference(state, source),
        current_original_source: current_original(state, source),
        original_actor: source.controller.into(),
        stage,
    })
}
pub fn capture(state: &GameState) -> Result<FactsV56, RlContractError> {
    if crate::linked_aura_operation_v1::locally_owned(state) {
        return Err(error(
            "Phoenix public facts require a settled generic operation boundary",
        ));
    }
    // Validate the original program, source contract, live choice and typed
    // answer before publishing any stage. No history receipt is synthesized.
    crate::effect::validate_pending_effect_choice(state).map_err(error)?;
    for item in &state.stack {
        crate::engine::validated_stack_item_target_spec(item, state).map_err(error)?;
    }
    let held = state
        .engine
        .pending_triggers
        .iter()
        .chain(state.legend_pending_v1.iter().flatten())
        .chain(
            state
                .pending_legend_rule_v1
                .iter()
                .flat_map(|p| p.waiting_triggers()),
        )
        .chain(
            state
                .standard_v1
                .iter()
                .filter_map(|s| s.resolution_play.as_ref())
                .flat_map(|p| p.deferred_triggers.iter()),
        )
        .collect::<Vec<_>>();
    for pending in &held {
        crate::engine::validate_pending_trigger_identity(state, pending).map_err(error)?;
    }
    let id = crate::card_def::card_id_by_name("Flamewake Phoenix")
        .ok_or_else(|| error("Phoenix actual printing absent"))?;
    let meaning = meaning::printed_meaning(id).map_err(error)?;
    let mut seen = std::collections::BTreeSet::new();
    let mut rows = Vec::new();
    let mut ferocious_now = [false; 2];
    for (seat, player) in state.players.iter().enumerate() {
        for (zone, roster) in [
            (Zone::Battlefield, &player.battlefield),
            (Zone::Graveyard, &player.graveyard),
        ] {
            for object in roster {
                let live = state
                    .objects
                    .try_get(*object)
                    .ok_or_else(|| error("Phoenix public roster object absent"))?;
                if live.zone != zone
                    || !seen.insert(*object)
                    || (if zone == Zone::Battlefield {
                        live.controller.index()
                    } else {
                        live.owner.index()
                    }) != seat
                {
                    return Err(error("Phoenix public roster changed or duplicated"));
                }
                checked_native_prefix(state, *object)?;
                if zone == Zone::Battlefield
                    && crate::engine::object_has_type(state, *object, CardType::Creature)
                {
                    ferocious_now[seat] |= crate::engine::effective_power(state, *object) >= 4;
                }
                // Concealment precedes printing identity, including token/copy cases.
                if live.v4.face_down_v1.is_none() && live.v4.face_index == 0 && live.card_def == id
                {
                    rows.push(current(state, *object));
                }
            }
        }
    }
    // Native token cessation removes live membership and retains the last zone
    // marker in the arena. Graveyard token tombstones are historical objects.
    if state.objects.iter().any(|(object, live)| {
        (live.zone == Zone::Battlefield || (live.zone == Zone::Graveyard && !live.v4.is_token))
            && !seen.contains(&object)
    }) {
        return Err(error("Phoenix public roster incomplete"));
    }
    let mut triggers = Vec::new();
    for pending in held {
        if let Some(source) = pending.source_contract.filter(|s| s.card_def == id) {
            // Generic validation above authenticates grants independently. A
            // Phoenix hosting an equipment trigger is not its printed return.
            if pending.granted_by.is_some()
                || (source.zone != Zone::Graveyard && pending.effect != meaning::return_program())
            {
                continue;
            }
            if pending.effect != meaning::return_program() {
                return Err(error("Phoenix held root changed"));
            }
            triggers.push(trigger(
                state,
                source,
                StageV56::Held {
                    placement_ordered: pending.placement_ordered,
                },
            )?);
        }
    }
    let mut spells = Vec::new();
    for item in &state.stack {
        if item.kind == StackItemKind::Spell {
            if let Some(s) = item.v4.source_contract.filter(|s| s.card_def == id) {
                spells.push(SpellV56 {
                    source: reference(
                        state,
                        AbilitySourceContractV4 {
                            source: s.source,
                            card_def: s.card_def,
                            owner: s.owner,
                            controller: s.controller,
                            zone: s.zone,
                            zone_change_count: s.zone_change_count,
                            attached_to: None,
                        },
                    ),
                    is_copy: item.is_copy,
                });
            }
        } else if item.kind == StackItemKind::TriggeredAbility {
            if let Some(source) = item.v4.ability_source_contract.filter(|s| s.card_def == id) {
                if item.v4.granted_by.is_some()
                    || (source.zone != Zone::Graveyard
                        && item.inline_effect.as_ref() != Some(&meaning::return_program()))
                {
                    continue;
                }
                if item.inline_effect.as_ref() != Some(&meaning::return_program()) {
                    return Err(error("Phoenix published root changed"));
                }
                let stage = if let Some(pending) = state
                    .engine
                    .pending_effect
                    .as_ref()
                    .filter(|p| p.resolving_item.v4.stack_item_id == item.v4.stack_item_id)
                {
                    match (
                        &pending.choice,
                        &pending.answered_choice_guard,
                        pending.frames.as_slice(),
                    ) {
                        (
                            Some(PendingEffectChoice::ChooseBoolean {
                                player,
                                purpose: EffectBooleanChoicePurpose::PayManaThen { .. },
                                ..
                            }),
                            None,
                            [],
                        ) => StageV56::PaymentOffered {
                            can_pay: crate::engine::can_pay_effect_mana(
                                *player,
                                &[crate::mana::ManaColor::R],
                                0,
                                state,
                            ),
                        },
                        (
                            None,
                            Some(EffectAnsweredChoiceGuard::PayManaThen { .. }),
                            [EffectFrame::PayManaThen { path, .. }],
                        ) => {
                            if path.last() == Some(&1) {
                                StageV56::AcceptedAwaitingPayment
                            } else {
                                StageV56::DeclinedAwaitingCompletion
                            }
                        }
                        _ => return Err(error("Phoenix resolving payment stage absent")),
                    }
                } else {
                    StageV56::Published
                };
                triggers.push(trigger(state, source, stage)?);
            }
        }
    }
    Ok(FactsV56 {
        schema: SCHEMA.into(),
        producer_sha256: producer_sha256(),
        card_db_hash: crate::card_def::KERNEL_CARDDB_HASH,
        meaning,
        ferocious_now,
        current: rows,
        spells,
        triggers,
    })
}
/// Full recapture from owned state is authority; no caller-computed digest is used.
pub fn validate(state: &GameState, value: &FactsV56) -> Result<(), RlContractError> {
    if capture(state)? == *value {
        Ok(())
    } else {
        Err(error("Phoenix current public facts differ"))
    }
}

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod ceased_token_tests {
    use super::*;
    use crate::{
        event::{self, ProposedEvent},
        ids::PlayerId,
        policy_observation_v6::tests::{put, ready_state_for},
    };

    #[test]
    fn native_ceased_food_preserves_public_facts_but_live_roster_erasures_refuse_both_seats() {
        for player in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(player);
            let food = put(&mut state, player, "Food Token", Zone::Battlefield);
            let original = capture(&state).unwrap();
            let mut missing_live = state.clone();
            missing_live.players[player.index()]
                .battlefield
                .retain(|id| *id != food);
            assert!(capture(&missing_live).is_err());

            // Actual native zone and cessation primitives retain the inert id.
            event::propose_and_commit(
                &mut state,
                ProposedEvent::zone_change(food, Zone::Graveyard),
            );
            assert!(event::cease_to_exist(&mut state, food));
            assert_eq!(state.objects.get(food).zone, Zone::Graveyard);
            assert!(!state.players[player.index()].graveyard.contains(&food));
            assert_eq!(capture(&state).unwrap(), original);
            let restored = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
            assert_eq!(capture(&restored).unwrap(), original);

            let card = put(&mut state, player, "Llanowar Elves", Zone::Graveyard);
            state.players[player.index()]
                .graveyard
                .retain(|id| *id != card);
            assert!(capture(&state).is_err());
        }
    }
}
