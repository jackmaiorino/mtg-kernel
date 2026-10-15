# Ravenous Amulet soul-counter storage prerequisite

Issue #110, source only. Ravenous Amulet is registered as Partial after Fake Your Own Death, prospective ID443 in444 definitions/252 FDN records. Candidate coverage is236 Full/2 Partial/48 Missing. The accepted default and PR266 inventory remain unchanged. Public schema versions, tensor shapes and model admission remain unqualified.

Pinned Mage source `a5c90fe180021e70e2a644ade00eeab07f857a40` defines a {2} artifact. Its first ability costs {1}, tap, sacrifice a creature, draws one card then puts a soul counter, and is sorcery-speed only. Its second costs {4}, tap, sacrifice itself; each opponent loses life equal to its soul counters. `CountersSourceCount` reads the source permanent or last-known information. Clearing counters on zone change cannot supply that loss amount.

The privileged `Counters` structure now stores soul independently. Zero omits its JSON field and adds no hash bytes; nonzero appends a distinct hash marker and signed count. Existing zone transitions clear it. The shared all-counter transfer scales and checks soul before mutation, retains it across restore, and refuses overflow. The old public card and Human counter-transfer shapes refuse nonzero soul rather than silently dropping it. Their zero-value JSON shapes remain unchanged.

The appended `AddSoulCounterToAbilitySourceV1` instruction places one counter only on the original battlefield generation. A prior draw remains effective when the source departs or reenters. The placing actor is the original ability controller; recipient control selects its counter replacements. Both the placement and shared soul-transfer path use checked actor modifiers and recipient multipliers before mutation. A committed soul event retains source, actor, replacements, and before/count/after. Frozen rules-vector V1 emits only Opaque for this instruction; no charge or +1/+1 meaning substitutes for soul.

Each soul placement now captures a canonical original world with its event history and trigger log removed, following the existing counter-journal pattern. The private input and public counter event append consecutively in one primitive call after arithmetic succeeds. The existing counter-journal validation entry point checks their unique positional relationship, original actor/source generation, recipient control and replacements, and checked before/count/after arithmetic against that world. It also rejects private inputs or duplicate/changed soul events in the trigger log. Later departure and reentry do not replace the original inputs. Retained soul history refuses unqualified hidden-world resampling before the first owner permutation, including after the trigger log clears or the source departs. This establishes internal placement replay consistency; printing-specific paid ownership must still anchor those inputs to the genuine executing ability. An invented internally consistent original world is not a paid-ability receipt.

The appended `LoseLifeEachOpponentEqualSourceSoulV1` instruction reads the original battlefield generation or one exact, unique departure record. A returned incarnation never substitutes its counters. Nonzero soul and definitions with this exact activated root retain counter LKI, including zero for those definitions. Unused zero-soul cards keep the old absent cache. The original ability controller determines the opponent after a source control change. Before mutation, the new loss instruction checks the exact Standard life-loss multiplier and resulting life total, then submits the original amount through the existing life-loss event path. Legacy loss callers remain unchanged. Missing, duplicate, negative, future-generation, and overflowing operands refuse.

Generator recipes express both printed abilities. The first uses the existing controlled-creature sacrifice cost, including an animated Amulet itself, and orders draw before placement. Only this ability is sorcery-speed. The second uses tap and self-sacrifice with an untargeted loss root. The existing engine freezes the original source before payment. The actual Partial registry row copies the attributed prepared metadata and names a separate40-card reference. Full deck resolution refuses both Partial printings. Previous251 records are preserved. Actual generated count/hash and Rust execution remain pending; existing profile literals are retained.

