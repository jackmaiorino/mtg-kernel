#!/usr/bin/env python3
"""Independent stdlib-only verifier for the S2 ladder schedule additions.

Reimplements, from the Self-Play Ladder Design Contract (S2) Section 2 text
and the Rust owner module `native_trainer_schedule_v2.rs` doc comments only
(not by importing or transcribing Rust source), the two new seed derivations:

  - train-opponent-pool-choice(base_seed, episode_index)
  - train-opponent-policy-substep(base_seed, episode_index,
        opponent_physical_decision_index, substep_index)

using the same sha256 atom construction the V1 native trainer schedule uses
(see python/mtg_kernel_rl/determinism.py's `_trainer_seed` / `_atom` for the
precedent this construction follows):

    atom(tag, payload) = u32be(len(tag_utf8)) || tag_utf8 ||
                          u64be(len(payload)) || payload
    seed = sha256(atom("version", SEED_VERSION) ||
                   atom("namespace", namespace) ||
                   atom("field-name", name) || atom("u63", u64be(value))
                   for each field in order)[:8] as u64be & 0x7fff_ffff_ffff_ffff

The seed-derivation version string is intentionally NOT the Python-Rust
parity string ("kernel-python-rl-trainer-sha256-v2"): there is no Python
production reference for these two ladder namespaces, so reusing that string
would misrepresent a cross-language parity claim that does not exist.

This script embeds the SAME pinned golden vectors as the Rust
`#[test]`s in native_trainer_schedule_v2.rs (native_trainer_schedule_v2_pool_
choice_goldens.in / native_trainer_schedule_v2_policy_substep_goldens.in) and
independently recomputes and checks every one. Exits non-zero on any
mismatch.
"""

from __future__ import annotations

import hashlib
import sys

SEED_VERSION = "kernel-native-ladder-trainer-sha256-v1"
POOL_CHOICE_NAMESPACE = "train-opponent-pool-choice"
POLICY_SUBSTEP_NAMESPACE = "train-opponent-policy-substep"
U63_MAX = (1 << 63) - 1
MASK_U63 = U63_MAX


def _atom(hasher: "hashlib._Hash", tag: str, payload: bytes) -> None:
    tag_bytes = tag.encode("utf-8")
    hasher.update(len(tag_bytes).to_bytes(4, "big"))
    hasher.update(tag_bytes)
    hasher.update(len(payload).to_bytes(8, "big"))
    hasher.update(payload)


def _checked_u63(name: str, value: int) -> int:
    if type(value) is not int or value < 0 or value > U63_MAX:
        raise ValueError(f"{name} out of u63 domain: {value}")
    return value


def _derive_seed(namespace: str, fields: list[tuple[str, int]]) -> int:
    hasher = hashlib.sha256()
    _atom(hasher, "version", SEED_VERSION.encode("utf-8"))
    _atom(hasher, "namespace", namespace.encode("utf-8"))
    for name, value in fields:
        _atom(hasher, "field-name", name.encode("utf-8"))
        value = _checked_u63(name, value)
        _atom(hasher, "u63", value.to_bytes(8, "big"))
    digest = hasher.digest()
    return int.from_bytes(digest[:8], "big") & MASK_U63


def derive_pool_choice_seed(base_seed: int, episode_index: int) -> int:
    return _derive_seed(
        POOL_CHOICE_NAMESPACE,
        [
            ("base_seed", base_seed),
            ("episode_index", episode_index),
        ],
    )


def derive_policy_substep_seed(
    base_seed: int,
    episode_index: int,
    opponent_physical_decision_index: int,
    substep_index: int,
) -> int:
    return _derive_seed(
        POLICY_SUBSTEP_NAMESPACE,
        [
            ("base_seed", base_seed),
            ("episode_index", episode_index),
            (
                "opponent_physical_decision_index",
                opponent_physical_decision_index,
            ),
            ("substep_index", substep_index),
        ],
    )


