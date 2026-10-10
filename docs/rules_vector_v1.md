# Rules vector v1 (step 1: extractor and audit)

Status: engineering only. Nothing here is read by a model, trainer or search. Training decisions (steps 2 and 3 of the plan) belong to the research lead and pass the usual design review first.

## Why

The play model sees each card as a learned 16-dimensional ID embedding (`native_policy_value_net_v1.rs:71-72`). That embedding carries no meaning for a card it has not trained on. Brewing needs a play model that can learn a decklist it has never played, so cards need a representation built from what they do. The project's rules-only principle rules out reading card text with a language model. This extractor builds the representation from the engine's own card programs instead.

## Reviews this design answers

- Codex (Astra) review: collab `CODEX-ASTRA-RULES-VECTOR-PLAN-REVIEW-20261009.md` (dcb3bab8), verdict go ahead with changes.
- Five-lens Claude panel, verdict revise; synthesis in the project files (`notes/rules_vector/CLAUDE-PANEL-REVIEW-20261009.md`).

Both reviews agree that rules features alone are not expected to make the model discover the Spy line. Self-targeting has a negative advantage under the policy's own continuation, so the move gets almost no gradient. The headline test for features is therefore held-out deck learning. Step 1 makes no training claim.

## What it reads

`src/rules_vector_v1/sources.rs` destructures `CardDef` with every field named. A new field fails to compile until someone decides whether it carries rules. The walk also reads every rules source the engine keys outside `CardDef`:

- `trigger::triggers_for` and `trigger::target_spec_for_trigger`. These matter most for Spy, whose whole behaviour lives in its trigger.
- `trigger::required_optional_additional_cost_for_trigger`.
- `engine::static_self_boost_for` and `engine::static_controlled_subtype_boost_for`.
- Mana programs through `CardDef::mana_ability_program_for`.
- Modes, alternative forms, saga chapters, equipment-granted abilities, attachments and transform faces.
- Created tokens, one level deep.

Rules implemented only in engine code are listed as opaque (`Atom::Opaque`) rather than guessed. The report names each one.

Never read: card names, `cards_v1.json`, `mechanics`, `complexity`, `java_file`, recipe strings, or EffectOp variant identifiers as features. `tests.rs` checks the extractor sources for these.

## What it emits

There is one exhaustive meaning table per rules enum (`meaning/`): EffectOp, TargetSpec, EffectCond, DynamicValueDef, DynamicCountDef, the object filters, TriggerCondition and CostComponent. None has a `_` arm or `..` pattern. Each variant maps to mechanical facets (`facets.rs`):

- Effects: event kind, from zone, to zone, whose cards, object class, amount and duration. Every zone change is `Move`, so mill, discard, destroy, bounce and reanimate share structure.
- Reads: owner, zone, object class and aggregate. Every "creature cards in your graveyard" encoding gives the same shape.
- Target legality, costs, trigger event classes, and control structure (optional, conditional, choose).

Each atom contributes its full tuple and a few marginals. Pairs within one ability ("enters trigger" with "library to graveyard") add structure. Counts are log-scaled and L2-normalised. The frozen v1 vocabulary is the feature set of the 192 Pauper registry cards. Features of other cards outside it are out of vocabulary.

Outputs, written by `cargo run --release --bin rules_vector_v1 -- OUT_DIR`:

- `rules_vector_v1.<build>.json`: the canonical table.
- `.manifest.json`: sha256 of each extractor file, the output and `KERNEL_CARDDB_HASH`.
- `.report.md`: the audit.

## Pre-registered checks (written before the first full run)

These live in `src/rules_vector_v1/tests.rs`. A failure leads to a logged bug fix in a meaning table, never to reweighting.

1. Determinism: two builds of the table are byte-identical.
2. Coverage: every spell program, trigger and activated ability yields facts.
3. Equivalence:
   - Elvish Mystic, Fyndhorn Elves and Llanowar Elves get identical features.
   - No two nine-deck cards share features while having different rules records.
