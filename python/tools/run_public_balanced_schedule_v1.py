"""Guarded schedule-only training with complete natural-game and optimizer audit."""
import argparse
import copy
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path
import os
import shutil
import time

from public_training_dispatch_v2 import read, write, pin, checked
from public_training_storage_v1 import require_storage_choice, dispatch_qualified
from run_state_prevention_training_v1 import snapshot
from public_balanced_schedule_analysis_v1 import statistical_checks, breadth_checks, bound_opponent


def audit(group_pin, manifest, root, remote_only=False):
    group = read(checked(group_pin))
    assert set(group["jobs"]) == {"control", "balanced"}
    if remote_only:
        recovery_path = checked(group_pin).parent / "haleyspc/recovery-verification.json"
        archived = read(recovery_path)
        assert archived["mismatches"] == 0
        archive = recovery_path.parent / "results.zip"
        assert pin(archive)["sha256"] == archived["archive"]["zip_sha256"]
        archive_pin = pin(recovery_path)
    else:
        archived = read(checked(group["archive"]))
        assert archived["mismatches"] == 0
        for shard in archived["shards"]:
            checked(shard["archive"])
        archive_pin = group["archive"]
    arms = {}
    for arm, report_pin in group["jobs"].items():
        report = read(checked(report_pin))
        if remote_only:
            assert report["placement"]["host"] == "haleyspc"
        assert report["config"] == manifest["training_configs"][arm]
        execution = read(checked(report["execution"]))
        assert execution["exit_code"] == 0 and not execution["timeout"]
        assert execution["observed_gpu_uuid"] == report["placement"]["gpu_uuid"]
        completion = read(checked(report["completion"]))
        assert completion["first_update"] == 0 and completion["next_update"] == 200
        assert len(completion["receipts"]) == 200
        expected = set()
        for index, receipt in enumerate(completion["receipts"]):
            assert receipt["update"] == index and receipt["episodes"] == receipt["natural_games"] == 10
            expected.update(f"{index:04}/{name}" for name in ["checkpoint.json", "optimizer.json"] + [f"episode-{i:03}.json" for i in range(10)])
            checkpoint = read(checked(report["outputs"][f"{index:04}/checkpoint.json"]))
            assert checkpoint["optimizer_sha256"] == report["outputs"][f"{index:04}/optimizer.json"]["sha256"]
            assert checkpoint["trajectory_sha256"] == [report["outputs"][f"{index:04}/episode-{i:03}.json"]["sha256"] for i in range(10)]
        assert set(report["outputs"]) == expected
        for item in report["outputs"].values():
            checked(item)
        final = read(checked(report["outputs"]["0199/optimizer.json"]))
        assert final["legacy_adam_step"] == 32600 and final["public"]["adam_step"] == 200
        for field in ["object", "object_first", "object_second", "state", "state_first", "state_second"]:
            assert set(final["public"][field]) <= {0, 1 << 31}
        endpoint = root / "endpoints" / arm
        endpoint.mkdir(parents=True)
        pins = {}
        for field in ["checkpoint", "optimizer"]:
            original = report["outputs"][f"0199/{field}.json"]
            shutil.copy2(checked(original), endpoint / f"{field}.json")
            pins[field] = pin(endpoint / f"{field}.json")
            assert pins[field]["sha256"] == original["sha256"]
        arms[arm] = dict(complete=True, updates=200, natural_games=2000, legacy_adam_step=32600,
            public_adam_step=200, **pins, report=report_pin, seconds=execution["seconds"], placement=report["placement"])
    return dict(complete=True, full_natural_games=4000, arms=arms, group=group_pin, archive=archive_pin,
        non_claim="Completed learning only. Do not select or claim strength before both complete independent BO3 panels.")


