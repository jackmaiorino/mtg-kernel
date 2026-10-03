"""Collect the exact assigned D3 panel, retaining missing and failed jobs.

This read-only collector changes local file locations, never native requests,
outcomes, assignments or the formal verdict. Aggregate analysis runs locally.
"""
import argparse
import copy
import json
from pathlib import Path

from g115_d3_payload_v1 import checked, read, require, sha, write
from g115_d3_native_results_v1 import analyze, withhold_outcomes
from g115_d3_launch_v1 import SOURCE, PANEL_SHA
from g115_d3_baseline_parity_v1 import compare as compare_baseline, compare_overlap


def execution_counts(receipts, expected):
    """Disjoint job categories. An absent receipt is unknown, not unstarted."""
    states={}
    for receipt in receipts:
        for identifier in receipt.get('not_started', []):
            require(identifier not in states, 'Duplicate execution category')
            states[identifier]='unstarted'
        for row in receipt['rows']:
            identifier=row['id']
            require(identifier not in states, 'Duplicate execution category')
            if row['complete']:kind='completed'
            elif row.get('error')=='Whole-match time bound reached':kind='timeout'
            elif row.get('error')=='Shard interrupted':kind='interrupted'
            else:kind='other_failure'
            states[identifier]=kind
    require(set(states)<=set(expected), 'Execution counts include foreign jobs')
    return {kind:sum(states.get(identifier,'unknown')==kind for identifier in expected)
            for kind in ('completed','timeout','interrupted','unstarted','other_failure','unknown')}


def collect(manifest, recovery):
    require(manifest['schema'] == 'g115-d3-formal-launch/v1', 'Wrong experiment manifest')
    placement = manifest['placement']
    require(placement is not None, 'No frozen fleet assignment to collect')
    require(manifest['source_commit'] == SOURCE and manifest['panel']['sha256'] == PANEL_SHA,
            'Collection differs from the fixed D3 source or shared panel')
    require(manifest['formal_measurement'] is True and manifest.get('preparation_only') is not True,
            'Preparation is not a formal execution manifest')
    require(sha(Path(__file__)) == manifest['documents']['collector']['sha256'],
            'Collector differs from the prelaunch analysis pin')
    panel = read(checked(manifest['panel']))
    expected = {job['id']: job for job in panel['jobs']}
    assignments = placement['assignments']
    require(len(expected) == len(panel['jobs']) == 2048 and set(assignments) == set(expected),
            'The full shared panel must be assigned')
    hosts = set(assignments.values())
    require(set(recovery) == hosts == set(placement['hosts']), 'Every assigned host needs a recovery location')
    jobs, errors, receipts, execution_hashes, count_receipts = [], [], {}, {}, []
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
                require(set(receipt['not_started']) <= selected and
                        len(set(receipt['not_started'])) == len(receipt['not_started']),
                        'Invalid unstarted-job list')
                count_receipts.append(receipt)
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
    require(sha(Path(__file__).with_name('g115_d3_baseline_parity_v1.py')) ==
            manifest['documents']['baseline_parity']['sha256'], 'Baseline parity code changed')
    reference = read(checked(manifest['documents']['baseline_archive']))
    require(reference['panel_sha256'] == PANEL_SHA, 'Baseline archive belongs to another panel')
    precheck=read(checked(manifest['baseline_parity']['precheck']))
    parity = compare_baseline(panel, execution, reference, manifest['baseline_parity']['v3_envelopes'],precheck)
    overlap=compare_overlap(panel,execution,read(checked(manifest['baseline_parity']['prior_attempt'])),manifest['baseline_parity']['v3_envelopes'])
    # Check receipt/store identity before analyze can compute any statistics.
    for job in jobs:
        path=Path(job['output_directory'])/'match-000000.json'
        if path.exists() and execution_hashes.get(job['id']) != sha(path):
            errors.append(dict(id=job['id'], error='Recovered store does not match its shard execution receipt'))
    counts=execution_counts(count_receipts, expected)
    native = analyze(panel, execution, validity_failures=[*errors, *parity['failures'],*overlap['failures']])
    if errors:
        # Integrity failures discovered after reading stores also withhold all
        # aggregate statistics. Individual raw records remain for diagnosis.
        withhold_outcomes(native)
    return dict(schema='g115-d3-collected-results/v1', complete=not errors and native['complete'],
                formal_verdict=None, execution_counts=counts,
                collection_errors=errors, baseline_parity=parity, prior_attempt_overlap=overlap, native=native), execution


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
