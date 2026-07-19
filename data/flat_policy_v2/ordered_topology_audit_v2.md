# Flat policy V2 ordered-topology audit

Status: normative audit for `flat-policy-feature-inventory-v2`.

This audit covers every `ListSpec` reachable from `OBSERVATION_SPEC` in
`python/mtg_kernel_rl/features.py`.  It asks whether two canonical model
values that differ only in list or mapping topology can be reconstructed as
different V2 scorer-visible packets.  It does not treat operational-only
leaves as model state, and it does not reinterpret set-like lists whose
canonical contract sorts their elements.

## Reconstruction rules

The following ordering mechanisms are part of the V2 reconstruction contract:

1. Fixed actor-relative arrays use index 0 for the acting player and index 1
   for the opponent.
2. Rows in each object group are ordered by `visible_ordinal`.  Group order and
   row order are semantic, not an implementation accident.
3. Variable-width child tables use their parent's `start`/`count` slice and
   the child's explicit `order` field or row order inside that slice.
4. Relation rows use `primary_order`, `secondary_order`, and
   `associated_order`; those fields are semantic.
5. `CombatAttacker.blocked_order: Option<u32>` reconstructs the outer
   `attacker_to_ordered_blockers` mapping. `None` means absent. `Some(i)` means
   present at exact zero-based mapping index `i`, including an empty blocker
   list. Present indices must be unique and contiguous.
6. Paths listed in Python's `SET_LIKE_LISTS` are JSON-canonicalized by sorted
   element value. Their input order is intentionally not model state.

Legend: **fixed** = rule 1; **object** = rule 2; **child** = rule 3;
**relation** = rule 4; **blocked-order** = rule 5; **set-like** = rule 6;
**omitted** = every descendant leaf is operational-only.

## Exhaustive observation list inventory

