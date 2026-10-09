"""Bounded timing replay of a completed panel, never new scientific outcomes.

First qualification scope is the currently idle compute host host. Reject if the maintainer is
also eligible rather than silently omitting it from the allocation comparison.
The guard itself supports any fully measured allocation and storage inventory.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import zipfile

from public_evaluation_dispatch_v1 import read, write, pin, checked, inventory, ssh, REMOTE, prepare_remote
from public_evaluation_dispatch_v2 import dispatch
from public_training_storage_v1 import storage
from evaluation_throughput_v2 import require_choice


EXPORTER = '''import hashlib,json,pathlib,sys,time,zipfile
source,dest=map(pathlib.Path,sys.argv[1:]);dest.mkdir()
expected=json.loads((source/'export-manifest.json').read_text())
began=time.monotonic();total=0
with zipfile.ZipFile(dest/'results.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=1) as archive:
 for name,digest in expected.items():
  path=source/name
  with path.open('rb') as stream: actual=hashlib.file_digest(stream,'sha256').hexdigest()
  assert actual==digest,name
  total+=path.stat().st_size;archive.write(path,name)
 archive.writestr('export-manifest.json',json.dumps(expected))
with (dest/'results.zip').open('rb') as stream: digest=hashlib.file_digest(stream,'sha256').hexdigest()
print(json.dumps(dict(files=len(expected),uncompressed_bytes=total,sha256=digest,seconds=time.monotonic()-began)))
'''


def full_recovery(root, remote, source_native, source_hashes, plan_pin, index):
    canonical = root / f'recovery-{index}'
    canonical.mkdir()
    native = remote['native_root'] + f'/recovery-{index}'
    stage = {}
    start = time.monotonic()
    before = time.monotonic()
    exported = json.loads(ssh(f"& python '{remote['native_root']}/recovery-export.py' '{source_native}' '{native}'", timeout=180))
    stage['export_ssh_wall'] = time.monotonic()-before
    before = time.monotonic()
    subprocess.run(['scp', '-q', f'{REMOTE}:{native}/results.zip', str(canonical/'results.zip')], check=True, timeout=180)
    stage['download'] = time.monotonic()-before
    before = time.monotonic()
    archive_pin = pin(canonical/'results.zip')
    assert archive_pin['sha256'] == exported['sha256']
    stage['archive_verification'] = time.monotonic()-before
    before = time.monotonic()
    recovered = canonical/'recovered'
    recovered.mkdir()
    with zipfile.ZipFile(checked(archive_pin)) as archive:
        assert all((recovered/name).resolve().is_relative_to(recovered.resolve()) for name in archive.namelist())
        assert set(archive.namelist()) == set(source_hashes) | {'export-manifest.json'}
        archive.extractall(recovered)
    stage['extraction'] = time.monotonic()-before
    before = time.monotonic()
    assert read(recovered/'export-manifest.json') == source_hashes
    total = 0
    for name, digest in source_hashes.items():
        assert pin(recovered/name)['sha256'] == digest
        total += (recovered/name).stat().st_size
    assert total == exported['uncompressed_bytes'] and len(source_hashes) == exported['files']
    stage['file_verification'] = time.monotonic()-before
    result = dict(complete=True, plan=plan_pin, seconds=time.monotonic()-start, stage_seconds=stage,
                  exported_native_seconds=exported['seconds'], archive=archive_pin,
                  verified_files=len(source_hashes), mismatches=0, uncompressed_bytes=total,
                  source_fingerprint=hashlib.sha256(json.dumps(source_hashes,sort_keys=True).encode()).hexdigest())
    write(canonical/'receipt.json',result)
    print(dict(recovery=index,seconds=result['seconds'],stage_seconds=stage),flush=True)
    return pin(canonical/'receipt.json'), result


def qualify(root, reference, target_plan=None):
    reference_pin = pin(reference/'plan.json')
    reference_plan = read(checked(reference_pin))
    plan_pin = pin(target_plan or reference/'plan.json')
    plan = read(checked(plan_pin))
    assert plan['expected_jobs'] == 48 and plan['expected_matches'] == 3072
    assert len(plan['qualification_jobs']) == 48
    assert all(len(j['command']['matches']) == 1 for j in plan['qualification_jobs'])
    completion = read(reference/'completion.json')
    assert completion['complete'] and completion['matches'] == reference_plan['expected_matches']
    assert reference_plan['binary'] == plan['binary']
    result_pin = completion['dispatch']
    completed = read(checked(result_pin))
    assert set(completed['allocation']) == {'computehost'}
    assert len(completed['jobs']) == reference_plan['expected_jobs']
    source_recovery_pin = completed['jobs'][0]['recovery']
    source_recovery = read(checked(source_recovery_pin))
    assert source_recovery['mismatches'] == 0
    root.mkdir()
    availability = {}
    snapshots = {}
    for host in ('desktop','computehost'):
        snap = inventory(host)
        write(root/f'{host}-inventory.json',snap)
        snapshots[host] = snap
        availability[host] = dict(checked_at=snap['at'], evidence=pin(root/f'{host}-inventory.json'),
                                  eligible=not snap['active'], reason='Actual native ownership at bounded replay qualification.')
    assert not availability['desktop']['eligible'], 'the maintainer is now available; extend qualification to all local storage and both-host placements'
    assert availability['computehost']['eligible'], 'preserve active compute host work'
    cloud = Path('E:/mtg-meta-recovery-20260920/public-device-placement-001/runpod-inventory.json')
    availability['runpod'] = dict(checked_at=read(cloud)['checked_at'], evidence=pin(cloud), eligible=False,
                                  reason='Authenticated inventory HTTP403; no new paid allocation authorized.')
    disk = storage(snapshots['computehost'],'C')
    settings = dict(drive='C',disk_serial=disk['disk_serial'],disk_name=disk['disk_name'])
    write(root/'design.json',dict(plan=plan_pin,reference_plan=reference_pin,source_recovery=source_recovery_pin,source_dispatch=result_pin,
          question='Separate measured full-output recovery from repeated native throughput to avoid multiplying fixed recovery overhead by the match scale.',
          allocations=[1,8,16],repeats=2,qualification_cases=len(plan['qualification_jobs']),
          maximum_native_matches=2*3*sum(len(j['command']['matches']) for j in plan['qualification_jobs']),
          recovery_repeats=2,full_panel_launch=False,training=False,paid_compute=False,
          review='Known zero-read Fable HTTP429 through September22 07:00EDT; no retry or endorsement.',
          caveat='Execution scaling still includes native job startup and is conservative for larger batches; this slice corrects recovery accounting only.'))
    old_remote = read(checked(plan_pin).parent/'remote-staging.json')
    assets = [item for name,item in old_remote['assets'].items() if name.startswith('inputs/')]
    remote = prepare_remote(root,assets)
    exporter = root/'recovery-export.py'
    exporter.write_text(EXPORTER)
    subprocess.run(['scp','-q',str(exporter),f"{REMOTE}:{remote['native_root']}/recovery-export.py"],check=True,timeout=60)
    samples=[]
    for i in range(2):
        receipt,last = full_recovery(root,remote,source_recovery['native_directory'],source_recovery['hashes'],reference_pin,i)
        samples.append(receipt)
    calibration = dict(schema='full-panel-recovery-calibration/v1', plan=reference_pin,binary=plan['binary'],
          placements={'computehost':settings},allocation_weights={'computehost':1},jobs=reference_plan['expected_jobs'],
          matches=reference_plan['expected_matches'],files=last['verified_files'],uncompressed_bytes=last['uncompressed_bytes'],
          source_fingerprint=last['source_fingerprint'],source_recovery=source_recovery_pin,source_dispatch=result_pin,samples=samples)
    write(root/'recovery-calibration.json',calibration)
    candidates=[]
    # Reverse parallel order in repetition two; retain a serial baseline first.
    for repeat, counts in enumerate(((1,8,16),(16,8,1))):
        for workers in counts:
            identifier=f'computehost-w{workers}-r{repeat}'
            allocation={'computehost':dict(settings,workers=workers,job_weight=1)}
            report = dispatch(root,identifier,plan['binary'],plan['qualification_jobs'],allocation,remote,300)
            candidates.append(dict(id=identifier,allocation_id=f'computehost-w{workers}',report=report))
            r=read(checked(report))
            print(dict(case=identifier,execution=r['execution_seconds'],sample_recovery=r['recovery_seconds']),flush=True)
    choice=dict(schema='cpu-bo3-allocation/v2',plan=plan_pin,binary=plan['binary'],inventory=availability,
          eligible_storage=[('computehost','C',settings['disk_serial'])],remote_setup_seconds=remote['seconds'],
          candidates=candidates,selected=None,
          recovery_calibrations={f'computehost-w{n}':pin(root/'recovery-calibration.json') for n in (1,8,16)},
          dependencies=[pin(Path(__file__).with_name(n)) for n in ('evaluation_throughput_v2.py','public_evaluation_dispatch_v1.py','public_evaluation_dispatch_v2.py')])
    write(root/'choice-draft.json',choice)
    selected=require_choice(root/'choice-draft.json',plan_pin,plan['binary'])
    choice['selected']=selected['id']
    write(root/'compute-choice.json',choice)
    assert require_choice(root/'compute-choice.json',plan_pin,plan['binary']) == selected
    write(root/'qualification.json',dict(complete=True,selected=selected,compute_choice=pin(root/'compute-choice.json'),
          native_matches=288,unique_cases=48,exact_file_comparisons=240,full_panel_launched=False))
    print(selected,flush=True)


if __name__=='__main__':
    p=argparse.ArgumentParser()
    p.add_argument('--root',type=Path,required=True)
    p.add_argument('--reference',type=Path,required=True)
    p.add_argument('--plan',type=Path,help='Prepared new target plan; its timings/replays are measured with reference recovery volume.')
    a=p.parse_args()
    if not __debug__:
        raise RuntimeError('Python validation must be enabled')
    qualify(a.root,a.reference,a.plan)
