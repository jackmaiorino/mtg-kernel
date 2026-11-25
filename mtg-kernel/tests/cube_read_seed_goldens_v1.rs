//! Seed goldens for the Python port in tools/cube_v1/seeds_v1.py (D7, read plan v2, change 11).
//! The constants were computed by the Python port; this test asserts the Rust gives the same.
//! `GameRetry` is the variant m4-interfaces/v2 adds (D5); until D5 lands, its rows are skipped.
#![cfg(feature = "eternal-fixtures")]

use mtg_kernel::cube_draft_v1::{derive_seed, game_seed, pod_game_seats, SeedStream};
use mtg_kernel::state::SplitMix64;

#[test]
fn splitmix64_matches_the_published_vector() {
    let mut rng = SplitMix64::seed(1_234_567);
    let expected: [u64; 5] = [
        6457827717110365317,
        3203168211198807973,
        9817491932198370423,
        4593380528125082431,
        16408922859458223821,
    ];
    for value in expected {
        assert_eq!(rng.next_u64(), value);
    }
}

#[test]
fn derive_seed_matches_the_python_port() {
    let cases: [(u64, SeedStream, u64, u64); 25] = [
        (0, SeedStream::Game, 0, 17342717373898743084),
        (0, SeedStream::Game, 27, 1436214522166747746),
        (0, SeedStream::GameCoin, 0, 6412058593177545804),
        (0, SeedStream::GameEnv, 0, 6450973617022139783),
        (0, SeedStream::GameActions, 0, 6327371770725502258),
        (1, SeedStream::Game, 0, 4107345111158512935),
        (1, SeedStream::Game, 27, 9862851748266521902),
        (1, SeedStream::GameCoin, 0, 5713607585791298340),
        (1, SeedStream::GameEnv, 0, 11806238557170168938),
        (1, SeedStream::GameActions, 0, 3130641507755530477),
        (81985529216486895, SeedStream::Game, 0, 149832123990471848),
        (81985529216486895, SeedStream::Game, 27, 6304134740434610908),
        (
            81985529216486895,
            SeedStream::GameCoin,
            0,
            14797589963287401808,
        ),
        (
            81985529216486895,
            SeedStream::GameEnv,
            0,
            4396725098881826446,
        ),
        (
            81985529216486895,
            SeedStream::GameActions,
            0,
            1395649033028093445,
        ),
        (
            9223372036854775807,
            SeedStream::Game,
            0,
            7132182498610909436,
        ),
        (
            9223372036854775807,
            SeedStream::Game,
            27,
            11139677046640204592,
        ),
        (
            9223372036854775807,
            SeedStream::GameCoin,
            0,
            8521563237094384751,
        ),
        (
            9223372036854775807,
            SeedStream::GameEnv,
            0,
            10539902046128495598,
        ),
        (
            9223372036854775807,
            SeedStream::GameActions,
            0,
            6784129164960503782,
        ),
        (
            18446744073709551615,
            SeedStream::Game,
            0,
            13278693861982554827,
        ),
        (
            18446744073709551615,
            SeedStream::Game,
            27,
            8801077725902905928,
        ),
        (
            18446744073709551615,
            SeedStream::GameCoin,
            0,
            16763472772232297675,
        ),
        (
            18446744073709551615,
            SeedStream::GameEnv,
            0,
            3346255405197952324,
        ),
        (
            18446744073709551615,
            SeedStream::GameActions,
            0,
            15045546485419571670,
        ),
    ];
    for (parent, stream, index, expected) in cases {
        assert_eq!(derive_seed(parent, stream, index), expected);
    }
    for (pod, game, expected) in [
        (0, 27, 1436214522166747746),
        (1, 27, 9862851748266521902),
        (81985529216486895, 27, 6304134740434610908),
        (9223372036854775807, 27, 11139677046640204592),
        (18446744073709551615, 27, 8801077725902905928),
    ] {
        assert_eq!(game_seed(pod, game), expected);
    }
}

#[test]
fn pod_game_seats_match_the_python_port() {
    let cases: [(u64, usize, [usize; 2]); 9] = [
        (0, 0, [1, 0]),
        (0, 5, [0, 6]),
        (0, 27, [7, 6]),
        (1, 0, [1, 0]),
        (1, 5, [0, 6]),
        (1, 27, [7, 6]),
        (81985529216486895, 0, [0, 1]),
        (81985529216486895, 5, [0, 6]),
        (81985529216486895, 27, [6, 7]),
    ];
    for (pod, game, expected) in cases {
        assert_eq!(pod_game_seats(pod, game).expect("game in range"), expected);
    }
}
