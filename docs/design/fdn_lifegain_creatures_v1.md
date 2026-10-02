# FDN life-gain creatures

The next fixture pair is Exemplar of Light and Sun-Blessed Healer. Both
remain unsupported until their complete behavior is implemented and tested.

| Card | Required behavior |
| --- | --- |
| Exemplar of Light | `{2}{W}{W}`, 3/3 Angel, flying. Each positive life-gain event controlled by its controller creates one incarnation-bound counter trigger. Its controller placing one or more counters on it creates one draw trigger per turn, with the limit consumed when it triggers. The limit survives control changes and restore, and resets for a new incarnation or turn. |
| Sun-Blessed Healer | `{1}{W}`, 3/1 Human Cleric, lifelink, kicker `{1}{W}`. A kicked entry creates a targeted return trigger for a nonland permanent card in its controller's graveyard with mana value at most two. Both trigger-time and resolution-time kicker gates and exact target incarnation are required. |

The first prerequisite corrects life-gain event granularity. One source
dealing simultaneous damage to several recipients causes one gain event;
different sources cause separate events, and sequential damage remains
separate. Replacement/prevention determines the actual total before grouping.
Zero or negative proposed gain emits no gain event.
See [Comprehensive Rules](https://media.wizards.com/2026/downloads/MagicCompRules%2020260619.pdf),
119.9 and 702.15e, and
[official release notes](https://magic.wizards.com/en/news/feature/ravnica-remastered-release-notes).

Next, every positive counter-placement effect must use the committed counter
event already added for entry/doubling. The once-per-turn limit must be
public in custom observations, saved/restored, and refused by frozen flat
formats that lack a representation. It must not reuse an activated-ability
usage slot. The graveyard target filter must cover all permanent types,
exclude lands and opponent graveyards, and recheck mana value and identity.

Validation will cover split damage, distinct sources, prevention, multiple
life-gain events, controller-only placement, one draw per turn, bounce,
restore during the draw and return decisions, kicker costs, all target-filter
boundaries, and interactions with Dazzling Angel and Good-Fortune Unicorn.
Matching XMage implementations are `ExemplarOfLight.java` and
`SunBlessedHealer.java` at `a5c90fe180021e70e2a644ade00eeab07f857a40`.

This branch currently contains only the life-gain prerequisite and its five
focused tests. Card implementation and catalog succession remain pending.

The prerequisite passes all 14 event tests in both default and Limited
builds, 21 fixture-B and 24 targeted-spell regressions, the exact default
environment-hash golden, and feature-enabled library Clippy with warnings
denied. Tested local source is `8931c1a4`; remote source is
`aefdd0a`. Logs remain outside Git at
`C:/Users/haley/fdn-lifegain-tests-001.log/.exit` and `-002.log/.exit`,
both exit zero. Checks are CPU only, two Cargo build jobs; no GPU or
formal run was launched.
