"""Six bounded executions of two exposed development cases, never a win-rate panel."""
import argparse
import hashlib
import json
import pathlib
import shutil
import subprocess
import time


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def write(path, data):
    with path.open("x", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2, ensure_ascii=False)


def visible_hash(visible):
    payload = [visible["observation"], visible["ordered_actions"]]
    return hashlib.sha256(json.dumps(payload, ensure_ascii=False, separators=(",", ":")).encode()).hexdigest()


def audit_baseline(result, original):
    old_games = original["collected"]["trajectory"]["games"]
    assert len(result["games"]) == len(old_games)
    expected_decisions = []
    for index, (game, old) in enumerate(zip(result["games"], old_games), 1):
        summary = original["collected"]["games"][index - 1]
        assert game["start"] == old["start"]
        assert game["environment_seed"] == summary["environment_seed"]
        assert old["terminal"]["classification"] == "natural"
        expected_winner = {"p0_win": 0, "p1_win": 1, "draw": None}[old["terminal"]["outcome"]]
        assert game["winner"] == expected_winner
        rows = [d for d in old["decisions"] if d["visible"]["kind"] == "gameplay"]
        for decision_index, row in enumerate(rows):
            selected = row["behavior"]["selected_index"]
            expected_decisions.append(dict(game_index=index, decision_index=decision_index,
                actor=row["actor"], visible_sha256=visible_hash(row["visible"]),
                raw_index=selected, applied_index=selected,
                raw_action=row["visible"]["ordered_actions"][selected],
                applied_action=row["visible"]["ordered_actions"][selected]))
    assert result["decisions"] == expected_decisions, "baseline decision stream differs from original collector"
    return len(expected_decisions)


