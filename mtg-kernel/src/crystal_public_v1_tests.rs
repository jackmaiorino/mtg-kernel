use super::*;
use crate::policy_observation_v6::tests::{put, ready_state};

#[test]
fn ordinary_predecessor_parts_hidden_identity_and_artifact_identity_both_seats() {
    for actor in [PlayerId::P0, PlayerId::P1] {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        put(&mut state, actor, "Forest", Zone::Hand);
        let hidden = put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        let surface = PolicySurfaceV5::new();
        let old = crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        let public = observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap();
        public.validate_artifact().unwrap();
        assert_eq!(public.projection, old.projection);
        assert_eq!(public.extensions, old.extensions);
        assert!(public.static_sources.is_empty());
        assert!(public.prevention_menu.is_none());
        let definition = crate::card_def::card_id_by_name("Island").unwrap();
        let object = state.objects.get_mut(hidden);
        object.card_def = definition;
        object.name = "Island".into();
        object.v4 = crate::state::ObjectStateV4::from_card_def(definition);
        assert_eq!(
            observe_v1(&state, &surface, actor, 0, 0, 0, 1).unwrap(),
            public
        );
        let mut bad = public.clone();
        bad.schema.push('x');
        assert!(bad.validate_artifact().is_err());
        let mut bad = public;
        bad.producer_sha256.clear();
        assert!(bad.validate_artifact().is_err());
    }
}
