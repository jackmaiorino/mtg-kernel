# London mulligans validation status

Schema 4 adds London mulligans to the original fixture gameplay interface.
The focused Rust rules, original fixture games and pending-session restore
passed on Ubuntu. Windows verification, complete CI and the XMage reference
execution remain pending.

The first hosted attempt, CI37041270977 at87db2401, found an unused import in
the new module. Removed it; the original lint log is retained. This is a lint
failure, not a verified rules result. Await the new source's CI results.

The same attempt's Ubuntu rules test executed11 cases:10 passed,1 failed.
The unchanged original UG/WG subprocess case passed: seed123 ran twice with
identical transcripts/natural terminals, swapped seed701 reached a natural
terminal, and each game performed four actual bottom-card selections. The
remaining failure expected `Main1` immediately after upkeep. The skipped
first draw still has a draw-step priority window. Correct the test to pass
both windows and assert the hand stays seven at each. The library/session
restore test did not execute because the integration command failed first.

Observed checks:

- CI37042426390 at0a876ce3: Ubuntu's London step passed both commands, covering
  all11 integration cases and the pending-bottom-menu session restore.
  Witness's24 focused cases also passed in the same job. Both Ubuntu Python
  shards passed. The Windows London step is still active.
- The same attempt's all-target lint found missing exhaustive arms in the
  historical trace walkers. The131a56f7 repair handles London decisions in
  benchmark policies and explicitly refuses them in frozen trace consumers;
  it also updates existing regression drivers. No old reset enables London.
  Pinned formatting and diff checks pass; hosted validation of this repair
  remains pending.
- Pinned Rust 1.94.1 formatting and `git diff --check`: passed.
- `py -3.11 -m unittest python.tests.test_limited_decks_v1 python.tests.test_limited_session_v1`:
  22 passed in 2.166 seconds. These include schema-4 protocol identity and
  refusal checks using a fake transport. They do not prove gameplay.

Pending checks:

- Windows Rust rules/protocol integration tests: announcements before redraw, redraw before
  bottoming, repeated mulligans, forced zero-card keep, exact private bottom
  order, invalid/stale-choice rejection, snapshot/JSON restore and earlier
  schema compatibility.
- Windows pending bottoming session restore: identical legal menu, binding, response
  and resulting privileged environment hash; invalid action leaves it intact.
- Windows original unchanged UG/WG through the schema-4 subprocess, seed123 twice and
  swapped seed701. Both players take actual mulligans and bottom four cards in
  total before playing; require natural terminal outcomes and exact replay.
- Required existing regressions, default/Limited/native/CUDA lint and CI.
- Existing XMage `LondonMulliganTest` and16 Witness reference cases: focused
  hosted Mage workflow37044173670 is active atd9536815d58. Its Java23.0.2 /
  Maven3.9.9 source build preserves both local PCs' actual reservations.

The initial draft bottomed only when keeping. Corrected that design before
implementation: every mulligan immediately bottoms its current count, then
the next announcement round begins. This follows CR103.5 and the inspected
XMage `LondonMulligan.java` implementation. No new Java mulligan implementation
or playing-strength comparison is claimed.

Both PCs remain reserved for other owners' measurement windows. Jack PID47760
was confirmed live while preparing this batch, and the Haley Q006 owner posted
continued actual activity at13:25EDT. No heavy local/remote kernel or main Java
build was started. Hosted CI supplies Rust and bounded reference verification;
all subsequent local checks preserve those owners and actual release sequencing.