# Pinned golden vectors: mirrors
# mtg-kernel/src/native_trainer_schedule_v2_pool_choice_goldens.in exactly.
POOL_CHOICE_GOLDENS: list[tuple[int, int, int]] = [
    (0, 0, 4233314276625499709),
    (0, 1, 2215562506902174161),
    (0, 2, 6540482296316027195),
    (0, 63, 7055592793160920580),
    (0, 64, 590037278367733612),
    (0, 65, 4877767773180543406),
    (0, 4095, 6900990129539523722),
    (1, 0, 5132456814066307247),
    (1, 1, 7551821724860547868),
    (1, 2, 2044241403252759980),
    (1, 63, 5454074711123886409),
    (1, 64, 3081822956355020771),
    (1, 65, 3329815405788314681),
    (1, 4095, 7901945882556667515),
    (71501, 0, 6954660678222725018),
    (71501, 1, 6075393014239735260),
    (71501, 2, 6150835019196012738),
    (71501, 63, 3480970119526505535),
    (71501, 64, 282399062358369423),
    (71501, 65, 7978813255302552705),
    (71501, 4095, 7403336062162496997),
    (9223372036854775807, 0, 2585088019676301259),
    (9223372036854775807, 1, 705501238110673549),
    (9223372036854775807, 2, 4868884603155906183),
    (9223372036854775807, 63, 7001527856780566005),
    (9223372036854775807, 64, 2231087653157556802),
    (9223372036854775807, 65, 6307166405646997750),
    (9223372036854775807, 4095, 5800308896549791952),
]

# Pinned golden vectors: mirrors mtg-kernel/src/
# native_trainer_schedule_v2_policy_substep_goldens.in exactly.
POLICY_SUBSTEP_GOLDENS: list[tuple[int, int, int, int, int]] = [
    (71501, 0, 0, 0, 205688141244912042),
    (71501, 0, 0, 1, 5207049071793363381),
    (71501, 0, 1, 0, 6443616737763373017),
    (71501, 0, 1, 1, 8181503509488250494),
    (71501, 0, 7, 0, 2597053402239883903),
    (71501, 0, 7, 1, 8826452575706309170),
    (71501, 0, 1023, 0, 1957289507048869074),
    (71501, 0, 1023, 1, 2397167886895766994),
    (71501, 1, 0, 0, 3030150930484670530),
    (71501, 1, 0, 1, 6535780556918608931),
    (71501, 1, 1, 0, 4894629490439078492),
    (71501, 1, 1, 1, 7102302712651159613),
    (71501, 1, 7, 0, 7728210378001374878),
    (71501, 1, 7, 1, 2123038068643507685),
    (71501, 1, 1023, 0, 739790042907476013),
    (71501, 1, 1023, 1, 2128189190911984324),
    (71501, 2, 0, 0, 2078165776913127065),
    (71501, 2, 0, 1, 8164835781562338981),
    (71501, 2, 1, 0, 4996592101494835628),
    (71501, 2, 1, 1, 4911798662957462896),
    (71501, 2, 7, 0, 8080160093823333418),
    (71501, 2, 7, 1, 5260849559370587369),
    (71501, 2, 1023, 0, 161026565190512491),
    (71501, 2, 1023, 1, 309102027114862061),
    (71501, 63, 0, 0, 5904321936784105996),
    (71501, 63, 0, 1, 7182316261527292331),
    (71501, 63, 1, 0, 6904853789733743110),
    (71501, 63, 1, 1, 5505795830172495793),
    (71501, 63, 7, 0, 3544345082596498450),
    (71501, 63, 7, 1, 1560299204036700803),
    (71501, 63, 1023, 0, 7718649199188440386),
    (71501, 63, 1023, 1, 98813719504857499),
    (71501, 64, 0, 0, 5389302532882077737),
    (71501, 64, 0, 1, 6961002208843634022),
    (71501, 64, 1, 0, 7629297860835709114),
    (71501, 64, 1, 1, 2389024932886648250),
    (71501, 64, 7, 0, 4582342068811250166),
    (71501, 64, 7, 1, 6394902445754742443),
    (71501, 64, 1023, 0, 1113401715149629266),
    (71501, 64, 1023, 1, 9164753099317239360),
    (71501, 65, 0, 0, 365775326435860327),
    (71501, 65, 0, 1, 7582513620224070361),
    (71501, 65, 1, 0, 7923904358440548667),
    (71501, 65, 1, 1, 8096882391090245837),
    (71501, 65, 7, 0, 7798578729360587300),
    (71501, 65, 7, 1, 5871932682279976724),
    (71501, 65, 1023, 0, 8591285273523899),
    (71501, 65, 1023, 1, 4359717079024885279),
    (71501, 4095, 0, 0, 4065402028786337341),
    (71501, 4095, 0, 1, 6166616845990979152),
    (71501, 4095, 1, 0, 377916015788951287),
    (71501, 4095, 1, 1, 1581314995397586703),
    (71501, 4095, 7, 0, 1988446520122370964),
    (71501, 4095, 7, 1, 8040371742791678730),
    (71501, 4095, 1023, 0, 7625151349648345600),
    (71501, 4095, 1023, 1, 7356779172232142845),
    (0, 0, 0, 0, 2855703026437126001),
    (9223372036854775807, 4095, 1023, 1, 2681946496151887235),
]

