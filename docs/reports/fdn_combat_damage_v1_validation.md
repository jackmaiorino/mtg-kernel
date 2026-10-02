# Foundations combat damage validation

Source commit `d1772146af6dd9435169ddbd2e5bacee568ef736` adds custom-game
damage choices, trample and priority between first-strike and normal waves.
HaleysPC's matching source is committed as
`029376492fa7ef9be965bf09af8face191d29e8e`; it also includes the earlier
batch B test-helper configuration fix. Both owned worktrees are on their
own branches.

The opt-in binary flag is `--foundations-combat-v1`, Limited JSONL schema 3.
Source, recipient and range identify each damage answer; the public
observation includes prior assignments and the current wave. Frozen
schemas 1/2 and catalog-driven Pauper sessions retain their behavior.
The native scorer vocabulary explicitly refuses the new transport action.

| Check | Result |
| --- | --- |
| Combat rules | 20 passed: arbitrary distributions, trample, marked damage, deathtouch, overassignment, prevention, indestructibility, simultaneous damage, first/double strike, stale incarnations, public partial assignments, clone/JSON restore and binary replay. |
| Session restore | Two passed, including restoration of a pending damage choice's response, action ID, environment binding and next transition. |
| Existing Limited | Batch A 19, batch B 21, priority six and custom session nine passed. |
| Default state/environment | All 28 state tests passed; legacy randomness bytes and exact environment-hash goldens also passed. |
| Lint | Default workspace all-targets Clippy and Limited-feature all-targets Clippy passed with `-D warnings`. Formatting and diff checks passed. Legacy example/test matches account for the appended enum variant. |
| Python | 12 deck-import, six client, eight flat-V2 golden and 13 Pauper-manifest tests passed. |
| External client | Two seed-123 games reached the same natural terminal with identical transcripts: 12 casts, 16 land plays and six damage-range answers per replay. |
| Hosted CI | Pending the combat PR. Batch B's rerun `36933504326` has passed lint and all four Python shards; both Rust jobs remain in progress at this report. |

The external game used a supported constructed mirror, each mainboard
containing 20 Forest, 12 Llanowar Elves and eight Spinewoods Paladin. It
ended after 376 policy steps and 368 physical decisions. This validates
gameplay transport and deterministic damage choices. It does not establish
original UG/WG fixture completion, rules parity with XMage, or playing strength.

Small manifest: CPU only, two Cargo build jobs on HaleysPC; GPU ordinal none;
seed 123. Rust 1.94.1 (`e408947bf`), Cargo 1.94.1 (`29ea6fb6a`), MSVC linker
file version `14.50.35725.0`, Python 3.12.10 remotely and 3.11 locally.
Jack's PC remained reserved for the lead's formal work. No training,
formal measurement or paid compute was launched.

| Input/output | SHA-256 |
| --- | --- |
| Input deck text, UTF-8 LF, trailing LF | `83d08c588941f706db93dd89975ce67536ae3da4e92f373ff497f94a62067bd8` |
| Combat module | `b2157d44ed7a9ec8f03b85b329629101115b6ac1ed625761f1fb49248f399d42` |
| Combat integration tests | `4c5d22613c29c7674b2e0e3489686622a98a0f489fdfaa5456993223fe0c9664` |
| Tested Limited binary | `2d221899be511b9f359ed1f36b2c2a4947bf231c0649cd39437c38fd5ab1861c` |
| External transcript | `855703bce81061336a3ccba3a8d344c05d1c3b9dd16c2a7049dd252ea2640a7c` |
| External result JSON | `13a5bd899adaa87e386bcdf260908fcd0b75bc6d2cca1e76b89588eb07441926` |

Final command logs are `C:/Users/haley/fdn-combat-lint-003.log` and
`C:/Users/haley/fdn-combat-regressions-002.log`. The latter contains passing
regressions followed by the type-complexity lint that was subsequently
fixed; final lint and combat/session checks are in `lint-003`. The external
driver and result are `C:/Users/haley/fdn-combat-external-001.py` and `.json`.
Two additional Torch-dependent Python feature generators could not run in
the local Python 3.11 environment because Torch is absent. No full local
Python or Rust-suite completion is claimed.

The original fixture milestone remains active: UG still resolves 23/40
copies and WG 25/40, with 22 distinct card names outstanding. Mulligans,
remaining cards/tokens, their interaction tests and executable XMage
comparisons remain required. XMage source inspection at
`a5c90fe180021e70e2a644ade00eeab07f857a40` covered `DamageDistributionTest`
and `FirstStrikeTest`; those are reference scenarios, not executed parity
evidence.
