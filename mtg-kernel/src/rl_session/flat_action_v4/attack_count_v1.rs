//! #454: the frozen flat scorers receive the per-turn attack count only
//! while the observation they encode shows a card that reads it.

use crate::policy_observation_v6::ObservationV6;
use crate::rl_session::FlatActionDecisionSliceErrorV1;

/// Drops the per-turn attack count from an observation about to be scored
/// when nothing in that observation reads it (#454). Called only by the
/// scoring observations `FastActorSessionV1::flat_policy_observation_v3`
/// (FlatV3, via `build_scoring_owned_v3`) and `flat_policy_observation_v4`
/// (FlatV4, V7 and V8, via `build_scoring_owned_v4`), which the scorers read
/// just before `build_globals`. The shared builders,
/// `rl::observe_policy_v6_unhashed_for_flat_policy` and the raw session
/// observations (`flat_policy_observation_raw_v3`/`_v4`), which human
/// adapters, viewer recordings, traces and diagnostics read, still carry any
/// nonzero count. The FlatV2 (unhashed V5 observation, `build_cache`),
/// V5 (V8 observation, `build_scoring_owned_v5`) and V6 (V9 Raid observation
/// over the V5 encoder) paths do not call this and still refuse the count.
///
/// The test reads only the completed observation: the count stays when any
/// card identity the observation carries is a reader. An identity is any
/// number (or list of numbers) under a key that `card_identity_key_v1`
/// accepts, in any field; today the V6 tree carries `card_db_id`
/// (`CardStableRefV1`, `FaceDownCardKnowledgeV1`) and `source_card_def`
/// (`PublicRestrictedManaV1`). That is a function of what the acting seat
/// receives, so a card it is not shown, such as an opponent's concealed card
/// or a face-down card behind its neutral identity, cannot change the result.
/// A reader that keeps the field still refuses in `build_globals`; this
/// omits an unread field, it never ignores an unknown one.
///
/// Cost: the observation is serialized and walked only while the field is
/// present, that is on decisions after an attack in the current turn. In
/// ordinary default-build games the engine records no attack (its writer
/// needs `standard-magezero-fixtures`), so the field is absent and this
/// returns at once.
pub(crate) fn omit_unread_attack_count_v1(
    observation: &mut ObservationV6,
) -> Result<(), FlatActionDecisionSliceErrorV1> {
    if observation
        .projection
        .creatures_attacked_this_turn
        .is_none()
    {
        return Ok(());
    }
    let shown = serde_json::to_value(&*observation)
        .map_err(|_| FlatActionDecisionSliceErrorV1::InvalidDecisionRelation)?;
    if !shows_attack_count_reader_v1(&shown) {
        observation.projection.creatures_attacked_this_turn = None;
    }
    Ok(())
}

/// Keys whose numbers are card definition ids: a `card`/`cards` segment
/// ending the name with `id`, `ids`, `def` or `defs` (`card_db_id`,
/// `printed_card_db_id`, `card_def`, `source_card_def`, `token_card_id`), any
/// `_def`/`_defs` suffix, and `card` or `cards` themselves. Wider than the
/// two names the V6 tree uses today; a false match can only keep the field.
/// `eternal_actor_card_identity_field_names_are_pinned` fails when a field
/// whose name looks like a card id but is not accepted here is declared.
fn card_identity_key_v1(key: &str) -> bool {
    let segments: Vec<&str> = key.split('_').collect();
    let last = segments.last().copied().unwrap_or_default();
    matches!(key, "card" | "cards")
        || (segments.len() > 1 && matches!(last, "def" | "defs"))
        || (segments
            .iter()
            .any(|segment| matches!(*segment, "card" | "cards"))
            && matches!(last, "id" | "ids"))
}

