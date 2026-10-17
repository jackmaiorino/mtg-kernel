# Spinner of Souls source preparation

Status: genuine Partial printing and uncompiled, unrun source preparation for
issue110. Parent base is `86db24c2816b771cf4381c558c0b4d940f60aba6`.
Local append slot444 conflicts with the separate Phoenix preparation. The root
must reconcile IDs against its accepted parent before qualification. No catalog
hash/profile is guessed and no Full or consumer clearance is granted here.

Pinned primary source: mage `a5c90fe180021e70e2a644ade00eeab07f857a40`.
`Mage.Sets/src/mage/cards/s/SpinnerOfSouls.java` has SHA256
`3b7ba7d583a97d3865e74c286f8c866c4fab8dee6a0976d56728b1db8f38a172`.
It defines {2}{G},4/3 Spider Spirit, reach, another controlled nontoken creature
dying, and optional public reveal through a creature to hand with remaining
revealed cards on the bottom in random order. Its reused effect source,
`Mage/src/main/java/mage/abilities/effects/common/RevealCardsFromLibraryUntilEffect.java`,
has SHA256 `78273939381ab87bacaf2bcbcaaa332302badccfd2c9a58c6852c8c51c096ae8`.

The typed `OtherControlledNontokenDeathDefV1` descriptor owns the recipe.
Its appended trigger condition extends the established prebatch death capture,
so simultaneous source removal preserves every qualifying observer and excludes
the source, opposing creatures and tokens. It does not change Hunter's any-controller
filter or use a printing-name dispatch. Optional typed JSON intrinsic keywords
generate reach without a new name-based keyword branch.

The optional recipe uses the existing generic option wire shape, decline0 and
accept1. Its validator reconstructs the exact descriptor-owned root and checks
the suspended options and single answered frame before resumption. Decline
does not read/reveal/reorder the library or advance randomness. The reusable
`RevealUntilCardTypeToHandRandomBottomV1` operation publicly reveals the dynamic
prefix, moves the first matching card to hand with public identity, and moves
only the nonmatches to the random bottom. It stages the whole consequence on
a clone, using the existing checked legacy/environment-v2 bottom randomizer.
Empty libraries do nothing; a no-creature library all goes to random bottom.
No draw, mill, fixed-prefix private look or rest-order choice is substituted.

The active consolidated fixture module pays for actual Spinner, Llanowar Elves,
Lightning Bolt, Goblin Surprise and Day of Judgment. Prepared cases cover both
seats and both RNG modes, top/deeper/empty/no-creature libraries, public hand
knowledge, unaffected suffix, randomized-bottom knowledge loss, exact restored
results, decline, token/opponent/self exclusions, two simultaneous controlled
deaths with a departed Spinner, and malformed pending/answered payload refusal.
The opponent exclusion uses a physical opposing creature as fixture input and
an actually paid Bolt for its death. These are test bodies, not observed passes.

Observed checks: pinned `cargo +1.94.1 fmt --all` and `git diff --check` pass.
A small static Python check verifies all252 prior FDN records remain unchanged,
the true Partial local444 append, unchanged frozen printing/token inputs, exact
predecessor `EffectOp` and `TriggerCondition` prefixes, and no Spinner runtime
or codegen name dispatch. Absent descriptors add no catalog semantic token;
empty intrinsic-keyword lists leave the old keyword expression unchanged.

Follow-up review repairs correct the test keyword API to `Keywords::has`, pin
the standalone totals to445 definitions/253 FDN rows, and retain every prior
prefix hash and ID. The capability canary now requires exactly Partial
Fake442/Ravenous443/Spinner444 and explicitly checks each genuine printing's
Full-only refusal. The test-only frozen-V1 audit excludes Spinner's intentionally
refused descriptor and has explicit extractor and complete-table refusal tests;
production capability and extractor refusal are unchanged. Actual small Python
results:61 Limited deck cases and6 frozen-pool cases pass. Pinned formatting,
diff checks and repeated static prefix/inventory checks pass. Rust test bodies
remain uncompiled and unrun.

No Cargo build/test/check/clippy, native launch, gameplay, simulation, GPU,
paid execution, push or PR occurred. The new descriptor explicitly refuses the
frozen V1 rules source extractor, and the new effect/condition are opaque in its
meaning vocabulary. Complete public facts, observation/backend/Human bindings,
model and sampling coverage, current catalog identity, native qualification,
actual printing gameplay and full issue110 acceptance remain root-owned work.
PR272 and native147 source/target/reservation were left unchanged.
