# V4 sampler source review disposition

2026-09-22. Fresh read-only v4-sampler-source-review-001, session7bf28eb4-73bd-45ff-9c0d-0c79337f9236, completed exit0/is_error=false with actual sampler, state, engine, effect and trigger reads. Verdict: whole-object direction accepted as an engineering component, four changes required before any dependent caller.20/20 fixed checks do not close these gaps.

Accepted F1: scan pending-effect frame/choice/context bindings and transient event-log references. Scry can expose bound cards without positional knowledge locks, unlike mill, so root equality alone cannot establish resumability. Add root-deterministic exclusion for affected pooled references; do not pin or redraw. Typed reference audit remains implementation work, including nested/answered frames and interpreter-owned bound effect ops.

Accepted F2: require baseline physical hidden-card state; reject residue instead of silently normalizing the sampled world. Correct the existing hidden-source test helpers to perform production-like zone reset. The original synthetic witness remains useful for exposing relabeling, but its battlefield residue was unrealistic. No ordinary-play prevalence or strength claim survives merely by fixing that fixture.

Accepted F3: remove dead hand-knowledge rebinding. Self-hand reveal is a no-op, and root-own hand is never pooled; the old assertion loop was vacuous. Assert that invariant explicitly and positively test the other observer's knowledge of the actor's library.

Accepted F4: add opponent-pool historical-source zone crossing and subsequent stepping for both seats. Current32seed probe changes only positions within the root's own library. Missing hand/library crossing coverage is material. No broad engine-valid sampling acceptance until addressed.

Reviewer verified target contracts exclude hidden-zone live incarnations, ordinary public-to-hand cards are locked, event-history checks use identity/generation, Fisher-Yates/canonical ordering are consistent, and partial rejection mutation is harmless only on disposable clones. It did not audit all PolicySurfaceV5 hidden caches. Preserve that uncertainty; no playing route, gate, M1 or posterior claim.
