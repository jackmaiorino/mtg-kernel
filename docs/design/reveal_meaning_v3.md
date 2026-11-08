# Player-visible reveal meaning, protocol v3

Proposal for #543. Independent two-world design review is required before code.
The frozen W1 entrant, transcript schema and soak binaries are unchanged. A
successor is admitted only after the owning W1 soak work has finished; the
completed 4k-k1 report's reveal share exceeds the issue's 2% priority trigger,
but that is not evidence that every soak/acceptance obligation is complete.

## Visible evidence, not native object identity

A reveal currently carries a name without saying whether the card just moved
into the hand or is merely being shown there again. A successor transcript
must expose the player-visible move first and bind the reveal to that event.
It must never derive a same-copy assertion from a private native ID, decklist
composition, hidden library position or the number of same-name hand copies.

The v3 reveal carries one of two meanings:

* `moved`: an `arrival` token identifies a single public transcript event
  that moved a card into this player's hand, within the same atomic visible
  operation as the reveal. Only two evidence forms qualify: a publicly
  described search-and-reveal operation that explicitly identifies its own
  outcome, or an object whose identity remains continuously public while it
  crosses into hand. An ordinary anonymous draw followed by a reveal does
  not qualify, even if adjacent or name-compatible. The token is minted by
  the viewer's transcript sequence, not the engine's allocation order.
* `present`: the named card is shown in the hand without a publicly established
  one-copy arrival link. This is a name-level existence fact. It never asserts
  that the same physical copy was shown previously or that another copy arrived.

An engine reveal after an aborted cast may use `moved` only when a corresponding
player-visible transfer is exported, including its visible source and arrival.
No committed engine event is required, but a real visible transfer is required.
If the source or correspondence was not observable, the reveal is `present`.
Repeated UI delivery reuses its public event token and is idempotent. Separate
reveals of the same name do not imply distinct copies.

## Producer, adapter and reducer

The kernel's event-time journal records only the evidence needed to reconstruct
the viewer's visible transfer. A per-viewer adapter mints arrival tokens when
it emits those transfers. An atomic operation explicitly groups its transfer
and reveal in the public transcript. Before binding a reveal the adapter
checks that group, recipient, destination and one of the two evidence forms
above. Tokens become ineligible for new bindings at the end of that operation,
or earlier on an ambiguous hidden transfer, shuffle or loss of public source
correspondence. Historical tokens remain available only to deduplicate exact
re-delivery. No later observation can repair an expired binding through a
private native ID. XMage/other producers need equivalent visible evidence or
emit `present`.

Spellbench adds an opt-in transcript schema with closed validation of the two
meaning forms. Unknown meanings, reused links to a different arrival, missing
arrival tokens, stale links and inconsistent owners refuse. The kernel reducer consumes
the same projected event stream the entrant receives. Native private history
is not a second source of meaning for that reducer.

For `moved`, the reducer refines the claim already created by that arrival;
it never creates another arrival or increments a count merely because a
reveal was received. An already named public arrival is unchanged. A prior
unlinked `present(X)` is an overlapping existence constraint, not a separate
copy: combine its lower bound with the arrival-derived lower bound by maximum,
not addition. Two separately proven live arrivals may establish two copies;
one arrival plus any number of unlinked reveals establishes at most one from
that evidence. Delayed binding after a separate reveal operation is stale
and refuses. For `present`, the reducer records that at least one copy of that
name is in hand without increasing a known count or attaching a hidden physical
identity. Existing facts and unknown
departures retain their multiset/survivor semantics. Unsupported live references
across an unlinked reveal still refuse. This does not promise to eliminate
every R1 case; it removes only cases for which the enriched visible transcript
actually proves continuity.

Fair search may replace fallback D with those verified name constraints in a
separate successor entrant. The transcript/export/entrant identities are all
versioned. V1/V2 transcripts, frozen fixtures, current W1 policies and existing
results retain their original semantics.

## Required review and acceptance

Review the producer's proposed visible evidence for search outcomes, repeated
reveals, known public transfers after failed casts and unlinked hand reveals.
Include an anonymous draw followed by showing an older same-name card versus
showing the newly drawn card, a named arrival followed by its reveal, an
unlinked reveal overlapping a later named arrival, expired tokens and exact
duplicate delivery. The first pair must remain `present` in both worlds;
the overlap cases must not invent a second copy.
In each pair of worlds with identical player-visible history, vary hidden
same-name copies, departed copies, unseen library order and native IDs. Tokens,
reveal meanings, facts, canonical exports and refusals must agree, with no
hidden branch affecting whether a link is claimed. A counterexample stays
`present`; do not add private lineage to make the fixture pass.

Before closure: implement opt-in producers and Spellbench validation, test
duplicate delivery and invalid links, exercise both seats through the actual
adapter and reducer, and replay the four reveal classes end to end. Show that
the successor enforces valid hand-name constraints while the frozen W1
transcript and fallback behavior remain byte-identical. All affected checks
must pass on the default branches. No new soak or playing-strength claim is
authorized by this engineering note.
