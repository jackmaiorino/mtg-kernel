use mtgo_blackbox_v1::{
    MtgoOfflineLondonPregameEpisodeV1, MtgoOfflinePregameEpisodeActionV1,
    MtgoOfflinePregameEpisodeStageV1,
};
use std::collections::HashSet;

const EPISODE: &str = include_str!("../fixtures/offline_london_pregame_episode_20260810_v1.json");

#[test]
fn live_episode_fixture_has_canonical_fifteen_stage_path() {
    let episode: MtgoOfflineLondonPregameEpisodeV1 = serde_json::from_str(EPISODE).unwrap();
    assert_eq!(episode.schema_version, 1);
    assert_eq!(episode.frames.len(), 15);
    assert_eq!(episode.declared_transition_actions.len(), 14);
    assert_eq!(episode.choice_candidate_commitments_sha256.len(), 7);

    for index in 0..7 {
        assert_eq!(
            episode.frames[index].stage,
            MtgoOfflinePregameEpisodeStageV1::MulliganChoice {
                prospective_keep_size: u8::try_from(7 - index).unwrap()
            }
        );
    }
    for index in 0..7 {
        assert_eq!(
            episode.frames[7 + index].stage,
            MtgoOfflinePregameEpisodeStageV1::Bottoming {
                selected_count: u8::try_from(index).unwrap()
            }
        );
    }
    assert_eq!(
        episode.frames[14].stage,
        MtgoOfflinePregameEpisodeStageV1::GameplayFirstMain
    );
}

#[test]
fn live_episode_fixture_declares_fourteen_distinct_source_legal_edges() {
    let episode: MtgoOfflineLondonPregameEpisodeV1 = serde_json::from_str(EPISODE).unwrap();
    for index in 0..6 {
        assert_eq!(
            episode.declared_transition_actions[index],
            MtgoOfflinePregameEpisodeActionV1::Mulligan {
                next_hand_size: u8::try_from(6 - index).unwrap()
            }
        );
    }
    assert_eq!(
        episode.declared_transition_actions[6],
        MtgoOfflinePregameEpisodeActionV1::KeepOpeningHand
    );
    let selected_ids: HashSet<_> = episode.declared_transition_actions[7..13]
        .iter()
        .enumerate()
        .map(|(index, action)| match action {
            MtgoOfflinePregameEpisodeActionV1::SelectForBottom {
                adapter_object_id,
                selection_ordinal,
            } => {
                assert_eq!(usize::from(*selection_ordinal), index + 1);
                adapter_object_id.as_str()
            }
            _ => panic!("expected SelectForBottom"),
        })
        .collect();
    assert_eq!(selected_ids.len(), 6);
    assert_eq!(
        episode.declared_transition_actions[13],
        MtgoOfflinePregameEpisodeActionV1::SubmitBottoming
    );

    let frame_manifests: HashSet<_> = episode
        .frames
        .iter()
        .map(|frame| frame.source_manifest_sha256.as_str())
        .collect();
    let raw_frames: HashSet<_> = episode
        .frames
        .iter()
        .map(|frame| frame.source_frame_sha256.as_str())
        .collect();
    assert_eq!(frame_manifests.len(), 15);
    assert_eq!(raw_frames.len(), 15);
}
