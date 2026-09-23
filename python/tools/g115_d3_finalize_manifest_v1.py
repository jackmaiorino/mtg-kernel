"""Assemble D3 placement from completed qualification evidence; never execute it."""
import argparse
import copy
import itertools
import json
from pathlib import Path
import time

from g115_d3_payload_v1 import checked, read, require, sha, write
from g115_d3_launch_v1 import cluster_allocation, projected_shard_seconds, validate_plan
from g115_d3_qualify_v1 import minimum_reserve


def ref(path):
    path = Path(path)
    return dict(path=str(path), sha256=sha(path))


def finalize(base, evidence, inventory, lease):
    """Evidence names qualified hosts and gives explicit exclusions for the rest.

    Every qualification is checked again by the existing launch guard. There
    are no guessed runtimes, default authority, outcomes or manual assignments.
    """
    result = copy.deepcopy(base)
    require(base['preparation_only'] is True and base['formal_measurement'] is False,
            'Require an unlaunched preparation')
    require(set(base['design_disposition']['unresolved_launch_blockers']) == {
        'New-source qualification and measured placement incomplete',
        'Fresh dispatch inventory and formal lease absent'},
        'A different review condition cannot be discharged by placement evidence')
    qualified = {}
    for host, item in evidence['qualified_hosts'].items():
        spec = read(checked(item['qualification_spec']))
        completion = read(checked(item['qualification_result']))
        require(completion['complete'] is True and completion['host'] == host,
                'Qualification is not complete: ' + host)
        phase = max(completion['phases'], key=lambda p: p['matches_per_second'])
        require(host in base['bindings'] and host in base['payloads'], 'Host input binding absent')
        overhead = item['startup_transfer_recovery_seconds']
        require(overhead >= 0 and item['overhead_basis'].strip(), 'Measured overhead basis required')
        allocation = dict(qualification_spec=item['qualification_spec'],
            qualification_result=item['qualification_result'], toolchain_receipt=item['toolchain_receipt'],
            workers=phase['workers'], command_prefix=spec['command_prefix'], runtime_files=spec['runtime_files'],
            reserve_bytes=minimum_reserve(host), job_timeout_seconds=1800,
            shard_timeout_seconds=14400 if host == 'runpod' else 28800,
            startup_transfer_recovery_seconds=overhead,
            guard_directory='/run/phase1/' + lease['name'] if host == 'runpod' else None,
            lease_name=lease['name'] if host == 'runpod' else None)
        if host == 'runpod':
            allocation['hardware_receipts'] = item['hardware_receipts']
        qualified[host] = allocation
    require(qualified, 'No qualified allocation')
    excluded = evidence['excluded_hosts']
    require(set(qualified).isdisjoint(excluded) and
            set(qualified) | set(excluded) == {'jack', 'haleyspc', 'runpod'},
            'Account for all three hosts')
    for host, item in excluded.items():
        require(item['reason'].strip(), 'Explain unqualified host: ' + host)
        checked(item['evidence'])
    rates = {h: max(read(checked(a['qualification_result']))['phases'],
                   key=lambda p: p['matches_per_second'])['matches_per_second'] for h, a in qualified.items()}
    overhead = {h: a['startup_transfer_recovery_seconds'] for h, a in qualified.items()}
    choices = []
    for size in range(1, len(qualified) + 1):
        for hosts in itertools.combinations(sorted(qualified), size):
            owners, seconds = cluster_allocation(hosts, rates, overhead)
            native_seconds = projected_shard_seconds(hosts, owners, rates)
            over_bound = [h for h in hosts if native_seconds[h] > qualified[h]['shard_timeout_seconds']]
            choices.append(dict(id='+'.join(hosts), hosts=list(hosts), projected_seconds=seconds,
                                projected_native_seconds=native_seconds, eligible=not over_bound,
                                ineligibility_reason='Projected native work exceeds shard bound: '+','.join(over_bound)
                                if over_bound else ''))
    feasible = [choice for choice in choices if choice['eligible']]
    require(feasible, 'No qualified allocation fits its declared shard bounds')
    selected = min(feasible, key=lambda c: (c['projected_seconds'], c['id']))
    owners, _ = cluster_allocation(selected['hosts'], rates, overhead)
    panel = read(checked(base['panel']))
    result['placement'] = dict(inventory=inventory, qualified_hosts=qualified,
        hosts={h: copy.deepcopy(qualified[h]) for h in selected['hosts']},
        choices=choices, selected=selected['id'],
        assignments={j['id']: owners[j['cell']*8+j['replica']] for j in panel['jobs']},
        excluded_hosts=excluded, overhead_basis={h: evidence['qualified_hosts'][h]['overhead_basis'] for h in qualified})
    result['design_disposition']['unresolved_launch_blockers'] = []
    result.update(formal_measurement=True, preparation_only=False)
    if 'runpod' in selected['hosts']:
        require(time.time() + 14400 + lease['recovery_seconds'] + 600 < lease['deadline_epoch'],
                'Formal cloud lease cannot fit the declared bound')
    for host in selected['hosts']:
        validate_plan(result, host)
    return result


def main():
    parser = argparse.ArgumentParser()
    for name in ('base', 'evidence', 'inventory', 'lease', 'output'):
        parser.add_argument('--'+name, type=Path, required=True)
    args = parser.parse_args()
    result = finalize(read(args.base), read(args.evidence), ref(args.inventory), read(args.lease))
    write(args.output, result)
    print(json.dumps(dict(manifest=ref(args.output), placement=result['placement']['selected'],
                          native_dispatched=False, allocated=False)))


if __name__ == '__main__':
    main()
