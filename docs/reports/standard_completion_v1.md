# MageZero Standard completion

Assignment: finish engine support for all sixteen unchanged decks in
`data/standard/magezero_v1/decks`. The baseline at `ddff546a` admits 91 Full,
21 Partial and 113 missing distinct nonbasic cards, and zero complete decks.

Acceptance requires all 225 distinct nonbasic deck cards to have complete
implemented behavior, all sixteen deck files to resolve, executable tests for
the added choices and interactions, and deterministic terminal gameplay through
the public session interface. Preserve the 62-card blue fixture and empty
sideboards. Passing inventory alone is insufficient. Existing frozen Pauper
and FDN identities and checkpoint interfaces remain unchanged.

## Integrated implementation

The canonical draft PR is #218, tracking #217. The change integrates the older
E/F source and implements the remaining B/C/D/G/H cards, Vehicles and crew,
loyalty X, ninjutsu, disguise, discover, hideaway, alternative cast forms,
resolution-time casting, linked exile, restricted mana and effective Room and
transform characteristics. All original deck files are preserved.

The generic public interface now carries the new payment and effect choices,
poison/counter state, source links and effective identities. Suspended parent
resolutions retain their incarnation-bound references, defer triggers until
resolution finishes, and resume below spells cast during that resolution.
Frozen policy encoders refuse unsupported Standard extensions instead of
dropping them. A separate Standard policy training task is outside this scope.

Source review found and repaired missing Phyrexian payment alternatives,
Blue Sun's Twilight target admission without taxes/reductions, Djeru's free-cast
timing, and unlocked Room characteristics. Aegis/Cauldron final repairs and the
catalog admission reconciliation remain in progress. Capability flags are
implementation candidates until the affected checks and public sessions pass.

The new public-session acceptance test resolves all sixteen decks, preserves
the 62-card blue fixture and empty sideboards, plays eight fixed paired-deck
games to natural terminals, and repeats them with identical transcript hashes.
This is rules/harness verification, not a playing-strength experiment.

## Verification and resources

Rust toolchain remains pinned at 1.94.1. The root queues one combined BelowNormal verification core through
`python/tools/host_slots_v1.py`, with Cargo `-j1` and its cache on D:. Earlier
worker checks were canceled to avoid duplicate compilation. A bounded local
verification process releases its slot after five minutes without another
check; it is not a recurring automation. Existing host reservations remain binding. Root scratch is
`D:/e-scratch/magezero-standard-completion`; its code/build logs are reproducible,
with a 20 GiB cap and at least 60 GiB free reserved on D:. Current tests are
pending, not passed. Source references and tests stay in Git; bulk caches do not.

Generic public decision observations must describe the supported rules state.
Frozen Pauper policy encoders may refuse new Standard state rather than silently
drop it. Training a new Standard policy is a separate research task.

The first combined check failed in the build script on duplicated cost/program
variants and a stale helper call. Those integration defects are fixed in source.
The next check is queued behind existing reservations. Formatting and diff
checks pass; native compilation, executable regressions, frozen identity checks,
public-session acceptance and current-head CI remain pending.
