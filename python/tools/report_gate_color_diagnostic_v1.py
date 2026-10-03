"""Recount preserved diagnostic outputs without executing games or selecting cases."""
import collections
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
repo = pathlib.Path(__file__).resolve().parents[2]
cards = json.loads((repo / "data/cards_v1.json").read_text(encoding="utf-8"))["cards"]
name = lambda cid: cards[cid]["name"]
load = lambda path: json.loads(path.read_text(encoding="utf-8-sig"))
digest = lambda path: hashlib.sha256(path.read_bytes()).hexdigest()
completion = load(root / "completion.json")
manifest = load(root / "manifest.json")
assert completion["status"] == "COMPLETE" and len(completion["executions"]) == 6
for pin in manifest["pins"]:
    assert digest(pathlib.Path(pin["path"])) == pin["sha256"]
all_outputs = []
for receipt in completion["executions"]:
    assert receipt["exit_code"] == 0 and not receipt["timed_out"]
    path = root / "outputs" / (receipt["name"] + ".json")
    assert digest(path) == receipt["output_sha256"]
    all_outputs.append(load(path))
rows = []
for seat in range(2):
    for mode in ["baseline", "unique_printed_color"]:
        result = load(root / "outputs" / f"{mode}-seat{seat}.json")
        assert result["request"]["mode"] == mode
        assert result["actual_base_models"] == result["request"]["expected_models"]
        games = []
        total_changed = 0
        for game in result["games"]:
            decisions = [d for d in result["decisions"] if d["game_index"] == game["start"]["game_index"]]
            assert [d["decision_index"] for d in decisions] == list(range(len(decisions)))
            own = [d for d in decisions if d["actor"] == f"p{seat}"]
            changes = [d for d in decisions if d["raw_index"] != d["applied_index"]]
            for d in changes:
                assert d["actor"] == f"p{seat}" and mode == "unique_printed_color"
                raw, applied = d["raw_action"], d["applied_action"]
                assert raw["action_kind"] == applied["action_kind"] == "choose_effect_color"
                assert name(applied["source"]["card_db_id"]) in ["Sea Gate", "Citadel Gate"]
                assert raw["source"] == applied["source"]
                assert applied["color"] == ("W" if name(applied["source"]["card_db_id"]) == "Sea Gate" else "U")
            total_changed += len(changes)
            casts = collections.Counter(name(d["applied_action"]["source"]["card_db_id"]) for d in own if d["applied_action"]["action_kind"] == "cast_spell")
            gate_colors = collections.Counter(name(d["applied_action"]["source"]["card_db_id"]) + ":" + d["applied_action"]["color"] for d in own
                if d["applied_action"]["action_kind"] == "choose_effect_color" and name(d["applied_action"]["source"]["card_db_id"]) in ["Sea Gate", "Citadel Gate"])
            games.append(dict(game=game["start"]["game_index"], winner=game["winner"], decisions=len(decisions), own_decisions=len(own),
                overrides=len(changes), spell_cast_selections=dict(casts), gate_colors=dict(gate_colors)))
        wins = sum(game["winner"] == seat for game in result["games"])
        losses = sum(game["winner"] == 1-seat for game in result["games"])
        assert max(wins, losses) == 2
        assert result["outcome"] == {"winner": {"winner": seat if wins == 2 else 1-seat}}
        rows.append(dict(seat=seat, mode=mode, wins=wins, losses=losses, overrides=total_changed, games=games))
report = dict(schema="gate-color-independent-recount/v1", rows=rows,
    unique_matches=4, unique_games=sum(len(r["games"]) for r in rows),
    executions=6, physical_games_including_replays=sum(len(r["games"]) for r in all_outputs),
    unique_decisions=sum(g["decisions"] for r in rows for g in r["games"]),
    source_commit=manifest["source_commit"], executable_sha256=manifest["executable_sha256"],
    conclusion="The intervention did not reverse either selected 0-2 match loss. This does not show no broader benefit or establish the cause of losses.")
with (root / "independent-recount.json").open("x", encoding="utf-8") as handle:
    json.dump(report, handle, indent=2)
lines = ["# Gates color counterfactual result", "", "Complete engineering diagnostic; g115 remains unchanged. Both selected matches remained 0-2 losses after the Gate color intervention.", "",
    "| Gates physical seat | Baseline game score | Intervention game score | Changed color choices |", "|---|---:|---:|---:|"]
for seat in range(2):
    a,b = [r for r in rows if r["seat"] == seat]
    lines.append(f"| {seat} | {a['wins']}-{a['losses']} | {b['wins']}-{b['losses']} | {b['overrides']} |")
lines += ["", f"Four unique BO3s, {report['unique_games']} natural games, {report['unique_decisions']} gameplay decisions. Including the exact baseline and intervention replays: six executions, {report['physical_games_including_replays']} physical games, {completion['total_worker_seconds']:.3f} worker seconds.", "",
    "Both baselines reproduce all 698 prior gameplay decisions: visible observation/menu hashes, selected indices and semantics, game starts, environment seeds and terminal winners. Both repeated output files are byte-identical. The common prefix and raw draw at the first override match in both seats. The real V4 hidden-state fixture and three engine/wrapper tests passed.", "",
    "The same one development-exposed match seed was run in both physical seats against the familiar A48 training opponent. This is not a win-rate sample, a retention test or a candidate promotion. Subsequent trajectories diverge; the raw/applied record does not mean every later choice matches the baseline state or sequence. Printed costs omit ability/alternate costs and sideboard needs. Only Gate color selection was explicitly overridden.", "",
    "The next diagnosis should examine remaining spell-use and resource decisions before selecting another learning intervention. The negative result does not justify removing legitimate color information or declaring color choices harmless. No new training or paid compute was launched. Independent Fable review remains unavailable after the recorded zero-read HTTP429 until September 22 07:00 EDT."]
with (root / "RESULTS.md").open("x", encoding="utf-8") as handle:
    handle.write("\n".join(lines) + "\n")
print(json.dumps(report, indent=2))