4. Synonyms collapse:
   - `Surveil { count: 1 }` and `SurveilOne` give the same facets.
   - Four graveyard-creature-count encodings give one read shape.
   - Forest-style and Guildgate-style tap-for-mana share a feature.
5. Named facts:
   - Spy: an enters trigger that moves library cards to a chosen player's graveyard until a land, and can target you or an opponent.
   - Thought Scour, Mental Note and Winding Way share library to graveyard.
   - Lotleth Giant: an enters trigger with damage scaled by your graveyard's creature count, targeting an opponent only.
   - Dread Return: graveyard to battlefield for a creature, and a graveyard cast whose cost sacrifices three.
   - Mesmeric Fiend targets an opponent.
   - Goblin Tomb Raider's static is flagged opaque.
   - Avenging Hunter takes the initiative.
6. Transfer: in the `limited-fdn-fixtures` build, at most 5% of FDN non-token cards have a feature outside the Pauper vocabulary. The EffectOp-name baseline is 23%.
7. Information content: the share of Pauper cards holding a feature no other card has is reported against the op-name baseline of 47%.
8. Mechanical neighbour checks:
   - Spy shares the targeted-player mill feature with Thought Scour.
   - Lightning Bolt is not among Spy's five nearest cards.
9. Isolation: the extractor sources contain no card-name literal and no registry label.
10. Backward chaining (diagnostic): goal regression from "opponent loses life" over the Spy deck's facets finds both lines, Spy then Lotleth Giant, and Spy then Dread Return on Lotleth Giant. It uses no card names. Costs are not regressed in v1.

## First-run results

Every check above passes except the FDN transfer gate (6), which failed as registered: 99 of 121 FDN non-token cards (82%) have at least one feature outside the Pauper vocabulary. The test is kept, marked ignored with that result, and not loosened.

The breakdown in the FDN report explains it:
- Most of each FDN card's features are shared. The in-vocabulary share of a card's feature counts has a median of 0.91, a 10th percentile of 0.75 and a minimum of 0.50.
- The misses are concentrated in the most specific families. Within-ability pairs miss on 50% of cards, full effect tuples on 43%, printed facts on 38% (new subtypes and keywords), and effect marginals on 33% (for example "gain exactly 1 life", which no Pauper card does).
- The 23% baseline counted EffectOp names, a much coarser unit, so it is not a like-for-like comparison.

Information content (7) moves the same way: 76% of Pauper cards hold a feature no other card has, against 47% for op names. The specific families behave like per-card IDs. For step 2, this argues for feeding the coarse families, or hierarchical features with dropout, and treating the full tuples and pairs as optional.

Backward chaining (10) is a diagnostic only, per the independent review of it (collab `CODEX-ASTRA-BACKWARD-SEARCH-REVIEW-20261009.md`). Nothing in play, search or training reads it. That review found four bugs, all now fixed and tested:
- circular proofs were accepted;
- creature-only damage counted as hitting the opponent;
- flashback was never linked to the graveyard;
- pump signs were lost.

After the fixes it finds 40 well-founded proofs in the Spy deck, all scaling. They use five card sets: Spy + Lotleth, Spy + Dread Return + Lotleth, and three with Winding Way, which can mill creatures by naming Land. Other decks get only direct-damage lines, or none: CawGates, Elves, Faeries and Terror get none, because combat damage is engine behaviour (rule 510), not a card effect.

Known gaps:
- costs and timing are not regressed;
- target limits and choice branches are merged;
- "scaling" only means the damage amount is dynamic.

The review's view, which this design adopts, is that the regression relations are a hand-picked abstraction. A principled version would regress over the engine's real transitions, which is what forward engine search does.

## Not in v1

- Probe-state verification of each meaning row against engine events. Rows are static-only and checked by review and by the named-fact tests.
- Lifting opaque engine predicates into data.
- Any model input. Steps 2 and 3 need the bit-identical zeroed-input control, a permuted-features arm, ID dropout for held-out cards, and the action-hash ablation the reviews call for.
