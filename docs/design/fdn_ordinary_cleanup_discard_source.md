# Ordinary cleanup ordering regression

Original regression based on38832ef; the transient production repair below is
now present. Rust compilation and execution are UNRUN.

The legal initial scene has nine active-player hand cards, including registered
Full Involuntary Employment410, four untapped Mountains, and opposing registered
Full Firebrand Archer261. The real spell is announced, selects its issued legal
target and pays four lands before publication. Its resolution leaves eight hand
cards and genuinely transfers the same Archer incarnation to the active player.
Ordinary turn advance then reaches the issued one-card discard. CR514.1 requires
that player to retain control through that discard; CR514.2 expires the control
effect afterward. The test checks both seats, the exact hand/generation/rosters,
and actual discard ZoneChange before the genuine cleanup/expiry history rows.

The original source called temporary_control_v1::expire_at_cleanup before it
staged the hand-size discard in engine.rs. The original test expresses a source-
identified regression, not an observed runtime failure. Initial lands, hand and
creature are setup, not
claimed cast payments for those setup objects. No synthetic control effect,
mana pool, public priority or post-resolution state is supplied.


The production repair uses a distinct current ordinary-cleanup state. The real
End-step completion stages DiscardResume::None before expiration, then performs
cleanup expiration, fixed-point SBAs and pending-trigger collection. Any SBA or
waiting trigger grants cleanup priority; passing on an empty stack starts
another full cleanup, including a fresh hand limit check. A paid Clockwork/Growth
scene covers death, priority, a new Growth, repeated expiration and native restore.

Current turn, active player, End-step index, phase, stage index and outstanding
hand incarnations are hashed while active. A scalar history-only stage marker
binds the phase sequence; it retains no objects, hands or private worlds. The
hand binding is cleared after discard and all current state is cleared at finish.
Only current bound hand slots are protected from resampling. Genuine paid
sampling/restore tests cover lawful unknown slots before and after cleanup,
phase/turn/hand/discard and erased-owner forgeries, plus the existing immutable
validation frame. Time Stop retains its separate replay-bound disposal journal;
an actual paid Time Stop during ordinary cleanup closes the scalar ordinary stage.

The rejected intermediate full-World ordinary journal design is preserved on
codex/ordinary-cleanup-order-20261016 at 41c1850a8 and is not part of this repair.
It would have blocked ordinary sampling after every first cleanup. No such
historical private snapshots or blanket history refusal remain in this branch.
The new event kind has no harvested gameplay sample yet. The original 237 samples and all
old event/decision/action enum prefixes remain unchanged. The new module is an
explicit 41st ordered input of the existing semantic source digest, which remains
bound once by each of 99 producer arrays. Local Rust execution remains prohibited;
source checks and formatting do not establish runtime or Full qualification.

The actual BeginningEndStep marker now also exists in the default catalog, so
the shared ordinary transition has a real entry there. Limited and Standard
retain their prior marker and capture order. Direct low-level step-entry tests
keep their original primitive behavior; the two genuine production completion
routes use the new cleanup owner. The inherited Hatcher inventory assertion is
corrected from486 to the actual487 definitions, without changing admission checks.

A follow-on source review found that a forged scalar Finished suffix plus an
erased owner could otherwise bypass generic Growth expiration after the real
discard. Exposed absent-owner Finished state still naming this exact current
Cleanup/turn/player/End entry now refuses. The genuine Finished transition stays
internal until native advance leaves Cleanup. GameOver keeps its native owner;
erasing it also refuses. The paid Growth/discard sampling group covers both
forged terminal suffixes, snapshot restoration and read-only native/sampler
refusal, while retaining its positive next-turn replay and lawful sampling.
