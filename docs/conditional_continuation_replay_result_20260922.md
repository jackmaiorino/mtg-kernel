# Natural continuation replay passed

Both fixed roots passed the reviewed restart checks using unchanged g115. No formal continuation seed or strength gate ran. Evidence: `E:/mtg-meta-recovery-20260921/continuation-natural-replay-001/completion.json`. Executable source 8bb9f086, SHA256 5ab5e510a3bc7500003da0b14b7b1a5681e15b96e2997ae72e93cb014242fd32; build took 131.383 seconds.

| Root | Native game / decision | Remaining gameplay records | Archived prefix | No-reseed remainder and terminal | Off/on collection | Fresh-seed byte replay |
| --- | --- | ---: | --- | --- | --- | --- |
| cell25-r0-s0-g115 | 1 / 242 | 8 | Exact | Exact | Exact | Exact |
| cell45-r0-s0-g115 | 2 / 113 | 75 | Exact | Exact | Exact | Exact |

Prefix comparison includes all records through the root and excludes only the changed runtime package identifier. Both seat model sources are pinned and identical to the archive. Current records are hashed by the typed Rust serializer. The no-reseed comparison includes exact actor-visible observations, ordered actions, behavior masses, selected actions and terminal. Fresh samples use the separate engineering domain. Both fresh checks reached natural terminals; their outcomes are not formal study evidence and do not change the seed plan.

Inherited remaining physical/policy headroom before the selected action was 99,764 / 199,761 for cell25 and 99,944 / 199,935 for cell45. The root consumes one of each. Full-state diagnostic hashes before/after were 17319569750921641613 / 2051691085955189421 and 16554805486830451030 / 8141721143385656488. These versioned FNV64 hashes include environment randomness; exact replay and pinned inputs remain the stronger correctness evidence.

Measured native off/same/fresh/replay seconds: cell25 2.423 / 1.341 / 1.414 / 1.446; cell45 1.099 / 1.024 / 1.060 / 1.888. These are engineering timings, not a parallel-allocation qualification or general throughput estimate.

Read-only placement inventory at 08:09 EDT found the maintainer's 16-core/24-thread i7-13700K with about 82.8 GiB free RAM; its listed active native work was this completed build. The compute host SSH timed out, and RunPod returned HTTP403. No credentials or paid allocation changed. Fresh eligibility is required before qualification and formal work.

Next supported path: python/tools/conditional_continuation_dispatch_v1.py. It compares 32 fixed engineering continuations in 16 root-replay batches at 1/2/4/8/16 workers, verifies full-output SHA identity, then permits the fastest qualified setting for the fixed 16-by-25 formal batches. The different engineering and formal batch sizes mean qualification timing is not a direct formal ETA; prefix replay/serialization overhead remains included. Source/analysis review is running before any formal launch. No raw CLI campaign or unqualified launch is authorized. g115 unchanged, M1 unmet.
