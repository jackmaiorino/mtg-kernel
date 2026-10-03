"""Bounded Gates BO3 compatibility and replay, with no outcome selection."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import subprocess

from qualify_public_evaluation_v1 import CAMPAIGN, execute, pin, read, sha, write

TRAIN = CAMPAIGN / "public-state-only-engineering-001"
COVERAGE = CAMPAIGN / "public-canonical-coverage-002"
GATES = "published-44ae71e1e126b63d"


def verify(item):
    assert sha(item["path"]) == item["sha256"], item
    return Path(item["path"])


def prepare(root, binary, transfer_binary):
    root.mkdir()
    old_adapter = CAMPAIGN / "public-spell-adapter-qualification-004"
    transfer = read(old_adapter / "transfer-request.json")
    transfer["output_directory"] = str(root/"adapter")
    write(root/"transfer-request.json", transfer)
    execute(root, "transfer", transfer_binary, root/"transfer-request.json")
    old_source = read(old_adapter/"adapter/play-import-source.json")
    new_source = read(root/"adapter/play-import-source.json")
    old_envelope = read(verify(old_source["transfer_envelope"]))
    new_envelope = read(verify(new_source["transfer_envelope"]))
    old_build = old_envelope["receipt"]["destination_build_git_head"]
    old_envelope["receipt"]["destination_build_git_head"] = new_envelope["receipt"]["destination_build_git_head"]
    assert old_envelope == new_envelope, "V3 rebind changed parameters or optimizer"
    for key in ["source_checkpoint", "source_registry"]:
        assert old_source[key]["sha256"] == new_source[key]["sha256"]
    write(root/"import-comparison.json", dict(only_changed_field="receipt.destination_build_git_head",
        old_build=old_build, new_build=new_envelope["receipt"]["destination_build_git_head"],
        old=old_source["transfer_envelope"], new=new_source["transfer_envelope"]))
    roster = {}
    requests = []
    coverage = read(COVERAGE / "manifest.json")
    opponent = None
    for job in coverage["jobs"]:
        command = read(verify(job["request"]))
        seat = int(job["label"][-1])
        other = command["sources"][1-seat]
        assert opponent is None or opponent == other
        opponent = other
        for item in command["matches"]:
            for deck in item["registered"]:
                assert deck["label"] not in roster or roster[deck["label"]] == deck
                roster[deck["label"]] = deck
    fixture_path = CAMPAIGN / "prevention-correction-replay-001/request-seat0.json"
    opponent = copy.deepcopy(opponent)
    opponent["source"]["play_import"] = pin(root/"adapter/play-import-source.json")
    fixture = read(fixture_path)
    roster[GATES] = fixture["registrations"][0]
    assert len(roster) == 8 and roster[GATES]["label"] == GATES
    pairs = [(own, other) for own in sorted(roster) for other in sorted(roster)
             if GATES in (own, other)]
    assert len(pairs) == 15
    checkpoints = {arm: dict(kind="public_checkpoint", config=pin(TRAIN/f"configs/{arm}.json"),
        checkpoint=pin(TRAIN/f"outputs/{arm}-w2/0003/checkpoint.json"))
        for arm in ["control", "structured"]}
    for index, (own, other) in enumerate(pairs):
        for seat in [0, 1]:
            label = f"case-{index:02}-s{seat}"
            registered = [roster[own], roster[other]]
            if seat == 1:
                registered.reverse()
            config = copy.deepcopy(fixture["config"])
            config["deck_ids"] = [d["label"] for d in registered]
            config["game_one_chooser"] = seat if index % 2 == 0 else 1-seat
            command = dict(sources=[checkpoints["structured"], opponent] if seat == 0 else [opponent, checkpoints["structured"]],
                matches=[dict(config=config, registered=registered, postboard=None)],
                cross_generation_evaluation=True, capture_decisions=True, output_directory=str(root/label))
            write(root/f"{label}-request.json", command)
            requests.append(dict(label=label, seat=seat, arm="structured", request=pin(root/f"{label}-request.json")))
    # The first ordered cell in each physical seat, chosen without outcomes.
    for seat in [0, 1]:
        for group in ["control", "replay"]:
            label = f"{group}-s{seat}"
            command = copy.deepcopy(read(requests[seat]["request"]["path"]))
            if group == "control":
                command["sources"][seat] = checkpoints["control"]
            command["output_directory"] = str(root/label)
            write(root/f"{label}-request.json", command)
            requests.append(dict(label=label, seat=seat, arm="control" if group == "control" else "structured",
                                 request=pin(root/f"{label}-request.json")))
    write(root/"manifest.json", dict(binary=pin(binary), transfer_binary=pin(transfer_binary),
        import_comparison=pin(root/"import-comparison.json"), runner=pin(__file__),
        helper=pin(Path(__file__).with_name("qualify_public_evaluation_v1.py")),
        source=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        training_qualification=pin(TRAIN/"qualification.json"), fixture=pin(fixture_path),
        jobs=requests, opponent=opponent, expected_matches=34, workers=4,
        first_case_seconds_cap=15, total_process_seconds_cap=300,
        non_claim="Engineering only: one reused seed, four-update checkpoints, 15 Gates ordered cells and familiar V3 opponent. No strength estimate, selection, or promotion. Fable review remains unavailable under known quota error."))


def run(root):
    m = read(root/"manifest.json")
    binary = verify(m["binary"])
    verify(m["runner"])
    verify(m["helper"])
    def job(item):
        return execute(root, item["label"], binary, verify(item["request"]))
    first = job(m["jobs"][0])
    assert first["seconds"] < m["first_case_seconds_cap"], "cheap timing gate failed"
    errors = []
    with ThreadPoolExecutor(max_workers=m["workers"]) as pool:
        pending = [pool.submit(job, item) for item in m["jobs"][1:]]
        for future in pending:
            try:
                future.result()
            except Exception as error:
                errors.append(str(error))
    assert not errors, errors
    games = decisions = forced = repairs = 0
    seconds = 0
    for item in m["jobs"]:
        label, seat = item["label"], item["seat"]
        execution = read(root/f"{label}.execution.json")
        assert execution["exit_code"] == 0 and not execution["timeout"]
        seconds += execution["seconds"]
        completion = read(root/label/"completion.json")
        path = root/label/"match-000000.json"
        assert completion["matches"] == 1 and completion["match_sha256"] == [sha(path)]
        match = read(path)
        assert match["match"] == read(verify(item["request"]))["matches"][0]
        assert completion["natural_games"] == len(match["games"]) == len(match["seed_resets"])
        assert completion["decisions"] == match["decision_count"] == len(match["decisions"])
        assert all(0 <= row["selected"] < row["legal_action_count"] for row in match["decisions"])
        assert match["models"][seat]["public_adam_step"] == 4
        assert match["models"][seat]["projection_mode"] == "state_only"
        assert match["models"][seat]["inputs_enabled"] == (item["arm"] == "structured")
        assert match["diagnostic_spell_target_repairs"] == [0, 0]
        games += len(match["games"])
        decisions += match["decision_count"]
        forced += sum(match["v3_forced_actions"])
        repairs += sum(match["v3_spell_target_repairs"])
    for seat in [0, 1]:
        assert (root/f"case-00-s{seat}/match-000000.json").read_bytes() == (root/f"replay-s{seat}/match-000000.json").read_bytes()
    assert seconds < m["total_process_seconds_cap"]
    result = dict(complete=True, executed_matches=34, unique_arm_cases=32, ordered_focal_matchups=15,
        physical_seats=[0, 1], natural_games=games, decisions=decisions, v3_forced_actions=forced,
        v3_reference_target_repairs=repairs, exact_fresh_process_replays=2, process_seconds=seconds,
        non_claim=m["non_claim"])
    write(root/"qualification.json", result)
    print(result, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "run"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--transfer-binary", type=Path)
    args = parser.parse_args()
    if args.mode == "prepare":
        prepare(args.root, args.binary, args.transfer_binary)
    else:
        run(args.root)
