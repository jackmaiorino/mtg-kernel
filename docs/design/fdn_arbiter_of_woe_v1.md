# Arbiter of Woe admission candidate

This issue110 candidate registers Arbiter368 after accepted v65. Gameplay,
live-profile qualification and default integration remain pending in PR213.

[Pinned XMage ArbiterOfWoe.java](https://github.com/jackmaiorino/mage/blob/a5c90fe180021e70e2a644ade00eeab07f857a40/Mage.Sets/src/mage/cards/a/ArbiterOfWoe.java)
defines a 4BB 5/4 Demon with Flying and a mandatory additional casting cost of
sacrificing a controlled creature. Its ETB makes the opponent discard one card
and lose two life, then its controller draws one and gains two life. The primary
source was read at that exact commit.

The casting cost uses existing selection. Native8 at a0032541 exposed an
actual ETB ordering defect: the legacy Sequence executor staged the discard
choice but immediately continued with life, draw and gain. The opponent-discard
fixture expected life20/20 before the answer but observed22/18. The other three
Arbiter cases passed; later card targets were not reached.

Repair456cbfe201b323fa0f67946d4a8566440faf95e8 adds a resumable boundary for
a nonterminal DiscardCards leaf in a flat root Sequence. The appended
DiscardResume variant binds the stack item, recipe path and original hand
incarnations. Restored state must match the declared recipe and remaining frames
before either advancement or a direct Discard action. Empty hands and forced
single-card hands continue automatically; an actual choice suspends all later
effects until payment. The paid continuation resumes privately without a
serialized intermediate payment marker. Terminal legacy discards retain their
existing path. This does not claim support for nested nonterminal leaves.

The public RL projection exposes the discard choice without its private hand
bindings; search refuses to redeal those bound incarnations. Common validation
is scoped to trusted definitions declaring the new boundary, preserving the
older Moon-Circuit Hacker optional continuation. The build recipe binds Flying,
the creature sacrifice cost and ordered ETB.

Four fixtures cover metadata, mana refusal, casting-cost candidates and
rollback, pending/answered-cost restore, token and borrowed-creature payment,
sacrifice death triggers before spell resolution, restored discard choice,
source departure, empty and forced one-card hands, and exactly one draw/two life
in both seats. Seven forged count/path/frame/hand/cross-slot/choice payloads
must refuse both direct actions and advancement without mutation. Read-only
review found and repaired a removed-discard bypass and a legacy Hacker
compatibility issue; exact final review of456cbfe2 found no actionable findings.
Native10 passed all four repaired Arbiter games and the Hacker regression at
456cbfe2. Its command and guard ended101 because eight cases in three other
fixture targets failed. These failed attempts remain retained. Source97da6a02
adds common step-entry validation and forged Pass refusal to the existing
seven tamper cases; this added boundary needs current-head qualification.

Historical sourcea70d648f passed both feature test compilations and44 focused
primitive/prior-game executions under guard5de40808e06145f2b53c4613c9d29167,
command exit0. The four then-unregistered Arbiter games were not executed.
Native8's failure and guard4a0f751ea5ea4e6699633f479e6f1e7a terminal101 remain
retained. No paid experiment is included.
