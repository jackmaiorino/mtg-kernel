# Sylvan Scavenging implementation and verification

Scavenging costs `{1}{G}{G}` and triggers at its controller's end step.
Its controller orders simultaneous triggers, selects a printed mode and
then supplies that mode's targets before priority. Counter mode targets
a controlled creature. Token mode is always selectable and tests current
creature power at resolution, including counters and temporary effects.

The definition owns both branches and their target specifications. The
pending trigger carries the selected branch without adding a hashed field
to every older trigger. An unselected modal root is rejected on the stack.
`ChooseTriggerMode` is appended to engine decisions/actions. The existing
flat mode representation continues to carry source, printed index and mode
count; executable actions distinguish a trigger mode from a spell mode.

FDN v47 is `874200c08d207e29`, with 204 definitions. Scavenging and its
green 3/3 nonlegendary Raccoon token append at IDs 202/203. Tokens cannot
be admitted as physical deck cards. Older profiles remain readable and
must match the live catalog to publish/resume. Default v32 is unchanged.

| Check | Observed result |
| --- | --- |
| Focused rules | Checks-004 phase 1: 21 passed, covering exact cost, mode/target timing, invalid actions without mutation, current power, source/target loss, token entry triggers, ordering and restored choices. |
| Generated definitions | Checks-002 phase 2: 46 passed. |
| Catalog history | Checks-002 phase 3: 141 passed, three existing ignores. |
| Python deck/client | 20 passed. |
| XMage | 12 strict reference cases passed; [Mage PR #12](https://github.com/jackmaiorino/mage/pull/12). |
| All-target lint | Checks-009 exited zero: Limited all-target Clippy passed with warnings denied (39.98 seconds). Exhaustive example/test adapters handle the appended decision. |
| Broader regressions/default compatibility | Checks-010 exited zero: 242 prior rules, 125 Limited engine, 62 Limited session/client, 124 default engine and 27 default RL checks pass. Default all-target Clippy and the frozen v32 golden pass; external binary built. |
| Release publication/resume boundaries | The compute host production-001 final exit zero at d85d3251: 2 pre-FDN, 28 prior-FDN and 1 round-trip checks pass; combined Limited/native-production release library Clippy passes. The subsequent public fix changes only the raw benchmark adapter. |
| External natural completion and replay | Two identical fixed-seed custom-deck games reached natural P1 wins at 366 policy steps each. Each cast Scavenging twice and selected counter mode three times and token mode four times; transcript SHA-256 `d663787890f082082ecafe38774c4d588e4f01b674ff152e57721158735080b1` matches. These smoke decks are distinct from the still-incomplete original fixtures. |
| Hosted CI | [Run 37025035376](https://github.com/jackmaiorino/mtg-kernel/actions/runs/37025035376) at `7d4c55186e09af803969e913114f1e246f070737` passed all eight checks. Windows Rust finished October 2 at 19:20:19 UTC; Ubuntu Rust, lint and all four Python shards also passed. Later local commits change reports only. |
| Windows full-log audit | Job `110897892556` has positive test summaries for all 18 Limited integration targets selected by its exact source workflow: 290 passed in total. The complete log has no failed test summaries or native compile-error markers. This audit checks the actual results behind the historical multiline PowerShell step. |

The original deck hashes are unchanged:

- UG: `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86`
- WG: `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7`

The inventory observes UG 38/40 and WG 39/40 supported copies. Its
286-name reference has 41 full, one partial and 244 missing names.
Celestial Armor, Witness Protection and London mulligans remain necessary
for the full original-fixture goal.

Retained local prefixes are `C:/Users/user/fdn-scavenging-local-core-001`,
`fdn-scavenging-local-rules-001/002` and `fdn-scavenging-checks-001` onward.
Manifests bind source, toolchains, inputs, outputs and storage limits.
Rust/Cargo 1.94.1, MSVC 14.50.35725.0, two Cargo jobs, no incremental
compilation/debug symbols, seed 123, no GPU. Checks project 2 GiB and
enforce a 4 GiB volume allocation cap plus a 60 GiB reserve.

All failed preparation logs remain: wrong empty-keyword API, phase-scoped
response mana, old expected definition count, premature stack-validation
assertion and missing exhaustive adapter arms. The benchmark adapter's
syntax correction is committed. An attempted source-fix stop found the
owned launcher already terminal; no other process was stopped.

These are bounded rules checks, with no full-set or playing-strength claim.

External receipt: `C:/Users/user/fdn-scavenging-external-001.json` and
its small manifest. Binary SHA-256
`e01a810e6b0e438520d843a9389014a10395ad9d53684e0cf6df5657983b4c27`,
5,848,064 bytes, preserved on E, a separate C copy and a hash-verified compute host copy. Both games use seed
123 and episode 7. The external launch checked binary-copy reserves and
bounded steps but lacked a live output-byte guardian; its manifest records
that limitation and was written after completion. Future launches use the
existing guarded launcher. No substantial simulation or GPU work occurred.

## Current-main integration

The Scavenging prefix incorporates Voyage 5b3fa367 and main's 192-definition/v34
catalog. Its generated Limited identity is 234 definitions/v49
592f678756e75cf5. Original and earlier composed catalog tuples remain readable,
with mutation restricted to the live identity. Existing card, token, modal and
restore scenarios remain in the grouped hosted checks. Catalog generation,
formatting, workflow lint and diff checks pass locally; composed-source Rust
qualification is pending.
