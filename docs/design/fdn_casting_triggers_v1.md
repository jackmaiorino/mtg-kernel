# FDN casting triggers v1

Balmor, Battlemage Captain ({U}{R}, legendary 1/3 Bird Wizard with flying)
observes its controller casting an instant or sorcery. Its trigger resolves
to +1/+0 and trample until end of turn for the creatures its captured
controller controls at resolution. The existing incarnation-bound team
boost snapshots that set: later entrants receive no earlier effect, affected
creatures retain the effect after control changes, and returning incarnations
do not retain it. Multiple casts create independently stacking effects.

Firespitter Whelp ({2}{R}, 2/2 Dragon with flying) observes its controller
casting a noncreature spell or a Dragon spell and deals one damage to its
opponent. The appended `CastNoncreatureOrSubtype` condition evaluates one OR
predicate, so a noncreature Dragon creates one trigger. Type and subtype
checks use the selected spell form. Supported Adventure/Omen spell faces have
no subtypes and cannot inherit the printed creature subtype. Trigger matching
happens at the committed cast; resolution reuses the frozen source/controller
contract and never rechecks the live spell or observing source.

These recipes follow the pinned local XMage classes
`BalmorBattlemageCaptain.java` and `FirespitterWhelp.java`, and the retained
Scryfall FDN records [Balmor](https://scryfall.com/card/fdn/237/balmor-battlemage-captain)
and [Whelp](https://scryfall.com/card/fdn/197/firespitter-whelp). Existing
instant/sorcery triggers, noncreature triggers, damage and boost effects keep
their meanings. No new effect, state, subtype or receipt enum is added.

This is source preparation for issue110, reserving catalog v66 / IDs338-339
following the serial v61-v65 batches. The current branch appends registry
records to its v60 base temporarily; its native ID assertions deliberately
require the eventual serial IDs. Catalog version/hash, prior store-profile
readability, CI admission and native qualification await those dependencies.
No generated catalog hash or native test pass is claimed.

`fdn_casting_triggers_v1.rs` prepares seven gameplay tests covering exact
metadata and cost, real cast filters, once-only Dragon triggers, Adventure
casts, stacked casts and a countered underlying spell, source departure and
control changes, resolution-time membership, stale incarnations, cleanup,
and full-state restore/replay equality. A unit regression tests the combined
predicate when both terms are true and excludes printed Dragon subtype from
an Adventure face. The synthetic forty-card reference deck has four copies
each of Balmor, Whelp, Firebrand Archer, Think Twice and Lembas plus ten each
Island and Mountain. Python deck checks are source admission evidence only.
