# Local human BO3 interface

This browser board connects to the native fixed-seat match service. The model uses its own native V3 observation and policy. No XMage engine or external service supplies its decisions.

Build `cargo build -p mtg-kernel --release --bin human_match_v1`. Then run `launch.py` with an absolute `--engine`, a pinned `--template`, and a writable `--sessions` directory. Optional arguments are `--card-text`, `--human-seat 0|1`, `--starting-player 0|1`, and `--seed`. A fresh session directory, configuration, journal and local server are created. Open the printed localhost URL. Reopening that URL retains the current in-memory session; invoking the launcher creates another match.

The prepared Rally mirror package is under `E:/mtg-kernel-learned-sideboarding-evidence/native-human-001/`. Its source is the engineering-007 Adam-3 checkpoint, not an automatically selected overnight successor. The human may take London mulligans, choose bottom cards, make legal game decisions, sideboard manually between games, choose play/draw after losing, and concede. The model keeps seven and its registered sideboard. Learned mulligans and learned sideboarding are not enabled in this package.

Select a card for public rules text and its current choices. `Show all choices` clears the filter. Card handles identify copies within the current prompt. Combat summaries show attackers, blockers and selections; the stack resolves from the top. A stale or interrupted response requires Refresh before another action. Unsupported descriptions stop the session instead of selecting a fallback. All public rules text is reference material; the native engine still determines legal actions.

The native fast-session surface automates some priority passes, including some fresh-own-stack situations. This limits timing choices available for human feedback and must be considered before a formal human-strength claim. Report interactions the interface cannot express; these games can expose both model and action-surface limitations.

The server binds only127.0.0.1 and sends typed human views. Opponent private cards, seeds, model scores and native identifiers remain outside the browser. Backend journals are for later analysis. Do not consult them during human feedback games. Full process-crash recovery is not implemented; preserved journals allow retrospective inspection. A server failure does not create a game win or loss. Concessions are marked separately from natural terminals.

Run `server.py` directly with `--engine`, `--config` and a fresh `--runtime-dir` to keep it in a console; Ctrl+C closes its native child. Background launcher sessions can be closed using the prepared stop script and their specific session path. Human games are feedback until a separate frozen evaluation protocol and player package are declared. Scripted choices verify engineering only.