| Canonical schema list path | Semantics | V2 reconstruction | Result |
|---|---|---|---|
| `observation.known_hand_cards` | player indexed | fixed | represented |
| `observation.known_hand_cards.[]` | set-like within owner | producer sorts by Python canonical-JSON key; known-hand object ordinal and relation membership | represented |
| `observation.known_library_cards` | player indexed | fixed | represented |
| `observation.known_library_cards.[]` | recorded library knowledge order | relation `primary_order` = relative owner; `secondary_order` = entry position | represented |
| `observation.own_hand` | oldest first | object group `SelfHand`, `visible_ordinal` | represented |
| `observation.projection.battlefield` | player indexed | fixed group identity (`SelfBattlefield`, `OpponentBattlefield`) | represented |
| `observation.projection.battlefield.[]` | battlefield entry order | object `visible_ordinal` within group | represented |
| `observation.projection.battlefield.[].[].ability_uses_this_turn` | sorted ability-use sequence | child `ability_use_start/count`; `FlatObjectAbilityUseV2.order` | represented |
| `observation.projection.battlefield.[].[].attachments` | operational-only arena handles | omitted from canonical model value | not applicable |
| `observation.projection.battlefield.[].[].characteristics.effective_subtype_ids` | schema-enforced sorted unique sequence | child `subtype_start/count`; subtype row order | represented |
| `observation.projection.battlefield.[].[].goaded_by` | set-like seats | set-like; goad child rows | represented |
| `observation.projection.combat.attacker_to_ordered_blockers` | ordered mapping, including present-empty values | blocked-order on attacker relations | represented in V2; V1 collision repaired |
| `observation.projection.combat.attacker_to_ordered_blockers.[].1` | ordered blockers for one attacker | blocker relation `secondary_order`; complementary attacker association | represented |
| `observation.projection.combat.ordered_attackers` | declared attacker order | attacker relation `primary_order` | represented |
| `observation.projection.continuous_effects` | effect order | effect relation/object `primary_order` and child `effect_order` | represented |
| `observation.projection.continuous_effects.[].add_subtype_ids` | set-like subtype ids | set-like; effect subtype child rows | represented |
| `observation.projection.continuous_effects.[].affected_objects` | set-like references | set-like; relation membership | represented |
| `observation.projection.continuous_effects.[].affected_players` | set-like seats | set-like; actor-relative effect flags | represented |
| `observation.projection.continuous_effects.[].remove_subtype_ids` | set-like subtype ids | set-like; effect subtype child rows | represented |
| `observation.projection.engine_context.pending_activation.chosen_targets` | selection order | context relation `primary_order` | represented |
| `observation.projection.engine_context.pending_activation.cost_discard_paid` | paid-discard order | context relation `primary_order` | represented |
| `observation.projection.engine_context.pending_cast.additional_cost_discarded` | discard order | context relation `primary_order` | represented |
| `observation.projection.engine_context.pending_cast.chosen_targets` | target order | context relation `primary_order` | represented |
| `observation.projection.engine_context.pending_cast.sacrifice_chosen` | sacrifice order | context relation `primary_order` | represented |
| `observation.projection.engine_context.pending_effect.choice.<boolean>.structural_path` | structural traversal order | child `context_path_start/count`; path-element row order | represented |
| `observation.projection.engine_context.pending_effect.choice.<color>.legal_colors` | legal color order | child `legal_color_start/count`; path-element row order | represented |
| `observation.projection.engine_context.pending_effect.choice.<color>.structural_path` | structural traversal order | child `context_path_start/count`; path-element row order | represented |
| `observation.projection.engine_context.pending_effect.choice.<number>.structural_path` | structural traversal order | child `context_path_start/count`; path-element row order | represented |
| `observation.projection.engine_context.pending_effect.choice.<options>.structural_path` | structural traversal order | child `context_path_start/count`; path-element row order | represented |
| `observation.projection.engine_context.pending_effect.choice.<targets>.legal_targets` | legal target order | context relation `primary_order` with legal/selected subrole | represented |
| `observation.projection.engine_context.pending_effect.choice.<targets>.selected_targets` | selected target order | context relation `primary_order` with selected count boundary | represented |
| `observation.projection.engine_context.pending_effect.choice.<targets>.structural_path` | structural traversal order | child `context_path_start/count`; path-element row order | represented |
| `observation.projection.engine_context.pending_optional_cost_sacrifice.chosen` | sacrifice order | context relation `primary_order` | represented |
| `observation.projection.engine_context.pending_triggers` | pending trigger order | context relation `primary_order` | represented |
| `observation.projection.engine_context.priority_passes` | player indexed | fixed `priority_passes: [bool; 2]` | represented |
| `observation.projection.exile` | recorded exile order | object group `Exile`, `visible_ordinal` | represented |
| `observation.projection.exile.[].ability_uses_this_turn` | sorted ability-use sequence | child `ability_use_start/count`; explicit order | represented |
| `observation.projection.exile.[].attachments` | operational-only arena handles | omitted from canonical model value | not applicable |
| `observation.projection.exile.[].characteristics.effective_subtype_ids` | schema-enforced sorted unique sequence | child `subtype_start/count`; row order | represented |
| `observation.projection.exile.[].goaded_by` | set-like seats | set-like; goad child rows | represented |
| `observation.projection.exile_play_permissions` | set-like permissions | set-like; permission relation/object payload | represented |
| `observation.projection.graveyards` | player indexed | fixed group identity (`SelfGraveyard`, `OpponentGraveyard`) | represented |
| `observation.projection.graveyards.[]` | recorded order, last is top | object `visible_ordinal` within group | represented |
| `observation.projection.graveyards.[].[].ability_uses_this_turn` | sorted ability-use sequence | child `ability_use_start/count`; explicit order | represented |
| `observation.projection.graveyards.[].[].attachments` | operational-only arena handles | omitted from canonical model value | not applicable |
| `observation.projection.graveyards.[].[].characteristics.effective_subtype_ids` | schema-enforced sorted unique sequence | child `subtype_start/count`; row order | represented |
| `observation.projection.graveyards.[].[].goaded_by` | set-like seats | set-like; goad child rows | represented |
| `observation.projection.hand_counts` | player indexed | fixed `hand_counts: [u32; 2]` | represented |
| `observation.projection.library_counts` | player indexed | fixed `library_counts: [u32; 2]` | represented |
| `observation.projection.life_totals` | player indexed | fixed `life_totals: [i32; 2]` | represented |
| `observation.projection.mana_pools` | player indexed | fixed outer array | represented |
| `observation.projection.mana_pools.[]` | fixed W/U/B/R/G/C fields | typed `FlatManaPoolV2`; no source list order remains | represented |
| `observation.projection.object_relations` | set-like relation records | set-like; typed relation rows | represented |
| `observation.projection.player_status` | player indexed | fixed actor-relative status fields | represented |
| `observation.projection.player_status.[].dungeon.completed_dungeons` | set-like dungeon ids | set-like; completed-dungeon child rows | represented |
| `observation.projection.policy_surface_context.private_combat_selection.remaining_after_current` | remaining candidate order | context relation `primary_order` and subrole | represented |
| `observation.projection.policy_surface_context.private_combat_selection.selected` | selected attacker/blocker order | context relation `primary_order` and subrole | represented |
| `observation.projection.stack` | bottom-to-top | object group `Stack`, `visible_ordinal` | represented |
| `observation.projection.stack.[].paid_cost_refs` | paid-cost announcement order | relation `primary_order` (`paid_order`) | represented |
| `observation.projection.stack.[].targets` | target announcement order | relation `primary_order` | represented |
| `observation.projection.surface_context.combat_priority_spent` | player indexed | fixed `combat_priority_spent: [bool; 2]` | represented |
| `observation.projection.surface_context.private_blockers.accumulated` | chosen blocker order | context relation `primary_order` and subrole | represented |
| `observation.projection.surface_context.private_blockers.remaining` | attacker candidate order | context relation `primary_order` and subrole | represented |
| `observation.projection.surface_context.private_blockers.remaining.[].1` | ordered blockers per remaining attacker | context relation `secondary_order` and association | represented |
| `observation.projection.surface_context.private_discard.chosen` | discard selection order | context relation `primary_order` and subrole | represented |
| `observation.projection.surface_context.private_discard.remaining_choices` | remaining choice order | context relation `primary_order` and subrole | represented |

## Collision disposition

The audit found one information-loss class in V1: an attacker relation exposed
whether its blocker list was nonempty, but did not expose whether an empty list
was absent from the outer mapping or present at a particular mapping index.
Consequently, the canonical values for mapping order `[A -> [], B -> []]` and
`[B -> [], A -> []]` could collapse to the same V1 tables. V2 removes the
redundant `was_blocked` field and adds `blocked_order: Option<u32>`. The checked
goldens in `goldens_v2.json` pin absent versus present-empty and both mapping
permutations, including six raw SHA-512 blocks and all 96 binary32 feature
bits for the red pair.

No other ordered observation list lacks a reconstruction mechanism under the
six rules above. This is an injectivity statement for the enumerated canonical
list topology only; it is not a claim that unrelated scalar fields or future
schema additions are automatically covered.
