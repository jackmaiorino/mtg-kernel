# Tensorize per-decision CPU cost, byte-identical (v1)

Status: branch `opus/tensorize-cost-v1`, engineering only. Goal text:
`collab/GOALS/opus-tensorize-cost-20260923.md`. Design review queued in
`collab/FABLE-QUEUE.md` ("Opus lane 2").

## Where the cost was

`NativeFlatTensorizerV2::fill` builds the thirteen model tensors for one
decision. Its state tail and every action row end in 96 digest features: six
SHA-512 digests of `namespace || counter_le32 || canonical_json`, counters 0
to 5. Base measurements (receipts `docs/reports/tensorize_cost_v1/base/`):

| Corpus / setting | fill | state SHA-512 | action SHA-512 | serialize | other |
| --- | --- | --- | --- | --- | --- |
| D5, 10,051 decisions, serial | 184 us | 153 us | 8 us | 12 us | 11 us |
| D5, 24 threads (per thread) | 384 us | 330 us | 17 us | 22 us | 16 us |

The state JSON averages 21.5 KB on D5 and about 40 KB in live training, and
it is hashed six times in full. In the live pilot (PR #107 round-2 workload,
16 updates, feature-gated counters) fill took 54 s of 207 s process CPU (26
percent), the CPU forward 85 s (41 percent), packet encoding 2.6 s.

## What changed (all exact by construction)

What is serialized, the hash, the namespaces, counters and the
block-to-feature mapping are untouched.

1. **Batched digests.** The full fill writes the state JSON and every action
   JSON first, then hashes all 6 + 6n messages in one call.
2. **Multi-buffer SHA-512** (`mtg-kernel/src/sha512_multi_v1.rs`). Four AVX2
   lanes plus two scalar lanes run interleaved in one round loop (hybrid
   mode), so the six same-length state digests take one pass and the integer
   ALUs work while the vector shift ports are saturated. Runtime AVX2
   detection; without it, the `sha2` crate as before.
3. **State prefix cache.** SHA-512 is Merkle-Damgard: the chaining state after
   block k depends only on the first k+1 blocks. A process-wide cache keeps
   the six chaining states at every eighth data-only block of the last 64
   state JSONs. A new JSON resumes from the deepest checkpoint inside its
   byte-for-byte common prefix with a cached JSON (full byte comparison, never
   a hash) and inside its own data-only blocks. The cache changes cost, never
   output. `MTG_KERNEL_TENSORIZE_STATE_CACHE=0` disables it.

## Identity evidence

TODO: final numbers.

## Measured gain

TODO.

## What remains

TODO.
