# Sideboarding observation successor

This lane implements the three input-format gaps found during learned
sideboarding integration: a Map ability whose token source has ceased, a
chooser-only library search, and Sleep of the Dead's staged Escape cost.
Rich observation V6 and flat action/scoring V3 carry those facts explicitly.

Search candidates are a temporary unordered multiset. Only the chooser sees
them, with no library positions or hidden incarnation counters in model input.
Canonical public-card classes and visible selected-prefix roles determine
ordering. Each physical candidate retains its executable engine action.
Historical sources are authorized by their exact public stack or pending-effect
record. They are not inferred from the current contents of an arena slot.
Object costs carry their cast method, cost kind, selected prefix, required count,
and remaining count; the old sacrifice field remains sacrifice-only.

Initiative transfer also requires two simultaneous controller views of Avenging
Hunter: the creature can remain controlled by the opponent while the transferred
initiative trigger belongs to the acting player. Exact validated historical
contexts may therefore add a distinct controller snapshot at the same arena
incarnation. Card identity, owner and zone must still match; ordinary live
references retain their consistency checks.

The producer validates the original engine decision, the full candidate set,
the exact source authority, and the current environment binding. Scorers receive
only actor-relative rows and row handles. A V3 binding cannot be consumed through
the V2 entry point. Failed encoding does not publish a partial output.

## Frozen play transfer

The Net8 weight and embedding arrays remain frozen. The successor retains the
219/98/41/195/25 state/object/edge/action/reference dimensions and 20 object
groups. Its feature identity is different, including for decisions with empty
extensions. A successful forward is evidence of executable transfer, not of
competent play on the new features or improved sideboarding.

Existing `run_batch` configurations default to the original contract. To use
the successor, add `play_observation_transfer_v3` with
`expected_feature_contract_digest` and `expected_feature_encoding_digest`
copied from `data/flat_policy_v3/feature_contract_v3.json`. The destination pins
are checked before the source export is loaded. The source export still passes
the original strict loader. The transfer receipt records both feature identities,
the Python authority source, descriptor hash, and unchanged weight identity.

The frozen `python/mtg_kernel_rl/features.py` and its feature digests stay
unchanged. Shared Rust implementation edits require refreshed implementation
source pins in the generated flat V2 inventory; the old tensor golden payloads
must remain unchanged. The build-time pin check remains enabled.

## Further supported decisions and limits

Blood Fountain may announce public Graveyard-origin targets. V3 accepts that
specific captured provenance, retaining an exact live row or a detached public
incarnation when the target moves. Hidden Hand and Library targets remain
rejected, as do inconsistent immutable facts. V2 keeps its historical guard.

Monstrous Emergence's `ChooseCreatureOrRevealCreature` is an explicit new V3
cost category. Its full name participates in the semantic hash; the eleven
old cost one-hot columns are zero for this category. Existing action-kind,
source, candidate, zone and remaining-count features are retained. This
preserves the 195-dimensional action input without aliasing a sacrifice cost
or accepting arbitrary unknown categories. The new identity records this
encoding, and the frozen V2 encoder remains unchanged.

This transfer does not establish complete Markov observations for every new
mechanic. In particular, the common rich stack observation does not expose
Monstrous Emergence's separate captured `power_lki` value. If its chosen
creature changes later, current card features need not recover that publicly
known historical power. That distinct observation-completeness issue is not
repaired by the cost-category encoding and is not a competence claim.
The pending cast also omits the already selected Battlefield/Hand branch.
The policy sees that branch through candidate zones, but the frozen value
head does not pool actions and can receive identical state inputs for the two
branches. A future observation extension must represent these facts before
claiming complete Markov coverage or evaluating value learning on this mechanic.

V3 attacker inclusion uses the engine's active-goad requirement at each
candidate's own prefix. A required attacker has one legal action, include.
Ordinary candidates keep ordered exclude/include actions. V5's original
candidate pair still supplies the private origin check, and the old V2 path
is unchanged. Python V6 independently checks this mask against the current
candidate's public battlefield goad records, including the expiry boundary.
This fixes the Arena-induced rejected declaration reproduced by Affinity/Elves
seed 2026091231 at policy step 400; it does not force an action after scoring.

## Review boundary

Jack explicitly assigned these fixes. A fresh read-only Fable review was
dispatched as session `323e67e6-82c2-42db-b1ed-6df762f9d966`, but Claude returned
HTTP 429 for the weekly quota before reading any source. There is no Fable
endorsement. The recorded failure is
`C:/Users/Jack/IdeaProjects/sideboarding-integration-20260912/fable-observation-successor-review-001/`.
Implementation and bounded CPU verification continue under Jack's assignment;
independent Fable review remains an explicitly unresolved review limitation.
A focused fresh goad review, session `b86c9969-f091-4455-9102-e2161dd74f79`,
also failed HTTP 429 before any source reads or input/output tokens. Its
receipt is in the sibling `fable-goad-legal-choice-review-001` directory.
No feedback was available to accept or reject. The correction follows the
existing engine requirement and is being checked through real action execution
and independent native/Python feature parity under the same disclosed limitation.
A fresh review of the Emergence encoding, session
`ba22de95-abff-4efb-a8f6-b348cf859f57`, also failed HTTP 429 with zero source
reads and zero tokens; see `fable-emergence-feature-review-001`. The accepted
Codex review points were to preserve the distinct semantic type, reject other
unknown categories, keep the original shape and V2 rejection, and disclose
the separate captured-power limitation. No Fable feedback was available.
No GPU training, formal strength measurement, checkpoint promotion, or main-branch
merge is authorized by this note.
