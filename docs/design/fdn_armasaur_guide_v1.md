# Armasaur Guide source preparation

This issue110 preparation tentatively follows Felidar366 as Armasaur367.
Registration and card gameplay qualification remain pending after v64/v65.

[Pinned XMage ArmasaurGuide.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/ArmasaurGuide.java)
defines a 4W 4/4 Dinosaur with Vigilance. Whenever its controller attacks with
three or more creatures, one controlled target creature gets a +1/+1 counter.
The primary source was read at that exact commit.

The appended trigger predicate uses the complete declared attacker set and
existing once-per-source controller declaration marker. The source need not
attack. Tokens count; creatures inserted attacking produce no declaration.
The threshold is checked when collected, with no resolution recheck. Existing
mandatory controlled-creature targeting and counter effect handle resolution.
No serialized state/event field or accepted catalog entry changes. Rules-vector
meaning records the attack event and creature-count read, and marks the
declaration-specific numeric condition Opaque because its vocabulary lacks it.

A registered-card predicate regression checks both seats, counts zero through
four, a nonattacking source, source incarnation, wrong controller, event kind
and restored state. Four unregistered gameplay fixtures cover exact metadata
and payment, pending-cast restore, actual zero-to-four declarations, source
attacking or not, token attackers, exactly one counter, opponent declarations,
combat insertion without declaration, mandatory target legality, source leaving,
target control/exile/reentry and restored resolution after attackers leave.
Native checks and read-only review are pending. No paid experiment is included.

Read-only review of5da24991 found no actionable defects. That exact source
passed both feature test compilations, the registered declaration predicate
regression,23 rules-vector cases (one existing ignore) and the frozen v63
catalog case under supported guard0c463538ef5b42428b6bf8c91b17b0da,
command exit0. Four card fixtures compiled but remain unexecuted until
registration. The owned idle compiler telemetry child was released after
verifying the completed command sequence.
