"""Replay old BO3s and new entropy checkpoints with the pinned CPU scorer."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import time

from public_training_dispatch_v2 import read, write, pin, checked
from public_evaluation_dispatch_v1 import inventory, prepare_remote, dispatch
from public_training_storage_v1 import storage
from qualify_public_evaluation_v1 import execute

OLD = Path("E:/mtg-meta-recovery-20260920/state-prevention-bo3-001/evaluation-manifest.json")
ENGINEERING = Path("E:/mtg-meta-recovery-20260921/public-entropy-engineering-001")
BUILD = Path("E:/mtg-meta-recovery-20260921/public-entropy-tools-002/build-completion.json")


def run(root, host, transfer_binary):
    current = inventory(host)
    assert not current["active"], "preserve competing native owners"
    build = read(BUILD)
    item = build["binaries"]["public_feature_evaluation_v1"]
    binary_pin = {key: item[key] for key in ["path", "sha256"]}
    binary = checked(binary_pin)
    engineering = read(ENGINEERING / "result.json")
    assert engineering["complete"] and engineering["natural_games"] == 80
    model = dict(kind="public_checkpoint", config=pin(ENGINEERING / "entropy-config.json"),
                 checkpoint=pin(ENGINEERING / "entropy-w10/outputs/0001/checkpoint.json"))
    root.mkdir()
    old = read(OLD)
    # The verifier reproduces the full transfer envelope including build provenance.
    # Use the native writer from the scorer's exact source, never edit its receipt.
    transfer_request = read(Path("E:/mtg-postboard-campaign-20260920/state-prevention-evaluation-002/transfer-request.json"))
    transfer_request["output_directory"] = str(root / "adapter")
    write(root / "transfer-request.json", transfer_request)
    execute(root, "transfer", transfer_binary, root / "transfer-request.json")
    old_descriptor = read(Path("E:/mtg-postboard-campaign-20260920/state-prevention-evaluation-002/adapter/play-import-source.json"))
    new_descriptor_pin = pin(root / "adapter/play-import-source.json")
    new_descriptor = read(checked(new_descriptor_pin))
    before = read(checked(old_descriptor["transfer_envelope"]))
    after = read(checked(new_descriptor["transfer_envelope"]))
    old_build = before["receipt"]["destination_build_git_head"]
    before["receipt"]["destination_build_git_head"] = after["receipt"]["destination_build_git_head"]
    assert before == after, "transfer changed parameters, optimizer or non-build provenance"
    assert after["receipt"]["destination_build_git_head"] == read(Path(BUILD.parent / "build-start.json"))["commit"]
    for field in ["source_checkpoint", "source_registry"]:
        assert old_descriptor[field] == new_descriptor[field]
    write(root / "import-comparison.json", dict(complete=True, only_changed_field="receipt.destination_build_git_head",
        old_build=old_build, new_build=after["receipt"]["destination_build_git_head"],
        old=old_descriptor["transfer_envelope"], new=new_descriptor["transfer_envelope"],
        source=new_descriptor_pin, writer=pin(transfer_binary), execution=pin(root / "transfer.execution.json")))
    labels = [f"{cohort}-{own}-p{seat}" for cohort, own in [
        ("canonical", "Affinity"), ("focal", "published-44ae71e1e126b63d")] for seat in [0, 1]]
    jobs = []
    for label in labels:
        original = next(job for job in old["jobs"] if job["arm"] == "g115" and job["label"] == label)
        request = read(checked(original["request"]))
        reference = pin(Path(original["output_directory"]) / "match-000000.json")
        old_completion = read(Path(original["output_directory"]) / "completion.json")
        assert reference["sha256"] == old_completion["match_sha256"][0]
        request["matches"] = request["matches"][:1]
        seat = int(label[-1])
        for variant in ["legacy", "entropy", "replay"]:
            name = f"{variant}-{label}"
            command = copy.deepcopy(request)
            command["sources"][1-seat]["source"]["play_import"] = new_descriptor_pin
            if variant != "legacy":
                command["sources"][seat] = model
            command["output_directory"] = str(root / name)
            write(root / f"{name}.request.json", command)
            jobs.append(dict(label=name, case=label, variant=variant, seat=seat,
                request=pin(root / f"{name}.request.json"), reference=reference))
    write(root / "manifest.json", dict(runner=pin(__file__), build=pin(BUILD), binary=binary_pin,
        original=pin(OLD), engineering=pin(ENGINEERING / "result.json"), model=model,
        import_comparison=pin(root / "import-comparison.json"),
        jobs=jobs, inventory=current, host=host, maximum_matches=12, workers=4,
        dependencies=[pin(Path(__file__).with_name(n)) for n in ["qualify_public_evaluation_v1.py", "public_evaluation_dispatch_v1.py"]],
        question="Does the new scorer preserve original BO3 bytes and deterministically play learned nonzero-entropy checkpoints in both seats?",
        non_claim="Reused fixed engineering cases, no strength or throughput qualification; native transfer writer changes only verified build provenance, not weights."))
    started = time.monotonic()
    def one(job):
        return execute(root, job["label"], binary, checked(job["request"]))
    folders = {}
    if host == "jack":
        first = one(jobs[0])
        assert first["seconds"] < 20, "cheap scorer timing exceeded envelope"
        with ThreadPoolExecutor(max_workers=4) as pool:
            executions = [first] + list(pool.map(one, jobs[1:]))
        folders = {job["label"]: root / job["label"] for job in jobs}
    else:
        assets = {item["path"]: item for item in [model["config"], model["checkpoint"],
            pin(ENGINEERING / "entropy-w10/outputs/0001/optimizer.json")]}
        # Preserve the exact existing opponent import rather than fabricate a
        # receipt for the new scorer. Native loading/replay must establish compatibility.
        for job in jobs:
            source = read(checked(job["request"]))["sources"][1-job["seat"]]["source"]["play_import"]
            assets[source["path"]] = source
            for value in read(checked(source)).values():
                if isinstance(value, dict) and "path" in value and "sha256" in value:
                    assets[value["path"]] = value
        remote = prepare_remote(root, list(assets.values()))
        store = storage(current, "C")
        allocation = {host: dict(drive="C", disk_serial=store["disk_serial"], disk_name=store["disk_name"], workers=4)}
        commands = [dict(id=job["label"], arm=job["variant"], label=job["case"],
                         command=read(checked(job["request"]))) for job in jobs]
        first_pin = dispatch(root, "cheap-one", binary_pin, commands[:1], allocation, remote, 60)
        first = read(checked(first_pin))
        assert first["execution_seconds"] < 25, "cheap remote scorer timing exceeded envelope"
        remainder_pin = dispatch(root, "remaining-eleven", binary_pin, commands[1:], allocation, remote, 180)
        output_jobs = first["jobs"] + read(checked(remainder_pin))["jobs"]
        executions = [read(checked(job["execution"])) for job in output_jobs]
        folders = {job["id"]: Path(job["output_directory"]) for job in output_jobs}
    games = decisions = 0
    for job in jobs:
        folder = folders[job["label"]]
        completion = read(folder / "completion.json")
        match_pin = pin(folder / "match-000000.json")
        match = read(checked(match_pin))
        request = read(checked(job["request"]))
        assert completion["matches"] == 1 and completion["match_sha256"] == [match_pin["sha256"]]
        assert match["match"] == request["matches"][0]
        assert completion["natural_games"] == len(match["games"]) == len(match["seed_resets"])
        assert match["diagnostic_spell_target_repairs"] == [0, 0]
        if job["variant"] == "legacy":
            expected = read(checked(job["reference"]))
            provenance = expected["models"][1-job["seat"]]["identity"]["source_import"]
            suffix = "; envelope_sha256=" + old_descriptor["transfer_envelope"]["sha256"]
            assert provenance["appended_rows"].endswith(suffix)
            provenance["appended_rows"] = provenance["appended_rows"][:-len(suffix)] + "; envelope_sha256=" + new_descriptor["transfer_envelope"]["sha256"]
            assert match == expected, "new scorer changed more than the verified import provenance"
        else:
            assert match["models"][job["seat"]]["public_adam_step"] == 2
            assert match["models"][job["seat"]]["inputs_enabled"] is False
            if job["variant"] == "replay":
                assert match_pin["sha256"] == pin(folders[f"entropy-{job['case']}"] / "match-000000.json")["sha256"]
        games += completion["natural_games"]
        decisions += completion["decisions"]
    result = dict(complete=True, matches=12, natural_games=games, decisions=decisions,
        original_gameplay_replays=4, exact_new_entropy_bo3_replays=4,
        original_comparison="All JSON fields equal after only opponent identity.source_import.appended_rows envelope digest changes; raw old/new files are not byte-identical.",
        source_parameters_and_optimizer_preserved=True, import_comparison=pin(root / "import-comparison.json"), physical_seats=[0, 1],
        native_seconds=sum(item["seconds"] for item in executions), wall_seconds=time.monotonic()-started,
        non_claim="Engineering only, not a playing-strength estimate or allocation qualification.")
    write(root / "qualification.json", result)
    print(result, flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--host", choices=["jack", "haleyspc"], default="jack")
    parser.add_argument("--transfer-binary", type=Path, required=True)
    if not __debug__:
        raise RuntimeError("run with Python validation enabled")
    args = parser.parse_args()
    run(args.root, args.host, args.transfer_binary)
