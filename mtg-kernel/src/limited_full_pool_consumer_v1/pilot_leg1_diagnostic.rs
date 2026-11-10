// Native numeric ordinal fixture derived from retained c0c0 leg 1 receipt.
// SHA-256 42fd980dbfcc70a34cbb2f1ff4811757d53c570a7bf7bc29636aace58dcd672f.
use super::*;

// Five hex digits per action: seat * 3 + kind, sealed width, sealed ordinal.
const ORDINALS: &str = concat!(
    "302010020030704302003010000100301000010030402302010010030201",
    "0010030200301000010030e0830100001003010000100301000010030100",
    "001000010030100001003010000602002013010000200001003010000100",
    "301000010030100001003010000100301003020030100001003010000100",
    "303020010030100001003010000100301000010030301302003010000100",
    "30d053010000100301000010000200001003010000100301000050300200",
    "001003010000100301000010030100001003010000100301000010030100",
    "304013030130201001003010000100302003020030100001003010000100",
    "301000010030100001003010000100301000010000302301000030230100",
    "006030070500600001003010000100301000010030100001003010000100",
    "301000010030100001003010030502304003020100100302010010030200",
    "301000010030201001003010000100301000010030100001003020030201",
    "001003020100100004033020030100004030040200301002000010030100",
    "004020020130100002013010010200002013010000201301000020000100",
    "301000010030100306013050330400302003010000100301000010030100",
    "001003010000100301000010030100001003010000100301000010030100",
    "001000050200403301000040100301002013010000401005030040200302",
    "301000010030100102010010030100001003010000100301000010030100",
    "001003010000100301000010030100001003010030605001003060430501",
    "304003010030100001003010000100302010010030100001003010000100",
    "301000010030201001003010000100006040050200401003010020000100",
    "301000010030100002013010000100301000010030100001003010000201",
    "3010000100301003060500100306050010030b0930302001003050400100",
    "303020010030300302010010030201001003030030200301000010030100",
    "001003010000100006040050300400003023010000302301000060000100",
    "301000020000201301000020000100301000010030100001003010000100",
    "301000010030100308063060500100306003010030200301000010030100",
    "001003010000100301000010030100001003010000100301000010030100",
    "001000070000600005030040000300002013010000200001003010000100",
    "301000010030100001003010000100301000010030100001003010030704",
    "305013040300100304030010030600302010010030302001003020100100",
    "302010010030201001003030200100302003010000100008020070630100",
    "008010010000602005020040000100003000020130100301000020000100",
    "301000010030100001003010000100301000010030100001003010000100",
    "301000010030100001003010030702306023050400100305033030030200",
    "301000010030301302003010000100301000010030100001003010000100",
    "301000010030100001000070630100007010060000500004010030100201",
    "301000020000100301000010030100001003010000100301000010030100",
    "001003010030807001003080430601305023040030301302003010000100",
    "302010010030100001003010000100301000010030200302003010000100",
    "301000010000700006000050200401003000020130100002013010000201",
    "301000020000100301000010030100001003010000100301000010030100",
    "008023090130805307053060230501304003030200100303020010030300",
    "301000010030100001003010000100301000010030100001003010000100",
    "308010070630100007063010000e0c00d02005043010000a0100100",
);

use crate::limited_match_v1::{DeckFileV1, SessionKindV1};
use crate::rl_session::FastActorDecisionKindV1;
use sha2::{Digest, Sha256};

#[test]
fn retained_numeric_pilot_leg_one_passes_the_original_eaten_alive_boundary() {
    let decks = [
        (
            "FDN_top_15157_BR",
            include_bytes!("../../../data/limited/draftzero_sample_v1/decks/FDN_top_15157_BR.json")
                .as_slice(),
            "d7d360a3b9e62531a9dde34a83fbeb0d11034cc0d2f805543e4e0dd0ccd7b661",
        ),
        (
            "FDN_top_09703_UB",
            include_bytes!("../../../data/limited/draftzero_sample_v1/decks/FDN_top_09703_UB.json")
                .as_slice(),
            "cef1a87b033b8031c436f1e9fd7f6f36f7304b4dccc8eda9bd96ad06dd8fe21b",
        ),
    ];
    let mainboards = decks.map(|(name, bytes, digest)| {
        assert_eq!(format!("{:x}", Sha256::digest(bytes)), digest);
        let deck: DeckFileV1 = serde_json::from_slice(bytes).unwrap();
        assert_eq!(deck.name.as_deref(), Some(name));
        let cards = deck.resolve_for(SessionKindV1::FullPoolV1).unwrap();
        assert_eq!(cards.len(), 40);
        cards
    });
    let mut owner = reset_numeric_reveal_facts_v3(&ResetV1 {
        expected_card_db_hash: KERNEL_CARDDB_HASH,
        episode_id: 552001,
        env_seed: 552001,
        starting_player: PlayerSeatV1::P1,
        deck_ids: decks.map(|(name, _, _)| name.into()),
        mainboards,
        max_physical_decisions: 4000,
        max_policy_steps: 4000,
    })
    .unwrap();
    let eaten = crate::ids::ObjectId(6);
    assert_eq!(
        CARD_DEFS[usize::from(owner.fixture_state().objects.get(eaten).card_def)].name,
        "Eaten Alive"
    );

    let parse = |bytes: &[u8]| u32::from_str_radix(std::str::from_utf8(bytes).unwrap(), 16).unwrap();
    let kinds = [
        FastActorDecisionKindV1::Surface,
        FastActorDecisionKindV1::AttackerInclusion,
        FastActorDecisionKindV1::BlockerInclusion,
    ];
    let actors = [PlayerSeatV1::P0, PlayerSeatV1::P1];
    assert_eq!(ORDINALS.len(), 539 * 5);
    for (step, entry) in ORDINALS.as_bytes().chunks_exact(5).enumerate() {
        let header = parse(&entry[..1]) as usize;
        let width = parse(&entry[1..3]);
        let selected = parse(&entry[3..]);
        let FastActorResponseV1::Decision(expected) = owner.current_response() else {
            panic!("pilot ended before step {step}: {:?}", owner.current_response());
        };
        assert_eq!(expected.step, step as u64);
        assert_eq!(expected.acting_player, actors[header / 3], "actor at {step}");
        assert_eq!(expected.decision_kind, kinds[header % 3], "kind at {step}");
        let bound = owner.encode(expected).unwrap_or_else(|error| {
            panic!("pilot projection at {step}: {error:?}; {:?}", owner.fixture_projection_error())
        });
        assert_eq!(bound.packet().actions().len(), width as usize, "sealed width at {step}");
        if step >= 532 {
            eprintln!(
                "pilot step {step}: {:?}",
                bound.fixture_backend_v75().actions()[selected as usize],
            );
        }
        owner.apply(&bound, selected).unwrap_or_else(|error| panic!("pilot action at {step}: {error:?}"));
    }
    let native = owner.fixture_state();
    assert!(
        native.engine.halted.is_none(),
        "pilot halted after the stored prefix: {:?}; source zone {:?}; pending Eaten {:?}",
        owner.current_response(),
        native.objects.get(eaten).zone,
        native.engine.pending_cast.as_ref().filter(|cast| cast.spell == eaten).map(|cast| {
            (cast.targets_chosen.as_slice(), cast.mandatory_additional_cost)
        }),
    );
}
