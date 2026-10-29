//! Run every existing shared paid family through the genuine fifth owner.
//! The adapter uses the sealed V59 backend's original ordered actions and an
//! independent native session clone. Response, full GameState and both counters
//! must match directly after each accepted action and each saved restore.
use super::*;
use crate::current_mixed_policy_v1::tests::ordinary_triggers::{self as shared, Owner};

impl Owner for NextFiveSession {
    type Decision = NextFiveDecision;
    type Snapshot = AuditSnapshot;

    fn own(state: GameState) -> Self {
        NextFiveSession::from_session(FastActorSessionV1::from_next_five_v59_fixture_state(state))
            .unwrap()
    }
    fn world(&self) -> &GameState {
        self.fixture_state()
    }
    fn counts(&self) -> (u64, u64) {
        self.fixture_counts()
    }
    fn actions(&self) -> Vec<ActionSemanticV1> {
        self.inner.decision_v59().unwrap().actions().to_vec()
    }
    fn encode(&self) -> Self::Decision {
        let decision = self.decision_v59().unwrap();
        assert_eq!(decision.packet().source(), Source::NextFiveV59);
        decision
    }
    fn packet(decision: &Self::Decision) -> &Packet {
        decision.packet()
    }
    fn erase(decision: &mut Self::Decision) {
        decision.fixture_packet_mut().state =
            crate::current_mixed_policy_v1::NumericTree::encode(&serde_json::json!({})).unwrap();
    }
    fn apply(&mut self, decision: &Self::Decision, index: u32) -> Result<()> {
        let mut reference = self.inner.fixture_native_v59();
        let expected = decision.backend.expected();
        let actual = NextFiveSession::apply(self, decision, index)?;
        assert_eq!(
            actual,
            reference
                .step(expected.episode_id, expected.step, index)
                .unwrap()
        );
        assert_eq!(self.current_response(), reference.current_response());
        assert_eq!(self.fixture_state(), reference.next_five_fixture_state());
        assert_eq!(
            self.fixture_counts(),
            (
                reference.policy_step_count(),
                reference.physical_decision_count()
            )
        );
        Ok(())
    }
    fn snapshot(&self) -> Self::Snapshot {
        self.snapshot_v59()
    }
    fn restore(&mut self, saved: &Self::Snapshot) -> Result<()> {
        // This restores both saved owners and compares their actual responses,
        // full worlds and native counters, without rebasing any differences.
        self.restore_v59(saved)
    }
}

#[test]
fn ashroot_stromkirk_paid_cast_real_target_choice_and_restore_next_five_both_seats() {
    shared::paid::<NextFiveSession>();
}
#[test]
fn strongbox_gourmand_paid_cast_attack_resolution_and_restore_next_five_both_seats() {
    shared::paid_entry_attack::<NextFiveSession>();
}
#[test]
fn memorial_panda_paid_cast_grants_and_gain_death_input_restore_next_five_both_seats() {
    shared::paid_memorial_panda::<NextFiveSession>();
}
#[test]
fn ashroot_stromkirk_private_opponent_printings_do_not_change_packet_next_five_both_seats() {
    shared::privacy::<NextFiveSession>();
}
#[test]
fn early_printings_paid_ordinary_activations_and_restore_next_five_both_seats() {
    shared::paid_early_activation::<NextFiveSession>();
}
#[test]
fn crystal_paid_cast_hexproof_prevention_control_restore_privacy_next_five_both_seats() {
    shared::paid_crystal::<NextFiveSession>();
}
#[test]
fn tyrant_herald_paid_cast_damage_flash_restore_privacy_next_five_both_seats() {
    shared::paid_tyrant_herald::<NextFiveSession>();
}
#[test]
fn rallier_bloom_paid_cast_private_pick_and_mana_restore_next_five_both_seats() {
    shared::paid_rallier_bloom::<NextFiveSession>();
}
#[test]
fn enchantment_hunter_paid_cast_trigger_restore_privacy_next_five_both_seats() {
    shared::paid_enchantment_hunter::<NextFiveSession>();
}
#[test]
fn eaten_paid_additional_branches_exile_restore_privacy_next_five_both_seats() {
    shared::paid_eaten::<NextFiveSession>();
}
#[test]
fn faebloom_paid_instant_reflexive_season_restore_privacy_next_five_both_seats() {
    shared::paid_faebloom::<NextFiveSession>();
}
#[test]
fn kellan_paid_upgrades_combat_impulse_restore_privacy_next_five_both_seats() {
    shared::paid_kellan::<NextFiveSession>();
}
#[test]
fn fiery_paid_optional_equipment_death_replacement_restore_privacy_next_five_both_seats() {
    shared::paid_fiery::<NextFiveSession>();
}
#[test]
fn divine_paid_ordinary_kicker_zero_multiple_restore_privacy_next_five_both_seats() {
    shared::paid_divine::<NextFiveSession>();
}
