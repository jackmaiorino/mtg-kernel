"""Collect the exact assigned D3 panel, retaining missing and failed jobs.

This read-only collector changes local file locations, never native requests,
outcomes, assignments or the formal verdict. Aggregate analysis runs locally.
"""
import argparse
import copy
import json
from pathlib import Path

from g115_d3_payload_v1 import checked, read, require, sha, write
from g115_d3_native_results_v1 import analyze


def collect(manifest, recovery):
    require(manifest['schema'] == 'g115-d3-formal-launch/v1', 'Wrong experiment manifest')
    placement = manifest['placement']
    require(placement is not None, 'No frozen fleet assignment to collect')
    require(sha(Path(__file__)) == manifest['documents']['collector']['sha256'],
            'Collector differs from the prelaunch analysis pin')
    panel = read(checked(manifest['panel']))
    expected = {job['id']: job for job in panel['jobs']}
    assignments = placement['assignments']
    require(len(expected) == len(panel['jobs']) == 2048 and set(assignments) == set(expected),
            'The full shared panel must be assigned')
    hosts = set(assignments.values())
    require(set(recovery) == hosts == set(placement['hosts']), 'Every assigned host needs a recovery location')
    jobs, errors, receipts, execution_hashes = [], [], {}, {}
    models = None
    for host in sorted(hosts):
        binding = read(checked(manifest['bindings'][host]))
        require(binding['source_commit'] == manifest['source_commit'], 'Host source differs')
        if models is None:
            models = binding['models']
        require(binding['models'] == models, 'Hosts use different actual models')
        bound = {job['id']: job for job in binding['jobs']}
        require(len(bound) == len(binding['jobs']) == 2048 and set(bound) == set(expected),
                'Host binding omits or duplicates panel jobs')
        selected = {identifier for identifier, owner in assignments.items() if owner == host}
        location = recovery[host]
        ref = location.get('shard_completion')
        if ref is None:
            errors.append(dict(host=host, error='Shard completion is missing'))
        else:
            try:
                receipt = read(checked(ref))
                receipts[host] = copy.deepcopy(ref)
                require(receipt['schema'] == 'g115-d3-shard-result/v1' and receipt['host'] == host,
                        'Wrong shard completion')
                require(receipt['formal_measurement'] is True and receipt['formal_verdict'] is None,
                        'Shard is not an unadjudicated formal measurement')
                require(len(receipt['assigned_jobs']) == len(selected) and
                        set(receipt['assigned_jobs']) == selected, 'Shard assignment differs')
                rows = receipt['rows']
                require(len({r['id'] for r in rows}) == len(rows), 'Duplicate shard execution record')
                require({r['id'] for r in rows} <= selected, 'Shard executed another host assignment')
                execution_hashes.update({r['id']: r['sha256'] for r in rows if r['complete']})
                require(receipt['complete'] is True and receipt['not_started'] == [] and
                        len(rows) == len(selected) and all(r['complete'] and r['exit_code'] == 0 for r in rows),
                        'Shard is incomplete; retained native records will still be inspected')
            except (OSError, ValueError, KeyError, TypeError) as error:
                errors.append(dict(host=host, error=str(error)))
        # Include all assigned jobs, even if the receipt or output is absent.
        # Native analysis independently reports every missing/invalid record.
        for identifier in sorted(selected):
            job = copy.deepcopy(bound[identifier])
            request = Path(location['request_root']) / (identifier + '.json')
            job['request']['path'] = str(request)
            job['output_directory'] = str(Path(location['output_root']) / identifier)
            jobs.append(job)
    require(len(jobs) == 2048 and len({j['id'] for j in jobs}) == 2048, 'Collection changed the panel')
    execution = dict(schema='g115-d3-collected-execution/v1', source_commit=manifest['source_commit'],
                     models=models, jobs=jobs, shard_completions=receipts, formal_verdict=None)
    native = analyze(panel, execution)
    for row in native['rows']:
        if execution_hashes.get(row['id']) != row['sha256']:
            errors.append(dict(id=row['id'], error='Recovered store does not match its shard execution receipt'))
    return dict(schema='g115-d3-collected-results/v1', complete=not errors and native['complete'],
                formal_verdict=None, collection_errors=errors, native=native), execution


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--recovery', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    result, execution = collect(read(args.manifest), read(args.recovery))
    args.output.mkdir()
    write(args.output / 'execution.json', execution)
    result['inputs'] = {key: dict(path=str(path), sha256=sha(path)) for key, path in
                       [('manifest', args.manifest), ('recovery', args.recovery), ('collector', Path(__file__))]}
    write(args.output / 'analysis.json', result)
    print(json.dumps(dict(complete=result['complete'], formal_verdict=None,
                          completed_jobs=result['native']['completed_jobs'],
                          collection_errors=len(result['collection_errors']))))


if __name__ == '__main__':
    main()
