# Current Net8 CP7 terminal response v4 result

Status: STOP. The selected candidate preserved general Pool3 play but produced
no measurable improvement against CP7 skill 7. Skill 8 was not run.

## Selected candidate

The 512-game CP7 corpus was collected at base seed `1930001` in `567.36`
seconds at `0.902428` games per second. Its SHA-256 is
`5b3ac6818c79be9ba0ff6f31e6fef897fa78cc1796b0013ffbcc33365338ed72`.

The beta `1.0` arm had no eligible checkpoint. The beta `0.3` arm selected
update 4 with mean row action TV `0.0146069`. Its fixed package identities are:

- manifest: `c2d3c258492dc0ed328d5f39cd1e4d817dd44c0fe1854ee21a29a47070fed043`
- payload: `caa483bb1a5ccd86037f21f8fdb4aeb0c8f3dd4fa5ded552b57ee78312d168b8`
- native state: `fdc13f19df23c6c26c169c2878def3a0d31be00b5cc9ed81bdcf4d4cc3811388`
- model parameters: `ac3b21fde5d71619144ef80b8440900527b266f3b2680e276b23b9a015349d1e`

## Pool3 gate

The formal Pool3 panel used 1,024 seat-swapped pairs at base seed `1830001`.
All frozen noninferiority floors passed:

| comparison | candidate better | GAE8 better | equal | net |
| --- | ---: | ---: | ---: | ---: |
| overall | 15 | 23 | 2,010 | -8 |
| candidate P0 | 9 | 12 | 1,003 | -3 |
| candidate P1 | 6 | 11 | 1,007 | -5 |

Candidate and GAE8 wins were `1,176` and `1,184`. The panel completed in
`522.06` seconds at `7.8458` combined games per second. Report SHA-256:
`0d38b7b871b3392f9dce7f64f0fe34de4e26a6398548859d79b5d85ca71ebf64`.

## CP7 skill-7 gate

The complete CP7 panel used 128 fresh seat-swapped pairs at base seed
`1840001`, 256 games per arm. The candidate scored 96 wins and GAE8 scored 97.
Terminal-order comparisons were 3 candidate-better, 4 GAE8-better, and 249
equal, for net `-1`. Candidate-seat nets were `-1` as P0 and `0` as P1.

The seat floors passed, but the frozen overall `+4` terminal-order margin and
`+4` raw-win margin both failed. This is a narrow miss with only seven
discordant comparisons. It is weak evidence against this exact fine-tune, not
a general refutation of KL anchoring or CP7-response learning.

The eight-worker panel completed in `535.61` seconds at `0.95591` games per
second. The initial parser rejected the arm-matched XMage metadata value
`legacy_v1` after all outcomes were sealed. No gameplay was rerun. A create-new
analysis manifest bound the immutable task outputs and corrected parser:

- analysis retry manifest SHA-256:
  `615aea3fe69ea8ae66664a1035ef21bb356ffb4203008c397af0b6062691b8df`
- final report SHA-256:
  `e91244b7bb5332d1bb5746f7809920cc740e21b824628372096969e3b35d8e94`
- final state SHA-256:
  `32cc2168522c0e031c3940c1e05f5d795c065583e42fbdb53f3d3ab7d9677571`

An independent full reparse reproduced package identities, task hashes,
terminal inventories, and gate arithmetic. Skill-8 base seed `1850001`
remains untouched.

## Interpretation and route

V4 established that a larger corpus, fresh optimizer, decayed learning rate,
and forward-KL anchor can move the policy inside the safety envelope without
destroying Pool3 play. It did not convert 512 CP7 games into a detectable
versus-CP7 strength gain. Together with the earlier fine-tune results, this
supports leaving few-hundred-game response fitting and returning to
self-play-scale training.

The next lane is the regularized continuation retest: repeat the original
512-update macro self-play recipe while adding only a fixed KL-to-parent
anchor. That tests whether unregularized sharpening caused the one
well-powered macro collapse and whether long self-play accumulation can be
made stable.

Terminal win, draw, or loss was the only playing-strength signal. Neither
Pool3 nor CP7 is professional-level evidence.
