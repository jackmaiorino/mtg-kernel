# Generated Foundations draft and construction

`limited_play_booster_v1.py` completes the supplied-pack environment in
`limited_draft_v1.py` with a reproducible, explicitly versioned Play Booster
simulation. `limited_draft_build_v1.py` binds generation, every pick and all
seats' deck partitions into one replayable host record. This addresses #551.

## Collation model

The frozen 286-name manifest remains the membership authority. The supplementary
collation file records the 70 borderless printings and its Scryfall source hash.
Gameplay identity ignores artwork and foil; printing annotations retain both
the collector number and foil flag. Tokens and art inserts are not drafted.

Each pack has 14 cards: seven common slots, three uncommon slots, one rare slot,
two wildcard slots and one land. A Special Guest replaces a common with 1.5%
probability. The rare and nonfoil wildcard category weights follow the
[published product description](https://magic.wizards.com/en/news/feature/collecting-foundations).
The land model assigns equal probability to a basic or a dual, with a 20% foil
chance. Basic artwork variants collapse to the same five gameplay names.

The product description does not specify foil rarity odds or physical sheet
correlations. Version 1 uses the nonfoil wildcard weights for the foil wildcard,
samples uniformly within categories, and prevents duplicates only within a
pack's regular common and uncommon groups. These are simulation assumptions,
not claims of exact commercial pack frequencies. Frozen metadata contains 101
uncommons whereas the product prose says 100; all 101 remain eligible. Every
one of the 286 frozen names has a positive-probability slot.

Generation uses a domain-separated SHA-256 counter stream and rejection sampling
with a 64-bit seed. The exact sampler, membership, printing metadata and collation
hashes are in the host record. A changed algorithm needs a successor identity.

## Draft and build

Pods contain 2 through 8 seats, each opening three packs. The existing environment
waits for all seats to pick, passes left/right/left, and offers each seat only its
current pack and acquired cards. Choices bind fresh session prompts. Copies have
private physical identifiers; replays validate both the visible index and copy.

Deck construction takes explicit per-seat partitions and added basic lands. It
conserves every drafted copy between mainboard and reserve, allows duplicate
cards, and permits only the five ordinary basics as additions. It uses the
existing executable mainboard admission, which requires at least 40 Full cards.
Partial printings may be drafted and reserved, but this tool does not grant them
Full consumer admission. That remains #550. The builder validates a choice; it
does not claim to optimize deck strength.

```python
import limited_play_booster_v1 as booster
import limited_draft_build_v1 as build

session = booster.start_draft(seed=42, seats=8)
while not session.complete:
    for seat in range(session.seat_count):
        view = session.view(seat)
        # Replace index 0 with the authenticated player's choice.
        session.pick(seat, view.prompt, 0)

# partitions: one ImportedDeck per seat, including its reserve.
# additions: one tuple of explicitly added basic DeckEntry rows per seat.
record = build.record_build(session, partitions, additions)
assert build.replay_build(record) == record
```

The CLI provides `generate --seed 42 --seats 8 --out draft.json`,
`build --draft completed.json --partitions plans.json --out built.json`, and
`replay built.json`. Output creation refuses to overwrite existing records.
Replay regenerates packs, replays all choices, reconstructs every deck, checks
registry identity and exact card-ID order, and prints the canonical record hash.

The seed, full pack record, other seats' picks and built decks are host-private.
Only `session.view(seat)` may go to that seat. The record is never an observation,
policy input or fair-search prior. New session UUIDs are intentionally absent
from replay records, so repeated complete drafts and builds have identical bytes.

Validation covers a complete eight-seat draft and eight 40-card constructions,
exact replay, copy conservation, generation/pick/deck tampering, slot invariants,
all-name eligibility and the existing draft privacy/rotation and construction
regressions. No training, game evaluation or strength claim is involved.
