//! Separate Akroma V40 transport. Private original snapshots never leave the backend.
use crate::ids::PlayerId;
use crate::policy_observation_v8::PolicyObservationExtensionsV8;
use crate::policy_surface_v5::PolicySurfaceV5;
use crate::rl::{
    CardPrivateV1, CardStableRefV1, KnownLibraryCardV4, PlayerSeatV1,
    PublicObservationProjectionV5, RlContractError,
};
use crate::state::GameState;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub const SCHEMA: &str = "mtg-kernel/akroma-public/v1";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActiveGrantV1 {
    pub source: CardStableRefV1,
    pub controller: PlayerSeatV1,
    pub layer_timestamp: u64,
    pub recipients: Vec<CardStableRefV1>,
    pub keyword_mask: u32,
    pub protection_color_mask: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorDepartureV1 {
    pub source_at_departure: CardStableRefV1,
    pub event_order: u64,
    pub original_face_index: u8,
    pub was_face_down: bool,
    pub color_mask: u8,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RootV1 {
    pub card_id: u16,
    pub root: crate::rules_meaning_akroma_v1::RootV1,
    pub root_sha256: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AkromaObservationV1 {
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
    pub extensions: PolicyObservationExtensionsV8,
    pub attacked_this_turn: [bool; 2],
    pub grants: Vec<ActiveGrantV1>,
    pub families: Vec<crate::akroma_family_public_v40::FamilyFactsV40>,
    pub color_departures: Vec<ColorDepartureV1>,
    pub roots: Vec<RootV1>,
    pub projection_sha256: String,
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn producer_sha256_v1() -> String {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let mut h = Sha256::new();
        for source in [
            SCHEMA,
            include_str!("akroma_public_v1.rs"),
            include_str!("akroma_family_public_v40.rs"),
            include_str!("sower_public_v1.rs"),
            include_str!("human_bo3_v1/sower_v14.rs"),
            include_str!("frenzied_public_v1.rs"),
            include_str!("human_bo3_v1/frenzied_v15.rs"),
            include_str!("wildwood_public_v1.rs"),
            include_str!("human_bo3_v1/wildwood_v16.rs"),
            include_str!("authority_public_v1.rs"),
            include_str!("human_bo3_v1/authority_v17.rs"),
            include_str!("employment_public_v1.rs"),
            include_str!("human_bo3_v1/employment_v18.rs"),
            include_str!("cerberus_public_v1.rs"),
            include_str!("human_bo3_v1/cerberus_v19.rs"),
            include_str!("librarian_public_v1.rs"),
            include_str!("human_bo3_v1/librarian_v20.rs"),
            include_str!("extra_turn_public_v1.rs"),
            include_str!("human_bo3_v1/extra_turn_v23.rs"),
            include_str!("juggernaut_public_v1.rs"),
            include_str!("human_bo3_v1/juggernaut_v25.rs"),
            include_str!("ashroot_public_v1.rs"),
            include_str!("human_bo3_v1/ashroot_v27.rs"),
            include_str!("banishing_public_v1.rs"),
            include_str!("human_bo3_v1/banishing_v26.rs"),
            include_str!("stromkirk_public_v1.rs"),
            include_str!("human_bo3_v1/stromkirk_v28.rs"),
            include_str!("gourmand_public_v1.rs"),
            include_str!("human_bo3_v1/gourmand_v29.rs"),
            include_str!("garruk_public_v1.rs"),
            include_str!("human_bo3_v1/garruk_v30.rs"),
            include_str!("crystal_public_v1.rs"),
            include_str!("human_bo3_v1/crystal_v22.rs"),
            include_str!("arahbo_public_v1.rs"),
            include_str!("human_bo3_v1/arahbo_v31.rs"),
            include_str!("strongbox_public_v1.rs"),
            include_str!("human_bo3_v1/strongbox_v32.rs"),
            include_str!("fiery_observation_v1.rs"),
            include_str!("human_bo3_v1/fiery_v33.rs"),
            include_str!("krenko_observation_v1.rs"),
            include_str!("human_bo3_v1/krenko_v34.rs"),
            include_str!("zul_ashur_observation_v1.rs"),
            include_str!("human_bo3_v1/zul_ashur_v35.rs"),
            include_str!("banner_observation_v1.rs"),
            include_str!("human_bo3_v1/banner_v36.rs"),
            include_str!("inspiration_observation_v1.rs"),
            include_str!("human_bo3_v1/inspiration_v37.rs"),
            include_str!("akroma_early_public_v40.rs"),
            include_str!("heartfire_public_v1.rs"),
            include_str!("raid_entry_public_v1.rs"),
            include_str!("rl_session/soul_shackled_zombie_v13.rs"),
            include_str!("bloom_tender_public_v1.rs"),
            include_str!("bloom_tender_mana_v1.rs"),
            include_str!("human_bo3_v1/akroma_early_v40.rs"),
            include_str!("human_bo3_v1/akroma_families_v40.rs"),
            include_str!("rules_meaning_akroma_v1.rs"),
            include_str!("controlled_creature_grants_v1.rs"),
            include_str!("rl.rs"),
            include_str!("rl/heartfire_history_v1.rs"),
            include_str!("heartfire_history_v1.rs"),
            include_str!("planeswalker_v1.rs"),
            include_str!("rl_session.rs"),
            include_str!("rl_session/akroma_v40.rs"),
            include_str!("human_bo3_v1/akroma_v40.rs"),
            include_str!("human_bo3_v1/visible.rs"),
            include_str!("human_bo3_v1/labels.rs"),
            include_str!("human_bo3_v1/life_ward_v8.rs"),
            include_str!("sower_restriction_capture_v1.rs"),
            include_str!("policy_observation_v8.rs"),
            include_str!("engine.rs"),
            include_str!("event.rs"),
            include_str!("effect.rs"),
            include_str!("trigger.rs"),
            include_str!("state.rs"),
            include_str!("card_def.rs"),
            include_str!("continuous_characteristics_v1.rs"),
            include_str!("standard_cards_v1.rs"),
            include_str!("../build.rs"),
        ] {
            h.update([0]);
            h.update(source.as_bytes());
        }
        for dependency in [
            crate::sower_public_v1::producer_sha256_v1(),
            crate::frenzied_public_v1::producer_sha256_v1(),
            crate::wildwood_public_v1::producer_sha256_v1(),
            crate::authority_public_v1::producer_sha256_v1(),
            crate::employment_public_v1::producer_sha256_v1(),
            crate::cerberus_public_v1::producer_sha256_v1(),
            crate::librarian_public_v1::producer_sha256_v1(),
            crate::extra_turn_public_v1::producer_sha256_v1(),
            crate::juggernaut_public_v1::producer_sha256_v1(),
            crate::ashroot_public_v1::producer_sha256_v1(),
            crate::banishing_public_v1::producer_sha256_v1(),
            crate::stromkirk_public_v1::producer_sha256_v1(),
            crate::gourmand_public_v1::producer_sha256_v1(),
            crate::garruk_public_v1::producer_sha256_v1(),
            crate::crystal_public_v1::producer_sha256_v1(),
            crate::arahbo_public_v1::producer_sha256_v1(),
            crate::strongbox_public_v1::producer_sha256_v1(),
            crate::fiery_observation_v1::producer_sha256_v1(),
            crate::krenko_observation_v1::producer_sha256_v1(),
            crate::zul_ashur_observation_v1::producer_sha256_v1(),
            crate::banner_observation_v1::producer_sha256_v1(),
            crate::inspiration_observation_v1::producer_sha256_v1(),
            crate::heartfire_public_v1::source_sha256(),
            crate::ooze_public_v1::source_sha256(),
            crate::soul_shackled_zombie_public_v1::source_sha256(),
            crate::bloom_tender_public_v1::producer_sha256_v1(),
            crate::raid_entry_public_v1::producer_sha256_v1(),
        ] {
            h.update([0]);
            h.update(dependency.as_bytes());
        }
        h.finalize().iter().map(|b| format!("{b:02x}")).collect()
    })
    .clone()
}
fn commitment(observation: &AkromaObservationV1) -> Result<String, RlContractError> {
    let mut value =
        serde_json::to_value(observation).map_err(|e| RlContractError(e.to_string()))?;
    value
        .as_object_mut()
        .ok_or_else(|| RlContractError("Akroma observation shape".into()))?
        .remove("projection_sha256");
    Ok(digest(
        &serde_json::to_vec(&value).map_err(|e| RlContractError(e.to_string()))?,
    ))
}
impl AkromaObservationV1 {
    pub fn validate_artifact(&self) -> Result<(), RlContractError> {
        if self.schema != SCHEMA
            || self.producer_sha256 != producer_sha256_v1()
            || self.kernel_version != crate::KERNEL_VERSION
            || self.roots != roots()?
            || self.substep_count == 0
            || self.substep_index >= self.substep_count
            || self.card_db_hash != crate::card_def::KERNEL_CARDDB_HASH
            || self.projection_sha256 != commitment(self)?
        {
            return Err(RlContractError(
                "Akroma observation identity mismatch".into(),
            ));
        }
        Ok(())
    }
}
pub(crate) fn validate_state(state: &GameState) -> Result<(), String> {
    crate::controlled_creature_grants_v1::validate_color_history(state)?;
    roots().map_err(|error| error.0)?;
    crate::akroma_family_public_v40::validate_state(state)?;
    Ok(())
}
pub(crate) fn roots() -> Result<Vec<RootV1>, RlContractError> {
    let mut rows = Vec::new();
    for (id, d) in crate::card_def::CARD_DEFS.iter().enumerate() {
        if d.controlled_creature_grant.is_none() {
            continue;
        }
        let selector = crate::rules_meaning_akroma_v1::live_selector()
            .map_err(|_| RlContractError("Akroma catalog selector".into()))?;
        for root in [
            crate::rules_meaning_akroma_v1::RootV1::PermanentSpell,
            crate::rules_meaning_akroma_v1::RootV1::ControlledCreatureGrant,
        ] {
            let m = crate::rules_meaning_akroma_v1::meaning(&selector, id as u16, root).map_err(
                |_| RlContractError("unsupported controlled creature grant definition".into()),
            )?;
            rows.push(RootV1 {
                card_id: m.card_id,
                root: m.root,
                root_sha256: m.root_sha256,
            });
        }
    }
    Ok(rows)
}
pub(crate) fn facts(
    state: &GameState,
) -> Result<(Vec<ActiveGrantV1>, Vec<ColorDepartureV1>), RlContractError> {
    crate::controlled_creature_grants_v1::validate_color_history(state).map_err(RlContractError)?;
    let mut grants = Vec::new();
    for (id, live) in state.objects.iter() {
        let Some(d) = crate::card_def::CARD_DEFS.get(usize::from(live.card_def)) else {
            continue;
        };
        let Some(grant) = d.controlled_creature_grant else {
            continue;
        };
        if !crate::controlled_creature_grants_v1::active(state, id) {
            continue;
        }
        let recipients = state.players[live.controller.index()]
            .battlefield
            .iter()
            .copied()
            .filter(|recipient| {
                crate::controlled_creature_grants_v1::applies(state, id, *recipient)
            })
            .map(|recipient| {
                crate::sower_restriction_capture_v1::masked_ref(
                    recipient,
                    state.objects.get(recipient),
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        grants.push(ActiveGrantV1 {
            source: crate::sower_restriction_capture_v1::masked_ref(id, live)?,
            controller: live.controller.into(),
            layer_timestamp: live.v4.layer_timestamp.unwrap_or(0),
            recipients,
            keyword_mask: grant.keywords.0,
            protection_color_mask: grant.protection_color_mask,
        });
    }
    let mut departures = Vec::new();
    for (order, event) in state.engine.event_history.iter().enumerate() {
        let crate::event::CommittedEvent::ProtectionSourceColorDepartureV1 { snapshot, .. } = event
        else {
            continue;
        };
        let original: crate::state::GameObject =
            serde_json::from_str(&snapshot.private_snapshot_json)
                .map_err(|_| RlContractError("invalid original visibility".into()))?;
        let was_face_down = original.v4.face_down_v1.is_some();
        departures.push(ColorDepartureV1 {
            source_at_departure: crate::sower_restriction_capture_v1::masked_ref(
                snapshot.source.source,
                &original,
            )?,
            event_order: order as u64,
            original_face_index: if was_face_down {
                0
            } else {
                original.v4.face_index
            },
            was_face_down,
            color_mask: snapshot.color_mask,
        });
    }
    Ok((grants, departures))
}
pub fn observe_v1(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<AkromaObservationV1, RlContractError> {
    observe_parts_v40(state, surface, actor, step, physical, substep)
        .map(|(observation, _)| observation)
}
type AkromaPublicPartsV40 = (
    AkromaObservationV1,
    Vec<crate::akroma_family_public_v40::FamilyProjectionV40>,
);
pub(crate) fn observe_parts_v40(
    state: &GameState,
    surface: &PolicySurfaceV5,
    actor: PlayerId,
    step: u64,
    physical: u64,
    substep: (u32, u32),
) -> Result<AkromaPublicPartsV40, RlContractError> {
    let (substep_index, substep_count) = substep;
    let roots = roots()?;
    let (grants, color_departures) = facts(state)?;
    let base = crate::rl::akroma_family_observation_storage_v40(
        state,
        surface,
        actor,
        step,
        physical,
        (substep_index, substep_count),
    )?;
    let storage = crate::policy_observation_v8::observation_storage_v8(state, actor, base)?;
    let mut base = storage.base;
    let mut parts =
        crate::akroma_family_public_v40::build(state, surface, actor, step, physical, substep)?;
    base.projection = crate::akroma_family_public_v40::merge_projection(&base.projection, &parts)?;
    let mut observation = AkromaObservationV1 {
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
        grants,
        families: parts.iter().map(|part| part.facts()).collect(),
        color_departures,
        roots,
        projection_sha256: String::new(),
    };
    observation.projection_sha256 = commitment(&observation)?;
    for part in &mut parts {
        part.set_projection(&observation.projection);
    }
    Ok((observation, parts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::state::Zone;
    #[test]
    fn registered_predecessor_projection_and_private_fields_match_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state_for(actor);
            put(&mut state, actor, "Forest", Zone::Hand);
            put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
            let surface = PolicySurfaceV5::new_for_session();
            let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
            let before = serde_json::to_vec(&state).unwrap();
            let observation = observe_v1(&state, &surface, actor, 0, 0, (0, 1)).unwrap();
            assert_eq!(observation.projection, old.projection);
            assert_eq!(observation.extensions, old.extensions);
            assert_eq!(observation.own_hand, old.own_hand);
            assert_eq!(observation.known_hand_cards, old.known_hand_cards);
            assert_eq!(observation.known_face_down_cards, old.known_face_down_cards);
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let mut bad = observation.clone();
            bad.card_db_hash ^= 1;
            assert!(bad.validate_artifact().is_err());
            let mut bad = observation;
            bad.producer_sha256.clear();
            assert!(bad.validate_artifact().is_err());
        }
    }
    #[test]
    #[ignore = "requires genuine Akroma catalog; typed face-down primitive is not an admitted disguise spell"]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn original_hidden_departure_masks_private_definition_after_later_reentry_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            assert!(crate::card_def::card_id_by_name("Akroma's Memorial").is_some());
            let mut state = ready_state_for(actor);
            let id = put(&mut state, actor, "Goblin Boarders", Zone::Battlefield);
            state.objects.get_mut(id).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
                disguised: true,
                lookers: 0,
                hidden_by: None,
                hidden_by_source: None,
            });
            let original_generation = state.objects.get(id).zone_change_count;
            crate::event::propose_and_commit(
                &mut state,
                crate::event::ProposedEvent::zone_change(id, Zone::Hand),
            );
            let bytes = serde_json::to_vec(&state).unwrap();
            let mut restored: GameState = serde_json::from_slice(&bytes).unwrap();
            for branch in [&mut state, &mut restored] {
                crate::event::propose_and_commit(
                    branch,
                    crate::event::ProposedEvent::zone_change(id, Zone::Battlefield),
                );
                assert!(branch.objects.get(id).zone_change_count > original_generation);
                let (_, departures) = facts(branch).unwrap();
                let old = departures
                    .iter()
                    .find(|row| row.source_at_departure.arena_id == id.0)
                    .unwrap();
                assert_eq!(
                    old.source_at_departure.zone_change_count,
                    original_generation
                );
                assert_eq!(
                    old.source_at_departure.card_db_id,
                    crate::card_def::card_id_by_name("Face-down creature").unwrap()
                );
                assert!(old.was_face_down);
                assert_eq!(old.original_face_index, 0);
                assert_eq!(old.color_mask, 0);
                let public = serde_json::to_string(&departures).unwrap();
                assert!(!public.contains("private_snapshot_json"));
                assert!(!public.contains("Goblin Boarders"));
            }
            assert_eq!(
                serde_json::to_vec(&state).unwrap(),
                serde_json::to_vec(&restored).unwrap()
            );
        }
    }
}
