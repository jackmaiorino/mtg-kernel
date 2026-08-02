# Native conservative selective-search signal v1 results

## Result

Reject the tested full-terminal retained-policy rollout search rule.

The fresh screen collected all 32 later-game roots and completed all 1,536
information-set samples. Every ranking rollout and confirmation pair ended
naturally. Redeterminized states were shared exactly across compared actions,
same-action confirmation matched exactly, and no branch or sampling integrity
gate failed.

The fixed conservative rule made 0 overrides. Its required six-point ranking
reward-sum margin was never reached, so the confirmed search-minus-parent
reward delta was `0 / 1024`. The required gates were at least 6 overrides,
more positive than negative override roots, and at least `52 / 1024` confirmed
reward improvement. All three signal gates failed.

The deterministic repeat reproduced the 748,112-byte report exactly:

- Formal report SHA-256: `5faaf4f9a1e77da2e6dd24bbbbae2b2c7fea5bdfe9e7ec1803b2214ee9c0d06f`
- Formal runtime: 139,889 ms
- Repeat runtime: 140,131 ms
- Both stderr files: empty

## Diagnostic review

The largest alternative-minus-parent ranking reward margin was 4. Six roots
had margin 4, six more had margin 2, and 20 had margin at most 0. This
post-result review confirms that zero overrides came from the declared margin,
not a missing alternative action or incomplete rollout. It is diagnostic only.
The margin and root selector will not be tuned on this report.

## Disposition

Close full-terminal information-set search that uses this retained checkpoint
as the continuation policy and this hidden-state sampler. Do not spend a live
generation-384 evaluation on it.

This result does not reject all search. In particular, it does not test a
learned value bootstrap, a stronger continuation policy, tree reuse, or search
at a future stronger checkpoint. It produces no trained model, XMage result,
promotion evidence, or professional-level play claim.

Evidence root: `D:\mtg-kernel-selective-search-signal-v1`.

