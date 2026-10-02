# London mulligans validation status

Schema 4 adds London mulligans to the original fixture gameplay interface.
The source implementation, focused tests and documentation are prepared;
Rust gameplay verification and complete CI remain pending.

Observed checks:

- Pinned Rust 1.94.1 formatting and `git diff --check`: passed.
- `py -3.11 -m unittest python.tests.test_limited_decks_v1 python.tests.test_limited_session_v1`:
  22 passed in 2.166 seconds. These include schema-4 protocol identity and
  refusal checks using a fake transport. They do not prove gameplay.

Pending checks:

- Ten Rust rules/protocol integration tests: announcements before redraw, redraw before
  bottoming, repeated mulligans, forced zero-card keep, exact private bottom
  order, invalid/stale-choice rejection, snapshot/JSON restore and earlier
  schema compatibility.
- Pending bottoming session restore: identical legal menu, binding, response
  and resulting privileged environment hash; invalid action leaves it intact.
- Original unchanged UG/WG through the schema-4 subprocess, seed123 twice and
  swapped seed701. Both players take actual mulligans and bottom four cards in
  total before playing; require natural terminal outcomes and exact replay.
- Required existing regressions, default/Limited/native/CUDA lint and CI.

The initial draft bottomed only when keeping. Corrected that design before
implementation: every mulligan immediately bottoms its current count, then
the next announcement round begins. This follows CR103.5 and the inspected
XMage `LondonMulligan.java` implementation. No new Java mulligan implementation
or playing-strength comparison is claimed.

Both PCs remain reserved for other owners' measurement windows. Jack PID47760
was confirmed live while preparing this batch, and the Haley Q006 owner posted
continued actual activity at13:25EDT. No heavy local/remote kernel or main Java
build was started. Hosted CI supplies initial Rust verification; later bounded
reference checks must preserve those owners and actual release sequencing.
