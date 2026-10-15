# Drake Hatcher Partial source preparation

Issue #110. Base `dce5617069369803e761e203e2a5a0c372bb1b4d`.
This is unqualified source only. All added Rust and fixture bodies are
uncompiled and unrun. No Full printing, model, sampling, strength or consumer
admission is claimed. No Rust build, test, check or clippy was launched.

The frozen printing is FDN 35, Scryfall
`bcaf4196-6bf3-47fa-b5c7-0e77f45cf820`, Oracle
`4de0ccab-bb5d-4c7c-827e-36aa1d00b182`. The frozen required Drake token is
`f4a73034-e20f-4e7e-ac15-3460b1e9c69b`. Its genuine pinned definition is
2/2 blue Drake with flying. It was absent from the executable catalog, so
Hatcher appends at local 444 and Drake Token at 445. This yields 446 total
rows, 254 FDN rows and exactly three Partial printings: Fake Your Own Death,
Ravenous Amulet and Drake Hatcher. IDs conflict with sibling source branches;
the integrating owner must serially renumber before qualification.

XMage pin: `a5c90fe180021e70e2a644ade00eeab07f857a40`.
Verified working files match the pinned blobs after CRLF normalization.

| Pinned source | Git-blob SHA-256 |
| --- | --- |
| `Mage.Sets/src/mage/cards/d/DrakeHatcher.java` | `ec315e1a92b30ed008863fd2372f7d7dd958f6b3ed93ce4aecb9e625bf1dc75d` |
| `Mage/src/main/java/mage/game/permanent/token/DrakeToken.java` | `e3e537fea71a435f8638b17dbe7a1848bfddc70779bf973bc3dafad9c89329e0` |

The source registers actual 1U 1/3 Human Wizard, vigilance, existing prowess,
and a combat trigger binding the positive final `CombatDamageToPlayer.amount`
after prevention/replacement. Resolution uses the captured source incarnation
and checked incubation placement. Typed source-counter payment removes three
before stacking; it is distinct from net, charge and distributed +1/+1 costs.
The independent token effect does not refund costs when countered. Counter
storage, transfer and presence tests carry incubation; zero preserves earlier
JSON/hash forms. Drake subtype appends, preserving all previous stable IDs.
Frozen public card/human counter projections refuse unrepresentable incubation;
card projections also refuse known Hatcher semantics. Frozen V1 inventory
explicitly refuses the typed cost and its test-only supported subset excludes it.

Fixture bodies cover both seats: paid cast and prowess; production combat
commit with saved damage unlike current power; genuine attack/block assignment;
full and partial prevention; suppression; first/double-strike waves; paid
activation, serde restore, opponent refusal, countered activation, departure,
new incarnation and checked overflow/underflow. Synthetic prevention and
suppression setup exercises existing engine primitives, not new printed cards.

Checks observed: `cargo +1.94.1 fmt --all`, `git diff --check`, small Python
catalog/prefix/pin/hash assertions. The actual
`python -m unittest python.tests.test_limited_decks_v1 python.tests.test_fdn_booster_pool_v1`
suites passed all 67 tests. A complete balanced enum-body token check preserves
all 230 EffectOp, 28 CostComponent and 152 subtype predecessor variants; each
new variant is appended at its actual enum tail. Positive combat binder
materializations now match the definition template, with source/incarnation
checks retained and historical amount custody still deferred. The multikind
transfer literal includes soul and incubation, with doubled expectations. The
committed 40-card by-name reference deck remains refused by Full-only tooling. Frozen 286 printings and 24 token dependencies,
profile constants, lockfiles and toolchain pins remain unchanged. No Rust check
has passed. Successor work still needs compiled engine fixtures, complete exact
public/transport/session/tensor/ordered-meaning consumers and adversarial restore
validation linking saved damage, trigger custody, payment and counter placement
to immutable committed history. The plain effect amount and counter transfer
are not claimed to establish that provenance. Full admission stays closed.

Source digest: `5b5d19da2b7ddcc6718d364ede6b2e66f40cd2fb35e7b92078a5cafea14cd470`.
Algorithm: sorted changed source/catalog/test/reference paths versus base, excluding this note;
SHA-256 over each UTF-8 path, NUL, then raw SHA-256 of LF-normalized file bytes.
Catalog LF SHA-256: `e9677499904a105bc9a1c889740d91e6414007950b80a4728e58aa4603b2ded2`.

