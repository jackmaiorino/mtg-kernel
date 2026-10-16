# Omniscience native source preparation

Base source `c344469427716fad07a682240ee9b2e1a84dad4d`. Pinned Mage
`a5c90fe180021e70e2a644ade00eeab07f857a40`, `Mage.Sets/src/mage/cards/o/Omniscience.java`
and `CastFromHandWithoutPayingManaCostEffect.java`: 7UUU enchantment; you may cast
nonland cards from your hand without paying their mana costs. Frozen printing
FDN161, Scryfall d33d91d0-1506-45e4-9def-975bf901815e.

This first checkpoint is UNREGISTERED. ID478 is reserved, but no registry,
printing row, reference deck or native test entry is added yet. Rust is uncompiled
and unrun. It supplies the closed native optional paid/free Hand choice and
source-bound original/finalized history, with absent optional fields omitted in
serde/Hash. No CastMethod discriminant changes. The free choice reuses the exact
native pending-form/target/additional-cost planner, fixes X at zero, forbids other
alternative costs and changes no timing permission. Ordinary paid Hand, graveyard
and exile casting remain independent. A disposable native announcement against
the actual active current-controlled source checks free-offer feasibility; it
creates no resolution/exile permission and changes no original native world.

Pending and finalized source/controller/hand-generation/choice fields are checked;
old public consumers refuse the new source or retained choice history. Actual
semantic children are included in the existing producer closures without removing
or reordering their old inputs. Full preflight, capability metadata and frozen
export/profile constants are unchanged. Genuine paid fixtures and registration
remain to be completed before this native appendix is a printing deliverable.
