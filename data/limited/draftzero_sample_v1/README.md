# DraftZero sample decks (v1)

The 80 FDN sample decks and their `pool.tsv` (40 `train`, 40 `eval`) from
[DraftZero](https://github.com/danieljbrooks/draft-zero) `assets/sample/`, copied byte for byte at
commit `3ee30495239c8d4b9e64c4ddd7b0f963dec3dca7`. Content SHA-256 (each deck's file name then
bytes, in name order, then `pool.tsv` and its bytes):
`fbf517d3136a5497a9068f7da363d84ec904976ebf8fd66a6c50b0937acda72f`.

They are the reference for issue #110's DraftZero integration: `kernel_limited_match_v1 coverage`
reports how many of them each session can play, and `python/tools/limited_match_v1.py draftzero`
plays them.

**Provenance and license.** Every deck comes from [17lands](https://www.17lands.com/) public FDN
Premier Draft game data (decks whose player is in the 60% or better win-rate bucket; each file's
`notes` names its 17lands draft id), published under
[CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). These files are a derivative of it,
redistributed under the same license. 17lands does not endorse this project. DraftZero's code is
MIT licensed.