def audit_intervention(result, baseline, seat):
    assert result["actual_base_models"] == baseline["actual_base_models"]
    assert result["printed_mainboard_colors"] == ["W", "U"]
    changes = []
    for row in result["decisions"]:
        if row["raw_index"] == row["applied_index"]:
            assert row["raw_action"] == row["applied_action"]
            continue
        raw, applied = row["raw_action"], row["applied_action"]
        assert row["actor"] == f"p{seat}"
        assert raw["action_kind"] == applied["action_kind"] == "choose_effect_color"
        assert raw["source"] == applied["source"] and raw["actor"] == applied["actor"]
        assert applied["color"] in ["W", "U"]
        changes.append(row)
    assert changes, "intervention did not exercise any override"
    # Exact common G1 prefix, including base draw at the first override.
    first = next(i for i, row in enumerate(result["decisions"]) if row["raw_index"] != row["applied_index"])
    assert result["decisions"][:first] == baseline["decisions"][:first]
    for key in ["game_index", "decision_index", "actor", "visible_sha256", "raw_index", "raw_action"]:
        assert result["decisions"][first][key] == baseline["decisions"][first][key]
    return dict(overrides=len(changes), identical_prefix_decisions=first, changes=changes)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", required=True, type=pathlib.Path)
    parser.add_argument("--binary", required=True, type=pathlib.Path)
    parser.add_argument("--previous", type=pathlib.Path, default=pathlib.Path("E:/mtg-postboard-campaign-20260920/behavior-trace-diagnostic-001"))
    args = parser.parse_args()
    root = args.root
    root.mkdir(parents=True, exist_ok=False)
    (root / "requests").mkdir()
    (root / "outputs").mkdir()
    binary = root / args.binary.name
    shutil.copy2(args.binary, binary)
    pins = []
    for seat in range(2):
        previous_request = args.previous / "requests" / f"case0-g115-seat{seat}.json"
        previous_output = args.previous / "outputs" / f"case0-g115-seat{seat}.json"
        old = read(previous_request)
        config = old["config"]
        for package in old["packages"]:
            assert package["opening"] == dict(kind="existing", protocol="keep_seven_v2")
            assert package["play_draw"] == dict(kind="fixed", choice="play")
            assert package["sideboard"] == dict(kind="keep")
            assert package["search"] == dict(kind="disabled")
        for mode in ["baseline", "unique_printed_color"]:
            request = dict(config={key: config[key] for key in ["deck_ids", "seed", "max_physical_games", "max_physical_decisions", "max_policy_steps"]},
                registrations=[dict(label=label, **registration) for label, registration in zip(config["deck_ids"], config["registrations"])],
                sources=[p["gameplay"]["source"] for p in old["packages"]],
                expected_models=[p["gameplay"]["identity"] for p in old["packages"]],
                tags=config["summary_tags"], diagnostic_seat=f"p{seat}", mode=mode)
            request["config"].update(game_one_chooser=int(config["initial_chooser"][-1]), opening_protocol="keep_seven_v2")
            path = root / "requests" / f"{mode}-seat{seat}.json"
            write(path, request)
            pins.append(dict(path=str(path), sha256=sha(path)))
        pins.extend([dict(path=str(p), sha256=sha(p)) for p in [previous_request, previous_output]])
    manifest = dict(schema="gate-color-diagnostic-manifest/v1", source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        source_script_sha256=sha(pathlib.Path(__file__)), executable_sha256=sha(binary), pins=pins,
        question="Do unique printed-mainboard-color Gate overrides change these two exposed g115 Gates/Affinity games?",
        design="Two physical seats, one existing exposed match seed; baseline and intervention. Keep7/Keep/play, no search. No statistical strength or adoption claim.",
        max_seconds_per_execution=60, max_total_worker_seconds=240, executions=6,
        gates=["all baselines reproduce every previous visible decision, selection, game start, seed and terminal",
               "baseline and intervention exact output replay", "first baseline times six below 240 seconds", "all six executions naturally complete"],
        review_gap="Fable Sep19 HTTP429 with zero reads until Sep22 07:00 EDT; no repeated retry; authorized bounded local diagnostic only")
    write(root / "manifest.json", manifest)
    executions = []

    def execute(mode, seat, replay=False):
        request_path = root / "requests" / f"{mode}-seat{seat}.json"
        name = f"{mode}-seat{seat}" + ("-replay" if replay else "")
        output = root / "outputs" / f"{name}.json"
        started = time.monotonic()
        with (root / f"{name}.stdout").open("wb") as stdout, (root / f"{name}.stderr").open("wb") as stderr:
            process = subprocess.Popen([str(binary), str(request_path), str(output)], stdout=stdout, stderr=stderr,
                creationflags=getattr(subprocess, "BELOW_NORMAL_PRIORITY_CLASS", 0))
            timed_out = False
            try:
                code = process.wait(timeout=60)
            except subprocess.TimeoutExpired:
                timed_out = True
                process.kill()
                code = process.wait()
        elapsed = time.monotonic() - started
        receipt = dict(name=name, pid=process.pid, seconds=elapsed, exit_code=code, timed_out=timed_out,
            output_sha256=sha(output) if output.exists() else None)
        write(root / f"{name}.execution.json", receipt)
        executions.append(receipt)
        assert code == 0 and not timed_out, receipt
        assert sum(row["seconds"] for row in executions) < 240
        if replay:
            assert sha(output) == sha(root / "outputs" / f"{mode}-seat{seat}.json")
        return read(output)

    try:
        baseline0 = execute("baseline", 0)
        count0 = audit_baseline(baseline0, read(args.previous / "outputs/case0-g115-seat0.json"))
        projected = executions[0]["seconds"] * 6
        write(root / "timing-qualification.json", dict(projected_seconds=projected, cap=240, passed=projected < 240))
        assert projected < 240
        execute("baseline", 0, True)
        baseline1 = execute("baseline", 1)
        count1 = audit_baseline(baseline1, read(args.previous / "outputs/case0-g115-seat1.json"))
        changed0 = execute("unique_printed_color", 0)
        changed1 = execute("unique_printed_color", 1)
        audits = [audit_intervention(changed0, baseline0, 0), audit_intervention(changed1, baseline1, 1)]
        execute("unique_printed_color", 0, True)
        results = dict(status="COMPLETE", executions=executions, total_worker_seconds=sum(r["seconds"] for r in executions),
            baseline_decisions_reproduced=[count0, count1], intervention_audits=audits,
            outcomes=[dict(seat=s, baseline=b["outcome"], intervention=c["outcome"], baseline_games=b["games"], intervention_games=c["games"])
                      for s,b,c in [(0,baseline0,changed0), (1,baseline1,changed1)]],
            non_claim="Two exposed selected cases are causal debugging evidence only, not a win-rate estimate, retention test or promotion.")
        write(root / "completion.json", results)
        print(json.dumps({key:value for key,value in results.items() if key not in ["intervention_audits", "outcomes", "executions"]}))
    except BaseException as error:
        write(root / "failure.json", dict(status="INCOMPLETE", error=repr(error), executions=executions))
        raise


if __name__ == "__main__":
    main()
