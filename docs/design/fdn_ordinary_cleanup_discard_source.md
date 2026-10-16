# Ordinary cleanup ordering regression

Source-only fixture based on38832ef. Rust compilation and execution are UNRUN.
No production engine behavior changes in this checkpoint.

The legal initial scene has nine active-player hand cards, including registered
Full Involuntary Employment410, four untapped Mountains, and opposing registered
Full Firebrand Archer261. The real spell is announced, selects its issued legal
target and pays four lands before publication. Its resolution leaves eight hand
cards and genuinely transfers the same Archer incarnation to the active player.
Ordinary turn advance then reaches the issued one-card discard. CR514.1 requires
that player to retain control through that discard; CR514.2 expires the control
effect afterward. The test checks both seats, the exact hand/generation/rosters,
and actual discard ZoneChange before the genuine cleanup/expiry history rows.

The current source calls temporary_control_v1::expire_at_cleanup before it stages
the hand-size discard in engine.rs. The new test therefore expresses a source-
identified regression, not an observed runtime failure. The shared cleanup repair
is a separate assigned lane. Initial lands, hand and creature are setup, not
claimed cast payments for those setup objects. No synthetic control effect,
mana pool, public priority or post-resolution state is supplied.
