"""Python port of the cube draft seed derivations in `mtg-kernel/src/cube_draft_v1.rs`.

`SplitMix64` (mtg-kernel/src/state.rs), `derive_seed`, `game_seed`, `pod_pairings` and the seat coin
of `pod_game_seats_unchecked`, bit for bit. The readers use them to recompute every planned game
seed, seat and attempt seed. Goldens: tests/test_readers.py pins values computed here, and
mtg-kernel/tests/cube_read_seed_goldens_v1.rs asserts the same constants against the Rust.
"""

from __future__ import annotations

U64 = (1 << 64) - 1
SEED_DOMAIN_V1 = 0x4355_4245_4452_4654

# SeedStream discriminants (cube_draft_v1.rs; GameRetry is m4-interfaces/v2 section 0).
PACKS = 1
PICKER = 2
DECK_BUILD = 3
POD = 4
GAME = 5
GAME_COIN = 6
GAME_ENV = 7
GAME_ACTIONS = 8
GAME_RETRY = 9

SEATS = 8


class SplitMix64:
    def __init__(self, seed: int) -> None:
        self.state = seed & U64

    def next_u64(self) -> int:
        self.state = (self.state + 0x9E3779B97F4A7C15) & U64
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & U64
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & U64
        return z ^ (z >> 31)


def derive_seed(parent: int, stream: int, index: int) -> int:
    first = SplitMix64(SEED_DOMAIN_V1 ^ parent).next_u64()
    second = SplitMix64(first ^ ((stream * 0x9E37_79B9_7F4A_7C15) & U64)).next_u64()
    return SplitMix64(second ^ ((index * 0xD1B5_4A32_D192_ED03) & U64)).next_u64()


def game_seed(pod_seed: int, game: int) -> int:
    return derive_seed(pod_seed, GAME, game)


def pod_pairings() -> list[tuple[int, int]]:
    """The 28 chair pairs (low, high), low < high, in lexicographic order."""
    return [(a, b) for a in range(SEATS) for b in range(a + 1, SEATS)]


def coin(game_seed_value: int) -> int:
    return SplitMix64(derive_seed(game_seed_value, GAME_COIN, 0)).next_u64() & 1


def pairing_seats(pod_seed: int, g: int) -> tuple[int, list[int]]:
    """Pairing g's game seed G and its session seats [p0, p1]: the low chair first if the coin is 0."""
    low, high = pod_pairings()[g]
    g_seed = game_seed(pod_seed, g)
    return g_seed, ([low, high] if coin(g_seed) == 0 else [high, low])


def attempt_seeds(g_seed: int) -> dict[str, int]:
    """Attempt 0 of a scheduled game: deal seed G, env and action seeds derived from it."""
    return {
        "deal_seed": g_seed,
        "env_seed": derive_seed(g_seed, GAME_ENV, 0),
        "action_seed": derive_seed(g_seed, GAME_ACTIONS, 0),
    }
