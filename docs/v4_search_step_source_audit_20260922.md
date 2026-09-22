# V4 hidden-source stepping audit

The root clone controls pass, but they do not establish a valid simulated future. The inherited sampler replaces card_def, name and v4 on unknown hand/library objects while preserving their object identities. An existing effect can retain a reference to one of those objects.

| Source read | Consequence to test |
| --- | --- |
| engine.rs validate_pending_trigger, target_spec_for_trigger(source.card_def, pending.effect), around8667 | A hidden targeted-trigger source relabeled to another card can fail definition-owned target validation despite identical root scorer input. |
| engine.rs storm_source_contract_is_structurally_valid, source.card_def == contract.card_def, around1925 | Frozen spell provenance can remain linked to a live object whose definition was changed by sampling. Root observation equality alone is insufficient. |
| rl_session.rs FastActorSessionV1::step_with_apply_path | One selected action can drive forced transitions before another visible decision. A guard only at the next explicit node may be too late. |

A bounded synthetic probe is running in v4-search-clone-controls-004: select the first seed among1..32 that relabels the Avenging Hunter source in the existing hidden ChooseTargets fixture; require equal tensor/output bits before taking the same legal action in original and sampled sessions; record both responses. This is an engineering counterexample search, not a prevalence estimate or a strength experiment. No result is claimed before the receipt.

Fresh independent design review v4-key-step-design-review-001 is checking the concrete safe next key/consume surface, all relevant historical links, and whether explicit context exclusions suffice or a different sampler is required. Any solution must preserve the information boundary; locking an unknown historical card to its actual hidden position is not a valid shortcut. No search-playing route is enabled.

## Verified witness

Controls-004 completed5/5 checks, invocation275.546 seconds, tests0.08 seconds. All three source hashes match. The first seed,1, relabeled the hidden source while retaining exact root tensor and leaf-output bits. Applying action0 to the original fixture returned a natural P0Win terminal; applying it to the sampled state returned StaleEnvironmentBinding, "prevalidated fast actor action failed internally; episode halted". The diagnostic fixture is not a playing-strength result or a measured tactical gain. The source audit identifies plausible invalid-provenance reads; the wrapped error alone does not localize the precise internal rejection.

**NO-ADVANCE to search-playing integration with this clone unchanged.** Root-visible equality is demonstrably insufficient for step validity. The passing probe preserves this counterexample, not evidence that the sampler is suitable for search. The clone remains diagnostic-only with no playing caller. Evidence `E:/mtg-meta-recovery-20260921/v4-search-clone-controls-004/test.log`; bounded seed search stopped at1 of32. Keep this witness when restricting or replacing the sampling path; do not turn rejected simulations into fabricated losses or silently drop them.
