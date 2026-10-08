"""Bounded repeated timing and actual full-output recovery on eligible PCs."""
import argparse
import json
from pathlib import Path
import subprocess
import time
import zipfile

from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, ssh, REMOTE, prepare_remote
from public_evaluation_dispatch_v2 import dispatch
from public_training_storage_v1 import storage
from evaluation_recovery_fixture_v1 import assignment, host_hashes, fingerprint, stage, recover_local
from evaluation_throughput_v4 import require_choice


def remote_recovery(root, remote, native, hashes, map_path, index):
    destination = root / f'remote-recovery-{index}'
    destination.mkdir()
    began = before = time.monotonic()
    archive_native = f'{native}-recovery-{index}.zip'
    exported = json.loads(ssh(f"& python '{remote['native_root']}/fixture.py' export '{native}' '{archive_native}' '{map_path}'", timeout=180))
    stages = dict(export=time.monotonic()-before)
    before = time.monotonic()
    subprocess.run(['scp', '-q', f'{REMOTE}:{archive_native}', str(destination/'results.zip')], check=True, timeout=180)
    stages['download'] = time.monotonic()-before
    before = time.monotonic()
    archive_pin = pin(destination/'results.zip')
    assert archive_pin['sha256'] == exported['archive']['sha256']
    recovered = destination/'recovered'
    recovered.mkdir()
    with zipfile.ZipFile(checked(archive_pin)) as archive:
        assert set(archive.namelist()) == set(hashes) | {'export-manifest.json'}
        assert all((recovered/n).resolve().is_relative_to(recovered.resolve()) for n in archive.namelist())
        archive.extractall(recovered)
    assert read(recovered/'export-manifest.json') == hashes
    for name, digest in hashes.items(): assert pin(recovered/name)['sha256'] == digest
    stages['extract_and_verify'] = time.monotonic()-before
    return dict(seconds=time.monotonic()-began, stage_seconds=stages, archive=archive_pin,
                files=len(hashes), hashes=hashes, mismatches=0, recovered_directory=str(recovered),
                uncompressed_bytes=sum((recovered/n).stat().st_size for n in hashes),
                match_bytes=sum((recovered/n).stat().st_size for n in hashes if '/outputs/match-' in n))


def calibrate(root, label, allocation, reference_pin, source_pin, recovery_pin, remote):
    folder = root/f'fixture-{label}'
    folder.mkdir()
    reference, source, recovered = read(checked(reference_pin)), read(checked(source_pin)), read(checked(recovery_pin))
    assert all(j['recovery'] == recovery_pin for j in source['jobs'])
    source_local = checked(recovery_pin).parent/'recovered'
    assigned = assignment(reference['jobs'], allocation)
    placed = {}
    for host, settings in allocation.items():
        hashes = host_hashes(recovered['hashes'], assigned[host], host)
        map_file = folder/f'{host}-hashes.json'
        write(map_file, hashes)
        if host == 'desktop':
            native = Path(f"{settings['drive']}:/mtg-evaluation-recovery-fixtures/{root.name}/{label}")
            staged = stage(source_local, native, hashes)
            placed[host] = dict(native=str(native), hashes=hashes)
        else:
            native = remote['native_root']+f'/fixture-{label}'
            remote_map = remote['native_root']+f'/fixture-{label}-hashes.json'
            subprocess.run(['scp', '-q', str(map_file), f'{REMOTE}:{remote_map}'], check=True, timeout=60)
            staged = json.loads(ssh(f"& python '{remote['native_root']}/fixture.py' stage '{recovered['native_directory']}' '{native}' '{remote_map}'", timeout=180))
            placed[host] = dict(native=native, hashes=hashes, remote_map=remote_map)
        assert staged['fingerprint'] == fingerprint(hashes) and staged['files'] == len(hashes)
        write(folder/f'{host}-stage.json', staged)
    samples = []
    for index in range(2):
        began = time.monotonic()
        hosts = {}
        for host in sorted(placed):
            entry = placed[host]
            if host == 'desktop':
                hosts[host] = recover_local(entry['native'], folder/f'local-recovery-{index}', entry['hashes'])
            else:
                hosts[host] = remote_recovery(folder, remote, entry['native'], entry['hashes'], entry['remote_map'], index)
        write(folder/f'sample-{index}.json', dict(schema='placed-recovery-sample/v1', complete=True,
              seconds=time.monotonic()-began, hosts=hosts, source_fingerprint=fingerprint(recovered['hashes']), mismatches=0))
        samples.append(pin(folder/f'sample-{index}.json'))
    write(folder/'calibration.json', dict(schema='placed-full-panel-recovery-fixture/v1', reference_plan=reference_pin,
          binary=reference['binary'], source_dispatch=source_pin, source_recovery=recovery_pin,
          source_fingerprint=fingerprint(recovered['hashes']), assigned_reference_jobs=assigned,
          placements={h: {k:v for k,v in a.items() if k not in ('workers','job_weight')} for h,a in allocation.items()},
          allocation_weights={h:a.get('job_weight',1) for h,a in allocation.items()}, samples=samples,
          dependencies=[pin(Path(__file__).with_name('evaluation_recovery_fixture_v1.py'))],
          limitation='Copies of a completed panel, not new scientific dispatch. Shared reference metadata retained verbatim. Target sample bytes must fit measured match volume.'))
    return pin(folder/'calibration.json')


