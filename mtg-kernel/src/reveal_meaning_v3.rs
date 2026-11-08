//! Opt-in event-time projection for the Spellbench reveal transcript. The
//! journal contains only viewer-visible names and operation-local links.
use crate::ids::{ObjectId, PlayerId};
use crate::state::Zone;
use serde_json::{json, Value};

mod reducer;
pub use reducer::RevealReducerV3;

#[derive(Clone, Copy)]
pub(crate) enum RevealV3 {
    Present,
    PublicTransfer(Zone),
    SearchReveal,
    Omit,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RevealJournalV3 {
    viewers: Option<[Vec<Value>; 2]>,
    // Foretell commits an ordinary exile move before marking it face down.
    // Scope the override to that exact move and never export the native ID.
    concealed_exile: Option<ObjectId>,
}

impl PartialEq for RevealJournalV3 {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}
impl Eq for RevealJournalV3 {}

fn seat(player: PlayerId) -> &'static str {
    if player == PlayerId::P0 {
        "p0"
    } else {
        "p1"
    }
}

impl RevealJournalV3 {
    pub(crate) fn enabled(&self) -> bool {
        self.viewers.is_some()
    }

    pub(crate) fn enable(&mut self) {
        self.viewers.get_or_insert_with(Default::default);
    }

    pub(crate) fn set_concealed_exile(&mut self, object: Option<ObjectId>) {
        if self.viewers.is_some() {
            self.concealed_exile = object;
        }
    }

    pub(crate) fn departure(
        &mut self,
        object: ObjectId,
        owner: PlayerId,
        name: &str,
        destination: Zone,
        named: bool,
    ) {
        let named = named && !(destination == Zone::Exile && self.concealed_exile == Some(object));
        let Some(viewers) = &mut self.viewers else {
            return;
        };
        for (viewer, operations) in viewers.iter_mut().enumerate() {
            operations.push(json!({"sequence": operations.len(), "events": [{
                "kind": "hand_departure", "owner_seat": seat(owner), "count": 1,
                "card_name": (named || viewer == owner.index()).then_some(name),
            }]}));
        }
    }

    pub(crate) fn shuffle(&mut self, owner: PlayerId) {
        let Some(viewers) = &mut self.viewers else {
            return;
        };
        for operations in viewers {
            operations.push(json!({"sequence": operations.len(), "events": [{
                "kind": "library_shuffled", "owner_seat": seat(owner),
            }]}));
        }
    }

    pub(crate) fn reveal(
        &mut self,
        viewer: PlayerId,
        owner: PlayerId,
        name: &str,
        meaning: RevealV3,
    ) {
        self.reveal_group(viewer, owner, [name], meaning);
    }

