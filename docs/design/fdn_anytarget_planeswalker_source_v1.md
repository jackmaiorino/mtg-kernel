# FDN AnyTarget planeswalker recipient source

Prepared from `846f15756d5d88646a679209b66eff3ebaa34ed6` for issue #110. This source does not register Gorehorn Raider or a planeswalker, change a profile, or qualify a Full card pool. Ajani, Caller of the Pride is an existing **Partial** definition used only as a recipient primitive. Its loyalty abilities and planeswalker combat destinations remain unsupported.

## Behavior and authority

`engine::legal_targets_for_controller_from_source(AnyTarget)` retains players first, then battlefield object order by seat. It admits effective Creature or Planeswalker recipients only under `limited-fdn-fixtures` AND NOT `standard-magezero-fixtures`. Standard wins catalog precedence when both features are enabled; default, Standard-only and combined builds retain their old creature-only enumeration. Hybrid types receive one object option. No target enum, object layout, observation schema, action/tensor layout, definition ID or catalog metadata changes.

Existing source-aware protection and opponent hexproof filters apply before announcement and again at resolution. `StackTargetContractV4` already binds battlefield zone and zone-change count. Departure and reentry invalidate the old target even when the physical object ID returns. The existing damage event path removes planeswalker loyalty without marking damage on a pure planeswalker. Creature/planeswalker hybrids retain both paths. Existing zero-loyalty SBAs move the recipient to its owner's graveyard. Existing definition-owned spell and continuation validation stays in force.

The fixture initializes Partial Ajani through a real Hand-to-Battlefield zone event using its registered starting loyalty, then actually announces and resolves registered Full Lightning Bolt through the engine surface. It does not cast Ajani or inject an admitted ability. Protection and hexproof use the existing temporary keyword primitive and are labeled recipient primitive setup. Sire life-Ward remains bound to its source, payer and exact Bolt stack incarnation, including after the source departs and returns. An unrelated planeswalker recipient does not acquire Sire's Ward.

## Prepared checks and execution boundary

`engine::anytarget_planeswalker_tests` contains eight named groups: feature precedence and target ordering; real Bolt target choice with both players, creatures and planeswalkers; both seats and both recipient controllers with loyalty damage and zero-loyalty SBA; departed/reentered recipient rejection and fresh recast; protection at announcement, resolution and damage prevention; hexproof controller filtering and resolution recheck; restored recipient grants surviving source departure and expiring through the production Cleanup entry action; and restored Sire life-Ward source/targeter/payer bindings with both payment answers. Pending casts, completed target selection, resolution and Ward decisions are serialized and restored using normal production validation, then compared by exact bytes and state hash.

These Rust fixtures remain **unexecuted** until parent-owned serial native qualification. Rustfmt and `git diff --check` are the permitted static checks. Run the named groups under Limited-only and the ordering/precedence group under default, Standard-only and combined features. Parent must verify old default/Standard/combined replay hashes against existing frozen evidence and qualify actual recipient traces before claiming gameplay support.

## Prepared fixture repair

`cast_bolt` now advances through the real surface after `ChooseTarget` and requires completed cast payment/finalization before inspecting stack targets or mutating the post-announcement recipient. The pending-cast prefix alone is not a finalized spell.

The queried source does **not** suppress external grants on Partial Ajani: `CardCapability::is_executable` returns false only for `NoEffect`, and `CardDef::is_executable` delegates it. The registered Ajani row is Partial, so it already reaches the normal generation-bound temporary grant query. Fixtures explicitly require `is_executable` and reject `has_full_support`, then distinguish printed keyword absence from a recorded recipient grant. No production keyword guard was relaxed. The new recipient-primitive group restores the actual grant, moves its setup source to the graveyard, verifies the grant survives, and invokes the production Cleanup entry action to verify expiry. This is a physical effect primitive test, not printed Ajani ability admission. Existing departure/reentry and actual Bolt target/recheck tests still exercise the separate source and recipient contracts.

## Consumer and acceptance dependencies

The frozen `raw_mtg_rules_meaning1` extractor, `rules_vector_v1/meaning/targets.rs`, explicitly emits AnyTarget as players plus Creature. This preparation leaves that historical table untouched. Its Limited meaning is incomplete for the new recipient set. Core/public target choices use existing object identities, but this is **not model or RulesMeaning admission**. An explicit successor must encode the Planeswalker target class under the same catalog precedence, version the meaning contract, and qualify consumer agreement before admitting models that depend on those facts. No successor ID, tensor mapping or catalog hash is fabricated here.

Planeswalker **combat destination selection** is a separate issue #110 acceptance dependency. Adding spell/ability recipients does not implement attacking planeswalkers or loyalty abilities. Full Ajani and any other planeswalker admissions remain pending their actual printed mechanics and consumer qualification.

The broader printed AnyTarget rule includes Battles. `CardType::Battle` is absent and the FDN booster card pool has no Battle card. This bounded FDN recipient patch does not add Battle support or establish complete general AnyTarget support. Other feature catalogs preserve their historical behavior by explicit scope.

## Primary reference

Owned Mage repository `C:/Users/Jack/IdeaProjects/mage`, pinned commit `a5c90fe180021e70e2a644ade00eeab07f857a40`:

- `Mage/src/main/java/mage/target/common/TargetAnyTarget.java` uses a permanent-or-player target with `FilterAnyTarget`.
- `Mage/src/main/java/mage/filter/common/FilterAnyTarget.java` admits Creature, Planeswalker or Battle permanents.
- `Mage/src/main/java/mage/abilities/effects/common/DamageTargetEffect.java` dispatches damage to the selected permanent or player.

Local source authority: `state.rs` target contracts; `engine.rs` source-aware legality and `stack_targets_still_legal`; `event.rs` prevention and damage commit; `planeswalker_v1.rs` generation-bound starting loyalty and damage; `trigger.rs`/`legend_rule_v1.rs` zero-loyalty SBAs. Historical schema/profile identity and frozen evidence are unchanged.
