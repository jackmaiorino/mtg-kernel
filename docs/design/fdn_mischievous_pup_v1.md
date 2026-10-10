# Mischievous Pup source preparation

This issue110 preparation tentatively follows Dreadwing Scavenger364 as
Mischievous Pup365. Registration, live-profile publication and card gameplay
qualification remain pending after v64/v65. Accepted coverage is unchanged.

[Pinned XMage MischievousPup.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/m/MischievousPup.java)
defines a 2W 3/1 Dog with Flash and an ETB returning up to one other target
controlled permanent to its owner's hand. The source was read at that commit.

TargetSpec58 appends the optional controlled-permanent pool without changing
existing IDs. The ephemeral targeting source now carries the captured zone
and generation. It excludes only that exact source incarnation, so an older
ability may target the same physical card after it leaves and returns.
Pending-prefix validation uses the captured source for this specification.
Existing target specs retain their validation behavior. No serialized fields,
legacy state-hash shapes or accepted catalog entries change. Dog is appended
after existing subtype IDs. Rules-vector meaning records battlefield/control
and optionality, with Opaque for the unavailable incarnation-exclusion facet.

A registered-card primitive regression covers both seats, exact source
exclusion, reentry, restored targeting, control changes, lands/artifact lands,
tokens, own hand/graveyard exclusion, duplicates, protection and missing source
provenance. Six unregistered card fixtures cover metadata/colored payment,
opponent-turn Flash, zero/one choices, all permanent types, borrowed ownership,
tokens ceasing, pending/answered/stack restore, forged pending self-target,
source departure, control changes and stale target incarnations. They require
actual registration before execution. Native source checks and read-only
review are pending; no experiment or paid execution is included.
