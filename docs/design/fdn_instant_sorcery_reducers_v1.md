# Controller instant/sorcery cost reducers, source preparation

This separate issue #110 preparation supports the next cost-reducer admission. Neither
Mocking Sprite nor Archmage of Runes is registered or accepted by this work.

The pinned Mage constructors at
[a5c90fe1](https://github.com/jackmaiorino/mage/tree/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards)
define Mocking Sprite as 2U 2/1 Faerie Rogue with Flying and Archmage of Runes
as 3UU 3/6 Giant Wizard. Both reduce controller instant/sorcery spells by one
generic; Archmage also draws one on each instant/sorcery cast. Both primary
constructors were read at that commit.

The casting design must determine one complete spell cost: selected base,
additional mana, kicker, Spree, X and increases, then reductions and one floor.
Spell-face types determine modifiers for Adventure/Omen and Bestow. Alternative,
flashback, Escape, Madness and Plotted costs must use the same planner. Freeze
the payment before a sacrifice can remove a reducer, pay mana once, and preserve
nonmana costs, Delve/Convoke, source reservations and one combined life budget.
Activation and resolution payments remain outside spell modifiers.

Source 8c3b9914 implements the u32 combined mana total without truncating at 255.
Both Limited-only and Standard/Limited test compilations and all 15 mana tests
passed under guard ab5c3985b65b40339b8c63a979a7eabd, two cores at BelowNormal,
exit 0, Rust 1.94.1 and MSVC 19.50.35725. Read-only review found no defects.

Source 02dc1469 collects every selected casting route and original nonmana
component groups. Six collector tests use existing registered cards. Native27
guard7eb5daccdc9848b68f8cc430b0010f2b expired unstarted with no_room after its
one-hour admission window; supervisor51012 is absent and no command log was
created. Observer27842 ended at its observation deadline, which alone is not
terminal evidence. Jack's occupied reservations were preserved.

Source 5c231a1c binds Flying, Archmage's existing cast-draw trigger, static
fingerprints, rules-vector extraction and live modifiers. Sources must be
executable front-face battlefield objects, currently controlled by the caster,
with printed abilities active. Raw Standard modifiers combine before flooring.
Read-only review found no actionable defects; native qualification is pending.

Source 64607708 prepares immutable Delve payment from the complete adjusted
total, preserving minimum-exile and oldest-first choices while excluding
reserved objects. It also sums mandatory life costs with Phyrexian payment and
preserves zero-life payment at negative life. Three new regressions cover these
risks. Read-only review found that post-solve life checking could reject a legal
mana allocation. Repair 41a74348 puts the remaining life budget inside pip
backtracking and adds the exact Aquifer/Swamp regression. Review then found
generic payment outside the pip search could miss a legal Phyrexian payment. Repair 5c761ec8 includes generic payment at the search's
terminal condition with isolated failed-branch state and a taxed-spell
regression. Exact read-only review of 5c761ec8 found both findings resolved
and no further actionable defects. Native execution is pending.

Convoke source 4bb578e0 solves ordinary mana and creature taps together against
the already adjusted total. Physical source aliases prevent double payment;
partial colored/Hybrid/Phyrexian coverage and colorless generic payment retain
pip requirements. Multi-yield mana dominates an aliased one-unit Convoke option
for generic payment. Planner indices are remapped before returning tap lists.
Primary ConvokeAbility.java was read at the pinned Mage commit above. Review
caught cached creature colors bypassing Aura overrides. Repair a6f96cc3 uses
object_color_mask and tests Witness Protection with a stale-incarnation check.
Exact repair review found no further defects. Six new Convoke regressions and
all newer payment changes remain unexecuted.

Source 3e5bf0ca extracts immutable selected-component choice validation and
reserved tap sources from legacy payment without changing its checks or order.
Two focused regressions cover invalid sacrifice choices and tap/mana source
sharing. Exact read-only review found no defects. Native execution is pending;
this is not yet complete nonmana preflight or casting-route integration.

Repair 892e828f preserves the explicit Alternative-cast requirement that at
least one creature convokes. Colored payment backtracks when floating mana
would leave no convoker; generic payment tries each available convoker on
isolated state, preserving another creature's multi-yield mana when needed.
Two new regressions cover these cases. Exact read-only review found the named
defect resolved. The future shared planner may change the legacy helper's
deterministic maximum-creature tap selection; integration must account for it.

At approximately 20:37 UTC Haley's core-slot and whole-host status both showed free. Source
892e828fc8e438c0298a8f1e8eca6c3ae3590e53 was transferred in a SHA256-checked
bundle (6cc834a30225f2bfbdb3681da14cb4cf7fa6ca4e4a1a7c817e1565c3bd9d5341)
to a new owned checkout, C:/mtg-node/codex-fdn-spell-costs-20261010.
Supported guard cb559b02c0d440fa9c4393847d110c82, supervisor156816,
admitted cores14-15 at BelowNormal priority. The actual command log confirms
Rust1.94.1/MSVC19.50.35725 and compilation started. This small correctness
sequence checks both feature configurations, mana/collector tests, rules
vectors, frozen v65 and the prior graveyard/static-team games. It is pending;
no newer source tests or card games are claimed as passed. Jack's older
Native27 queue and observer remain intact. The later local Native28 command
has not been submitted.

Source b492cadd adds immutable group preflight with separately assigned object
choices, tap-source reservations and mandatory/optional graveyard exclusions
for Delve. The caller supplies modifiers frozen before payment; projected
post-discard resources cannot recompute those modifiers. The caller must also
collect own_generic_reduction before costs and independently authenticate
discard, chosen-creature and optional-cost bindings. Two unexecuted regressions
cover a completed discard with shared life/frozen modifiers and graveyard
choices that must not double-pay or supply Delve. Source b086137a extends the
frozen-modifier assertion. Source 98758bf1 separates the legacy component
commit loop from mana derivation, preserving every mutation and Convoke arm.
Exact read-only review of 98758bf152060fdc4ed6b904f71ddd79a9197a43 found no
actionable defects; diff checks pass. These increments remain unqualified and
unwired until the successor native sequence executes them.

The earlier Haley Native6 sequence at892e828f passed both feature test
compilations and25 mana cases. Collector execution passed10 and failed one:
selected_tap_cost_reservation_prevents_one_land_paying_two_components listed
Tap before Mana, violating the existing Mana-first shape validator. The
subsequent rules, frozen identity and prior card games were not reached.
Repair10570d76 preserves the double-payment refusal assertion while ordering
the fixture components correctly. Exact read-only review found no defects.
The original source and command/guard logs are retained; guardcb559b02 ended101
after its owned idle VCTIP helper was identified by job membership, executable,
start time and stable CPU sample and stopped. No peer process was stopped.

Merge37c1a353beaa5933c6047f9c2eee377e206eaf64 incorporates accepted default
32080f8; read-only review confirms its payment planner files match the first
parent. After observing terminal Native6, the owned Haley checkout advanced
using bundle SHA256b691d1c3267dfe55258ae5281de69a0e382f8af7e421e1d6a703beb9bc91ddc6.
Supported Native9 guard0b4308d568e240baa8e3eb4583d722d6, supervisor154668,
admitted cores14-15 BelowNormal. The actual log confirms source37c1a353,
Rust1.94.1/MSVC19.50.35725 and compilation. Observer5388 monitors both feature
compilations,25 mana cases,13 collector/preflight cases, rules vectors, frozen
v65 and17 prior card games. Results remain pending. Native28 was not submitted.

Native9 actually passed both feature test compilations,25 mana and13
collector/preflight cases, then its command failed101 because rules_vector_v1
is a library filter, not an integration-test target. Rules, frozen identity and
prior games were not reached. Guard0b4308d568e240baa8e3eb4583d722d6 records
terminal101; the source, command and failure log remain retained.

Sourcea53aa44f7db51108c1d4eb57713b926d88bddf94 adds pure component-choice
assignment preserving base/additional ownership and canonical Escape order.
Two regressions cover partitioning and invalid graveyard picks. Exact read-only
review found no actionable findings. After prior terminal verification, bundle
SHA256649f64c2a1242281a8df80ca50769f06afdef1f62045be791ca86e0a4a53b76d
advanced the owned remote checkout to a53aa44f. Native11 corrects the rules
command to --lib rules_vector_v1::tests::. Supported guard
12db92f88f044517b443fe89429f0351, supervisor151276, actually admitted cores14-15
BelowNormal with pinned Rust1.94.1/MSVC19.50.35725. Observer71399 reports
command exit0: both feature test compilations and81 actual executions pass
(25 mana,15 collector/preflight,23 rules-vector, frozen v65,8 graveyard and9
static-team cases), with one existing rules-vector ignore. Guard release still
requires terminal verification in that initial command receipt. Subsequent
membership inspection identified only supervisor151276 and idle VCTIP18796.
Its executable/start time and stable ten-second CPU sample verified ownership;
stopping that helper released the guard with observed terminal0. No peer process
was stopped. This qualifies the preparatory primitives only;
the runtime casting paths have not been changed and Sprite/Archmage stay absent
from the registry.

Runtime integration must prepare an ephemeral payment before apply_discard
mutates cards, then consume it without recomputing reductions. Preserve the
base/additional choice partition, Escape ordering, chosen-creature power and
paid-cost provenance. Validate optional sacrifices/exiles against mandatory
resource use. Sacrificed lands may produce mana first; tap costs cannot reuse
their source for mana or Convoke. A future discard-plus-Delve shape needs a
projected resource state while preserving its pre-discard frozen cost. No
serialized PendingCast payment attestation is introduced.

Native12 qualifies the nonmana preparer at
6dff3f094d1a71ca37e0af2195bfcaf78e6e5dcb. Both feature test compilations and
84 executions pass:25 mana,18 collector/preparation,23 rules-vector, frozen v65,
8 graveyard and9 static-team cases, with one existing rules-vector ignore.
Guard7e431e33acbb498d99fd9c384b954f89 released with observed terminal0. The
preparer authenticates hand entries before legacy reveal predicates, applies
ordered cost groups to a private projection, and caches exact counter/reveal
actions for synchronous consumption. No restored payload can assert payment.

Source16f2f1186806762a5b89147c58ec417eba2827f6 wires full prepared payment into
finalize_cast and FinishCast discard. Modifiers freeze before actual discard;
the resource projection uses the Madness-aware discard path, pays one combined
mana/life total, then prepares base and additional groups and optional costs.
Chosen-creature power and paid-cost provenance retain the final paid snapshot.
Actual commitment consumes the prepared payment once. Mid-cast payability uses
the same selected total and object/GY reservations. Read-only reviews found no
actionable findings. Native13 guardc2a3c672335b4133a8efa0e116fb2373 admitted
cores14-15 BelowNormal, Rust1.94.1/MSVC19.50.35725. Both feature test compilations
and206 executions pass:169 engine,19 combined-feature mana,8 graveyard,9
static-team andone captured Hacker case. The command and supported guard both
ended with observed terminal0. No helper cleanup was necessary.

Source776b2aed8040a36774f8ad34b407bdf7004efc34 adds pure complete-cost quotes
to cast offers, spell forms, cast modes, target-dependent affordability,
kicker and X validation/choices, and Plotted offers. One supported interactive
object family is completed before quoting; tap picks reserve mana sources,
sacrificed lands may produce mana first, and exile picks cannot overlap separate
graveyard costs. Four added regressions cover discard/source exclusion,
sacrifice completion, Escape reservations, and malformed reveal-only hands.
All four new regressions passed in Native14. Review including Plotted and the new
cases found one orphaned wrapper with only test callers; sourceeab7cae9 gates
it with cfg(test) to preserve strict lint. No other actionable finding remains.
After clean-checkout/prior-terminal verification, bundle
SHA25669ffc2bdc922287bb51774039d3c983fafd0428cce9a0aa3070b5c004ece81d1
advanced the owned remote checkout to eab7cae9a0f745395acd3b242fa623116b975e4b.
Native14 guard9a4d7a309fa6448da8756b9a94e42554, supervisor157364, actually
admitted cores14-15 BelowNormal and compilation began with the same logged
pins. Observer66937 retains both feature engine tests, prior gameplay and
strict lint results. Native14 passed173 Limited engine cases, then failed one
combined-feature Koma fixture because the Standard registry excludes Koma.
Guard9a4d7a309fa6448da8756b9a94e42554 ended101. Later gameplay/lint commands
were not reached. The retained source and failure log remain evidence.

Sourced511c514 gates the Koma fixture and seven future Sprite/Archmage fixtures
to the actual Limited registry. Native15 passed both feature test compilations
and340 executions:173 Limited engine,149 combined engine,8 graveyard,9 static
team andone Hacker. Its strict lint failed on the inherited Aetherize collector
and a new nested kicker conditional; guard88d93b68a5c841839ac7a517b837f961
ended101. The equivalent filter/map and collapsed conditional repairs preserve
behavior. The next exact-source lint and regression qualification remains
pending. The seven new card fixtures are executable source but were only type
checked, because Sprite/Archmage remain unregistered. No card coverage is added.

Native16 at3dc199a5 again passed both feature test compilations and340 actual
regressions, then all-target lint reached an example with a non-exhaustive cost
match. Guard438d81ed53604e289ab5410a7e4b43b0 ended101. The exact existing
delivery repair0c0b0a96 is ported to walk_diff: the new sacrifice variant joins
its unsupported-cost fallback without inventing rendering semantics. The
library/runtime receipts remain compatible; both all-target feature lint
checks still require execution on the repaired source.

Native17 at a95eb446 passed strict all-target Limited lint. Combined-feature
lint then rejected a Convoke let-else in the Option-returning payment helper.
Guardff52b27d4bad479d82cf983b5a37e306 ended101. The equivalent question-mark
rewrite preserves the prior340 runtime receipts. Both feature lint checks on
the repaired source remain required; no pending check is a pass.

Native18 at d9e347ed reached the combined catalog all-target lint and failed
on three fixed Limited-only indices in fdn_activated_combat_v1. Those cards
are excluded by the Standard catalog. Gate that fixture to its actual Limited
catalog, as with the Koma and new reducer fixtures. The command ended101;
its log is retained. The Limited strict-lint and340 runtime receipts remain
compatible. Combined all-target lint on this repair is still pending.

Remaining work: finish native qualification of runtime integration and quotes;
actual Sprite/Archmage gameplay and restore tests; metadata/catalog admission;
live profile qualification; CI and default-branch acceptance. Accepted booster
coverage is169 after accepted PR213. No new card gameplay or experimental result is claimed.

Native19 at2bf6a03e passed strict all-target combined-catalog lint in1m18s.
Supported guardeaa94dbba304467eb8366c9621b4b9d5 ended0. The compatible
Limited lint and340 runtime receipts above complete framework qualification;
seven Sprite/Archmage games still require registration and execution.

Accepted v66 was merged cleanly into this owned branch at3226e574. Candidate
79995d87 admits Sprite369/Archmage370 and371 definitions; all37 affected Python
cases pass. Native20 uses two guarded cores, incremental disabled, and runs
the seven actual games before extracting the generated v67 hash. Live profile
migration and acceptance remain pending; candidate171 is not accepted coverage.