fn shows_attack_count_reader_v1(value: &serde_json::Value) -> bool {
    fn reads(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(id) => id
                .as_u64()
                .and_then(|id| u16::try_from(id).ok())
                .is_some_and(reads_attack_count_v1),
            serde_json::Value::Array(items) => items.iter().any(reads),
            _ => false,
        }
    }
    match value {
        serde_json::Value::Object(fields) => fields.iter().any(|(key, field)| {
            (card_identity_key_v1(key) && reads(field)) || shows_attack_count_reader_v1(field)
        }),
        serde_json::Value::Array(items) => items.iter().any(shows_attack_count_reader_v1),
        _ => false,
    }
}

/// Cards whose rules read this turn's attack record
/// (`GameState::creatures_attacked_turn_v1`): a generic cost reduction
/// counted by `DynamicCountDef::CreaturesAttackedThisTurn` (Witchstalker
/// Frenzy, Search Party Captain), plus the card-code readers below.
/// `eternal_actor_attack_count_read_sites_are_pinned` fails when a read site
/// or a data-driven reader is added.
fn reads_attack_count_v1(card_def: u16) -> bool {
    crate::card_def::CARD_DEFS
        .get(usize::from(card_def))
        .is_some_and(|definition| {
            matches!(
                definition.generic_cost_reduction,
                Some(crate::card_def::GenericCostReductionDef {
                    count: crate::card_def::DynamicCountDef::CreaturesAttackedThisTurn,
                    ..
                })
            ) || ATTACK_RECORD_CARD_CODE_READERS_V1.contains(&definition.name)
        })
}

/// Readers that card code names: Case of the Gateway Express's solve
/// condition (`standard_cards_v1::attackers_this_turn`) and Broadside
/// Bombardiers' boast restriction, which reads the record directly
/// (`eternal_legacy_e9a_v1::statics::attacked_this_turn`). The count cannot
/// express Bombardiers' per-creature history, so it keeps the refusal.
const ATTACK_RECORD_CARD_CODE_READERS_V1: [&str; 2] =
    ["Case of the Gateway Express", "Broadside Bombardiers"];

