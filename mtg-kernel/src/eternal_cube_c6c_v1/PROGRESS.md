Call 2: Narset uses appended planeswalker subtype ID 720. Karn reuses the
existing Karn Construct Token; no token IDs are allocated.

Rules tests are prepared for the coordinator. Compilation, test execution,
and the public-session test are deferred as instructed.

Call 5: Garruk reserves planeswalker subtype ID 721. Its new token row,
Garruk Beast Token, is appended to the registry (card IDs are build assigned).
Chandra remains no_effect: resolution casts from exile are necessarily free;
the paid resolution route only supports Bilbo's graveyard spell types.

Garruk is implemented and registered Full. Entry observations use the single
commit and completed-batch hooks; the draw is a direct root sequence element.
The temporary attack ability is player-owned, excludes attacks on planeswalkers,
and expires through the actual begin-turn hook (no round-valued turn key).
Its queued triggers drain even for an empty event batch, covering vigilance.
No approximation was introduced. Analogs: Garruk's Uprising for entry sampling,
Petradon for land targeting, Ravenous Baloth for the Beast's base characteristics,
and Teferi, Time Raveler for expiration at the controller's next turn.

The public-session test now covers all six Full walkers using two 60-card decks,
both seats and both policies: 12 games plus 12 exact-seed transcript replays.
Saheeli and Chandra stay out because they remain no_effect. Native tests include
entry counters/statics, dredge, batches, departed sources, land target selection
and incarnation checks, token characteristics/trample, captured attackers,
vigilance, stacking grants and expiration across real turns.

Read-only review found and corrected the empty-event trigger drain and two test
assumptions (optional activation target decision kind and trample allocation).
Rustfmt and git diff --check are the only executed validation. Compilation,
native tests and public-session execution await the coordinator; no runtime
success or session error is claimed. All changes remain uncommitted.
