# Ravenous Amulet soul-counter storage prerequisite

Issue #110, source only. Ravenous Amulet remains unregistered and Missing. This prerequisite does not change printing inventory, generated definitions, public schema versions, tensor shapes, or model admission.

Pinned Mage source `a5c90fe180021e70e2a644ade00eeab07f857a40` defines a {2} artifact. Its first ability costs {1}, tap, sacrifice a creature, draws one card then puts a soul counter, and is sorcery-speed only. Its second costs {4}, tap, sacrifice itself; each opponent loses life equal to its soul counters. `CountersSourceCount` reads the source permanent or last-known information. Clearing counters on zone change cannot supply that loss amount.

The privileged `Counters` structure now stores soul independently. Zero omits its JSON field and adds no hash bytes; nonzero appends a distinct hash marker and signed count. Existing zone transitions clear it. The shared all-counter transfer scales and checks soul before mutation, retains it across restore, and refuses overflow. The old public card and Human counter-transfer shapes refuse nonzero soul rather than silently dropping it. Their zero-value JSON shapes remain unchanged.

The appended `AddSoulCounterToAbilitySourceV1` instruction places one counter only on the original battlefield generation. A prior draw remains effective when the source departs or reenters. The placing actor is the original ability controller; recipient control selects its counter replacements. Both the placement and shared soul-transfer path use checked actor modifiers and recipient multipliers before mutation. A committed soul event retains source, actor, replacements, and before/count/after. Frozen rules-vector V1 emits only Opaque for this instruction; no charge or +1/+1 meaning substitutes for soul.

Each soul placement now captures a canonical original world with its event history and trigger log removed, following the existing counter-journal pattern. The private input and public counter event append consecutively in one primitive call after arithmetic succeeds. The existing counter-journal validation entry point checks their unique positional relationship, original actor/source generation, recipient control and replacements, and checked before/count/after arithmetic against that world. It also rejects private inputs or duplicate/changed soul events in the trigger log. Later departure and reentry do not replace the original inputs. Retained soul history refuses unqualified hidden-world resampling before the first owner permutation, including after the trigger log clears or the source departs. This establishes internal placement replay consistency; printing-specific paid ownership must still anchor those inputs to the genuine executing ability. An invented internally consistent original world is not a paid-ability receipt.

The appended `LoseLifeEachOpponentEqualSourceSoulV1` instruction reads the original battlefield generation or one exact, unique departure record. A returned incarnation never substitutes its counters. Nonzero soul and definitions with this exact activated root retain counter LKI, including zero for those definitions. Unused zero-soul cards keep the old absent cache. The original ability controller determines the opponent after a source control change. Before mutation, the new loss instruction checks the exact Standard life-loss multiplier and resulting life total, then submits the original amount through the existing life-loss event path. Legacy loss callers remain unchanged. Missing, duplicate, negative, future-generation, and overflowing operands refuse.

Unregistered generator recipes now express both printed abilities. The first uses the existing controlled-creature sacrifice cost, including an animated Amulet itself, and orders draw before placement. Only this ability is sorcery-speed. The second uses tap and self-sacrifice with an untargeted loss root. The existing engine freezes the original source before payment. Prepared metadata is deliberately outside active catalog loading, declares partial capability, and records both pinned Mage hashes. No active definition or registry row is added.

Sixteen focused Rust tests are written for storage/refusal/transfer, ordered draw and departure/reentry, distinct recipient control, original-generation LKI and restore, original-controller untargeted loss, life-total boundaries, Standard's actual Bloodletter multiplier, both-seat placement journal restore/refusal, and refusal before any hidden-owner permutation. The journal cases include changed actor, arithmetic, original world, input links, and private/duplicate trigger events, with read-only refusal assertions. The sampler case covers cleared trigger logs, live/departed sources and restored states. The existing opaque-program test covers both new instructions. Two additional paid-printing tests remain explicitly ignored until genuine registration and paid-owner qualification. They advance actual staged payments before asserting costs or LKI, use a real turn to untap, preserve the pre-cost count across labeled reentry, and distinguish first-ability sorcery timing from the zero-soul second ability above a real Think Twice. None has been compiled or executed. New enum variants append after all predecessor variants; a source-prefix comparison preserves both complete old enum bodies. Pinned Rust 1.94.1 formatting and diff checks pass. The future Doubling Season multikind test retains its existing ignore; its literal and assertions include soul. Forest operands establish primitive semantics, not printed Ravenous activations.

