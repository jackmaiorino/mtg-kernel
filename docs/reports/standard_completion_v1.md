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

The candidate contains all 225 nonbasic deck cards, with 254 definitions appended to
the frozen Pauper prefix. All extension entries have Full flags to enable acceptance
checks, including 32 token or masked-face definitions and the existing FDN Witness
Protection fixture for layer-ordering regression coverage. Python checks admit all 16
decks and bind their original SHA-256 values; Mono-U remains 62 cards and sideboards
remain empty. Standard appended IDs are fixed by an explicit Rust list. Registry deck
membership metadata matches the unchanged fixture files.

The source work spans lands/restricted mana/poison; removal, draw and modal selection;
set keywords and free casting; creature triggers/statics and last-known information;
planeswalkers, Rooms, Vehicles, craft, copying and transforming cards; and the five-color
legends. Generic observations represent the added public state and decision boundaries.

**Runtime acceptance remains incomplete.** At `b8d7adfe`, the eleven Standard
integration binaries compiled and executed: 307 tests passed and 40 failed.
All 21 catalog checks passed, including admission of all sixteen unchanged decks.
The first two public-session pairings reached natural terminals and replayed with
identical transcript hashes; the third exposed an outdated linked-exile observation
validator. No complete eight-pair session acceptance is claimed yet.

Repairs now cover linked-exile observation, activation payment and Channel reduction,
copy creation and event-bound continuations, Zoetic animation, Enduring return
provenance, transformed Incubator abilities, Preacher triggers, Case permissions,
foreign permanent spell control, and incorrectly staged test prerequisites.
A combined executable rerun is pending. Review also identified Case/Adventure route
selection and copy-rejection atomicity; those repairs are in progress.
The native build generator reports the updated catalog identity `0xd237d07494e53952`.

## Verification and resources

Rust remains pinned at 1.94.1 (`e408947bf`, LLVM 21.1.8). Correctness verification
uses Haley's permitted cores 13 and 15 through `python/tools/host_slots_v1.py`,
BelowNormal priority, Cargo `-j2`, no incremental compilation and no debug symbols.
The research reservation on the even cores remains untouched. The current guarded
worker is `haley-standard-verify-002`; its target directory is capped at 3 GiB,
with a 60 GiB disk-free floor. It releases its slot after five idle minutes.
The first worker exited before a diagnostic job produced compilation output; that
job has no test result and its replacement uses the same bounded launcher.

At `b8d7adfe`, focused native checks passed 45 card-definition tests, one Cauldron
hash test, five legacy observation hash tests, five typed Standard human observation
tests, two frozen-encoder refusal tests and the Heartfire regression. Five free-cast
preflight tests passed; two missing-card fixtures were repaired with registered cards
and await rerun. The existing 8 Standard deck and 15 generated policy Python tests
pass. CI's frozen Python source drift is repaired by restoring the three original
feature authorities exactly; 20 focused Python checks passed, including Standard
refusal at the existing strict observation boundary. Historical Rust fixture repairs
retain their original behavioral golden hashes. Current-head native checks and CI,
review completion, PR integration and default-branch acceptance remain pending.

Source and tests stay in Git; bulk build caches and logs stay outside Git under
`D:/e-scratch/magezero-standard-completion` and
`C:/mtg-node/codex-standard-completion-20261011` on Haley. One small verification
manifest binds source, archive, toolchain, guard, input seeds and output hashes.
No training, benchmark campaign, paid compute or playing-strength claim is part of
this assignment. Generic observations describe supported Standard state; frozen
Pauper encoders refuse unsupported extensions. Training a new Standard policy remains
a separate research task.
