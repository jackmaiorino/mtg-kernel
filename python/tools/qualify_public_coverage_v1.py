"""Release engineering coverage of the fixed seven-list BO3 development roster.

Reads historical inputs only. Outcomes never choose cases or models.
"""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import shutil
import subprocess

from qualify_public_evaluation_v1 import read, pin, sha, write, execute, CAMPAIGN
from qualify_public_spell_adapter_v1 import selected_zones

OLD = CAMPAIGN / "bo3-qualification-003"
ADAPTER = CAMPAIGN / "public-spell-adapter-qualification-003"


def prepare(root, binary):
    assert read(ADAPTER / "qualification.json")["status"] == "PUBLIC-EVALUATOR-SPELL-REPAIR-ENGINEERING-PASS"
    root.mkdir()
    shutil.copy2(binary, root / binary.name)
    template = read(ADAPTER / "structured-s0-request.json")
    candidate, opponent = template["sources"]
    jobs = []
    cells = set()
    for path in sorted((OLD / "configs").glob("*.json")):
        historical = read(path)
        seat = int(path.stem[-1])
        items = []
        for item in historical["matches"]:
            config = item["config"]
            key = (config["deck_ids"][seat], config["deck_ids"][1-seat], seat)
            assert key not in cells
            cells.add(key)
            items.append(dict(config=config, registered=item["registered"],
                              postboard=selected_zones(item, historical["policies"])))
        command = dict(sources=[candidate, opponent] if seat == 0 else [opponent, candidate],
                       matches=items, cross_generation_evaluation=True, capture_decisions=False,
                       output_directory=str(root / path.stem))
        write(root / f"{path.stem}-request.json", command)
        jobs.append(dict(label=path.stem, matches=len(items), input=pin(path),
                         request=pin(root / f"{path.stem}-request.json")))
    assert len(jobs) == 14 and len(cells) == 98 and all(j["matches"] == 7 for j in jobs)
    replay = copy.deepcopy(read(jobs[0]["request"]["path"]))
    replay["matches"] = replay["matches"][:1]
    replay["output_directory"] = str(root / "replay")
    write(root / "replay-request.json", replay)
    write(root / "manifest.json", dict(binary=pin(root / binary.name), jobs=jobs,
        replay=pin(root / "replay-request.json"), qualification=pin(ADAPTER / "qualification.json"),
        source_commit=subprocess.check_output(["git", "rev-parse", "HEAD"], text=True).strip(),
        runner=pin(__file__), cells=sorted(cells),
        design=dict(question="Can a learned public-input model finish every canonical matchup in both seats with deterministic replay at useful release cost?",
            unique_matches=98, executed_matches=99, workers=4, all_natural=True,
            replay_bytes_exact=True, first_job_projected_worker_seconds_limit=300,
            total_worker_seconds_limit=300, per_job_seconds_limit=180,
            projected_2352_match_seconds_limit=3600,
            non_claim="Engineering only. Four-update checkpoint is not a pilot parent. Seven fixed lists, static sideboards, KeepSevenV2 and one familiar development V3 opponent do not establish field coverage or strength. No CP7 selection or paid compute. Fable zero-read quota gap remains.")))


def run(root):
    manifest = read(root / "manifest.json")
    binary = Path(manifest["binary"]["path"])
    assert sha(binary) == manifest["binary"]["sha256"]
    assert sha(__file__) == manifest["runner"]["sha256"]
    def job(item):
        assert sha(item["request"]["path"]) == item["request"]["sha256"]
        return execute(root, item["label"], binary, Path(item["request"]["path"]))
    first = job(manifest["jobs"][0])
    projected = first["seconds"] / 7 * 98
    write(root / "timing.json", dict(first_job_seconds=first["seconds"], projected_worker_seconds=projected, limit=300))
    assert projected < 300, "first release job exceeds frozen timing qualification"
    # Retrieve every dispatched result, including failures, before returning.
    errors = []
    with ThreadPoolExecutor(max_workers=4) as pool:
        pending = [pool.submit(job, item) for item in manifest["jobs"][1:]]
        for future in pending:
            try: future.result()
            except Exception as error: errors.append(str(error))
    assert not errors, errors
    execute(root, "replay", binary, root / "replay-request.json")
    analyze(root)


def analyze(root):
    manifest = read(root / "manifest.json")
    games = decisions = repaired = moves = 0
    seconds = 0
    cells = []
    for job in manifest["jobs"]:
        label = job["label"]
        request = read(job["request"]["path"])
        assert sha(job["request"]["path"]) == job["request"]["sha256"]
        receipt = read(root / f"{label}.execution.json")
        assert receipt["exit_code"] == 0 and not receipt["timeout"]
        seconds += receipt["seconds"]
        completion = read(root / label / "completion.json")
        assert completion["matches"] == len(request["matches"]) == len(completion["match_sha256"]) == 7
        job_games = job_decisions = 0
        for index, expected in enumerate(completion["match_sha256"]):
            path = root / label / f"match-{index:06}.json"
            assert sha(path) == expected
            doc = read(path)
            assert doc["match"] == request["matches"][index]
            assert not doc["decisions"] and doc["decision_count"] > 0
            assert len(doc["games"]) == len(doc["seed_resets"])
            # The pinned BO3 runner obtains each game from game_summary_v1,
            # which rejects every non-natural terminal before publication.
            assert [g["start"]["game_index"] for g in doc["games"]] == list(range(1, len(doc["games"])+1))
            assert all(g["winner"] in [None, 0, 1] for g in doc["games"])
            assert doc["diagnostic_spell_target_repairs"] == [0, 0]
            job_games += len(doc["games"])
            job_decisions += doc["decision_count"]
            repaired += sum(doc["v3_spell_target_repairs"])
            moves += sum(len(row["selected_actions"]) for row in doc["sideboard_decisions"])
            seat = int(label[-1])
            ids = doc["match"]["config"]["deck_ids"]
            cells.append((ids[seat], ids[1-seat], seat))
        assert job_games == completion["natural_games"] and job_decisions == completion["decisions"]
        games += job_games
        decisions += job_decisions
    assert sorted(cells) == [tuple(c) for c in manifest["cells"]] and len(set(cells)) == 98
    replay = read(root / "replay.execution.json")
    assert replay["exit_code"] == 0 and not replay["timeout"]
    first_label = manifest["jobs"][0]["label"]
    a = root / first_label / "match-000000.json"
    b = root / "replay/match-000000.json"
    assert a.read_bytes() == b.read_bytes()
    projected = seconds / 98 * 2352
    assert seconds + replay["seconds"] < 300 and projected < 3600
    write(root / "qualification.json", dict(status="PUBLIC-CANONICAL-COVERAGE-ENGINEERING-PASS",
        distinct_matches=98, executed_matches=99, natural_games=games,
        replay_games=len(read(b)["games"]), decisions=decisions, scoring_repairs=repaired,
        sideboard_moves=moves, exact_fresh_process_replay=True,
        worker_seconds=seconds, replay_seconds=replay["seconds"],
        projected_2352_match_seconds=projected, cells=sorted(cells),
        non_claim=manifest["design"]["non_claim"]))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=["prepare", "run", "analyze"])
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()
    if args.mode == "prepare": prepare(args.root, args.binary)
    elif args.mode == "run": run(args.root)
    else: analyze(args.root)


if __name__ == "__main__": main()
