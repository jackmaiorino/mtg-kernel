use super::*;

#[test]
fn paid_complete_walkers_keep_private_mode_through_shared_prefix_and_legacy_refusal_both_seats() {
    use ObservationTextModeV2 as Mode;
    for player in [PlayerId::P0, PlayerId::P1] {
        let (kaito, _, _) = crate::effect::kaito_cunning_v75::tests::owner_v75_tests::owning_ready(
            player, false, "Island",
        );
        validate_copy_prefix_for_mode_v1(&kaito, Mode::KaitoV75Artifact).unwrap();
        for mode in [
            Mode::FullArtifact,
            Mode::TinybonesV72Artifact,
            Mode::ChandraV74Artifact,
        ] {
            let before = kaito.clone();
            assert!(validate_copy_prefix_for_mode_v1(&kaito, mode).is_err());
            assert_eq!(kaito, before);
        }

        let (chandra, _, _, _) =
            crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::owning_ready(
                player, false, "Island",
            );
        validate_copy_prefix_for_mode_v1(&chandra, Mode::ChandraV74Artifact).unwrap();
        validate_copy_prefix_for_mode_v1(&chandra, Mode::KaitoV75Artifact).unwrap();
        for mode in [
            Mode::FullArtifact,
            Mode::TinybonesV72Artifact,
            Mode::LilianaV73Artifact,
        ] {
            let before = chandra.clone();
            assert!(validate_copy_prefix_for_mode_v1(&chandra, mode).is_err());
            assert_eq!(chandra, before);
        }

        let copied =
            crate::effect::chandra_flameshaper_v1::tests::owner_v74_tests::copied_token_boundary(
                player,
            );
        for mode in [Mode::ChandraV74Artifact, Mode::KaitoV75Artifact] {
            validate_copy_prefix_for_mode_v1(&copied, mode).unwrap();
        }
        assert!(validate_copy_prefix_for_mode_v1(&copied, Mode::FullArtifact).is_err());
        let token = copied
            .objects
            .iter()
            .find(|(_, object)| {
                object.zone == crate::state::Zone::Battlefield
                    && object.v4.copied_end_step_sacrifices_v1 == 1
            })
            .unwrap()
            .0;
        // An unauthenticated count is a negative fixture, never a copied ability.
        let mut forged = copied.clone();
        forged
            .objects
            .get_mut(token)
            .v4
            .copied_end_step_sacrifices_v1 = 2;
        for mode in [Mode::ChandraV74Artifact, Mode::KaitoV75Artifact] {
            let before = forged.clone();
            assert!(validate_copy_prefix_for_mode_v1(&forged, mode).is_err());
            assert_eq!(forged, before);
        }
    }
}
