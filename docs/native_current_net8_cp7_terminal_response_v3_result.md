# Current Net8 CP7 terminal response v3 result

Status: STOP. No arm was eligible, no package was published, and no downstream
gameplay ran.

## Bound run

The four-arm measurement ran at reviewed code head
`61559aef91f42f4ff428174e39224e31138d754a` from the exact retained GAE8
state and exact revealed corpus. Evidence root:

`D:\mtg-kernel-current-net8-cp7-terminal-response-v3\development\screen-attempt-01`

The initial manifest SHA-256 was
`21162f88c13826f1ff21b170fc0811d9965610ee6f1a1186e576d2f23fe477fb`.
All four reports completed before analysis. The first selector invocation then
failed before reading any report because an argparse destination contained
hyphens. Commit `90084682c7fea787f6ea508e81d281b6a0550f8b` fixed only that mechanical
binding and added the three nonblocking tests requested in Fable review #170.
The selector reran against byte-unchanged reports. The completed manifest
SHA-256 is `4773d77a1f6f8e0a4f20b996fdbe6f78143e96d789ff57e7614b2a8589deeb88`.

## Movement results

| beta | seconds | mean TV | p90 TV | p99 TV | p99 group abs log | max group abs log | groups above 1 | L2 | eligible |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|:---:|
| 0.3 | 30.707 | 0.008123 | 0.027110 | 0.074646 | 0.247821 | 1.135950 | 1 | 0.227075 | no |
| 1.0 | 30.722 | 0.023169 | 0.087034 | 0.221639 | 1.357651 | 1.522940 | 62 | 0.244886 | no |
| 3.0 | 30.962 | 0.047556 | 0.128016 | 0.719244 | 2.521089 | 7.394447 | 261 | 0.456257 | no |
| 10.0 | 30.885 | 0.058969 | 0.148758 | 0.790359 | 2.936330 | 9.334218 | 286 | 0.745704 | no |

Report SHA-256s in arm order are:

- beta 0.3: `9737879a2b781f0e0b4441a58ad69ce646643378afba6c97d03f1cf1fe1201af`
- beta 1.0: `ef6160f288205f60ddd7701d80fc8dd853fd4bcc3d68aa07fb03036ab9af4660`
- beta 3.0: `e7641910a7f4e5ae8887e2e8f6585bc9bc49a0d585b33fcd93c841abeeb43566`
- beta 10.0: `169c76d649a66fef89df767e7a360889f47559af2a5e82a44da16757fb2d0bcd`

The selection report SHA-256 is
`7abf56ada717180999f58c68ad323af6de445fd5a375d06cc52a458af5846469`.
It independently recomputed every cap and selected `null`.

Beta 0.3 came closest. It passed both p99 caps but missed the mean-TV floor and
still had one physical group above absolute selected joint log ratio 1.0. Every
larger beta passed the mean-TV floor but failed the new tail caps and the
unchanged maximum-log cap.

## Interpretation

The first update was identical across all arms because forward-KL gradient is
exactly zero at the source policy. Separation began on update two. Under the
frozen learning rate and continued Adam state, larger beta caused oscillatory
overshoot rather than a monotonic contraction toward the old policy. This is a
result about this exact coefficient grid, optimizer continuation, learning
rate, four-update schedule, source, and corpus. It does not establish that
forward KL is generally harmful.

Per the predeclared stop rule, there is no local coefficient, learning-rate,
optimizer-reset, or threshold retry. Seeds `1830001`, `1840001`, and `1850001`
remain untouched. Terminal win, draw, or loss remains the only playing-strength
and promotion measure.

## Throughput

The four measurements took `123.276` seconds total, or `30.819` seconds per
arm, below the declared 180-second projection. The workload was CPU-bound. GPU
1 was reserved at 0 percent utilization and 9 MiB before launch and remained at
the same idle baseline after the run.