Sixteen focused Rust tests are written for storage/refusal/transfer, ordered draw and departure/reentry, distinct recipient control, original-generation LKI and restore, original-controller untargeted loss, life-total boundaries, Standard's actual Bloodletter multiplier, both-seat placement journal restore/refusal, and refusal before any hidden-owner permutation. The journal cases include changed actor, arithmetic, original world, input links, and private/duplicate trigger events, with read-only refusal assertions. The sampler case covers cleared trigger logs, live/departed sources and restored states. The existing opaque-program test covers both new instructions. Two additional genuine paid-printing tests are enabled with actual Partial registration. They advance actual staged payments before asserting costs or LKI, use a real turn to untap, preserve the pre-cost count across labeled reentry, and distinguish first-ability sorcery timing from the zero-soul second ability above a real Think Twice. None has been compiled or executed. New enum variants append after all predecessor variants; a source-prefix comparison preserves both complete old enum bodies. Pinned Rust 1.94.1 formatting and diff checks pass. The future Doubling Season multikind test retains its existing ignore; its literal and assertions include soul. Forest operands establish primitive semantics, not printed Ravenous activations.

The genuine cost branches now acquire a private nonserialized Amulet payment
token before paying. It captures the original source, chosen creature or self,
ability index and, for the self-sacrifice ability only, its soul count. Only successful generic full payment consumes it
at finalized publication. The appended private payment receipt binds the whole
stack row, original-generation count and unique ordered tap, sacrifice-marker
and departure occurrences. Both generator recipes share the checked program
factories. The existing paid-printing cases check absent draw-count
captures and zero/nonzero self-sacrifice counts; execution remains pending. Hidden-world sampling refuses payment
history even before a soul placement, and the counter-journal boundary rejects
private receipts in the trigger log. This new payment custody is uncompiled and
unrun. The shared journal validation boundary now checks indexed receipt
structure, full printed stack shape, source/controller/cost relations, actual
tap/sacrifice/departure links, unique receipt identities and exact live rows.
Self-sacrifice counts are checked against the original generation's stable LKI,
including after reentry. The existing paid case exercises ten restored
forgeries at both payment stages and read-only refusal.

The draw ability uses its actual resolution world, not a frozen pre-payment
count. Zimone can change an artifact's counters in response. Only the second
ability reads the self-sacrificed original generation's count, so its receipt
contains the optional count. The paid test now resolves another genuine
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
forgeries are exercised inside the paid case, after both real resolution
and the actual engine counter primitive. No named counterspell producer is
claimed. These changes remain uncompiled and unrun. The printed ownership path
is limited to the pure FDN feature set; the generic soul instruction retains its
separate Standard modifier behavior.

The V51 printing descriptor preserves both ordered costs and programs, instant
versus sorcery timing, untargeted loss, original ability controller, source LKI
and resolution-time placement. It binds the actual catalog and source producer,
contains no private payment/history operand and keeps model admission false.
Actual paid cases check its restore and changed-timing refusal. Frozen rules
vector V1 remains opaque. A third genuine paid case sacrifices a creature with
paid Fake, restores before its gained return, then checks one tapped return,
one Treasure, one draw and one soul counter with both journals valid. All new
Rust remains uncompiled/unrun.

V51 public facts now project nonzero public battlefield soul counts and paid
activation ordinals with published/countered/draw/loss outcomes. Draw results
expose success without the private card identity. Source references preserve
the original generation; a returned incarnation is not substituted. Sacrifice
references omit the creature's underlying printing, and private worlds,
payment records and executable indices remain internal. Paid fixtures check
JSON restore, changed-facts refusal and hidden-library-content noninterference.
This public projection now feeds the distinct V51 observation and session
source described below. Human and model admission remain pending. It has not
been compiled or executed.

The V51 battlefield card wrapper preserves the existing public card fields
with mandatory exact soul, wide +1/+1 and marked-damage values. Existing V1/V2
observation entry points retain their soul refusal; only the new wrapper uses
the shared non-soul field projector. Its roster checks reject absent, duplicate
or miscontrolled objects. Paid fixtures check complete card-view restore,
changed soul refusal, absent roster refusal and private-content noninterference.
The V51 observation and session source now consume this wrapper. No model is
admitted, and native compilation and execution remain pending.

The live-row owner now captures a canonical complete original operation world
before removal. Removal verifies the whole post-pop state and retains that
input; completion retains its exact canonical output. Validation reattaches the
actual outer history prefix and replays the original atomic program, comparing
every operation event, captured soul-input world and complete output. Histories
are linked through the outer prefix instead of nested in these two snapshots.
Countered operations retain no resolution borrow and produce no effects.

