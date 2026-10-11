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
timing, and unlocked Room characteristics. Aegis now resolves its copy choice on attachment and returns linked exiled cards immediately when it leaves. Catalog admission includes all
225 nonbasic deck cards. Capability flags are
implementation candidates until the affected checks and public sessions pass.

The new public-session acceptance test resolves all sixteen decks, preserves
the 62-card blue fixture and empty sideboards, plays eight fixed paired-deck
games to natural terminals, and repeats them with identical transcript hashes.
This is rules/harness verification, not a playing-strength experiment.

## Candidate admission and remaining acceptance

The v7 candidate contains all 225 nonbasic deck cards, with 253 definitions appended to
the frozen Pauper prefix. All extension entries have Full flags to enable acceptance
checks, including its 32 token or masked-face definitions. Python checks admit all 16
decks and bind their original SHA-256 values; Mono-U remains 62 cards and sideboards
remain empty. Standard appended IDs are fixed by an explicit Rust list. Registry deck
membership metadata matches the unchanged fixture files.

The source work spans lands/restricted mana/poison; removal, draw and modal selection;
set keywords and free casting; creature triggers/statics and last-known information;
planeswalkers, Rooms, Vehicles, craft, copying and transforming cards; and the five-color
legends. Generic observations represent the added public state and decision boundaries.

**Runtime verification is pending.** The integration lead still owns the combined
compile, affected executable tests, final untap/granted-trigger repairs,
terminal public-session/replay execution, source review and delivery. The native v7 build generator reports `0x020d7a9c4c45b4e2`, now bound by the catalog test. Full flags and Python admission alone are
not a verified support result. No training, benchmark campaign, paid compute or
playing-strength claim is part of this assignment.

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
The local verification core is now admitted. Subsequent checks exposed and repaired missing token/subtype generation, feature guards, new enum matches and stale policy source digests. The generated policy checks pass (15 Python tests); the deck importer checks pass (8 tests). Formatting and diff checks pass. The next native check is running; executable regressions, frozen identity checks, public-session acceptance and current-head CI remain pending.