    pub(crate) fn reveal_group<'a>(
        &mut self,
        viewer: PlayerId,
        owner: PlayerId,
        names: impl IntoIterator<Item = &'a str>,
        meaning: RevealV3,
    ) {
        let Some(viewers) = &mut self.viewers else {
            return;
        };
        let operations = &mut viewers[viewer.index()];
        let sequence = operations.len();
        let owner = seat(owner);
        let evidence = match meaning {
            RevealV3::Omit => return,
            RevealV3::Present => None,
            RevealV3::SearchReveal => Some(json!({"kind": "search_reveal"})),
            RevealV3::PublicTransfer(source) => {
                let from_zone = match source {
                    Zone::Battlefield => "battlefield",
                    Zone::Graveyard => "graveyard",
                    Zone::Stack => "stack",
                    Zone::Exile => "exile",
                    Zone::Command => "command",
                    Zone::Hand | Zone::Library => return,
                };
                Some(json!({"kind": "public_transfer", "from_zone": from_zone}))
            }
        };
        // One atomic reveal exposes a name multiset. Order its public events
        // by name so native hand, selection and allocation order stay private.
        let mut names: Vec<_> = names.into_iter().collect();
        names.sort_unstable();
        if names.is_empty() {
            return;
        }
        let mut events = Vec::new();
        for name in names {
            if let Some(evidence) = &evidence {
                let arrival = format!("arrival:{sequence}:{}", events.len());
                let arrival_name = matches!(meaning, RevealV3::PublicTransfer(_)).then_some(name);
                events.push(json!({"kind": "hand_arrival", "owner_seat": owner,
                    "card_name": arrival_name, "evidence": evidence}));
                events.push(json!({"kind": "card_revealed", "owner_seat": owner,
                    "card_name": name, "meaning": "moved", "arrival": arrival}));
            } else {
                events.push(json!({"kind": "card_revealed", "owner_seat": owner,
                    "card_name": name, "meaning": "present"}));
            }
        }
        operations.push(json!({"sequence": sequence, "events": events}));
    }

    pub(crate) fn take_delta(&self, cursor: &mut [usize; 2]) -> Option<Value> {
        let viewers = self.viewers.as_ref()?;
        let deltas = [viewers[0].get(cursor[0]..)?, viewers[1].get(cursor[1]..)?];
        let envelopes = [0, 1].map(|viewer| {
            json!({"viewer": viewer, "transcript": {
                "schema": "x_reveal_meaning_v3", "operations": deltas[viewer],
            }})
        });
        *cursor = viewers.each_ref().map(Vec::len);
        Some(json!({"schema": "kernel-reveal-support/v3", "viewers": envelopes}))
    }

    pub(crate) fn transcript(&self, viewer: PlayerId) -> Option<Value> {
        Some(json!({"schema": "x_reveal_meaning_v3",
            "operations": self.viewers.as_ref()?.get(viewer.index())?}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{propose_and_commit, ProposedEvent};
    use crate::policy_observation_v6::tests::put;
    use crate::state::{FaceDownV1, GameState};

    fn empty() -> GameState {
        let mut state = GameState::new_from_libraries(&[], &[], crate::rl::card_name, 543);
        state.reveal_meaning_v3.enable();
        state
    }

    fn operations(state: &GameState, viewer: PlayerId) -> Value {
        state.reveal_meaning_v3.take_delta(&mut [0; 2]).unwrap()["viewers"][viewer.index()]
            ["transcript"]["operations"]
            .clone()
    }

    #[test]
    fn reveal_meaning_v3_anonymous_draw_and_same_name_repeats_are_present() {
        let owner = PlayerId::P0;
        let viewer = owner.opponent();
        let run = |show_drawn: bool, hidden_first: bool| {
            let mut state = empty();
            if hidden_first {
                put(&mut state, owner, "Forest", Zone::Library);
            }
            let old = put(&mut state, owner, "Swamp", Zone::Hand);
            let drawn = put(&mut state, owner, "Swamp", Zone::Library);
            if hidden_first {
                state.players[owner.index()].library.swap(0, 1);
            } else {
                put(&mut state, owner, "Forest", Zone::Library);
            }
            propose_and_commit(&mut state, ProposedEvent::draw(owner));
            assert!(state.players[owner.index()].hand.contains(&drawn));
            let shown = if show_drawn { drawn } else { old };
            state
                .reveal_distinct_hand_card(viewer, owner, shown)
                .unwrap();
            state.reveal_hand_card(viewer, owner, shown).unwrap();
            operations(&state, viewer)
        };
        let actual = run(false, false);
        assert_eq!(actual, run(true, true));
        let event = json!({"kind": "card_revealed", "owner_seat": "p0", "card_name": "Swamp", "meaning": "present"});
        assert_eq!(
            actual,
            json!([
                {"sequence": 0, "events": [event.clone()]}, {"sequence": 1, "events": [event]},
            ])
        );
    }

    #[test]
    fn reveal_meaning_v3_public_move_and_hidden_departure_use_visible_names() {
        let owner = PlayerId::P1;
        let mut state = empty();
        let card = put(&mut state, owner, "Swamp", Zone::Graveyard);
        propose_and_commit(&mut state, ProposedEvent::zone_change(card, Zone::Hand));
        propose_and_commit(
            &mut state,
            ProposedEvent::private_bottom_library_insert(card),
        );
        let expected_arrival = json!({"sequence": 0, "events": [
            {"kind":"hand_arrival", "owner_seat":"p1", "card_name":"Swamp", "evidence":{"kind":"public_transfer", "from_zone":"graveyard"}},
            {"kind":"card_revealed", "owner_seat":"p1", "card_name":"Swamp", "meaning":"moved", "arrival":"arrival:0:0"},
        ]});
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let actual = operations(&state, viewer);
            assert_eq!(actual[0], expected_arrival);
            assert_eq!(
                actual[1],
                json!({"sequence": 1, "events": [{
                    "kind":"hand_departure", "owner_seat":"p1", "count":1,
                    "card_name":(owner == viewer).then_some("Swamp"),
                }]})
            );
        }
        let mut cursor = [0; 2];
        assert!(state.reveal_meaning_v3.take_delta(&mut cursor).is_some());
        assert_eq!(cursor, [2, 2]);
        let next = state.reveal_meaning_v3.take_delta(&mut cursor).unwrap();
        assert_eq!(next["viewers"][0]["transcript"]["operations"], json!([]));
    }

    #[test]
    fn reveal_meaning_v3_concealed_exile_return_and_state_identity() {
        let owner = PlayerId::P0;
        let mut state = empty();
        let card = put(&mut state, owner, "Swamp", Zone::Exile);
        state.objects.get_mut(card).v4.face_down_v1 = Some(FaceDownV1 {
            disguised: false,
            lookers: 1,
            hidden_by: None,
            hidden_by_source: None,
        });
        let mut frozen = state.clone();
        frozen.reveal_meaning_v3 = Default::default();
        for game in [&mut state, &mut frozen] {
            propose_and_commit(game, ProposedEvent::zone_change(card, Zone::Hand));
        }
        assert_eq!(operations(&state, owner.opponent()), json!([]));
        state.reveal_meaning_v3.shuffle(owner);
        assert_eq!(state, frozen);
        assert_eq!(state.state_hash(), frozen.state_hash());
        assert_eq!(
            serde_json::to_vec(&state).unwrap(),
            serde_json::to_vec(&frozen).unwrap()
        );
    }

    #[test]
    fn reveal_meaning_v3_aborted_face_up_cast_exports_real_stack_return() {
        use crate::engine::{self, Action, Decision};
        use crate::mana::ManaColor;
        use crate::state::{Step, Target};

        let mut state = empty();
        let owner = PlayerId::P0;
        let viewer = owner.opponent();
        state.step = Step::Main1;
        let spell = put(&mut state, owner, "Cast Down", Zone::Hand);
        let target = put(&mut state, viewer, "Avenging Hunter", Zone::Battlefield);
        state.players[owner.index()].mana_pool[ManaColor::B.pool_index()] = 2;
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { castable_spells, .. } if castable_spells.contains(&spell)));
        engine::step(&mut state, Action::CastSpell(spell)).unwrap();
        assert_eq!(state.objects.get(spell).zone, Zone::Stack);
        assert!(matches!(
            engine::advance_until_decision(&mut state),
            Decision::ChooseTargets { .. }
        ));
        // Force the actual abort path after a genuine face-up announcement.
        state.players[owner.index()].mana_pool.fill(0);
        engine::step(&mut state, Action::ChooseTarget(Target::Object(target))).unwrap();
        assert_eq!(state.objects.get(spell).zone, Zone::Hand);
        let wire = operations(&state, viewer);
        assert_eq!(wire[0]["events"][0]["kind"], "hand_departure");
        assert_eq!(
            wire[1]["events"][0]["evidence"],
            json!({"kind":"public_transfer", "from_zone":"stack"})
        );
        assert_eq!(wire[1]["events"][1]["meaning"], "moved");
        assert_eq!(wire[1]["events"][1]["arrival"], "arrival:1:0");
    }

    #[test]
    fn reveal_meaning_v3_explicit_search_outcome_binds_before_shuffle() {
        use crate::engine::{self, Action, Decision};
        use crate::mana::ManaColor;
        use crate::state::{Step, Target};

        let mut state = empty();
        let owner = PlayerId::P0;
        state.step = Step::Main1;
        let source = put(&mut state, owner, "Expedition Map", Zone::Battlefield);
        let land = put(&mut state, owner, "Forest", Zone::Library);
        state.players[owner.index()].mana_pool[ManaColor::C.pool_index()] = 2;
        assert!(matches!(engine::advance_until_decision(&mut state),
            Decision::CastSpellOrPass { activatable_abilities, .. } if activatable_abilities.contains(&(source, 0))));
        engine::step(&mut state, Action::ActivateAbility(source, 0)).unwrap();
        for _ in 0..16 {
            match engine::advance_until_decision(&mut state) {
                Decision::ChooseEffectTargets { legal_targets, .. }
                    if legal_targets.contains(&Target::Object(land)) =>
                {
                    engine::step(&mut state, Action::ChooseEffectTarget(Target::Object(land)))
                        .unwrap();
                }
                Decision::ChooseEffectTargets {
                    can_finish: true, ..
                } => {
                    engine::step(&mut state, Action::FinishEffectSelection).unwrap();
                }
                Decision::CastSpellOrPass { .. } if state.objects.get(land).zone == Zone::Hand => {
                    break
                }
                Decision::CastSpellOrPass { .. } => engine::step(&mut state, Action::Pass).unwrap(),
                other => panic!("unexpected search stage {other:?}"),
            }
        }
        assert_eq!(state.objects.get(land).zone, Zone::Hand);
        for viewer in [PlayerId::P0, PlayerId::P1] {
            let wire = operations(&state, viewer);
            assert_eq!(
                wire,
                json!([
                    {"sequence":0, "events":[
                        {"kind":"hand_arrival", "owner_seat":"p0", "card_name":null, "evidence":{"kind":"search_reveal"}},
                        {"kind":"card_revealed", "owner_seat":"p0", "card_name":"Forest", "meaning":"moved", "arrival":"arrival:0:0"},
                    ]},
                    {"sequence":1, "events":[{"kind":"library_shuffled", "owner_seat":"p0"}]},
                ])
            );
        }
    }

    #[test]
    fn reveal_meaning_v3_atomic_groups_ignore_private_selection_order() {
        let owner = PlayerId::P0;
        for meaning in [RevealV3::Present, RevealV3::SearchReveal] {
            let emit = |names: [&str; 2]| {
                let mut state = empty();
                let cards = names.map(|name| put(&mut state, owner, name, Zone::Hand));
                state.note_public_hand_reveal_group_v3(owner, cards, meaning);
                state.reveal_meaning_v3.shuffle(owner);
                operations(&state, owner.opponent())
            };
            let first = emit(["Swamp", "Forest"]);
            assert_eq!(first, emit(["Forest", "Swamp"]));
            let reveals: Vec<_> = first[0]["events"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|event| event["kind"] == "card_revealed")
                .collect();
            assert_eq!(reveals[0]["card_name"], "Forest");
            assert_eq!(reveals[1]["card_name"], "Swamp");
            if matches!(meaning, RevealV3::SearchReveal) {
                assert_eq!(reveals[0]["arrival"], "arrival:0:0");
                assert_eq!(reveals[1]["arrival"], "arrival:0:2");
            }
            assert_eq!(first[1]["events"][0]["kind"], "library_shuffled");
        }
    }
}
