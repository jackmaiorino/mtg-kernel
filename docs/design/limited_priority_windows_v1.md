# Opt-in engine priority windows

Issue #110, milestone 3's priority presentation slice. Launch
`kernel_limited_env --engine-priority-v1` and send Limited schema **2** requests.
Every reply, including errors and terminals, has schema 2 and
`"priority_mode":"engine_windows_v1"`. The default process accepts schema 1
and retains its H2 suppression behavior. Each mode refuses the other's schema
before changing a session or retry record. The process mode is immutable.

The request fields, content-bound decks and nested policy-V5 action menu are
unchanged. The Python client accepts `engine_priority=True`; its CLI flag
`--engine-priority-v1` selects both the binary option and schema 2.

The new surface exposes every `CastSpellOrPass` decision from the rules
engine, including menus containing only Pass. It bypasses step gating,
own-cast force passes, combat action throttling and empty-menu auto passes.
It retains discard/cost reshaping and the existing binary combat declaration
scan. Empty attacker declarations remain automatic. Untap and ordinary cleanup
are advanced by the engine; their rules exceptions remain the engine's job.

`engine_priority_version:1` is included in the surface context, public
observations and their hashes only for the new mode. This also distinguishes
exact combat bindings and session environment hashes. The field is absent
from default serialization, preserving historical bytes and golden hashes.
Nested schema 5 and policy/surface representation version numbers describe
the shared representation. The outer schema and explicit priority mode
identify priority behavior; they are not a complete Limited rules guarantee.

Rules reference: [Comprehensive Rules effective September 25, 2026](https://media.wizards.com/2026/downloads/MagicCompRules%2020260925.txt),
117.3a-d, 117.4-5 and 508-511. Those rules require the acting player to receive
priority after their action, consecutive passes before resolution, and active
player priority after resolution. Native tests exercise these interactions,
including a blocker removed after declarations and before damage. Serialized
rules-state restoration plus cloned surface restoration is checked during
held priority and a pending binary combat choice.

This PR changes presentation, not the engine's turn rules. London mulligans,
current arbitrary combat damage allocation and planeswalkers remain separate
milestone-3 work. FDN fixture cards remain unsupported until their complete
behaviors land; see `fdn_fixture_mechanics_v1.md`. No training or playing-strength
measurement is included.
