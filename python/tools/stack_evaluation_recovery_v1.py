"""Explicit copied-output recovery fixture, never a scientific result panel."""
import json
import math
import shutil
import subprocess
import time
import zipfile
from pathlib import Path
from public_evaluation_dispatch_v1 import read,write,pin,checked,ssh,REMOTE
from evaluation_recovery_fixture_v1 import assignment,host_hashes,fingerprint,stage,recover_local
from qualify_evaluation_placements_v6 import remote_recovery


def build(root,plan_pin,sample_pin,remote):
    plan=read(checked(plan_pin));sample=read(checked(sample_pin))
    assert sample['matches']==256 and len(sample['jobs'])==32
    folder=root/'recovery-fixture';folder.mkdir();source=folder/'source';source.mkdir()
    largest={}
    for job in sample['jobs']:
        directory=Path(job['output_directory']).parent
        size=sum(p.stat().st_size for p in directory.rglob('*') if p.is_file())
        if job['arm'] not in largest or size>largest[job['arm']][0]:largest[job['arm']]=(size,directory,job['id'])
    assert set(largest)=={'g115','disabled','permuted','structured'}
    # Each clone is visibly a fixture. Retain its source metadata verbatim;
    # never synthesize native completion or pretend it executed target cases.
    mapping={}
    for job in plan['jobs']:
        _,original,identifier=largest[job['arm']]
        destination=source/'jobs'/job['id'];destination.parent.mkdir(exist_ok=True)
        shutil.copytree(original,destination)
        mapping[job['id']]=dict(sample_job=identifier,source_directory=str(original))
    shutil.copy2(checked(plan['binary']),source/'public-evaluator.exe')
    # Account for the full-size dispatch metadata, separately labeled fixture.
    write(source/'spec.json',dict(fixture_only=True,binary=plan['binary'],jobs=[dict(id=j['id'],
        native_directory=str(source/'jobs'/j['id']),request=pin(source/'jobs'/j['id']/'request.json'),
        native_request=pin(source/'jobs'/j['id']/'request.json')) for j in plan['jobs']]))
    hashes={p.relative_to(source).as_posix():pin(p)['sha256'] for p in source.rglob('*') if p.is_file()}
    write(folder/'hashes.json',hashes)
    write(folder/'manifest.json',dict(schema='stack-copied-recovery-fixture/v1',plan=plan_pin,sample=sample_pin,
        source_directory=str(source),mapping=mapping,hashes=pin(folder/'hashes.json'),
        files=len(hashes),jobs=512,match_files=4096,binary=plan['binary'],
        method='Replicate the largest observed complete eight-match job per endpoint to each target job. Retain original metadata; these are copied bytes, not newly executed results.',
        limitation='Measures recovery of full file count and an observed byte envelope. Future rare long matches may exceed the sampled sizes; projection remains uncertain.'))
    helper=Path(__file__).with_name('evaluation_recovery_fixture_v1.py')
    subprocess.run(['scp','-q',str(helper),f"{REMOTE}:{remote['native_root']}/fixture.py"],check=True,timeout=60)
    archive=folder/'fixture.zip'
    with zipfile.ZipFile(archive,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=1) as z:
        for name in hashes:z.write(source/name,name)
    remote_source=remote['native_root']+'/copied-fixture'
    subprocess.run(['scp','-q',str(archive),f"{REMOTE}:{remote['native_root']}/copied-fixture.zip"],check=True,timeout=180)
    expected=pin(archive)['sha256']
    ssh(f"if ((Get-FileHash '{remote['native_root']}/copied-fixture.zip' -Algorithm SHA256).Hash.ToLower() -ne '{expected}') {{throw 'fixture transfer mismatch'}}; Expand-Archive -LiteralPath '{remote['native_root']}/copied-fixture.zip' -DestinationPath '{remote_source}'",timeout=180)
    return pin(folder/'manifest.json'),remote_source


