"""Offline D3 transport preparation. It does not allocate or launch anything."""
import argparse
import copy
import json
from pathlib import Path, PurePosixPath
import shutil

from g115_d3_payload_v1 import checked, read, require, sha, write


def portable_binding(binding, output):
    """Preserve request bytes; relocate only the runner's file references."""
    binding = Path(binding)
    output = Path(output)
    source = read(binding / 'execution.json')
    destination = PurePosixPath(source['destination'])
    require(destination.is_absolute() and '..' not in destination.parts, 'Absolute remote destination required')
    require(source['launchable'] is False and len(source['jobs']) == 2048, 'Complete unlaunched binding required')
    output.mkdir()
    (output / 'requests').mkdir()
    shutil.copytree(binding / 'payload', output / 'payload')
    result = copy.deepcopy(source)
    for job in result['jobs']:
        request = checked(job['request'])
        expected_request = str(destination / 'requests' / (job['id'] + '.json'))
        expected_output = str(destination / 'outputs' / job['id'])
        require(job['native_request'] == expected_request and job['native_output_directory'] == expected_output,
                'Unexpected native destination')
        command = read(request)
        require(command['output_directory'] == expected_output, 'Native output does not match binding')
        target = output / 'requests' / request.name
        shutil.copyfile(request, target)
        require(sha(target) == job['request']['sha256'], 'Transport changed native request')
        job['request']['path'] = expected_request
        job['output_directory'] = expected_output
    write(output / 'execution.remote.json', result)
    files = []
    for path in sorted(output.rglob('*')):
        if path.is_file():
            files.append(dict(path=path.relative_to(output).as_posix(), sha256=sha(path), bytes=path.stat().st_size))
    receipt = dict(schema='g115-d3-portable-binding/v1', launchable=False, source_commit=source['source_commit'],
                   source_execution=dict(path=str(binding / 'execution.json'), sha256=sha(binding / 'execution.json')),
                   destination=str(destination), files=files, jobs=2048,
                   changes='Runner request/output references only; all native request and payload bytes preserved.')
    write(output / 'transport.json', receipt)
    return receipt


def portable_support(manifest, host, portable, output, destination):
    """Copy evidence by digest without changing its contents or dispositions.

    This accepts incomplete preparation, but leaves it incomplete. The actual
    worker's normal validation remains the only route to native dispatch.
    """
    output = Path(output)
    portable = Path(portable)
    destination = PurePosixPath(destination)
    require(destination.is_absolute() and '..' not in destination.parts, 'Absolute support destination required')
    output.mkdir()
    (output / 'evidence').mkdir()
    (output / 'tools').mkdir()
    result = copy.deepcopy(manifest)
    copied = {}

    def evidence(ref):
        original = checked(ref)
        name = ref['sha256'] + '-' + original.name
        target = output / 'evidence' / name
        if not target.exists():
            shutil.copyfile(original, target)
        require(sha(target) == ref['sha256'], 'Evidence bytes changed')
        copied[name] = dict(original=str(original), sha256=ref['sha256'])
        return dict(path=str(destination / 'evidence' / name), sha256=ref['sha256'])

    result['documents'] = {k: evidence(v) for k, v in result['documents'].items()}
    result['panel'] = evidence(result['panel'])
    disposition = result['design_disposition']
    if disposition is not None:
        for key in ('record', 'verdict'):
            if key in disposition:
                disposition[key] = evidence(disposition[key])
    transport = read(portable / 'transport.json')
    for item in transport['files']:
        require(sha(portable / item['path']) == item['sha256'], 'Portable binding changed')
    remote = PurePosixPath(transport['destination'])
    execution = portable / 'execution.remote.json'
    payload = portable / 'payload/sources.json'
    result['bindings'] = {host: dict(path=str(remote / execution.name), sha256=sha(execution))}
    result['payloads'] = {host: dict(path=str(remote / 'payload/sources.json'), sha256=sha(payload))}
    placement = result['placement']
    if placement is not None:
        placement['inventory'] = evidence(placement['inventory'])
        for section in ('qualified_hosts', 'hosts'):
            for allocation in placement[section].values():
                for key in ('qualification_spec', 'qualification_result', 'toolchain_receipt'):
                    allocation[key] = evidence(allocation[key])
                if 'hardware_receipts' in allocation:
                    allocation['hardware_receipts'] = {k: evidence(v) for k, v in allocation['hardware_receipts'].items()}
    tools = Path(__file__).parent
    names = ('g115_d3_launch_v1.py', 'g115_d3_qualify_v1.py', 'g115_d3_native_results_v1.py',
             'g115_d3_analysis_v1.py', 'g115_d3_power_core.py', 'g115_d3_cloud_host_v1.py',
             'g115_d3_payload_v1.py', 'g115_d3_baseline_parity_v1.py')
    pins = {}
    for name in names:
        shutil.copyfile(tools / name, output / 'tools' / name)
        pins[name] = sha(output / 'tools' / name)
    for key, name in (('analysis', names[3]), ('power_core', names[4]), ('reader', names[2])):
        require(pins[name] == manifest['documents'][key]['sha256'], 'Frozen analysis source differs')
    write(output / 'manifest.remote.json', result)
    receipt = dict(schema='g115-d3-portable-support/v1', prepared=True, launched=False,
                   formal_ready=False, host=host, destination=str(destination), evidence=copied, tools=pins,
                   command=['python3', str(destination / 'tools/g115_d3_launch_v1.py'),
                            '--manifest', str(destination / 'manifest.remote.json'), '--host', host, '--check-only'],
                   manifest_sha256=sha(output / 'manifest.remote.json'))
    write(output / 'transport.json', receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='mode', required=True)
    binding = sub.add_parser('binding')
    binding.add_argument('--binding', type=Path, required=True)
    binding.add_argument('--output', type=Path, required=True)
    support = sub.add_parser('support')
    support.add_argument('--manifest', type=Path, required=True)
    support.add_argument('--host', choices=['jack', 'haleyspc', 'runpod'], required=True)
    support.add_argument('--portable-binding', type=Path, required=True)
    support.add_argument('--output', type=Path, required=True)
    support.add_argument('--destination', required=True)
    args = parser.parse_args()
    if args.mode == 'binding':
        receipt = portable_binding(args.binding, args.output)
        print(json.dumps(dict(jobs=receipt['jobs'], files=len(receipt['files']), launched=False)))
    else:
        receipt = portable_support(read(args.manifest), args.host, args.portable_binding, args.output, args.destination)
        print(json.dumps(dict(evidence_files=len(receipt['evidence']), launched=False, formal_ready=False)))


if __name__ == '__main__':
    main()
