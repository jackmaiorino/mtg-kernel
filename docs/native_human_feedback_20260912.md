# Native human feedback delivery

The browser interface now supports local play against the pinned Adam-3 model, including human London mulligans, ordered bottom selection, manual sideboarding, loser play/draw, concessions and BO3 scoring. Jack accepted native play when XMage did not provide a simple shortcut. The XMage prototype remains preserved and paused; see [its boundary](paused_xmage_human_adapter_20260912.md).

Read the [measured result](E:/mtg-kernel-learned-sideboarding-evidence/native-human-001/RESULT.md) and [package manifest](E:/mtg-kernel-learned-sideboarding-evidence/native-human-001/package/manifest.json). The prepared engine is source `92c808bad82e14b70575c3675fb89ba08f71e016`; subsequent training repairs do not alter that package or its live game.

| Check | Result |
| --- | --- |
| Human visibility, labels and opening tests | 34 passed |
| Scripted natural matches | 12 matches, 24 games, both human seats |
| Release replay | Exact response hashes matched the corresponding debug games |
| Browser controls and combat rendering | Passed using actual safe DTOs and real local controls |
| Actual Jack games | None verified at delivery |

Start a fresh game with [Start-Native-Human.ps1](E:/mtg-kernel-learned-sideboarding-evidence/native-human-001/Start-Native-Human.ps1). It prints the URL and evidence directory. The session prepared at delivery is http://127.0.0.1:65235/, with logs in `native-human-001/feedback-sessions/20260912-231949-ed59abd6`. Session paths and process identities must be checked before stopping a process; the provided stop script does this.

This is feedback play with native priority abstractions. The model keeps seven and keeps its sideboard unchanged. Unsupported projections stop explicitly. The pinned engine predates the new menace-prefix training repair, so keep any interrupted session and prepare a distinct replacement package if needed. Neither scripted interface checks nor its availability count as human playing-strength evidence.

Fresh Fable review failed on its weekly usage limit, with no source reads or endorsement. The completed independent Codex reviews and exact failure disposition are recorded in the result. Broader player evaluation, terminal sideboard comparisons, human matchup calibration, production integration and brewing remain campaign work.
