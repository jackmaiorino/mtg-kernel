"""Small real-file recovery and rejection checks, with no native simulation."""
import argparse
import copy
from pathlib import Path
import zipfile

from evaluation_recovery_fixture_v1 import (
    assignment, host_hashes, read, write, pin, checked, fingerprint,
    stage, recover_local, export, validate_calibration,
)


def check(root, reference):
    root.mkdir()
    completed = read(reference/'completion.json')
    dispatch = read(checked(completed['dispatch']))
    recovery_pin = dispatch['jobs'][0]['recovery']
    source = checked(recovery_pin).parent/'recovered'
    recovery = read(checked(recovery_pin))
    jobs = read(reference/'plan.json')['jobs'][:2]
    # Only two already completed match files are copied. Metadata below is an
    # explicitly small validator fixture and never a scientific run receipt.
    hashes = {f"jobs/{j['id']}/outputs/match-000000.json": recovery['hashes'][f"jobs/{j['id']}/outputs/match-000000.json"] for j in jobs}
    allocation = {'desktop':dict(drive='E',disk_serial='test-only',disk_name='test-only',workers=1,job_weight=1)}
    plan = dict(binary=read(reference/'plan.json')['binary'],expected_jobs=2,expected_matches=2,jobs=jobs)
    write(root/'reference.json',plan)
    write(root/'source-recovery.json',dict(mismatches=0,files=2,hashes=hashes))
    fixture_source = dict(matches=2,jobs=[dict(id=j['id'],recovery=pin(root/'source-recovery.json')) for j in jobs])
    write(root/'source-dispatch.json',fixture_source)
    stage(source,root/'native',hashes)
    samples=[]
    for i in range(2):
        receipt=recover_local(root/'native',root/f'recovered-{i}',hashes)
        write(root/f'sample-{i}.json',dict(schema='placed-recovery-sample/v1',complete=True,
              seconds=receipt['seconds']+.001,hosts={'desktop':receipt},source_fingerprint=fingerprint(hashes),mismatches=0))
        samples.append(pin(root/f'sample-{i}.json'))
    c=dict(schema='placed-full-panel-recovery-fixture/v1',dependencies=[],reference_plan=pin(root/'reference.json'),
           binary=plan['binary'],source_dispatch=pin(root/'source-dispatch.json'),source_recovery=pin(root/'source-recovery.json'),
           placements={'desktop':{k:v for k,v in allocation['desktop'].items() if k not in ('workers','job_weight')}},
           allocation_weights={'desktop':1},source_fingerprint=fingerprint(hashes),
           assigned_reference_jobs=assignment(jobs,allocation),samples=samples)
    write(root/'calibration.json',c)
    validate_calibration(pin(root/'calibration.json'),allocation,plan,plan['binary'])
    passed=['real_file_local_copy_and_full_readback','valid_typed_fixture']
    def rejects(name,operation):
        try: operation()
        except (AssertionError,FileNotFoundError,ValueError): passed.append(name)
        else: raise AssertionError('accepted invalid evidence: '+name)
    mutated=copy.deepcopy(c);mutated['allocation_weights']['desktop']=2
    write(root/'wrong-placement.json',mutated)
    rejects('wrong_assignment_weight',lambda:validate_calibration(pin(root/'wrong-placement.json'),allocation,plan,plan['binary']))
    mutated=copy.deepcopy(c);mutated['samples']=[samples[0],samples[0]]
    write(root/'duplicate-samples.json',mutated)
    rejects('duplicate_timing_receipt',lambda:validate_calibration(pin(root/'duplicate-samples.json'),allocation,plan,plan['binary']))
    damaged=root/'recovered-0'/next(iter(hashes));original=damaged.read_bytes();damaged.write_bytes(original+b' ')
    rejects('corrupt_recovered_output',lambda:validate_calibration(pin(root/'calibration.json'),allocation,plan,plan['binary']))
    damaged.write_bytes(original)
    extra=root/'recovered-0'/'unexpected';extra.write_text('unaccounted')
    rejects('unaccounted_output_file',lambda:validate_calibration(pin(root/'calibration.json'),allocation,plan,plan['binary']))
    weighted=assignment([dict(id=str(i)) for i in range(9)],{'desktop':dict(job_weight=2),'computehost':dict(job_weight=1)})
    assert weighted=={'desktop':['1','2','4','5','7','8'],'computehost':['0','3','6']}
    passed.append('weighted_assignment_matches_dispatch_order')
    rejects('escaping_fixture_path',lambda:stage(source,root/'bad-stage',{'../outside':'0'*64}))
    archive=export(root/'native',root/'export.zip',hashes)
    with zipfile.ZipFile(checked(archive['archive'])) as z:
        assert set(z.namelist())==set(hashes)|{'export-manifest.json'}
        import hashlib
        for name,digest in hashes.items():assert hashlib.sha256(z.read(name)).hexdigest()==digest
    passed.append('remote_export_payload_exact')
    write(root/'result.json',dict(complete=True,checks=passed,native_games=0,source_recovery=recovery_pin,
          validation_scope='Two actual completed match files; remote network and full-size placement timing still require runtime qualification.'))
    print(passed)


if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--root',type=Path,required=True);p.add_argument('--reference',type=Path,required=True)
    args=p.parse_args();check(args.root,args.reference)
