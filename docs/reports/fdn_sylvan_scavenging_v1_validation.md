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
| Broader regressions/default compatibility | Pending. |
| Release publication/resume boundaries | Pending. |
| External natural completion and replay | Pending. |
| Hosted CI | Pending. |

The original deck hashes are unchanged:

- UG: `bb618d6eaddf04b0a9e51e9a88cd512a04635e99ebca91b11ace4c305d634c86`
- WG: `be026f1c86e3aabcb294517188d0c5f4f0cdfa3c5e95ee9dedfc51d3cdf814f7`

The inventory observes UG 38/40 and WG 39/40 supported copies. Its
286-name reference has 41 full, one partial and 244 missing names.
Celestial Armor, Witness Protection and London mulligans remain necessary
for the full original-fixture goal.

Retained local prefixes are `C:/Users/Jack/fdn-scavenging-local-core-001`,
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