def reuse_completed_report(path, allocation, snapshots, previous_root):
    """Reuse completed engineering cases only with a clean following owner check."""
    report_pin = pin(path)
    report = read(checked(report_pin))
    assert report['allocation'] == allocation
    require_post_ownership(path, snapshots, previous_root)
    return report_pin


def require_post_ownership(path, snapshots, previous_root):
    from datetime import datetime
    published = path.stat().st_mtime
    following = []
    for p in sorted(previous_root.glob('owner-check-*.json')):
        owners = read(p)
        if min(datetime.fromisoformat(s['at']).timestamp() for s in owners.values()) >= published:
            following.append((p, owners))
    assert following, 'No recorded post-case ownership check; preserve but do not reuse timing'
    owners = following[0][1]
    assert all(bool(owners[h]['active']) == bool(snapshots[h]['active']) for h in snapshots), 'Contended prior timing'


def compatible_recovery(choice_root, reference_pin, allocation, snapshots):
    """Find prior recovery evidence, never prior target gameplay timings."""
    from datetime import datetime, timezone
    completion = read(choice_root/'qualification.json')
    assert completion['complete']
    choice = read(checked(completion['compute_choice']))
    assert choice['schema'] in ('cpu-bo3-allocation/v3', 'cpu-bo3-allocation/v4')
    for host in allocation:
        prior_inventory = read(checked(choice['inventory'][host]['evidence']))
        assert prior_inventory['cpu'] == snapshots[host]['cpu'], 'Recovery hardware changed'
        elapsed = (datetime.now(timezone.utc)-datetime.fromisoformat(prior_inventory['at'])).total_seconds()
        assert 0 <= elapsed < 86400, 'Recovery timing is stale'
    placements = {h:{k:v for k,v in a.items() if k not in ('workers','job_weight')} for h,a in allocation.items()}
    weights = {h:a.get('job_weight',1) for h,a in allocation.items()}
    for candidate in choice['recovery_calibrations'].values():
        path = checked(candidate)
        calibration = read(path)
        if (calibration['reference_plan'] != reference_pin or
            calibration['placements'] != placements or calibration['allocation_weights'] != weights):
            continue
        # The fixture can have originated in a predecessor qualification root.
        require_post_ownership(path, snapshots, path.parents[1])
        return candidate
    return None


