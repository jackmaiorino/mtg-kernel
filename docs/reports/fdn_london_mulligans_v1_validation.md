# London mulligans validation status

> October 5 integration status: canonical kernel PR #140 preserves this
> report and its source history. Fixture source `92881658` has passed both full
> Rust matrices and all four Python shards. Its current-main composition needs
> fresh CI and exact-head review/merge. Earlier pending/failure statements below
> refer to their named historical sources. See
> [current fixture validation](fdn_fixture_gameplay_v1_validation.md).

## Current composed source

The source incorporates main fe479186 and the complete original fixture
catalog (236 definitions/v51, `bd1385731e43c4a1`). Both unchanged decks resolve
40/40. Composed-source gameplay, replay/restore and full hosted qualification
remain pending; the older isolated-stack results below do not qualify it.

CI37088170810 at2397921e failed default Clippy because main's V6 observation
initializer omitted London's public projection and its human prompt consumers
did not cover the new action variants. Both observation versions now use the
same public mulligan projection. The actor match covers London choices while
the Pauper human prompt retains its refusal of custom-game choices. The new
`policy_v5_and_v6_preserve_london_public_state_and_own_hand` case checks announce,
bottom and complete phases for both observers, exact own-hand identities, and
omission in legacy games. Hosted verification of the repair remains pending.
Local Rust 1.94.1 formatting, diff checks and all 22 focused Python deck/session
tests pass after the repair; they do not establish Rust gameplay qualification.

## Historical qualification

Schema 4 adds London mulligans to the original fixture gameplay interface.
The focused Rust rules, original fixture games and pending-session restore
passed on Ubuntu and the compatible Windows PR140 step. The final XMage suite passed146 cases in16 classes,
including 16 Witness, 7 London and 28 combat comparisons. At publicd43d59a2,
formatting/all-feature lint and all four Python shards passed. Windows job
111036127744 at PR140 source 36d54594 completed both London commands under Bash
at 22:12:10 UTC. Its engine, cards, original decks and London tests are unchanged
from d43d59a2. Bash propagates either command's failure, resolving the historical
PowerShell wrapper's final-command-only ambiguity. The original games, exact
replay and pending pregame restore are qualified on both operating systems.
The full Ubuntu job 110978361641 completed at 21:11 UTC. Its complete log confirms
all 21 selected Limited integration targets and345 passing cases, including
all 11 London cases and24 Witness cases. The library restore, 1706 default
library tests, Limited library regressions and host-safe CUDA commands passed.
The 43 default and 7 CUDA ignored tests are existing platform/device exclusions.
Full Windows and CI-follow-up verification remains pending.

The first hosted attempt, CI37041270977 at87db2401, found an unused import in
the new module. Removed it; the original lint log is retained. This is a lint
failure, not a verified rules result. The corrected-source results are above.

The same attempt's Ubuntu rules test executed11 cases:10 passed,1 failed.
The unchanged original UG/WG subprocess case passed: seed123 ran twice with
identical transcripts/natural terminals, swapped seed701 reached a natural
terminal, and each game performed four actual bottom-card selections. The
remaining failure expected `Main1` immediately after upkeep. The skipped
first draw still has a draw-step priority window. Correct the test to pass
both windows and assert the hand stays seven at each. The library/session
restore test did not execute because the integration command failed first.

Observed checks:

- CI37049183922 at d43d59a2: seed123 produced two identical complete receipts,
  each 545 requests with 538 physical and 544 policy decisions, natural P0 win,
  and transcript SHA-256
  `02bfc4448318cc2eba465ef9258ff5b7c0db191f304469cd73f6d422bef7d695`.
  Swapped seed701 used 492 requests with 479 physical and 491 policy decisions,
  natural P0 win, and transcript SHA-256
  `f7161673bf17ea48ae29ac1e126cb8d1809c910dccb11403eafaeada8ba8aa02`.
  All three receipts bind both original deck SHA-256s and four actual bottom
  choices. The complete current-head Ubuntu Rust job passed. Windows gameplay
  and restore passed under PR140's compatible Bash step; its full job is live.
- CI37042426390 at0a876ce3: Ubuntu's London step passed both commands, covering
  all 11 integration cases and the pending-bottom-menu session restore.
  Witness's24 focused cases also passed in the same job. All four Python
  shards passed. The Windows London step is still active.
- The same attempt's all-target lint found missing exhaustive arms in the
  historical trace walkers. The131a56f7 repair handles London decisions in
  benchmark policies and explicitly refuses them in frozen trace consumers;
  it also updates existing regression drivers. No old reset enables London.
  Pinned formatting and diff checks pass; hosted validation of this repair
  remains pending. The Ubuntu full-suite failure identifies the same missing
  benchmark decision/semantic arms, covered by that repair.
- Hosted Mage run37044173670 atd9536815d58: all16 Witness and7 London reference
  cases passed with zero failures, errors or skips. Maven3.9.9 / Temurin23.0.2+7
  built from source in04:01minutes. Source Git blobs, downloaded output hashes
  and both XML reports were verified. Retained source:
  `E:/mtg-fdn-fixtures/fdn-mage-witness-hosted-001`, with an independent verified
  mirror at `C:/Users/user/fdn-mage-witness-hosted-001-sealed`.
- Mage run37049882555 at 5cc4decd8ffe passed all 146 cases in16 classes, including
  the four new Foundations allocation positions and24 existing combat cases.
  All source/output hashes and XML counts were independently verified. Sealed
  evidence: `E:/mtg-fdn-fixtures/fdn-fixture-final-reference-002`; independent
  mirror: `C:/Users/user/fdn-fixture-final-reference-002-sealed`.
- Pinned Rust 1.94.1 formatting and `git diff --check`: passed.
- `py -3.11 -m unittest python.tests.test_limited_decks_v1 python.tests.test_limited_session_v1`:
  22 passed in 2.166 seconds. These include schema-4 protocol identity and
  refusal checks using a fake transport. They do not prove gameplay.

Pending checks:

- Complete Windows regressions and Windows-only native publication/resume
  canaries. London rules, original games, exact replay and pending restore have
  passed on both operating systems.
- Complete PR140's default/Limited/native/CUDA tests, including execution of
  every grouped library filter and the unchanged isolated timing gate. Its
  all-feature lint and all four Python shards have passed.

The initial draft bottomed only when keeping. Corrected that design before
implementation: every mulligan immediately bottoms its current count, then
the next announcement round begins. This follows CR103.5 and the inspected
XMage `LondonMulligan.java` implementation. No new Java mulligan implementation
or playing-strength comparison is claimed.

Both PCs remain reserved for other owners' measurement windows. The maintainer PID47760
was confirmed live while preparing this batch, and the compute host Q006 owner posted
continued actual activity at13:25EDT. No heavy local/remote kernel or main Java
build was started. Hosted CI supplies Rust and bounded reference verification;
all subsequent local checks preserve those owners and actual release sequencing.

## Current-main integration

London incorporates Witness 226929f5, including main fe479186. It adds no card
or catalog definitions: the default 192-definition/v34 and Limited
236-definition/v51 bd1385731e43c4a1 identities remain. The public observation
preserves London's pregame fields and main's shared combat projection.
All 21 fixture integration targets, the original fixed seeds, exact replay
and pending-bottom restore are retained. Local formatting, workflow lint,
Python deck/session checks and diff checks pass; final composed-source
Linux/Windows gameplay and regression qualification is pending.
