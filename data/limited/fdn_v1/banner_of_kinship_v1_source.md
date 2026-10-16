# Banner of Kinship unregistered prerequisites

Source checkpoint only. No definition, ID, registry entry, profile, consumer,
model, transport or Full admission is added. All Rust fixtures are unrun.

The frozen pool selects FDN 127, Scryfall
`a14c16c0-4053-46b0-8fa6-be8b4a7a1c8a`, Oracle
`8c220dbd-6572-4715-aae6-dd09a4252d68`. The pinned Mage implementation is
`Mage.Sets/src/mage/cards/b/BannerOfKinship.java` at
`a5c90fe180021e70e2a644ade00eeab07f857a40`.

That source specifies a {5} artifact. As it enters, choose a creature type and
put a fellowship counter on it for each creature its controller controls of
that type. Its static ability gives controlled creatures of that type +1/+1
for each current fellowship counter. The count before entry and the later
current counter amount are separate values. Losing creatures later does not
remove counters, and adding creatures later does not add entry counters.

Implemented prerequisites:

- Sparse, full i32 fellowship-counter storage, appended tagged hashing, native
  counter transfer with replacements and checked overflow, and departure LKI.
  Zero/absent storage retains predecessor bytes and hash input.
- Read-only capture of an actual proposed battlefield entry, its original
  scene, source incarnation and intended controller. Its answer quotes a
  pre-entry count and applies native entry-counter replacements. It offers
  all 324 V60 types and rejects changed input and unsupported V60 animation
  or unknown-subtype composition. It does not publish a native decision.
- Read-only current-counter anthem arithmetic, with exact live source
  incarnation, current controller, printed-ability suppression, effective
  creature-type matching and checked counter validity. Calling this helper
  does not grant an ability or install a continuous effect.
- Existing RL, optional-trigger, Drake, V61 and Standard Human counter shapes
  reject fellowship values they cannot represent. V61's mandatory 16-counter
  shape is unchanged; it refuses this seventeenth counter kind.

The genuine paid fixture uses Llanowar Elves, Doubling Season when selected,
and Heraldic Banner's actual WUBRG pre-entry stage. Forests pay their actual
costs. Generic creature-type calculations against that captured entry remain
read-only, and native Heraldic entry still commits only its printed chosen
color with zero fellowship. This is primitive coverage, not paid Banner of
Kinship coverage. Other tests explicitly exercise native counter primitives.

Follow-up Standard fixtures pay for real Innkeeper's Talent casts and level
activations using Forests, for both seats. A fellowship-only Elf receives the
actual level-two ward trigger after an opposing paid Lightning Bolt targets
it; zero and negative fellowship retain the predicate's earlier semantics.
Without Season, level-three Talent doubles ordinary fellowship amounts, and
both native transfer and entry quotes refuse overflow rather than saturating.
The entry overflow fixture uses 31 genuinely paid, levelled Talents and a
genuinely paid Gingerbrute entry proposal. The fellowship values remain native
storage/quote primitives, not fabricated printed abilities. The new counter
kind always uses checked arithmetic; older counter-kind arithmetic is unchanged.

Remaining registration boundary:

1. Add a distinct creature-type entry checkpoint to the existing immediate
   operation runner, retaining its authentic original operation, full stack,
   APNAP pre-entry batch roster, answers and replay validator. The current
   WUBRG checkpoint cannot be retagged as a 324-choice creature-type stage.
2. Store the chosen type with the exact entering incarnation before entry
   observers run. Apply the quoted fellowship amount in the entry-counter
   commit path, including copies, batches, replacement redirection and errors.
   Wire the current-counter anthem into the checked native characteristic
   calculation only for its complete selected definition.
3. After source review, register the exact printing and add genuine paid
   Banner accept/entry/copy/control/suppression/reentry and Season fixtures.
   No fabricated definition or ignored future-printing test substitutes for
   this registration-dependent coverage.
4. Include this new semantic module and its consumed V60/native replacement
   inputs in each future observer's source identity. No current observer calls
   these quote helpers or obtains a new permission from them.

The existing V60 legal domain is pinned to Wizards' 2026-09-25 CR 205.3m.
The integration must preserve CR 614.12 pre-entry replacement processing,
613.4c additive power/toughness modification and 122 counter identity. Native
mutable-world inputs retain their existing trust boundary; these read-only
calculations are not sealed snapshot authority.