def run(root, compute, scorer):
    from compute_throughput_v2 import require_allocation
    from public_entropy_remote_replica_v1 import jobs_for
    from public_entropy_available_replica_v2 import availability
    from public_training_dispatch_v2 import dispatch_qualified as remote_dispatch
    m, q = read(root/'manifest.json'), read(compute/'qualification.json')
    qm = read(compute/'manifest.json')
    assert m['schema'] == 'matched-public-balanced-schedule/v1'
    assert q['complete'] and not q['full_training_launched']
    assert qm['pilot'] == pin(root/'manifest.json')
    assert not (root/'training-launch.json').exists(), 'preserve existing measurement'
    for item in [m['runner'], m['design'], m['analysis'], m['bootstrap_implementation'],
                 *m['dependencies'], qm['runner'], *qm['dependencies']]:
        checked(item)
    assert m['analysis'] == pin(Path(__file__).with_name('public_balanced_schedule_analysis_v1.py'))
    choice = read(compute/'compute-choice.json')
    remote_only = 'archive_scheme' not in choice
    if remote_only:
        selected = require_allocation(compute/'compute-choice.json', m['training_binary']['sha256'], jobs_for(m['training_configs']))
        fresh = compute/'launch-availability'
        fresh.mkdir()
        availability(fresh)
        dispatcher = remote_dispatch
    else:
        selected = require_storage_choice(compute/'compute-choice.json', m['training_binary'], m['training_configs'])
        dispatcher = dispatch_qualified
    assert selected == q['selected'] and selected['projected_seconds'] < 3600
    sq = read(scorer/'qualification.json')
    assert sq['complete'] and sq['original_gameplay_replays'] == sq['exact_new_entropy_bo3_replays'] == 4
    assert read(scorer/'manifest.json')['binary'] == m['evaluation_binary']
    comparison = read(checked(sq['import_comparison']))
    opponent = copy.deepcopy(m['evaluation_opponent'])
    opponent['source']['play_import'] = comparison['source']
    write(root/'evaluation-binding.json', dict(pilot=pin(root/'manifest.json'), binary=m['evaluation_binary'],
        qualification=pin(scorer/'qualification.json'), opponent=opponent,
        scope='Reuse unchanged native scorer/import equivalence; new panel still requires exact-workload v2 throughput/replay qualification.'))
    bound_opponent(root, m); statistical_checks(); breadth_checks()
    cuda = Path('C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/v12.8')
    os.environ['CUDA_PATH'] = str(cuda)
    os.environ['PATH'] = str(cuda/'bin') + os.pathsep + os.environ['PATH']
    (root/'native-temp').mkdir()
    os.environ['TEMP'] = os.environ['TMP'] = str(root/'native-temp')
    write(root/'training-launch.json', dict(pilot=pin(root/'manifest.json'), compute=pin(compute/'qualification.json'),
        choice=pin(compute/'compute-choice.json'), selected=selected, runner=pin(__file__),
        analysis=m['analysis'], bootstrap_implementation=m['bootstrap_implementation'],
        evaluation_binding=pin(root/'evaluation-binding.json'), expected_natural_games=4000,
        native_wall_cap_seconds=1800, no_prefix_selection=True, no_paid_compute=True, review=m['review']))
    telemetry = root/'training-telemetry'; telemetry.mkdir()
    hosts = sorted({p['host'] for p in selected['placements'].values()})
    started = time.monotonic()
    with ThreadPoolExecutor(max_workers=1) as pool:
        future = pool.submit(dispatcher, root/f"balanced-replica-{m['replica']}-training", m['training_binary'],
            m['training_configs'], compute/'compute-choice.json', 1800)
        tick = 0
        while not future.done():
            row = dict(elapsed_seconds=time.monotonic()-started, hosts={})
            for host in hosts:
                try:
                    row['hosts'][host] = snapshot(host)
                except Exception as error:
                    row['hosts'][host] = dict(telemetry_error=str(error))
            write(telemetry/f'{tick:04}.json', row)
            print('training telemetry', round(row['elapsed_seconds']),
                  {h: v.get('cpu_percent', 'unavailable') for h, v in row['hosts'].items()}, flush=True)
            tick += 1
            for _ in range(30):
                if future.done(): break
                time.sleep(1)
        result = future.result()
    audited = audit(result, m, root, remote_only=remote_only)
    audited['wall_seconds_including_audit'] = time.monotonic()-started
    write(root/'training-audit.json', audited)
    print(dict(complete=True, natural_games=4000, root=str(root)), flush=True)


if __name__ == '__main__':
    if not __debug__:
        raise RuntimeError('Validation must remain enabled')
    p = argparse.ArgumentParser()
    p.add_argument('--root', type=Path, required=True)
    p.add_argument('--compute', type=Path, required=True)
    p.add_argument('--scorer', type=Path, required=True)
    a = p.parse_args()
    run(a.root, a.compute, a.scorer)
