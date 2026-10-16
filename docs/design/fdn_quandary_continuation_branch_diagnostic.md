# Quandary continuation branch diagnostic

Native180 and exact076 retain a genuine Prepared Quandary/Spinner fixture halt:
`InvalidEffectContinuation:source:82`, policy step8. The old executable has no
inner rejection label. This source change adds constant `cfg(test)` stderr
labels only; no guard, error, instruction order, world mutation or fixture
assertion changes. Rust remains uncompiled and unrun for this change.

The initial world has80 Forest cards. `prepared::tests::ready` adds two Forests,
then the first Quandary branch allocates Painful Quandary82, Think Twice83 and
the discardable Elf84. The later Spinner branch creates another fresh world
and allocates Spinner82. The existing `prepared_fixture::quandary_decline`,
`quandary_discard` and `spinner_reveal` phase labels distinguish these worlds;
source82 alone cannot establish which old-executable phase failed.

The concrete structural lead is the boundary between native engine custody
and public continuation validation. `resume_owned_pending_effect` validates
the pending boundary, acquires existing owners and removes its exact resolving
stack row before calling generic `resume_resumable_resolution`. Quandary's
printed trigger selects `requires_printed_optional_trigger_stage`; that public
resume entry immediately runs `validate_pending_effect_choice` again. Its
resolving-row equality requires the row to remain on the public stack. The
same validation gate also covers generic ferocious payment roots. This is a
source-level pre-removal versus post-removal contract mismatch, not proof of
the branch taken in the retained run. Other earlier validators can reject.

The next authorized genuine execution can distinguish:

- `native_continuation::pending_boundary_validation_rejected`: common native
  validation failed before removal.
- `native_continuation::validate::resolving_stack_row_rejected`: exact row
  equality failed inside the public continuation validator.
- `native_continuation::resume::required_choice_validation_rejected`: the
  optional/ferocious public resume gate propagated its validation error.
- `native_continuation::resume::driver_rejected`: native resumption received
  an error after removing the row and restored it before preserving its halt.

The labels expose no source ID, printing, actor, hand, continuation operands or
dynamic error text. They grant no owning authority. A production repair still
needs evidence of the actual rejection and must retain authenticated public
validation plus the genuine native removal/restore contract. No SSH, Cargo,
CI/native launch or observer was run or changed for this diagnostic.
