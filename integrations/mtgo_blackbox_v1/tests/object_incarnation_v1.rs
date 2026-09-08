use mtg_kernel::rl::PlayerSeatV1;
use mtg_kernel::state::Zone;
use mtgo_blackbox_v1::{start_checked_untrusted_visible_object_ledger_v1, MtgoVisibleObjectSeedV1};

#[test]
fn public_ledger_surface_exposes_commitments_but_no_authority() {
    let ledger = start_checked_untrusted_visible_object_ledger_v1(
        &"1".repeat(64),
        vec![MtgoVisibleObjectSeedV1 {
            display_ordinal: 0,
            visible_object_id: "current-frame:hand-slot-0".to_owned(),
            visible_card_name: "Island".to_owned(),
            owner: PlayerSeatV1::P0,
            controller: PlayerSeatV1::P0,
            zone: Zone::Hand,
        }],
    )
    .unwrap();
    assert_eq!(ledger.object_count(), 1);
    assert_eq!(ledger.transition_count(), 0);
    assert_eq!(ledger.ledger_commitment_sha256().len(), 64);
    assert!(!ledger.safe_for_observation_v5());
    assert!(!ledger.safe_for_policy_scoring());
    assert!(!ledger.safe_for_input());
}
