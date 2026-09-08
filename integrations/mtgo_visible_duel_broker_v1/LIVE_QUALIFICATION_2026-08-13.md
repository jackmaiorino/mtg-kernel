# Live producer v1.5 lobby qualification, 2026-08-13

## Result

After a normal MTGO exit and ClickOnce relaunch, the release-pinned live broker
loaded producer v1.5 into the clean signed client and invoked its observation
method. No duel was open. The only outward result was:

`{"result_kind":"abstained","reason":"duel_surface_unavailable"}`

The output SHA-256 was
`74efeb3e679eb695dd351205136243e612f5bee6ce43333cb48bc9b6842da9a4`.
The client remained responsive. The live build rejects every dispatch command,
so this run could not send a client action.

## Manifest

- adapter commit: `d1958d4`
- process start UTC: `2026-08-13T08:18:17.0229803Z`
- client product and file version: `3.4.158.4691`
- MTGO.exe SHA-256: `bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92`
- DuelScene.dll SHA-256: `72b99e1169f9f9445a510b2dae52f9212fb7300c2483b8bc8e02f5760f11904e`
- Card.dll SHA-256: `071338a98d845d5c8db6ebd2f3c847e38ad548f50ba11d2a36973438cdec2ea8`
- WotC.MtGO.Client.Model.Reference.dll SHA-256: `f3fef1adfd5b1b6d25a5db577f9a1b184c8b91bb98f19a13428c669266c20dc8`
- live broker SHA-256: `6b008edc7c261a3022729f0d7f8ea68eaa727ef86b9b90b2328830c4e121afd9`
- bootstrap SHA-256: `1d764382d56fe27aa845acf10b92ee8b9effd79d161baeaace1294a2d01c8c9b`
- producer SHA-256: `a99751da026d9e9e0b023c090cb24e06b8399a52bb745f9bea1b1f9be22e53e9`
- strict validator SHA-256: `e95e60bdf3ff6b4e2347609e79b6b9950152912d92cb6105ccef9dc95085fd16`
- Rust: `rustc 1.94.1`, `cargo 1.94.1`
- .NET SDK used for the managed build: `9.0.200`
- MSVC: `19.50.35725` x64
- GPU ordinal: none
- seeds: none

## Nonclaims

This run proves only the clean-process identity-pinned loader, current producer
hash, fixed lobby abstention, and continued absence of live dispatch. It does
not qualify duel-root discovery, a successful visible projection, action-set
completeness, model scoring, client input, event entry, spending, League play,
or Challenge play. The next live qualification requires an already-open
no-stakes two-player duel and must remain observation-only.