def qualify(root, reference, target, reuse=None, reuse_recovery=None, placements_from=None):
    reference_pin, target_pin = pin(reference/'plan.json'), pin(target)
    old, plan = read(checked(reference_pin)), read(checked(target_pin))
    complete = read(reference/'completion.json')
    assert complete['complete'] and complete['matches'] == old['expected_matches'] == 3072
    assert plan['expected_matches'] == 3072 and plan['expected_jobs'] == 48
    assert len(plan['qualification_jobs']) == 48 and all(len(j['command']['matches']) == 1 for j in plan['qualification_jobs'])
    assert old['binary'] == plan['binary']
    source_pin = complete['dispatch']
    source = read(checked(source_pin))
    assert set(source['allocation']) == {'computehost'}
    recovery_pin = source['jobs'][0]['recovery']
    reuse_roots = []
    cursor = reuse
    while cursor is not None:
        assert cursor not in reuse_roots, 'Cyclic reuse chain'
        prior = read(cursor/'design.json')
        assert prior['target'] == target_pin and prior['reference'] == reference_pin
        checked(prior['runner'])
        reuse_roots.append(cursor)
        ancestor = prior.get('reused_design')
        cursor = checked(ancestor).parent if ancestor else None
    root.mkdir()
    available, snapshots = {}, {}
    for host in ('desktop', 'computehost'):
        snap = inventory(host); snapshots[host] = snap
        write(root/f'{host}-inventory.json', snap)
        available[host] = dict(checked_at=snap['at'], evidence=pin(root/f'{host}-inventory.json'),
                               eligible=not snap['active'], reason='Actual native process inventory at qualification start.')
    assert any(available[h]['eligible'] for h in snapshots), 'No eligible idle PC'
    cloud = Path('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json')
    available['runpod'] = dict(checked_at=read(cloud)['checked_at'], evidence=pin(cloud), eligible=False,
                               reason='Authenticated inventory HTTP403; no new paid allocation authorized.')
    if reuse is not None:
        for host in snapshots:
            before = read(reuse/f'{host}-inventory.json')
            assert before['cpu'] == snapshots[host]['cpu'], 'hardware changed'
            assert bool(before['active']) == bool(snapshots[host]['active']), 'eligible host set changed'
    preferred_drives, placement_evidence = {}, None
    if placements_from is not None:
        completed = read(placements_from/'qualification.json')
        assert completed['complete']
        prior_pin = completed['compute_choice']
        prior_choice = read(checked(prior_pin))
        prior_plan = read(checked(prior_choice['plan']))
        assert prior_choice['binary'] == plan['binary']
        assert prior_plan['expected_jobs'] == plan['expected_jobs'] == 48
        assert prior_plan['expected_matches'] == plan['expected_matches'] == 3072
        # Retain the existing measured storage ranking for the same native
        # workload family; measure the new target's worker scaling separately.
        for host in snapshots:
            if not available[host]['eligible']: continue
            old_inventory = read(checked(prior_choice['inventory'][host]['evidence']))
            assert old_inventory['cpu'] == snapshots[host]['cpu'], 'placement hardware changed'
            subset = dict(prior_choice, selected=None,
                candidates=[c for c in prior_choice['candidates'] if set(read(checked(c['report']))['allocation']) == {host}],
                eligible_storage=[s for s in prior_choice['eligible_storage'] if s[0] == host])
            assert subset['candidates'], 'prior grid did not measure this host'
            path = root/f'{host}-inherited-storage.json'
            write(path, subset)
            best = require_choice(path, prior_choice['plan'], plan['binary'])['allocation'][host]
            current = storage(snapshots[host],best['drive'])
            assert current['disk_serial'] == best['disk_serial'] and current['disk_name'] == best['disk_name']
            preferred_drives[host] = (best['drive'],)
        placement_evidence = prior_pin
    stores, cases = [], []
    for host in snapshots:
        if not available[host]['eligible']: continue
        for drive in preferred_drives.get(host, ('C','D','E') if host == 'desktop' else ('C',)):
            disk = storage(snapshots[host], drive)
            settings = {k:disk[k] for k in ('drive','disk_serial','disk_name')}
            stores.append((host,drive,settings['disk_serial']))
            for workers in ((1,12,24) if host == 'desktop' else (1,8,16)):
                cases.append((f'{host}-{drive.lower()}-w{workers}', {host:dict(settings,workers=workers,job_weight=1)}))
    write(root/'design.json',dict(runner=pin(__file__), target=target_pin, reference=reference_pin,
          question='Choose measured eligible CPU/storage allocation using repeated exact replays and full-output recovery measured once per placement, not per-match extrapolation.',
          native_cases=48, maximum_native_matches=96*(len(cases)+(2 if all(available[h]['eligible'] for h in snapshots) else 0)), repeats=2, launch_budget_seconds=1200,
          allocations=cases, possible_cross_allocations='Fastest measured local and remote placements at 1:1 and 2:1 fixed job weights, two timings each.',
          full_panel_launched=False, training=False, paid_compute=False, reused_design=None if reuse is None else pin(reuse/'design.json'),
          reused_recovery=None if reuse_recovery is None else pin(reuse_recovery/'qualification.json'),
          inherited_storage_ranking=placement_evidence,
          review='Fable known zero-read HTTP429 until September22 07:00EDT; no retry or endorsement.',
          limitations='A bounded worker grid, not a global optimum. Reference recovery metadata is conservative; future target lengths can exceed sample estimates.'))
    started = time.monotonic()
    owner_checks = 0
    def owners():
        nonlocal owner_checks
        now = {h:inventory(h) for h in snapshots}
        write(root/f'owner-check-{owner_checks:03}.json',now)
        owner_checks += 1
        for h,s in now.items():
            assert bool(s['active']) != available[h]['eligible'], 'Ownership changed: preserve evidence and qualify the new eligible set'
        return now
    remote = None
    if available['computehost']['eligible']:
        staging = read(target.parent/'remote-staging.json')
        remote = prepare_remote(root, [v for k,v in staging['assets'].items() if k.startswith('inputs/')])
        helper = Path(__file__).with_name('evaluation_recovery_fixture_v1.py')
        subprocess.run(['scp','-q',str(helper),f"{REMOTE}:{remote['native_root']}/fixture.py"],check=True,timeout=60)
    candidates, calibrations, cache = [], {}, {}
    new_matches = reused_matches = 0
    def measure(label, allocation):
        nonlocal new_matches, reused_matches
        assert time.monotonic()-started < 1200, 'Bounded qualification launch budget exhausted'
        key = json.dumps({h:{k:v for k,v in a.items() if k != 'workers'} for h,a in allocation.items()},sort_keys=True)
        if key not in cache:
            previous_calibration = next((r/f'fixture-{label}'/'calibration.json' for r in reuse_roots if (r/f'fixture-{label}'/'calibration.json').is_file()), None)
            shared_calibration = None if reuse_recovery is None else compatible_recovery(reuse_recovery, reference_pin, allocation, snapshots)
            if previous_calibration is not None and previous_calibration.is_file():
                from evaluation_recovery_fixture_v1 import validate_calibration
                require_post_ownership(previous_calibration, snapshots, previous_calibration.parents[1])
                validate_calibration(pin(previous_calibration), allocation, plan, plan['binary'])
                cache[key] = pin(previous_calibration)
            elif shared_calibration is not None:
                from evaluation_recovery_fixture_v1 import validate_calibration
                validate_calibration(shared_calibration, allocation, plan, plan['binary'])
                cache[key] = shared_calibration
            else:
                owners()
                cache[key] = calibrate(root,label,allocation,reference_pin,source_pin,recovery_pin,remote)
                owners()
        calibrations[label] = cache[key]
        for repeat in range(2):
            previous_report = next((r/f'{label}-r{repeat}'/'result.json' for r in reuse_roots if (r/f'{label}-r{repeat}'/'result.json').is_file()), None)
            if previous_report is not None and previous_report.is_file():
                report = reuse_completed_report(previous_report, allocation, snapshots, previous_report.parents[1])
                reused_matches += 48
            else:
                owners()
                report = dispatch(root,f'{label}-r{repeat}',plan['binary'],plan['qualification_jobs'],allocation,remote,300)
                new_matches += 48
                owners()
            candidates.append(dict(id=f'{label}-r{repeat}',allocation_id=label,report=report))
            value=read(checked(report))
            print(dict(case=label,repeat=repeat,execution=value['execution_seconds']),flush=True)
    for label,allocation in cases: measure(label,allocation)
    choice = dict(schema='cpu-bo3-allocation/v4', plan=target_pin,binary=plan['binary'],inventory=available,
          inherited_storage_ranking=placement_evidence,
          recovery_projection=dict(method='linear-output-bytes-with-fixed-cost-floor',max_byte_ratio=1.10),
          eligible_storage=stores,remote_setup_seconds=0 if remote is None else remote['seconds'],candidates=candidates,
          selected=None,recovery_calibrations=calibrations,dependencies=[pin(Path(__file__).with_name(n)) for n in
          ('evaluation_throughput_v4.py','evaluation_recovery_fixture_v1.py','public_evaluation_dispatch_v1.py','public_evaluation_dispatch_v2.py')])
    # Pick cross-host storage/worker settings from completed same-host timings,
    # not outcomes. Preserve all earlier measurements as candidate evidence.
    if all(available[h]['eligible'] for h in snapshots):
        best = {}
        for host in snapshots:
            subset = dict(choice, candidates=[c for c in candidates if set(read(checked(c['report']))['allocation']) == {host}],
                          eligible_storage=[s for s in stores if s[0] == host])
            write(root/f'{host}-draft.json',subset)
            best[host] = require_choice(root/f'{host}-draft.json',target_pin,plan['binary'])['allocation'][host]
        for weight in (1,2):
            allocation={h:dict(a,job_weight=weight if h=='desktop' else 1) for h,a in best.items()}
            measure(f'both-desktop-weight-{weight}',allocation)
    write(root/'choice-draft.json',choice)
    selected = require_choice(root/'choice-draft.json',target_pin,plan['binary'])
    choice['selected'] = selected['id']
    owners()
    write(root/'compute-choice.json',choice)
    assert require_choice(root/'compute-choice.json',target_pin,plan['binary']) == selected
    write(root/'qualification.json',dict(complete=True,selected=selected,compute_choice=pin(root/'compute-choice.json'),
          native_matches=48*len(candidates),new_native_matches=new_matches,reused_native_matches=reused_matches,unique_cases=48,exact_file_comparisons=48*(len(candidates)-1),full_panel_launched=False))
    print(selected,flush=True)


if __name__ == '__main__':
    assert __debug__, 'validation must be enabled'
    p=argparse.ArgumentParser()
    p.add_argument('--root',type=Path,required=True)
    p.add_argument('--reference',type=Path,required=True)
    p.add_argument('--plan',type=Path,required=True)
    p.add_argument('--reuse-complete',type=Path)
    p.add_argument('--reuse-recovery',type=Path)
    p.add_argument('--placements-from',type=Path)
    args=p.parse_args()
    qualify(args.root,args.reference,args.plan,args.reuse_complete,args.reuse_recovery,args.placements_from)
