"""Require every frozen retained byte comparison before the original D3 analysis.

This additional check does not dispatch work or change the frozen collector.
The retained index and this wrapper must both be pinned before a new launch.
"""
import argparse
import json
from pathlib import Path

from g115_d3_payload_v1 import checked, read, require, sha, write
from g115_d3_launch_v1 import SOURCE, PANEL_SHA
from g115_d3_collect_v1 import collect


def validate_index(prior):
    require(prior['schema'] == 'g115-d3-retained-raw-index/v1', 'Wrong retained index')
    require(prior['manifest']['sha256'] ==
            '3ad1cd267692672ccf6706bccbf07d02f4f884efb13f607d661264fed2dc5438',
            'Index must retain the frozen attempt003')
    original = read(checked(prior['manifest']))
    expected, seen_kinds = set(), set()
    for ref in prior['evidence']:
        proof = read(checked(ref))
        if proof.get('kind') == 'missing_local_launcher_and_children':
            kind, host = 'interruption', 'jack'
            require(proof['actual_processes'] == [] and not proof['worker_completion_present'],
                    'Local interruption proof differs')
            rows = proof['receipt_store_checks']
        elif proof.get('schema') == 'g115-d3-shard-result/v1':
            kind, host = 'terminal', proof['host']
            require(host == 'haleyspc', 'Expected the original Haley terminal receipt')
            rows = proof['rows']
        else:
            raise ValueError('Unrecognized retained terminal evidence')
        require(kind not in seen_kinds, 'Duplicate terminal evidence')
        seen_kinds.add(kind)
        completed = {row['id'] for row in rows if row['complete']}
        require(all(original['placement']['assignments'].get(i) == host for i in completed),
                'Evidence contains foreign shard jobs')
        require(not expected.intersection(completed), 'Duplicate completed evidence')
        expected.update(completed)
    require(seen_kinds == {'interruption', 'terminal'}, 'Both original shards must be represented')
    require({row['id'] for row in prior['stores']} == expected,
            'Retained index omits or adds a completed execution')


def compare_bytes(prior, assignments, recovery):
    require(prior['source_commit'] == SOURCE and prior['panel_sha256'] == PANEL_SHA,
            'Retained source or panel differs')
    stores = prior['stores']
    require(len(stores) == prior['completed_count'] > 0 and
            len({row['id'] for row in stores}) == len(stores), 'Invalid retained count or duplicate ID')
    require({row['id'] for row in stores} <= set(assignments), 'Retained job missing from candidate panel')
    rows = []
    for row in stores:
        identifier = row['id']
        try:
            old_path = checked(row['store'])
            new_path = Path(recovery[assignments[identifier]]['output_root']) / identifier / 'match-000000.json'
            require(old_path.resolve() != new_path.resolve(), 'Same file is not independent reproduction')
            require(new_path.is_file(), 'Retained job has no candidate output')
            current_hash = sha(new_path)
            rows.append(dict(id=identifier, equal=current_hash == row['store']['sha256'],
                             candidate_sha256=current_hash))
        except (OSError, ValueError, KeyError, TypeError) as error:
            rows.append(dict(id=identifier, equal=False, error=str(error)))
    return dict(complete=all(row['equal'] for row in rows), expected=len(stores),
                equal=sum(row['equal'] for row in rows), rows=rows, outcomes_read=False)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--recovery', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    require(not args.output.exists(), 'Fresh collection output required')
    manifest, recovery = read(args.manifest), read(args.recovery)
    refs = manifest['retained_raw_overlap']
    require(sha(Path(__file__)) == refs['collector_sha256'], 'Retained comparison code changed')
    prior = read(checked(refs['index']))
    require(sha(args.manifest) != prior['manifest']['sha256'], 'Cannot replay the original attempt as its own replication')
    validate_index(prior)
    overlap = compare_bytes(prior, manifest['placement']['assignments'], recovery)
    if not overlap['complete']:
        result = dict(schema='g115-d3-retained-byte-refusal/v1', complete=False,
                      formal_verdict=None, outcomes_withheld=True, retained_raw_overlap=overlap)
        execution = None
    else:
        # The original collector still checks the full panel, native validity,
        # 1,024 baselines and all812 attempt002 overlaps before statistics.
        result, execution = collect(manifest, recovery)
        result['retained_raw_overlap'] = overlap
    result['retained_inputs'] = dict(index=refs['index'],
                                     collector=dict(path=str(Path(__file__)), sha256=sha(Path(__file__))))
    result['inputs'] = {name: dict(path=str(path), sha256=sha(path)) for name, path in
                        [('manifest', args.manifest), ('recovery', args.recovery),
                         ('collector', checked(manifest['documents']['collector']))]}
    args.output.mkdir()
    if execution is not None:
        write(args.output / 'execution.json', execution)
    write(args.output / 'analysis.json', result)
    print(json.dumps(dict(complete=result['complete'], retained_expected=overlap['expected'],
                          retained_equal=overlap['equal'], formal_verdict=None)))
    raise SystemExit(0 if result['complete'] else 1)


if __name__ == '__main__':
    main()
