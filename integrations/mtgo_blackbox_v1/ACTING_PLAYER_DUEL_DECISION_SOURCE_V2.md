# Acting-player duel decision source v2

## Outcome

The model scorer must never receive a real MTGO decision through the existing mock-frame identity alone. A live decision commitment must bind the exact checked DXGI source, the source-bound reconstruction audit, and the perception profile that produced every semantic leaf. The current v1 scorer remains a valid offline and mock path.

## Required chain

1. `check_untrusted_dxgi_capture_artifact_v1` validates the persisted visible-desktop artifact and produces the opaque checked source.
2. `validate_dxgi_bound_observation_reconstruction_audit_v1` binds the ten-group readiness inventory to that exact manifest hash, canonical pixel hash, client size, and acting-player duel role.
3. A future constrained perception interface consumes the checked source plus canonical pixels internally. It emits region commitments, semantic labels, confidence, classifier identity, and a perception-profile commitment. Raw callers must not be able to mint this interface from booleans or hashes.
4. A source-bound decision validator accepts the perception result, the complete ordered legal actions, object bindings, and the existing `MtgoObservedDecisionV1` payload. It reuses all current observation, action, provenance, current-frame, and confidence checks.
5. The live decision commitment is domain separated and binds the existing decision commitment plus capture manifest, canonical pixels, output identity, capture role, reconstruction-audit commitment, and perception-profile commitment.
6. The native scorer accepts a small validated-decision view implemented by the existing offline wrapper and the future source-bound live wrapper. The live wrapper implements that view only after the perception profile is separately admitted.
7. Selection, coordinate grounding, input authorization, and postcondition confirmation bind the live decision commitment, not the inner v1 commitment.

## Type boundary

Add a `CheckedUntrustedMtgoDxgiObservedDecisionCandidateV1` first. It may expose commitments and blockers only. It must not expose the inner validated decision, `ObservationV5`, legal actions, model request, coordinates, or input methods.

Add an `AdmittedMtgoDxgiObservedDecisionV1` only after a reviewed perception profile has a concrete trust root and measured accuracy gate. This opaque type may expose the common scoring view, but still no coordinates or input authority.

Do not serialize either opaque wrapper. Do not reinterpret `MtgoMockFrameV1` as a real source. The live wrapper may reuse the v1 payload validation internally, but its public commitment must retain the complete DXGI source chain.

## Fail-closed tests

- A real source cannot enter the mock-only scoring executable.
- Manifest, canonical-pixel, output-identity, role, client-size, sequence, audit, or perception-profile substitution rejects.
- Every current observation, binding, and legal-action leaf roots in the exact current DXGI frame.
- An incomplete reconstruction group, uncalibrated label, confidence below threshold, extra action, or ambiguous action rejects.
- The checked-untrusted candidate has no scorer or intent conversion.
- The admitted live decision has no coordinate or input conversion.
- Any source-chain byte change changes the live decision commitment and invalidates a prior selection.

## Merge order

The capture-role and source-bound audit commits touch the DXGI and reconstruction files. The external scorer commits touch the model-scoring and deployment files. Merge the former into the scorer branch first, then add the v2 source-bound decision types in `contract.rs` and `validation.rs`. The current branch comparison shows those two files are unchanged between the lanes, so this order avoids rewriting either completed tranche.

## Nonclaims

This design does not establish perception accuracy, create a live observation, authorize a human match, authorize League or Challenge entry, or send input. It only specifies how those later stages must preserve visible-source identity when they become separately available.
