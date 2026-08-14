# MTGO background visible-equivalent qualification, 2026-08-13

## Build and runtime identity

- Adapter commit: `11789e48d0d2adb4045f8f7d00883ab6a3201f76`
- Rust: `rustc 1.94.1 (e408947bf 2026-03-25)`
- Cargo: `cargo 1.94.1 (29ea6fb6a 2026-03-24)`
- MSVC used for the pinned native artifacts: `19.50.35725` x64 with `/Brepro`
- MTGO executable SHA-256: `bb9c1a189674cd7333b1d997259109576cafe78767f0f11badaad2203c388e92`
- Observe-only broker SHA-256: `3906163c2ddd56030c629df7ca6474de03fec337f9150f5e97e75663d0198a07`
- Bootstrap SHA-256: `1d764382d56fe27aa845acf10b92ee8b9effd79d161baeaace1294a2d01c8c9b`
- Producer SHA-256: `857478e466fc3cb7e4473d069ec46837f95abfa137b815062da818859b237e60`
- Strict validator SHA-256: `4e0eea73bf592a0a80aa5e42a05f191640b4f1f46d917f1b1bca80a81815334c`
- Seeds: not applicable to this deterministic runtime qualification
- GPU ordinal: not applicable; no GPU work occurred

## Result

The release-pinned observe-only producer ran twice against one unchanged MTGO
process incarnation. Both strictly parsed visible-equivalent results were
byte-identical and returned the fixed abstention `duel_surface_unavailable`.

- Runtime identity commitment: `39c6608b73d6a9f0d4f7c0656ad90847de21853cab375e612a641d44eaba7e86`
- Sanitized result SHA-256: `74efeb3e679eb695dd351205136243e612f5bee6ce43333cb48bc9b6842da9a4`
- Stability commitment: `23e5aecd9f3ded58cce7cbfb2693fe51455d044fe35c4a76c7e582801da146f0`

The client remained responsive. The qualification did not require foreground
focus, pixel capture, window movement, model scoring, input, event entry, or
spending. Process identity was used only privately for the two-call join and
was discarded.

## Non-claims

This lobby abstention proves the signed background load, strict validation,
and stability route. It does not prove a data-bearing duel projection, model
scoring, ordinary or combat action completeness, input, League or Challenge
entry, or spending. Both background source-ratification roots remain `None`.
