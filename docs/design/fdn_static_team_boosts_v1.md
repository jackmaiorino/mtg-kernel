# Foundations static team bonuses, source preparation

This bounded batch claims Anthem of Champions and Empyrean Eagle under
[issue #110](https://github.com/jackmaiorino/mtg-kernel/issues/110#issuecomment-6088410424).
Tentative catalog v65 appends IDs 336-337 after the coordinated v61-v64
batches. Source preparation starts at main `5873cda8e`, including the frozen
full booster target. Registration, synthetic deck, profile/version admission,
CI wiring and the generated checksum remain deferred until those predecessors
qualify. No new catalog or gameplay qualification is claimed here.

Printed rules match retained Scryfall FDN records and clean XMage source at
`a5c90fe180021e70e2a644ade00eeab07f857a40`:

| Card | Printing / oracle ID | Printed behavior |
| --- | --- | --- |
| Anthem of Champions, FDN 116 | `42fe3a40-9cbe-4235-86f9-32576aaebba8` / `f1d8e9a6-1903-4be5-8609-1009a463f393` | {G}{W} enchantment; controlled creatures get +1/+1 |
| Empyrean Eagle, FDN 239 | `577e99a7-4a55-4314-8f08-2ae0c33b85c7` / `270d14b2-07bc-46bc-918f-658102265ccf` | {1}{W}{U}, 2/3 Bird Spirit with flying; other controlled creatures with flying get +1/+1 |

Primary XMage implementations are
[AnthemOfChampions.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/AnthemOfChampions.java)
and [EmpyreanEagle.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/e/EmpyreanEagle.java).

The engine recomputes these layer-7c P/T deltas on every existing effective
power/toughness read. A separate controlled-creature table uses an all-creature
or effective-keyword predicate and optional source exclusion. Live source and
recipient zones/controllers, executable definitions, the source's printed
abilities and its face determine eligibility. Multiple sources accumulate;
Eagle excludes only its own current object and can boost another Eagle.

Effective creature type and layer-6 flying are read before the bonus. Keyword
evaluation does not read P/T or this table, so Eagle's predicate cannot recurse
into its own team bonus. Existing timestamp rules decide whether flying grants
survive a later ability-removing Aura. Base-setting creature Auras remain layer
7b; counters and these bonuses are added afterward. Loss of a source or of its
printed abilities removes its bonus immediately, including at the next SBA.

No state, serde, snapshot or public-projection field is added. Existing Dwynen
subtype-lord code and semantic binding remain separate. Existing registered
cards retain their generated recipes; only future Anthem/Eagle names gain
semantic tokens and Eagle's flying selector. The default Pauper build does not
apply this feature-gated team table.

Nine prepared focused regressions cover printed metadata and costs, both seats,
late entrants, noncreatures, reach versus flying, other-only and cumulative
lords, source/recipient control and zone changes, source suppression, flying
grants before/after removal and expiry through real End-to-Cleanup passes,
layer-7b base overrides, lethal damage
after source departure, actual unblocked combat, and pending-cast JSON/snapshot
restore. These tests are unexecuted and require the future registry definitions.
Pinned Rust 1.94.1 formatting and diff checks are the available source checks.
No desktop native run, experiment, paid work or reservation action occurs.

The retained6d6a7782 preparation is now ported onto v64 candidate8603ef7f.
Both P/T insertion conflicts were resolved by keeping the accepted Standard
forest/counter bonuses alongside the new Limited team bonus. Registration
and profile changes wait for PR210 to merge; no accepted coverage changes.

Read-only port review found that the old preparation omitted rules-vector
extraction and the keyed-name inventory. The shared runtime team table is
now exposed internally to the extractor, which records its keyword filter,
source exclusion and P/T delta. Static meaning records controlled creatures
and +1/+1 while the source is on the battlefield. Eagle's effective Flying
predicate/source exclusion are explicitly Opaque where the facet vocabulary
cannot express them. The default non-Limited getter returns no team rule.
Both keyed names are inventoried. This repair awaits native qualification.

Read-only review ofafcbd786 confirms the omission resolved. That exact source
passed Limited-only and combined Standard/Limited test compilation,23
rules-vector cases (one existing ignore), including the source inventory,
and the frozen v64 catalog case under supported guard
15200372022b42bdbf1e4be2456af6e6, command exit0. Its owned idle telemetry
child was released after verifying the completed sequence. All other static
fixture card names were checked against the registry; the two candidate
printings and behaviors were reread from the pinned primary Mage source.
The nine static-team card cases remain unexecuted until registration.

PR210 is now merged at53fab604 after all23 checks passed. Its actual merge
tree matches the reviewed composition and34 affected Python cases pass.
This branch merges that accepted v64 and admits the two pinned metadata rows
as IDs336-337, bumps the generated header to v65, adds the exact40-card
reference import and wires the nine focused gameplay cases into CI.
All35 affected Python3.13.14 cases pass. Candidate coverage is138 full,
one partial and147 missing; accepted coverage remains136 pending native
gameplay, observed generated identity, live-profile qualification and merge.

Sourcefe7ab696 generated catalog identity42bf6f9ca62d6615 in47.93 seconds
under supported guard40956da4c5df4c98847e6214be702705, terminal exit0.
Rust1.94.1/MSVC19.50.35725 and the source SHA are logged. Generated IDs and
both names were checked. FdnStaticTeamBoosts uses this observed live identity,
retains v64 readability and adds rejection before stale resume/publication
mutations. Metadata/deck and planned profile migration review found no
actionable defects. Focused native gameplay and hosted qualification are next.