/// #454: the per-turn attack count reaches the frozen scorers only while the
/// observation shows a card that reads it. Every test is named
/// `eternal_actor_*` so the Eternal library filter runs it too.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_def::{DynamicCountDef, GenericCostReductionDef, CARD_DEFS};
    use crate::engine::{advance_until_decision, step, Action, Decision};
    use crate::flat_policy_v2::{
        encode_observation_owned_tables_for_fixture_v3, FlatDecisionErrorV2,
        FlatScoringOwnedBuffersV2,
    };
    use crate::ids::{ObjectId, PlayerId};
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::rl::PlayerSeatV1;
    use crate::rl_session::{FastActorDecisionKindV1, FastActorResponseV1, FastActorSessionV1};
    use crate::state::Zone;
    use crate::state::{GameState, ObjectLinkV4, Step};
    use std::collections::BTreeMap;
    use std::path::Path;

    /// `attacker` declares one real attack with a Generous Ent after `setup`
    /// adds its cards. Only builds with the Standard feature record it.
    fn after_attack(
        attacker: PlayerId,
        setup: impl FnOnce(&mut GameState),
    ) -> (GameState, ObjectId) {
        let mut state = ready_state_for(attacker);
        let creature = put(&mut state, attacker, "Generous Ent", Zone::Battlefield);
        setup(&mut state);
        state.step = Step::DeclareAttackers;
        assert!(matches!(
            advance_until_decision(&mut state),
            Decision::DeclareAttackers { player, .. } if player == attacker
        ));
        step(&mut state, Action::DeclareAttackers(vec![creature])).unwrap();
        let recorded = u32::from(cfg!(feature = "standard-magezero-fixtures"));
        assert_eq!(state.creatures_attacked_this_turn_v1(), recorded);
        (state, creature)
    }

    fn after_attack_for_scoring(
        attacker: PlayerId,
        setup: impl FnOnce(&mut GameState),
    ) -> (GameState, ObjectId) {
        after_attack(attacker, |state| {
            // The policy skips lone passes and empty block declarations.
            put(
                state,
                attacker.opponent(),
                "Voldaren Epicure",
                Zone::Battlefield,
            );
            setup(state);
        })
    }

    /// What the shared flat entry returns to every consumer.
    fn entry(state: &GameState, observer: PlayerId) -> ObservationV6 {
        crate::rl::observe_policy_v6_unhashed_for_flat_policy(
            state,
            &crate::policy_surface_v5::PolicySurfaceV5::new(),
            observer,
            0,
            0,
            0,
            1,
        )
        .unwrap()
    }

    /// What the model path scores.
    fn scored(state: &GameState, observer: PlayerId) -> ObservationV6 {
        let mut observation = entry(state, observer);
        omit_unread_attack_count_v1(&mut observation).unwrap();
        observation
    }

    fn encode_error(observation: &ObservationV6) -> Option<FlatDecisionErrorV2> {
        encode_observation_owned_tables_for_fixture_v3(observation).err()
    }

    /// The V4 scorer on the session's own current decision.
    fn score_v4(state: GameState) -> Result<(), FlatDecisionErrorV2> {
        let turn = state.turn_key_v1();
        let defender = PlayerSeatV1::from(state.active_player.opponent());
        let session = FastActorSessionV1::from_v3_fixture_state(state);
        let response = session.current_response();
        let FastActorResponseV1::Decision(expected) = response else {
            panic!("expected a live blocking decision, got {response:?}");
        };
        assert_eq!(session.state.turn_key_v1(), turn);
        assert_eq!(expected.acting_player, defender);
        assert_eq!(
            expected.decision_kind,
            FastActorDecisionKindV1::BlockerInclusion
        );
        assert_eq!(expected.legal_action_count, 2);
        let (mut objects, mut relations, mut object_subtypes) =
            (Vec::new(), Vec::new(), Vec::new());
        let (mut ability_uses, mut goads, mut completed_dungeons) =
            (Vec::new(), Vec::new(), Vec::new());
        let (mut effect_subtype_changes, mut context_path_elements) = (Vec::new(), Vec::new());
        let (mut actions, mut action_refs) = (Vec::new(), Vec::new());
        session
            .encode_current_flat_scoring_decision_owned_v4(
                expected,
                &mut crate::flat_policy_v4::FlatDecisionEncoderV4::default(),
                &mut FlatScoringOwnedBuffersV2 {
                    objects: &mut objects,
                    relations: &mut relations,
                    object_subtypes: &mut object_subtypes,
                    ability_uses: &mut ability_uses,
                    goads: &mut goads,
                    completed_dungeons: &mut completed_dungeons,
                    effect_subtype_changes: &mut effect_subtype_changes,
                    context_path_elements: &mut context_path_elements,
                    actions: &mut actions,
                    action_refs: &mut action_refs,
                },
            )
            .map(|_| ())
    }

    /// Every build. In the default build the engine records no attack, so the
    /// count is zero and the omission returns at once; the record is then
    /// written directly to exercise the rule itself.
    #[test]
    fn eternal_actor_attack_count_is_omitted_without_a_shown_reader() {
        for attacker in [PlayerId::P0, PlayerId::P1] {
            let (state, creature) = after_attack_for_scoring(attacker, |_| {});
            assert_eq!(score_v4(state.clone()), Ok(()));
            let mut recorded = state.clone();
            recorded.creatures_attacked_turn_v1 = Some((
                recorded.turn_key_v1(),
                attacker,
                vec![ObjectLinkV4 {
                    object: creature,
                    zone_change_count: recorded.objects.get(creature).zone_change_count,
                }],
            ));
            assert_eq!(recorded.creatures_attacked_this_turn_v1(), 1);
            for state in [&state, &recorded] {
                let count = state.creatures_attacked_this_turn_v1();
                for observer in [PlayerId::P0, PlayerId::P1] {
                    // Human adapters, viewers and traces still see the count.
                    assert_eq!(
                        entry(state, observer)
                            .projection
                            .creatures_attacked_this_turn,
                        (count != 0).then_some(count)
                    );
                    let observation = scored(state, observer);
                    assert_eq!(observation.projection.creatures_attacked_this_turn, None);
                    assert_eq!(encode_error(&observation), None);
                }
            }
        }
    }

    #[cfg(feature = "standard-magezero-fixtures")]
    #[test]
    fn eternal_actor_attack_count_with_a_reader_in_own_hand_still_refuses() {
        for attacker in [PlayerId::P0, PlayerId::P1] {
            let (state, _) = after_attack(attacker, |state| {
                put(state, attacker, "Witchstalker Frenzy", Zone::Hand);
            });
            let observation = scored(&state, attacker);
            assert_eq!(observation.projection.creatures_attacked_this_turn, Some(1));
            assert_eq!(
                encode_error(&observation),
                Some(FlatDecisionErrorV2::ObservationContract)
            );
            let mut cleared = observation.clone();
            cleared.projection.creatures_attacked_this_turn = None;
            assert_eq!(encode_error(&cleared), None, "the count alone refuses");
            // The opponent is not shown that hand card.
            let opponent = scored(&state, attacker.opponent());
            assert_eq!(opponent.projection.creatures_attacked_this_turn, None);
            assert_eq!(encode_error(&opponent), None);
        }
    }

    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn eternal_actor_attack_count_with_a_reader_on_the_battlefield_refuses_for_both_seats() {
        for reader in ["Search Party Captain", "Broadside Bombardiers"] {
            for attacker in [PlayerId::P0, PlayerId::P1] {
                for controller in [PlayerId::P0, PlayerId::P1] {
                    let (state, _) = after_attack_for_scoring(attacker, |state| {
                        put(state, controller, reader, Zone::Battlefield);
                    });
                    assert_eq!(
                        score_v4(state.clone()),
                        Err(FlatDecisionErrorV2::ObservationContract),
                        "{reader}"
                    );
                    for observer in [PlayerId::P0, PlayerId::P1] {
                        let observation = scored(&state, observer);
                        assert_eq!(
                            observation.projection.creatures_attacked_this_turn,
                            Some(1),
                            "{reader}"
                        );
                        assert_eq!(
                            encode_error(&observation),
                            Some(FlatDecisionErrorV2::ObservationContract)
                        );
                        let mut cleared = observation.clone();
                        cleared.projection.creatures_attacked_this_turn = None;
                        assert_eq!(encode_error(&cleared), None, "{reader}: the count alone");
                    }
                }
            }
        }
    }

    /// A concealed opposing card cannot change what the observing seat is
    /// scored on: swapping it between a reader and an inert card leaves the
    /// scored observation, its serialized bytes and the encode outcome equal.
    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn eternal_actor_attack_count_ignores_a_concealed_opposing_reader() {
        for attacker in [PlayerId::P0, PlayerId::P1] {
            for observer in [PlayerId::P0, PlayerId::P1] {
                let concealed = |name: &'static str| {
                    after_attack(attacker, |state| {
                        put(state, observer.opponent(), name, Zone::Hand);
                    })
                    .0
                };
                let (reader, inert) = (concealed("Search Party Captain"), concealed("Island"));
                let (left, right) = (scored(&reader, observer), scored(&inert, observer));
                assert_eq!(left, right);
                assert_eq!(
                    serde_json::to_vec(&left).unwrap(),
                    serde_json::to_vec(&right).unwrap()
                );
                assert_eq!(left.projection.creatures_attacked_this_turn, None);
                assert_eq!(encode_error(&left), encode_error(&right));
                assert_eq!(encode_error(&left), None);
            }
        }
    }

    #[cfg(feature = "eternal-fixtures")]
    fn face_down(state: &mut GameState, object: ObjectId, lookers: u8) {
        state.objects.get_mut(object).v4.face_down_v1 = Some(crate::state::FaceDownV1 {
            disguised: false,
            lookers,
            hidden_by: None,
            hidden_by_source: None,
        });
    }

    /// A reader in an opposing hand that was revealed to the seat is shown,
    /// so the count stays and the frozen encoders refuse.
    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn eternal_actor_attack_count_with_a_revealed_opposing_reader_refuses() {
        for attacker in [PlayerId::P0, PlayerId::P1] {
            for observer in [PlayerId::P0, PlayerId::P1] {
                let owner = observer.opponent();
                let (state, _) = after_attack(attacker, |state| {
                    let card = put(state, owner, "Search Party Captain", Zone::Hand);
                    state.reveal_hand_card(observer, owner, card).unwrap();
                });
                let observation = scored(&state, observer);
                assert_eq!(observation.projection.creatures_attacked_this_turn, Some(1));
                assert_eq!(
                    encode_error(&observation),
                    Some(FlatDecisionErrorV2::ObservationContract)
                );
            }
        }
    }

    /// A face-down exiled card the seat may not look at shows only its neutral
    /// identity: swapping a reader for an inert card changes nothing the seat
    /// is scored on. One it may look at is shown, so the count stays.
    #[cfg(feature = "eternal-fixtures")]
    #[test]
    fn eternal_actor_attack_count_follows_face_down_look_permission() {
        for attacker in [PlayerId::P0, PlayerId::P1] {
            for observer in [PlayerId::P0, PlayerId::P1] {
                let owner = observer.opponent();
                let exiled = |name: &'static str, lookers: u8| {
                    after_attack(attacker, |state| {
                        let card = put(state, owner, name, Zone::Exile);
                        face_down(state, card, lookers);
                    })
                    .0
                };
                let hidden = 1 << owner.index();
                let (reader, inert) = (
                    exiled("Search Party Captain", hidden),
                    exiled("Island", hidden),
                );
                let (left, right) = (scored(&reader, observer), scored(&inert, observer));
                assert_eq!(left, right);
                assert_eq!(
                    serde_json::to_vec(&left).unwrap(),
                    serde_json::to_vec(&right).unwrap()
                );
                assert_eq!(left.projection.creatures_attacked_this_turn, None);
                assert_eq!(encode_error(&left), encode_error(&right));

                let looked = exiled("Search Party Captain", hidden | (1 << observer.index()));
                let observation = scored(&looked, observer);
                assert_eq!(observation.projection.creatures_attacked_this_turn, Some(1));
                assert_eq!(
                    encode_error(&observation),
                    Some(FlatDecisionErrorV2::ObservationContract)
                );
            }
        }
    }

    /// Fails when a field whose name looks like a card id (a `card` or
    /// `cards` segment, or a `_def` suffix) holds an integer under a name
    /// `card_identity_key_v1` does not accept, so a new identity carrier
    /// cannot slip past the walk under a new name.
    #[test]
    fn eternal_actor_card_identity_field_names_are_pinned() {
        for accepted in [
            "card_db_id",
            "source_card_def",
            "printed_card_db_id",
            "card_def",
            "token_def",
            "card_id",
        ] {
            assert!(card_identity_key_v1(accepted), "{accepted}");
        }
        for refused in ["card_token", "card_db_hash", "maximum_cards", "arena_id"] {
            assert!(!card_identity_key_v1(refused), "{refused}");
        }
        let card_like = |name: &str| {
            let segments: Vec<&str> = name.split('_').collect();
            segments
                .iter()
                .any(|segment| matches!(*segment, "card" | "cards"))
                || (segments.len() > 1 && matches!(segments[segments.len() - 1], "def" | "defs"))
        };
        let mut found = std::collections::BTreeSet::new();
        let mut directories = vec![Path::new(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    directories.push(path);
                    continue;
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                for line in std::fs::read_to_string(&path).unwrap().lines() {
                    let mut line = line.trim();
                    for prefix in ["pub(crate) ", "pub(super) ", "pub "] {
                        if let Some(rest) = line.strip_prefix(prefix) {
                            line = rest;
                            break;
                        }
                    }
                    let Some((name, declared)) = line.split_once(':') else {
                        continue;
                    };
                    if declared.starts_with(':')
                        || !name.starts_with(|c: char| c.is_ascii_lowercase())
                        || !name
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
                    {
                        continue;
                    }
                    let integer = declared
                        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                        .any(|token| matches!(token, "u16" | "u32" | "u64"));
                    if integer && card_like(name) && !card_identity_key_v1(name) {
                        found.insert(name.to_string());
                    }
                }
            }
        }
        let expected: std::collections::BTreeSet<String> = [
            "base_card_rows",
            "card_db_hash",
            "card_def_idx",
            "card_definition_domain",
            "card_definition_id_permutation",
            "card_denominator",
            "card_embedding_dim",
            "card_games",
            "card_name",
            "card_table_bits",
            "card_token",
            "card_token_mapping_version",
            "card_vocab_size",
            "card_zone_change_count",
            "cards_drawn",
            "chosen_by_card",
            "current_rust_card_token_max",
            "expected_card_db_hash",
            "maximum_cards",
            "offered_actions_by_card",
            "offered_decisions_by_card",
            "opponent_plan_cards_in",
            "own_card_outcomes",
            "referenced_cards",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        assert_eq!(
            found, expected,
            "a field or binding whose name looks like a card id holds an integer under a name \
             card_identity_key_v1 (rl_session/flat_action_v4/attack_count_v1.rs) does not accept. If it can carry a card \
             definition id into ObservationV6, make card_identity_key_v1 accept the name; \
             otherwise add it to this list"
        );
    }

    /// Fails when a read site of this turn's attack record or a data-driven
    /// reader is added, moved or removed, so no new reader is omitted
    /// silently.
    #[test]
    fn eternal_actor_attack_count_read_sites_are_pinned() {
        let mut data_driven: Vec<&str> = CARD_DEFS
            .iter()
            .filter(|definition| {
                matches!(
                    definition.generic_cost_reduction,
                    Some(GenericCostReductionDef {
                        count: DynamicCountDef::CreaturesAttackedThisTurn,
                        ..
                    })
                )
            })
            .map(|definition| definition.name)
            .collect();
        data_driven.sort_unstable();
        data_driven.dedup();
        let mut expected = Vec::new();
        if cfg!(feature = "eternal-fixtures") {
            expected.push("Search Party Captain");
            assert!(crate::card_def::card_id_by_name("Broadside Bombardiers").is_some());
        }
        if cfg!(feature = "standard-magezero-fixtures") {
            expected.push("Witchstalker Frenzy");
            assert!(crate::card_def::card_id_by_name("Case of the Gateway Express").is_some());
        }
        assert_eq!(
            data_driven, expected,
            "card data now registers DynamicCountDef::CreaturesAttackedThisTurn for another \
             card; reads_attack_count_v1 in rl_session/flat_action_v4/attack_count_v1.rs already counts it, so add it to \
             the tests beside this one and update this list"
        );

        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let tokens = [
            concat!("creatures_attacked_", "this_turn_v1"),
            concat!("creatures_attacked_", "turn_v1"),
            concat!("CreaturesAttacked", "ThisTurn"),
            concat!("attackers_", "this_turn("),
            concat!("attacked_", "this_turn("),
        ];
        let wrappers = [
            concat!("attackers_", "this_turn("),
            concat!("attacked_", "this_turn("),
        ];
        let mut found = BTreeMap::new();
        let mut wrapper_calls = std::collections::BTreeSet::new();
        let mut scan = |path: &Path| {
            let text = std::fs::read_to_string(path).unwrap();
            let relative = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let lines = text
                .lines()
                .filter(|line| tokens.iter().any(|token| line.contains(token)))
                .count();
            for line in text.lines().map(str::trim) {
                if wrappers.iter().any(|wrapper| line.contains(wrapper)) && !line.starts_with("fn ")
                {
                    wrapper_calls.insert((relative.clone(), line.to_string()));
                }
            }
            if lines > 0 {
                found.insert(relative, lines);
            }
        };
        let is_rust = |path: &Path| path.extension().is_some_and(|extension| extension == "rs");
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if is_rust(&path) {
                scan(&path);
            }
        }
        let mut directories = vec![root.join("src")];
        while let Some(directory) = directories.pop() {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    directories.push(path);
                } else if is_rust(&path) {
                    scan(&path);
                }
            }
        }
        let expected: BTreeMap<String, usize> = [
            ("build.rs", 2),
            ("build_eternal_e6b_v1.rs", 1),
            ("src/card_def.rs", 1),
            ("src/engine.rs", 4),
            ("src/eternal_legacy_e9a_v1/statics.rs", 3),
            ("src/rl_session/flat_action_v4/attack_count_v1.rs", 12),
            ("src/infoset_v1/export.rs", 1),
            ("src/regret_census_v1/stage4a.rs", 1),
            ("src/regret_census_v1/stage4a/history_guard_tests.rs", 1),
            ("src/rl.rs", 4),
            ("src/rl/heartfire_history_v1.rs", 2),
            ("src/rules_vector_v1/meaning/reads.rs", 1),
            ("src/standard_cards_v1.rs", 3),
            ("src/state.rs", 5),
        ]
        .into_iter()
        .map(|(path, lines): (&str, usize)| (path.to_string(), lines))
        .collect();
        assert_eq!(
            found, expected,
            "a read site of this turn's attack record (GameState::creatures_attacked_turn_v1, \
             GameState::creatures_attacked_this_turn_v1, the attackers_this_turn wrapper or \
             DynamicCountDef::CreaturesAttackedThisTurn) was added, moved or removed. If a card \
             now reads it, make reads_attack_count_v1 in rl_session/flat_action_v4/attack_count_v1.rs recognize the card \
             (card-code readers go in ATTACK_RECORD_CARD_CODE_READERS_V1) and add a test for it \
             beside this one, then update this list"
        );

        // Each call of a card-code wrapper names its card, and that card must
        // be one of ATTACK_RECORD_CARD_CODE_READERS_V1.
        let tied = [
            (
                "src/eternal_legacy_e9a_v1/statics.rs",
                concat!(
                    "(BROADSIDE_BOMBARDIERS, 0) => attacked_",
                    "this_turn(state, source),"
                ),
                "Broadside Bombardiers",
            ),
            (
                "src/standard_cards_v1.rs",
                concat!(
                    "CASE_OF_THE_GATEWAY_EXPRESS => attackers_",
                    "this_turn(state) >= 3,"
                ),
                "Case of the Gateway Express",
            ),
        ];
        for (_, _, reader) in tied {
            assert!(
                ATTACK_RECORD_CARD_CODE_READERS_V1.contains(&reader),
                "{reader}"
            );
        }
        let tied: std::collections::BTreeSet<(String, String)> = tied
            .into_iter()
            .map(|(path, line, _)| (path.to_string(), line.to_string()))
            .collect();
        assert_eq!(
            wrapper_calls, tied,
            "a card-code wrapper of this turn's attack record (attackers_this_turn in \
             standard_cards_v1.rs, attacked_this_turn in eternal_legacy_e9a_v1/statics.rs) has a \
             new or changed call. Add the calling card to ATTACK_RECORD_CARD_CODE_READERS_V1 in \
             rl_session/flat_action_v4/attack_count_v1.rs, add a test for it beside this one, then update this list"
        );
    }
}
