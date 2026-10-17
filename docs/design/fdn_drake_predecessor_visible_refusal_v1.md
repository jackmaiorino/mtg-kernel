# Drake predecessor visible-identity refusal

Source repair from `018a82b95f2d4af449cc1c5351318e10f806b6ed`, issue #110.
The shared `rl::private_card` now applies the existing Drake successor refusal
after its callers select an actor-authorized identity. This covers own hand,
known hand/library and actor-visible library choices. It does not inspect unknown
opponent hands/libraries. The existing public-card and retained-ledger refusals
remain in force; catalog metadata, frozen profiles and serialized fields are unchanged.

Five added Rust fixture bodies cover both seats: genuinely paid Think Twice and
Preordain through V51/V52 ownership and schema4, restore and stale bindings,
visible Elf positive counterparts, and concealed Hatcher/Elf noninterference.
A genuinely paid native Duress reaches the revealed-hand prompt before V51/V52
construction refuses without changing its original state. This uses the native
engine because the predecessor stream must already refuse if the Hatcher owner
receives priority before Duress resolves. Initial mana, library order and hidden
printings are explicit fixture inputs. No journal or reveal result is fabricated.

Observed checks: 101 affected deck/pool, schema4 and predecessor V3 Python tests
passed; pinned `cargo +1.94.1 fmt --all --check` and `git diff --check` passed.
All added Rust is uncompiled and unrun. No native, build, test, check, clippy,
network, public push or PR was launched. Genuine Partial deck reset refusal
remains separate from this visible-mechanics refusal. No V53 observation/backend,
Full, model, search, catalog hash or runtime admission is claimed.
