# Native scaled self-play replay result v1

Status: `REPLAY-BIT-MATCH-ADVANCE` on 2026-08-06.

The successor replay completed all three authorized seeds through global
generation 512. The terminal-blind all-three validator returned `ADVANCE`.
Each successor Store matched its source retest Store in full native-state
SHA-256, model-parameter SHA-256, Adam step 512, completed episode count
32,768, next episode index 32,768, and successful update count 512.

The execution manifest is
`D:\mtg-kernel-scaled-selfplay-population-v1\replay\three-lineage-replay\attempt-001\replay-execution-manifest.json`,
SHA-256 `dd10fd12f2339d988ceef629cc71213de7852774aaf2a3d83743c69472c09578`.
The handoff manifest SHA-256 is
`a3331a805c47bc2865ec22328d93fc28a242f4232b8a0381683e447d2be60baf`,
and its validation SHA-256 is
`e35dab6c69aecfa38c7c02ef99ec14f64e5c8cb67906b62c2bcccbea558e67ea`.

Replay processed 98,304 episodes in 7,860.074 seconds, or 2.183 hours, at
12.5068 aggregate episodes/s. The selected topology was GPU 0 plus GPU 1 for
seeds 970001 and 970002, followed by exclusive GPU 1 for seed 970003.

This is a mechanical identity and continuation result. It is not playing
strength evidence, does not read terminal outcomes for selection, and does
not itself establish a population, exploiter, professional-level, or
promotion claim.