An opaque narrow enclosing-Advance input preserves linked-operation ownership
only on the disposable verification replay. It records the actual original
stack and operation header without nesting the Runner's original worlds or
entry loans. The installer checks the original row and prefix, permits no
choice, entry, departure or answer work, and removes the local scope on return.
This prevents a genuine Banishing link from opening a second operation while
verifying Draw/Soul. It never installs runtime ownership in a restored game.
The meaning producer binds this new direct linked-operation dependency.

The paid Fake coexistence fixture now checks changed returned-creature, hand,
stack, input/output worlds and read-only refusal with two real own hand cards.
An additional both-seat paid Amulet case preserves a genuine Banishing Aura
link across live-row restore and complete resolution. Banishing's initial
permanent entry is the existing fixture operand, not a paid Banishing cast.
These new Rust paths and cases are uncompiled and unrun.

The distinct Ravenous V51 observation preserves every V50 family and adds a
mandatory exact card roster for both public battlefields, including zero soul,
wide counters and marked damage. Only a private V51 projection mode permits
its internal common rows to omit soul, paired with that exact roster. Actual
current validation rebuilds the whole observation and validates all inherited
journals. Older observation entry points keep their previous shapes and refuse
visible Ravenous, authorized known-library identities and retained paid history.
Authorized resolution-time search candidates receive the same refusal without
examining unrelated hidden cards. Source review found and repaired a missing
known-library check and an older-mode conversion path.

The appended V51 session mode binds the complete current candidate vector,
observation and predecessor family projections. Typed and generic restore
authenticate the original owner and menu before rebinding the revision; stale
bindings and foreign modes cannot be imported. Older sessions refuse before
advancement and again with the actual resulting actor before publication, so
a real draw cannot expose a newly visible Ravenous through an older menu.
Public deck reset continues to reject the Partial printing.

Five backend test bodies cover both seats: actual mana production and paid
Amulet cast, creature-sacrifice draw/soul and self-sacrifice loss; restore at
cost, published draw and loss; exact-width and candidate tampering; stale and
foreign restore ownership; zero-soul downgrade and predecessor refusal;
known-library perspective; a real upkeep-to-draw publication; and a paid Grim
Tutor search containing the genuine Amulet, followed by choice/restore parity.
Observation tests also cover paid Fake coexistence. All these Rust paths and
tests remain uncompiled and unrun. Pinned formatting and diff checks are the
only executed Rust tooling checks.

Remaining: actual native cross-family qualification, Human support and further
consumers. No Full, model, sampler, fair-search or training claim follows from
this prerequisite. Accepted Fake parent integration precedes the next Ravenous
delivery qualification; no catalog hash is guessed from the source preparation.

The distinct `ravenous_observation_v51` source prerequisite now retains every
V50 mixed-family field and producer, adds Ravenous paid facts and ordered
meanings, and publishes a mandatory `battlefield: [Vec<CardV51>; 2]` roster.
Every public battlefield card has exact soul, +1/+1 and marked-damage fields,
including zero. The common projection retains its predecessor slots only for
family composition; the V51 roster is authoritative for these three values.
Artifact checks require identical common card/order/control, unique battlefield
objects, compatible narrow placeholders and the same sorted nonzero soul facts.
`validate_current` rebuilds the entire value from the current owned state,
surface and supplied decision, so a recomputed commitment cannot authorize a
changed roster, fact, occurrence or decision identity.

A private `RavenousV51Artifact` construction mode and distinct
`CommonProjectionScopeV42::RavenousV51` admit nonzero soul only for public
battlefield storage used by this DTO. Prior V1/V2/V50 entry points keep their
previous modes and refuse nonzero soul. The new scope preserves every prior
family capability, including the explicit Crystal/Twinflame Fake-aware menus.
It does not modify any frozen profile, registry, model, tensor or sampler.
Scoped extension and core candidate wrappers validate the V51 state before
reusing existing private family builders. Core/Divine/variable-cast candidate
semantics use stable references and do not serialize a narrow card counter.

