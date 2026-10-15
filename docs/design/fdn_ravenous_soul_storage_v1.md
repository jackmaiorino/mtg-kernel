# Ravenous Amulet soul-counter storage prerequisite

Issue #110, source only. Ravenous Amulet remains unregistered and Missing. This prerequisite does not change printing inventory, generated definitions, public schema versions, tensor shapes, or model admission.

Pinned Mage source `a5c90fe180021e70e2a644ade00eeab07f857a40` defines a {2} artifact. Its first ability costs {1}, tap, sacrifice a creature, draws one card then puts a soul counter, and is sorcery-speed only. Its second costs {4}, tap, sacrifice itself; each opponent loses life equal to its soul counters. `CountersSourceCount` reads the source permanent or last-known information, so later implementation must capture the genuine pre-sacrifice incarnation and counter count. Clearing counters on zone change cannot supply that loss amount.

The privileged `Counters` structure now stores soul independently. Zero omits its JSON field and adds no hash bytes; nonzero appends a distinct hash marker and signed count. Existing zone transitions clear it. The shared all-counter transfer scales and checks soul before mutation, retains it across restore, and refuses overflow. The old public card and Human counter-transfer shapes refuse nonzero soul rather than silently dropping it. Their zero-value JSON shapes remain unchanged.

Five Rust tests are written for zero compatibility, storage/restore/zone clearing, both old public-card refusals, Human transfer refusal, and both-seat transfer/overflow. Pinned Rust 1.94.1 formatting and diff checks pass. These changes have not been compiled or executed. The future Doubling Season multikind test retains its existing explicit ignore because that printing is not registered; its literal and assertions now include soul.

Remaining: authenticated counter placement and event replay; paid sorcery-speed creature sacrifice and ordered draw/counter resolution; genuine pre-self-sacrifice count capture and untargeted opponent loss; card recipe and registration; public/meaning/observation/consumer support; actual paid both-seat tests and qualification. No Full, model, sampler, fair-search, or training claim follows from this prerequisite.

Primary source receipts:

- `Mage.Sets/src/mage/cards/r/RavenousAmulet.java`: blob `0d010bb77fcf85f9168ade1060b9430782571e0f`, SHA-256 `e89ec45785f01de0ef55d485fdcfbc34fb6f8deb71601511b214d43043b39d74`.

- `Mage/src/main/java/mage/abilities/dynamicvalue/common/CountersSourceCount.java`: blob `574ee108e69e7710de3d7c46b95a47e6eb15f132`, SHA-256 `00add2937bf1f70a9877114d432b9f87f6e65d18e106d7366319e2fcc80233d4`.
