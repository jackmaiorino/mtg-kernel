# Crystal Barricade prepared source

Base: `10e93ce1c10fcff810b526766a74484cfddf0894`. The frozen Foundations
booster pool contains Crystal Barricade, collector number 7. Both actual
Default and FDN card registry inputs at this base omit the name. No prior
prepared file or owned worktree was found. The separate prepared JSON is not
a generator input and assigns no card ID, capability, profile or catalog hash.

## Primary authority

Pinned XMage `a5c90fe180021e70e2a644ade00eeab07f857a40`,
`Mage.Sets/src/mage/cards/c/CrystalBarricade.java`: {1}{W}, white artifact
creature Wall, 0/4. Defender. You have hexproof. Prevent all noncombat damage
that would be dealt to other creatures you control. The primary executable
uses a controller hexproof static effect and a while-on-battlefield prevention
effect filtered to other controlled creatures. No private zones are consulted.

## Core source

Existing generic creature casting and the existing lowercase `defender`
mechanic need no new spell opcode, target specification or enum ID. New
definition-owned static queries require Limited without Standard, a known
executable current Crystal Barricade definition, battlefield, front face and
active printed abilities. They derive the current controller directly; owner,
tapped state and original cast controller do not limit these continuous effects.
Copies with the actual current definition naturally use the same source query.

The common controller/source-aware legal-target function filters opposing
player targets through this query. This covers announcement, existing typed
target completion and resolution recheck; the controller can still target
themself. The creature itself gains no object hexproof. Source removal,
reentry, suppression and control changes update the rule immediately without
stale stored source or target bindings.

The actual damage replacement pipeline checks prevention after existing
Standard damage modification and alongside existing protection, respecting
the accepted global damage-cannot-be-prevented override. The recipient must
currently be another battlefield creature controlled by the current source
controller. Combat damage, player damage, source-self damage and zero damage
are not prevented by that source. A second source can protect the first.
Fully prevented proposals reach no committed damage, damage-trigger or
lifelink consequence. No installed replacement, event, state field, receipt or
Hash input is introduced. Default, Standard and absent-source queries are pure
no-ops. Only the future actual Crystal definition receives an extra canonical
static recipe; accepted absent-card catalog bytes remain unchanged.

## Fixtures and verification

Registered Llanowar Elves primitives exercise both-seat absence, readonly
static queries, existing player targets, actual damage commitment, combat,
zero and exact snapshot replay. Future printed fixtures require true parent
registration and exercise actual casting, defender metadata, legal creature
targets with prevented Bolt damage, own/opponent player targeting, target
recheck when hexproof appears after announcement, source departure/reentry,
current-control rules, two-source exclusion, a real Witness response using
registered High Fae Trickster, real Flaring Pain, privacy and restore.
Zone/control response setups are labeled typed primitives, not invented
printed cast producers. All casts advance actual finalization before response
or restore. No test root-validation hook or registration bypass is added.

Pinned Rust 1.94.1 formatting, JSON parse and diff checks are the only local
verification. No native/typecheck/gameplay results, admission, complete public
consumer, model or sampler qualification are claimed.

## Exact remaining prerequisites

The inherited prevention engine chooses active replacements deterministically.
It has no affected-player ordering choice when this static prevention overlaps
a consumable PreventNextDamage shield. Both orders can prevent the damage,
but differ in whether a shield is spent. This preparation follows the existing
pipeline and does not establish complete mixed-prevention printed semantics.
An affected-player replacement-order successor with prevention recheck after
each choice is required before that interaction can be admitted as complete.
No broadened replacement-order identity is silently assigned here.

Frozen V8/V9 continuous-effect projection explicitly refuses an active source:
its fields cannot represent player hexproof and prevention over dynamically
controlled other creatures. The predecessor remains byte-identical when the
source is absent or its printed abilities are suppressed. A public successor
must represent authenticated source incarnation, current controller, player
hexproof and the noncombat/other-controlled-creature prevention scope. It needs
precise selected-definition RulesMeaning, safe Human source handles and owning
restore/retry authority. Old V1 fixture projection checks privacy only. Full
model/vocabulary/vector, sampler and catalog/profile admission remain separate.