REQUIRED_EPISODE_INDICES = {0, 1, 2, 63, 64, 65, 4095}
REQUIRED_DECISION_INDICES = {0, 1, 7, 1023}
REQUIRED_SUBSTEP_INDICES = {0, 1}


def main() -> int:
    failures: list[str] = []

    if len(POOL_CHOICE_GOLDENS) < 24:
        failures.append(
            f"pool-choice golden count {len(POOL_CHOICE_GOLDENS)} < 24"
        )
    if len(POLICY_SUBSTEP_GOLDENS) < 24:
        failures.append(
            f"policy-substep golden count {len(POLICY_SUBSTEP_GOLDENS)} < 24"
        )

    seen_episodes = {episode for (_, episode, _) in POOL_CHOICE_GOLDENS}
    if not REQUIRED_EPISODE_INDICES.issubset(seen_episodes):
        failures.append(
            "pool-choice goldens missing required episode indices: "
            f"{REQUIRED_EPISODE_INDICES - seen_episodes}"
        )

    for base_seed, episode_index, expected in POOL_CHOICE_GOLDENS:
        actual = derive_pool_choice_seed(base_seed, episode_index)
        if actual != expected:
            failures.append(
                "pool-choice mismatch base_seed="
                f"{base_seed} episode_index={episode_index} "
                f"expected={expected} actual={actual}"
            )

    seen_episodes = {episode for (_, episode, _, _, _) in POLICY_SUBSTEP_GOLDENS}
    seen_decisions = {
        decision for (_, _, decision, _, _) in POLICY_SUBSTEP_GOLDENS
    }
    seen_substeps = {
        substep for (_, _, _, substep, _) in POLICY_SUBSTEP_GOLDENS
    }
    if not REQUIRED_EPISODE_INDICES.issubset(seen_episodes):
        failures.append(
            "policy-substep goldens missing required episode indices: "
            f"{REQUIRED_EPISODE_INDICES - seen_episodes}"
        )
    if not REQUIRED_DECISION_INDICES.issubset(seen_decisions):
        failures.append(
            "policy-substep goldens missing required decision indices: "
            f"{REQUIRED_DECISION_INDICES - seen_decisions}"
        )
    if not REQUIRED_SUBSTEP_INDICES.issubset(seen_substeps):
        failures.append(
            "policy-substep goldens missing required substep indices: "
            f"{REQUIRED_SUBSTEP_INDICES - seen_substeps}"
        )

    for base_seed, episode_index, decision_index, substep_index, expected in (
        POLICY_SUBSTEP_GOLDENS
    ):
        actual = derive_policy_substep_seed(
            base_seed, episode_index, decision_index, substep_index
        )
        if actual != expected:
            failures.append(
                "policy-substep mismatch base_seed="
                f"{base_seed} episode_index={episode_index} "
                f"decision_index={decision_index} substep_index={substep_index} "
                f"expected={expected} actual={actual}"
            )

    # The two namespaces must not collide for identical shared fields.
    for base_seed, episode_index in [(71501, 0), (0, 0), (U63_MAX, U63_MAX)]:
        pool_choice = derive_pool_choice_seed(base_seed, episode_index)
        substep = derive_policy_substep_seed(base_seed, episode_index, 0, 0)
        if pool_choice == substep:
            failures.append(
                "namespace collision at base_seed="
                f"{base_seed} episode_index={episode_index}"
            )

    if failures:
        for failure in failures:
            print(f"FAIL: {failure}", file=sys.stderr)
        print(
            f"{len(failures)} failure(s) across "
            f"{len(POOL_CHOICE_GOLDENS)} pool-choice and "
            f"{len(POLICY_SUBSTEP_GOLDENS)} policy-substep vectors",
            file=sys.stderr,
        )
        return 1

    print(
        "OK: "
        f"{len(POOL_CHOICE_GOLDENS)} pool-choice vectors and "
        f"{len(POLICY_SUBSTEP_GOLDENS)} policy-substep vectors "
        "independently reproduced"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
