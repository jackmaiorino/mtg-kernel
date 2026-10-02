# FDN life-gain creatures

Exemplar of Light (ID 188) and Sun-Blessed Healer (ID 189) append to the
Limited feature. The original UG and WG fixtures retain their exact bytes.
The extension contains 190 definitions, with Ajani still partial.

| Card | Behavior |
| --- | --- |
| Exemplar of Light | `{2}{W}{W}`, 3/3 Angel, flying. Each positive life-gain event gives one incarnation-bound counter trigger. Its controller placing one or more counters on it gives one draw trigger per turn. The limit is consumed when the ability triggers, survives control changes and restore, and resets for a new incarnation or turn. |
| Sun-Blessed Healer | `{1}{W}`, 3/1 Human Cleric, lifelink, kicker `{1}{W}`. A kicked entry targets a nonland permanent card in its controller's graveyard with mana value at most two. The resolution rechecks kicker and target identity. Returning an Aura chooses a legal creature at resolution, ignoring hexproof and respecting protection. If no host exists, the Aura stays in the graveyard. |

The once-per-turn ledger uses the source incarnation, ability index, round
counter and active player. It is public in custom observations and survives
save/restore. Zone departure prunes the old incarnation; Untap clears the
ledger. Frozen flat formats refuse the extra state before publishing buffers.

Positive counter-placement effects emit the common committed counter event.
Feature gating preserves the default catalog's existing event-history hashes.
One source dealing simultaneous lifelink damage to multiple recipients causes
one gain event; different sources and sequential damage cause separate events.
Prevention determines the surviving amount. Zero or negative proposed gain
emits no event.

The new live catalog is `kernel_carddb/v39`, hash `3f6b7e8df71f3195`.
Every earlier catalog pair retains its original literal and read compatibility.
Publisher and resume paths require the actual live build identity.

Rules references: [Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf),
119.9, 303.4f and 702.15e, and
[official release notes](https://magic.wizards.com/en/news/feature/ravnica-remastered-release-notes).
Matching XMage classes are `ExemplarOfLight.java` and `SunBlessedHealer.java`
at `a5c90fe180021e70e2a644ade00eeab07f857a40`.

See `docs/reports/fdn_lifegain_creatures_v1_validation.md` for executed checks
and remaining verification. This pair is one implementation batch. The full
fixture milestone still requires the remaining cards, mulligans, original-deck
terminal games, replay, relevant parity checks and CI.
