# Arbiter of Woe source preparation

This issue110 preparation tentatively follows Armasaur367 as Arbiter368.
Registration and card gameplay qualification remain pending after v64/v65.

[Pinned XMage ArbiterOfWoe.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/ArbiterOfWoe.java)
defines a 4BB 5/4 Demon with Flying and mandatory additional casting cost of
sacrificing a controlled creature. Its ETB makes the opponent discard one card
and lose two life, then its controller draws one and gains two life. The primary
source was read at that exact commit.

This card composes existing mandatory additional-cost selection and sequential
discard/life/draw effects. No engine primitive, enum, serialized field or
accepted catalog entry changes. The build recipe binds Flying, the creature
sacrifice cost and ordered ETB; the trigger source inventory includes its name.

Four unregistered fixtures cover exact metadata, colored mana refusal,
mandatory casting-cost candidates and rollback, pending/answered-cost restore,
token and borrowed-creature payment, sacrifice death triggers before the spell
resolves, opponent discard choice and restored effect continuation, source
departure, empty opponent hands and exactly one draw/two life in both seats.
Native checks and read-only review remain pending. No paid experiment is included.