def calibrate(root,label,allocation,fixture_pin,remote,remote_source):
    fixture=read(checked(fixture_pin));plan=read(checked(fixture['plan']));hashes=read(checked(fixture['hashes']))
    folder=root/('recovery-'+label);folder.mkdir()
    assigned=assignment(plan['jobs'],allocation);placed={}
    for host,settings in allocation.items():
        selected=host_hashes(hashes,assigned[host],host)
        mapping=folder/(host+'-hashes.json');write(mapping,selected)
        if host=='desktop':
            native=Path(f"{settings['drive']}:/mtg-evaluation-recovery-fixtures/{root.name}/{label}")
            result=stage(fixture['source_directory'],native,selected)
            placed[host]=dict(native=str(native),hashes=selected)
        else:
            native=remote['native_root']+'/fixture-'+label;remote_map=native+'-hashes.json'
            subprocess.run(['scp','-q',str(mapping),f'{REMOTE}:{remote_map}'],check=True,timeout=60)
            result=json.loads(ssh(f"& python '{remote['native_root']}/fixture.py' stage '{remote_source}' '{native}' '{remote_map}'",timeout=180))
            placed[host]=dict(native=native,hashes=selected,remote_map=remote_map)
        assert result['fingerprint']==fingerprint(selected) and result['files']==len(selected)
    samples=[]
    for repeat in range(2):
        began=time.monotonic();hosts={}
        for host in sorted(placed):
            p=placed[host]
            if host=='desktop':hosts[host]=recover_local(p['native'],folder/f'desktop-recovered-{repeat}',p['hashes'])
            else:hosts[host]=remote_recovery(folder,remote,p['native'],p['hashes'],p['remote_map'],repeat)
        path=folder/f'sample-{repeat}.json'
        write(path,dict(complete=True,seconds=time.monotonic()-began,hosts=hosts,mismatches=0))
        samples.append(pin(path))
    result=folder/'calibration.json'
    write(result,dict(schema='stack-placed-recovery/v1',fixture=fixture_pin,assigned=assigned,samples=samples,
        placements={h:{k:v for k,v in a.items() if k!='workers'} for h,a in allocation.items()}))
    return pin(result)


def validate(calibration_pin,allocation,plan_pin):
    c=read(checked(calibration_pin));assert c['schema']=='stack-placed-recovery/v1'
    assert c['placements']=={h:{k:v for k,v in a.items() if k!='workers'} for h,a in allocation.items()}
    f=read(checked(c['fixture']));assert f['schema']=='stack-copied-recovery-fixture/v1' and f['plan']==plan_pin
    plan=read(checked(plan_pin));assert f['binary']==plan['binary'] and f['jobs']==512 and f['match_files']==4096
    source=read(checked(f['sample']));originals={j['id']:j for j in source['jobs']}
    assert source['matches']==256 and len(originals)==32
    hashes=read(checked(f['hashes']));assigned=assignment(plan['jobs'],allocation)
    assert assigned==c['assigned'] and len(hashes)==f['files']
    assert set(f['mapping'])=={j['id'] for j in plan['jobs']}
    # Check that every purported clone really consists of bytes from an actual
    # completed sample, and that its endpoint and file count are preserved.
    for job in plan['jobs']:
        mapping=f['mapping'][job['id']];old=originals[mapping['sample_job']]
        directory=Path(old['output_directory']).parent
        assert mapping['source_directory']==str(directory) and old['arm']==job['arm']
        original={p.relative_to(directory).as_posix():pin(p)['sha256'] for p in directory.rglob('*') if p.is_file()}
        prefix='jobs/'+job['id']+'/'
        clone={n[len(prefix):]:v for n,v in hashes.items() if n.startswith(prefix)}
        assert clone==original and sum(n.startswith('outputs/match-') for n in clone)==8
    assert len(c['samples'])==2 and len({s['path'] for s in c['samples']})==2
    times=[]
    for item in c['samples']:
        sample=read(checked(item));assert sample['complete'] and sample['mismatches']==0 and set(sample['hosts'])==set(allocation)
        assert sample['seconds']>0
        for host,row in sample['hosts'].items():
            expected=host_hashes(hashes,assigned[host],host)
            assert row['hashes']==expected and row['mismatches']==0 and row['files']==len(expected)
            directory=Path(row['recovered_directory'])
            allowed=set(expected)|({'export-manifest.json'} if host=='computehost' else set())
            assert {p.relative_to(directory).as_posix() for p in directory.rglob('*') if p.is_file()}==allowed
            for name,digest in expected.items():assert pin(directory/name)['sha256']==digest
            assert row['uncompressed_bytes']==sum((directory/n).stat().st_size for n in expected)
            assert math.isfinite(row['seconds']) and row['seconds']>0
            assert all(math.isfinite(v) and v>=0 for v in row['stage_seconds'].values())
            assert sum(row['stage_seconds'].values())<=row['seconds']+.01
            if host=='computehost':checked(row['archive']);assert read(directory/'export-manifest.json')==expected
        assert math.isfinite(sample['seconds']) and sum(r['seconds'] for r in sample['hosts'].values())<=sample['seconds']+.01
        times.append(sample['seconds'])
    return max(times)
