# Foundations legend-rule validation

Source commit `eedceb932234b35097eb6f247bcdbdd8b165d837` adds resumable
legend-rule choices and completes Dwynen, Gilt-Leaf Daen. Matching source
on HaleysPC is committed as `f725389743cd7209877e5dc5bee046e9305d80e6`.
Follow-up source `a8ab51268211cccd601c59ab93414241588b6b53`
(remote `dc92aa0536b5451d490b731d004602afe055157b`) verifies refusal of
the newly superseded v35 catalog at the actual publisher and resume boundaries.
All nine production boundary checks passed.

Every controller chooses a survivor before any simultaneous legend-rule
moves commit. The continuation binds exact incarnations and preserves
other applicable SBAs, collected triggers and the chosen APNAP prefix.
A lethal survivor still dies in that pass; removing Dwynen can make another
Elf die in the next pass before priority. Indestructibility does not prevent
legend moves, and they go to the owner's graveyard without sacrifice.

| Check | Result |
| --- | --- |
| Legend mechanics | 15 passed, including APNAP, three copies, separate names/controllers, invalid and stale actions, real casts, simultaneous lethal/losses, secondary lethal, waiting entry triggers, JSON restore and public action identities. |
| Existing Limited integration | All 94 passed: combat cards 19, fixture A 19, B 21, combat 20, priority six and custom session nine. |
| Session restore | All three library checks passed, including pending legend choice, action identity, response/environment binding and identical next transition. |
| Definitions and catalog records | All 46 definition and 130 run-record tests passed; three existing run-record tests remain ignored. |
| Default compatibility | All 28 state tests and the exact environment-hash golden passed; the native action-slice rejection test preserves destination buffers. |
| Lint and build | Default workspace and Limited-feature all-target Clippy passed with warnings denied. Combined production and Limited library release Clippy, formatting and the external binary build passed. |
| Production catalog boundaries | Nine release checks passed under combined Limited/production features: eight v32/v33/v34/v35 publisher/resume refusals and the v36 publish/read/validate round trip. The refactored v35 decoder check also passed. |
| Python | 40 passed: deck import 13, client six, flat-V2 goldens eight and Pauper manifest 13. |
| External client | Two identical seed-123 replays ended naturally; each cast Dwynen seven times and answered five legend choices. |
| Hosted CI | Pending this PR. Parent #120 passed lint and all four Python shards; its Ubuntu/Windows Rust jobs remain in progress. |

External check: both decks contain 20 Forest, eight Llanowar Elves and 12
Dwynen. Episode 7 ended in `p0_win` at
406 policy steps and 401 physical decisions. The driver prioritizes
lands and spells, attacker/blocker inclusion, the first legend survivor,
lower-half damage choices and passing. This proves transport and deterministic
replay for this constructed check, not original-deck completion, executed
XMage parity or playing strength.

The Limited catalog is v36 `3d41aa36a5a75d8f`, with all 181 definitions full
(including two tokens). Default v32 remains `64c82a261e078f1a`; earlier
v33, v34 and v35 identities retain their literals and read compatibility.
States without pending legend choices retain prior serialized bytes/hashes.
The native fixed action vocabulary explicitly refuses the new legend action.

Small manifest: CPU only on HaleysPC, two Cargo build jobs, GPU ordinal none,
seed 123, episode 7. Rust 1.94.1 (`e408947bf`), Cargo 1.94.1 (`29ea6fb6a`),
MSVC linker `14.50.35725.0`; Python 3.12.10 remotely and 3.11 locally.
Jack's PC remains reserved for the lead. No training, formal measurement or
paid compute was launched. Logs, driver and result remain outside Git.

| Input/output | SHA-256 |
| --- | --- |
| Input deck, UTF-8 LF with trailing LF | `ca7a2a98054dbe4ef2d4d2c4595dc2d5429a200f519a1d244194ea66240173be` |
| Base registry | `af9d725658d238a6a1b0c99b52c12ff6823bdcaf8147a77556ee2e3b0b67d6bc` |
| FDN extension | `dbcecc09454c3d2bd4f464b5fb113965446a24f2f456353f4c4f502ff669acd6` |
| Legend implementation | `bedcfb95e9e60465d2f2e44c6be31d1fa9147321b6592e4cf2fb7782df8f277a` |
| Legend integration tests | `bb944ab63c2b73afb505281c10add4220a058562790ae0339296603fe06cadd4` |
| Tested binary | `099db7d0ac04a4b8e8b3308f808a5e5a0afce88f6d61b015c51f0cef524ec707` |
| External transcript | `9991b5edc3cc5ba1b786a371f2e1d2fc929582ec53cbffbc684033cc89a9e726` |
| Result JSON | `bbc714dd446caddf6ad44e089364cbf5439646460f787b890076812d686e643a` |

Validation logs are `C:/Users/haley/fdn-legend-tests-006.log` through
`-012.log`. Earlier failed logs remain: 006 found an obsolete hash assertion,
007 found a redundant capability assertion, and 008 requested formatting.
010 refused the uncommitted compatibility tests under the production source
guard; they were committed before 011. Passing checks and the corrected
checks are recorded above; 009, 011 and 012 have exit zero. The external
driver/result are `fdn-legend-external-001.py` and `.json` in the same remote
directory, with exit zero. The result retains the
binary hash before later builds. Full local suites are not claimed.

The full fixture goal remains active: UG and WG each resolve 26/40 copies,
with 19 distinct fixture names unsupported. Mulligans, remaining cards and
tokens, executable XMage comparisons and passing CI remain required. The
286-name reference has 24 full and 262 missing entries. The pinned original
deck files remain unchanged. See [legend contract](../design/fdn_legend_rule_v1.md)
for the official rules reference and the implementation boundary.

## Rebase onto main (2026-10-02)

After merging main (#112, `kernel_carddb/v34`, 192 Pauper definitions), the
Limited database is `kernel_carddb/v38`, hash `d85648876370d1e2`, under the
`FdnLegendRule` profile. The measurements above describe the original build.