The genuine cost branches now acquire a private nonserialized Amulet payment
token before paying. It captures the original source, chosen creature or self,
ability index and, for the self-sacrifice ability only, its soul count. Only successful generic full payment consumes it
at finalized publication. The appended private payment receipt binds the whole
stack row, original-generation count and unique ordered tap, sacrifice-marker
and departure occurrences. Both generator recipes share the checked program
factories. The existing ignored paid-printing cases check absent draw-count
captures and zero/nonzero self-sacrifice counts; their ignore remains. Hidden-world sampling refuses payment
history even before a soul placement, and the counter-journal boundary rejects
private receipts in the trigger log. This new payment custody is uncompiled and
unrun. The shared journal validation boundary now checks indexed receipt
structure, full printed stack shape, source/controller/cost relations, actual
tap/sacrifice/departure links, unique receipt identities and exact live rows.
Self-sacrifice counts are checked against the original generation's stable LKI,
including after reentry. The existing ignored paid case exercises ten restored
forgeries at both payment stages and read-only refusal.

The draw ability uses its actual resolution world, not a frozen pre-payment
count. Zimone can change an artifact's counters in response. Only the second
ability reads the self-sacrificed original generation's count, so its receipt
contains the optional count. The ignored paid test now resolves another genuine
draw activation after an accepted soul-placement response boundary and checks
two then three counters, followed by loss of three after self-sacrifice/reentry.
This is not qualification of a printed Zimone producer.

Resolution and counter removal now acquire a private one-use exact-live-row
owner before the real engine pop. The appended removed/completed journals bind
the actual paid row, removal kind, original source presence and committed effect
indices. Resolution executes the unchanged atomic program, checking draw before
paired soul input/placement or exact original-controller untargeted loss. A
countered ability keeps its paid costs and produces no effect. Restore validation
requires either the exact live paid row or its unique complete removal; an old
completed row cannot be installed again. Seven additional restored-record/replay
forgeries are exercised inside the ignored paid case, after both real resolution
and the actual engine counter primitive. No named counterspell producer is
claimed. These changes remain uncompiled and unrun. The printed ownership path
is limited to the pure FDN feature set; the generic soul instruction retains its
separate Standard modifier behavior.

Remaining: native qualification and cross-family original-world validation of
the new lifecycle, active registration, public/meaning/observation/consumer support; actual paid
both-seat tests and qualification. No Full, model, sampler, fair-search, or
training claim follows from this prerequisite.

Primary source receipts:

- `Mage.Sets/src/mage/cards/r/RavenousAmulet.java`: blob `0d010bb77fcf85f9168ade1060b9430782571e0f`, SHA-256 `e89ec45785f01de0ef55d485fdcfbc34fb6f8deb71601511b214d43043b39d74`.

- `Mage/src/main/java/mage/abilities/dynamicvalue/common/CountersSourceCount.java`: blob `574ee108e69e7710de3d7c46b95a47e6eb15f132`, SHA-256 `00add2937bf1f70a9877114d432b9f87f6e65d18e106d7366319e2fcc80233d4`.

- `Mage.Sets/src/mage/cards/z/ZimoneParadoxSculptor.java` at the same pinned commit: blob `2f5c267e6061c487471e2170953286d96fe91aac`, confirming instant-speed doubling of each counter kind on controlled artifacts/creatures.