New tests reuse the genuinely paid Amulet fixtures, rather than synthesizing
payment owners. Both seats cover live publication, actual engine counter
removal, draw/placement, self-sacrifice loss, GameState/DTO restore, recommitted
forgeries, predecessor soul refusal, private hand/library noninterference and
mandatory wide fields. The actual paid Fake coexistence case checks the whole
V51 observation before its gained return and after both restore branches.
The width case uses labeled numeric fixture operands on a public artifact; it
does not claim an additional printed producer. No original operation world,
executable payment index, draw identity or sacrifice printing is added to the
Ravenous facts. Own and known hand/library cards remain ordinary authorized
channels. These source changes have only pinned Rust 1.94.1 formatting and
static/diff checks. All new Rust remains uncompiled and unexecuted.

Remaining consumer work: an owning V51 backend must select this scope, bind
the exact current observation/actions/decision, and use the V51 candidate and
extension entry points; Human must resolve visible row handles and render the
mandatory exact roster and ordered Ravenous facts. Existing sessions/Human
continue their current refusal behavior. Restore/retry/stale/foreign-action and
mixed-family backend/Human tests, native cross-family qualification and any
future model admission remain separate. This observation prerequisite alone
does not admit Ravenous as Full or admit training, fair search or sampling.

The distinct `HumanRavenousSessionV51` and `HumanRavenousProjectorV51`
source now consume the owning V51 backend directly. Their complete ordered
menu binding, current observation and Human owner/revision are checked before
submission. Typed restore advances the Human generation and the backend
revision; the backend's generic restore also invalidates pending prompts.
Inherited family field helpers share a local observation type alias without
constructing or revalidating a V50 observation. Every V50 Human family remains
present. Public battlefield rows use `HumanPublicCardV51`, preserving the safe
common card and mandatory exact soul, +1/+1 and marked-damage values, including
zero and wide values. These exact values participate in handle canonicalization
before labels and the final roster are published.

Ravenous Human facts select ordered printed costs/instructions and public
activation outcomes explicitly. Original source/current-original-source use
safe card handles; sacrificed incarnations use prompt-local roles that retain
generation distinctions without object IDs, generation numbers or underlying
face-down printings. Draw outcomes expose success and placement, never the
private draw identity. No payment/world record or executable menu index enters
these DTOs. The observation producer now binds the direct V51 Human sources
alongside all predecessor dependencies.

Four new both-seat source tests pay actual Amulet cast and both abilities
through Human, exercise typed and generic restore parity, retry/conflict,
stale/foreign/forged inputs with read-only refusal, preserve a genuinely paid
Fake grant through Amulet sacrifice and return, and inspect exact zero/wide
values and private-hand noninterference. A face-down physical creature fixture
is sacrificed by the actual Human ability after reusing the existing genuinely
paid engine cast helper; only safe incarnation roles enter its Ravenous facts.
Numeric wide-field operands on an already paid public artifact do not claim
another printed producer. All these Rust paths and tests remain uncompiled
and unexecuted. Pinned `cargo +1.94.1 fmt --all`, source consistency checks and
`git diff --check` are the only executed checks for this Human source change.

Native cross-family qualification remains with the assigned lead. Fake PR272
must merge before Ravenous delivery qualification. Ravenous remains Partial
ID443; public deck reset still rejects it. No registry, Full, model, sampler,
fair-search or training admission changes follow from this Human source work.

Primary source receipts:

- `Mage.Sets/src/mage/cards/r/RavenousAmulet.java`: blob `0d010bb77fcf85f9168ade1060b9430782571e0f`, SHA-256 `e89ec45785f01de0ef55d485fdcfbc34fb6f8deb71601511b214d43043b39d74`.

- `Mage/src/main/java/mage/abilities/dynamicvalue/common/CountersSourceCount.java`: blob `574ee108e69e7710de3d7c46b95a47e6eb15f132`, SHA-256 `00add2937bf1f70a9877114d432b9f87f6e65d18e106d7366319e2fcc80233d4`.

- `Mage.Sets/src/mage/cards/z/ZimoneParadoxSculptor.java` at the same pinned commit: blob `2f5c267e6061c487471e2170953286d96fe91aac`, confirming instant-speed doubling of each counter kind on controlled artifacts/creatures.
