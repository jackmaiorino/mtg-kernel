# Controller instant/sorcery cost reducers, source preparation

This separate issue #110 preparation follows the 31-name admission. Neither
Mocking Sprite nor Archmage of Runes is registered or accepted by this work.

The pinned Mage constructors at
[a5c90fe1](https://github.com/jackmaiorino/mage/tree/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards)
define Mocking Sprite as 2U 2/1 Faerie Rogue with Flying and Archmage of Runes
as 3UU 3/6 Giant Wizard. Both reduce controller instant/sorcery spells by one
generic; Archmage also draws one on each instant/sorcery cast. Both primary
constructors were read at that commit.

The casting design must determine one complete spell cost: selected base,
additional mana, kicker, Spree, X and increases, then reductions and one floor.
Spell-face types determine modifiers for Adventure/Omen and Bestow. Alternative,
flashback, Escape, Madness and Plotted costs must use the same planner. Freeze
the payment before a sacrifice can remove a reducer, pay mana once, and preserve
nonmana costs, Delve/Convoke, source reservations and one combined life budget.
Activation and resolution payments remain outside spell modifiers.

Source 8c3b9914 implements the u32 combined mana total without truncating at 255.
Both Limited-only and Standard/Limited test compilations and all 15 mana tests
passed under guard ab5c3985b65b40339b8c63a979a7eabd, two cores at BelowNormal,
exit 0, Rust 1.94.1 and MSVC 19.50.35725. Read-only review found no defects.

Source 02dc1469 collects every selected casting route and original nonmana
component groups. Six collector tests use existing registered cards. Native27
is queued under guard 7eb5daccdc9848b68f8cc430b0010f2b, supervisor 51012, with
the existing observer retained. At 20:00 UTC eligible cores were occupied by
CI and Claude's fast-forward claims; no duplicate or raw launch was made.

Source 5c231a1c binds Flying, Archmage's existing cast-draw trigger, static
fingerprints, rules-vector extraction and live modifiers. Sources must be
executable front-face battlefield objects, currently controlled by the caster,
with printed abilities active. Raw Standard modifiers combine before flooring.
Read-only review found no actionable defects; native qualification is pending.

Source 64607708 prepares immutable Delve payment from the complete adjusted
total, preserving minimum-exile and oldest-first choices while excluding
reserved objects. It also sums mandatory life costs with Phyrexian payment and
preserves zero-life payment at negative life. Three new regressions cover these
risks. Read-only review found that post-solve life checking could reject a legal
mana allocation. Repair 41a74348 puts the remaining life budget inside pip
backtracking and adds the exact Aquifer/Swamp regression. Review then found
generic payment outside the pip search could miss a legal Phyrexian payment. Repair 5c761ec8 includes generic payment at the search's
terminal condition with isolated failed-branch state and a taxed-spell
regression. Exact read-only review of 5c761ec8 found both findings resolved
and no further actionable defects. Native execution is pending.

Convoke source 4bb578e0 solves ordinary mana and creature taps together against
the already adjusted total. Physical source aliases prevent double payment;
partial colored/Hybrid/Phyrexian coverage and colorless generic payment retain
pip requirements. Multi-yield mana dominates an aliased one-unit Convoke option
for generic payment. Planner indices are remapped before returning tap lists.
Primary ConvokeAbility.java was read at the pinned Mage commit above. Review
caught cached creature colors bypassing Aura overrides. Repair a6f96cc3 uses
object_color_mask and tests Witness Protection with a stale-incarnation check.
Exact repair review found no further defects. Six new Convoke regressions and
all newer payment changes remain unexecuted.

Source 3e5bf0ca extracts immutable selected-component choice validation and
reserved tap sources from legacy payment without changing its checks or order.
Two focused regressions cover invalid sacrifice choices and tap/mana source
sharing. Exact read-only review found no defects. Native execution is pending;
this is not yet complete nonmana preflight or casting-route integration.

Repair 892e828f preserves the explicit Alternative-cast requirement that at
least one creature convokes. Colored payment backtracks when floating mana
would leave no convoker; generic payment tries each available convoker on
isolated state, preserving another creature's multi-yield mana when needed.
Two new regressions cover these cases. Exact read-only review found the named
defect resolved. The future shared planner may change the legacy helper's
deterministic maximum-creature tap selection; integration must account for it.

At approximately 20:37 UTC Haley's core-slot and whole-host status both showed free. Source
892e828fc8e438c0298a8f1e8eca6c3ae3590e53 was transferred in a SHA256-checked
bundle (6cc834a30225f2bfbdb3681da14cb4cf7fa6ca4e4a1a7c817e1565c3bd9d5341)
to a new owned checkout, C:/mtg-node/codex-fdn-spell-costs-20261010.
Supported guard cb559b02c0d440fa9c4393847d110c82, supervisor156816,
admitted cores14-15 at BelowNormal priority. The actual command log confirms
Rust1.94.1/MSVC19.50.35725 and compilation started. This small correctness
sequence checks both feature configurations, mana/collector tests, rules
vectors, frozen v65 and the prior graveyard/static-team games. It is pending;
no newer source tests or card games are claimed as passed. Jack's older
Native27 queue and observer remain intact. The later local Native28 command
has not been submitted.

Remaining work: offer/pending/X/final-payment integration;
atomic nonmana payment; actual card gameplay and restore tests; metadata and
catalog admission; live profile qualification; CI and default-branch acceptance.
No new card gameplay, increased coverage or experimental result is claimed.
