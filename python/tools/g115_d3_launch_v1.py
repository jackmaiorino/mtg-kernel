"""Guarded D3 shard execution. No default authority, placement or budget.

One coordinator manifest assigns every complete seed cluster to exactly one
host. This worker only dispatches its assigned jobs after checking the frozen
panel, review disposition, measured scaling, runtime and current reservations.
The RunPod controller must additionally own recovery and lease release.
"""
import argparse
import concurrent.futures
import copy
import itertools
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import threading
import time

from g115_d3_qualify_v1 import checked, free_memory, read, require, sample, sha, write
from g115_d3_native_results_v1 import read_match

SOURCE = 'e258daf3ab807cd6d8616a1431a21ea5ee22ac0b'
REVIEW_SUBMITTED = 1790135328  # 2026-09-23 03:48:48 UTC
DEADLINE = 1791259200  # 2026-10-06 00:00:00 America/New_York
SEARCH = dict(schema='mtg-kernel-v4-information-set-estimate-search/v3',
              algorithm='v4-depth-keyed-estimate-library-independent-chance/v3',
              root_allocation='round_robin', interior_bonus='prior_free',
              simulations=128, transitions=1024, depth=8, experiment_seed=20260922,
              embedding_table_sha256='9f2ba50d7097345caf1930edd2e911bad87383524b30bbe09726fb344096f2bb',
              feature_contract_digest='c4af415a3b0cf1e9c9960dbe2bc2d134c63e9f08206a9a364e113121fea5538b',
              feature_encoding_digest='271c0e5a0fdce75663c897e89a9d7280ab1a3bbb6679bd10ecb5f524991952de',
              model_parameter_sha256='614326d2ec55c94583b1b050451f770ce9404e03bc21b9fb5fb6cb4f7d32263f',
              weights_sha256='e2ca2f2b5dd750a59e24c71a4bac325ed7449d97b5892a79a80132e45d538333')
PANEL_SHA = 'eaca43dc397894d7a8726401ca32d3240e862a0ae82274900330d9b5ab4a9dcc'


def load(ref):
    return read(checked(ref))


def cluster_allocation(hosts, rates, overhead):
    """Outcome-independent greedy placement of the512 equal paired clusters."""
    counts = {host: 0 for host in hosts}
    owners = []
    for _ in range(512):
        host = min(hosts, key=lambda name: (overhead[name] + (counts[name]+4)/rates[name], name))
        counts[host] += 4
        owners.append(host)
    seconds = max(overhead[name] + counts[name]/rates[name] for name in hosts)
    return owners, seconds


def validate(manifest, host, now=None):
    now = time.time() if now is None else now
    require(now < DEADLINE, 'Goal deadline reached')
    require(manifest['schema'] == 'g115-d3-formal-launch/v1', 'Wrong experiment')
    require(manifest['source_commit'] == SOURCE, 'Unqualified native source')
    # Documents are the human-readable source of authority. Their reviewed
    # hashes are carried in the disposition, not replaced by a boolean flag.
    docs = manifest['documents']
    for name in ('preregistration', 'literature', 'power', 'analysis', 'power_core', 'reader'):
        checked(docs[name])
    here = Path(__file__).parent
    for name, filename in [('analysis', 'g115_d3_analysis_v1.py'),
                           ('power_core', 'g115_d3_power_core.py'),
                           ('reader', 'g115_d3_native_results_v1.py')]:
        require(sha(here / filename) == docs[name]['sha256'], 'Frozen analysis code changed')
    review = manifest['design_disposition']
    require(review is not None, 'Design disposition pending; no formal dispatch')
    require(review['source_commit'] == SOURCE and review['search'] == SEARCH,
            'Review disposition does not cover this route and budget')
    require(review['document_hashes'] == {k: v['sha256'] for k, v in docs.items()},
            'Review disposition covers different documents')
    note = checked(review['record']).read_text(encoding='utf-8')
    require(review['record_excerpt'].strip() and review['record_excerpt'] in note,
            'Disposition text absent from its record')
    if review['mode'] == 'reviewed':
        verdict = checked(review['verdict']).read_text(encoding='utf-8')
        require(review['verdict_excerpt'].strip() and review['verdict_excerpt'] in verdict,
                'Actual reviewer verdict text absent')
        require(review['accepted'] is True and review['unresolved_launch_blockers'] == [],
                'Design review has unresolved launch conditions')
    else:
        require(review['mode'] == 'standing-authority-after-72h', 'Unknown review disposition')
        require(now > REVIEW_SUBMITTED + 72 * 3600, 'Design wait has not exceeded72 hours')
        require(review['reason'].strip() and review['reason'] in checked(docs['preregistration']).read_text(encoding='utf-8'),
                'Standing-authority reason must be recorded in pre-registration')
    require(manifest['formal_measurement'] is True and manifest.get('preparation_only') is not True,
            'Preparation is not a formal launch manifest')
    require(manifest['panel']['sha256'] == PANEL_SHA, 'Shared panel changed')
    panel = load(manifest['panel'])
    bound = load(manifest['bindings'][host])
    require(bound['source_commit'] == SOURCE and bound['schema'] == 'g115-d3-bound-panel/v1',
            'Wrong formal binding')
    expected = {j['id']: j for j in panel['jobs']}
    actual = {j['id']: j for j in bound['jobs']}
    require(len(expected) == len(panel['jobs']) == len(actual) == len(bound['jobs']) == 2048,
            'Require all2048 panel jobs without duplicates')
    require(set(actual) == set(expected), 'Formal jobs differ')
    coordinates = {(j['cell'], j['replica'], j['candidate_seat'], j['arm']) for j in expected.values()}
    require(coordinates == {(c, r, s, a) for c in range(64) for r in range(8)
                            for s in (0, 1) for a in ('baseline', 'search')}, 'Panel pairing differs')
    # Bindings may relocate files, but cannot alter any match, sideboard,
    # generation adapter, capture option or frozen search budget.
    payload = load(manifest['payloads'][host])
    require(payload['source_commit'] == SOURCE, 'Payload source differs')
    for leaf in payload['leaf_outputs']:
        checked(leaf)
    require({ref['sha256'] for ref in payload['originals']} ==
            {ref['sha256'] for ref in payload['leaf_outputs']}, 'Leaf artifacts changed')
    for identifier, job in expected.items():
        command = load(actual[identifier]['request'])
        base = job['base_command']
        require({k: v for k, v in command.items() if k not in ('sources', 'output_directory')} ==
                {k: v for k, v in base.items() if k not in ('sources', 'output_directory')},
                'Frozen native options changed: ' + identifier)
        seat = job['candidate_seat']
        candidate, opponent = command['sources'][seat], command['sources'][1-seat]
        wanted = copy.deepcopy(payload['candidate'])
        if job['arm'] == 'search':
            wanted = dict(kind='information_set_search_v3', source=wanted['source'], descriptor=SEARCH)
        require(candidate == wanted and opponent == payload['opponent'], 'Source binding changed')
        require(opponent['kind'] == 'legacy' and opponent['v3_forced_actions'] is True and
                opponent['v3_spell_target_reference_adapter'] is True, 'Frozen V3 adapters changed')
        require(candidate['source']['checkpoint']['sha256'] ==
                '88c0b997708c2b5156b44f3940ad9d5d682f78ac24d346978bb3c9f34c59e8d1', 'Wrong g115')
        if job['arm'] == 'search':
            require(candidate['kind'] == 'information_set_search_v3' and
                    candidate['descriptor'] == SEARCH, 'Frozen search budget changed')
        else:
            require(candidate['kind'] == 'legacy' and candidate['v3_forced_actions'] is False,
                    'Baseline route changed')
        require(command['output_directory'] == actual[identifier]['native_output_directory'],
                'Output binding changed')
    placement = manifest['placement']
    inventory = load(placement['inventory'])
    require(set(inventory) == {'jack', 'haleyspc', 'runpod'}, 'Three-host inventory required')
    require(0 <= now - inventory['jack']['checked_unix'] <= 1800, 'Refresh placement inventory')
    require(inventory[host]['complete'], 'Selected host unavailable')
    allocations = placement['hosts']
    assignments = placement['assignments']
    require(set(assignments) == set(expected) and set(assignments.values()) == set(allocations),
            'Fleet must assign all jobs exactly once')
    for c in range(64):
        for r in range(8):
            owners = {assignments[j['id']] for j in expected.values() if j['cell'] == c and j['replica'] == r}
            require(len(owners) == 1, 'Keep both arms and seats in a seed cluster on one host')
    rates, overhead, common_serial = {}, {}, None
    qualified = placement['qualified_hosts']
    require(set(allocations) <= set(qualified), 'Selected host lacks qualification')
    for name, allocation in qualified.items():
        qspec, qresult = load(allocation['qualification_spec']), load(allocation['qualification_result'])
        require(qspec['source_commit'] == SOURCE and qspec['host'] == name and
                qresult['host'] == name and qresult['complete'] is True, 'Host qualification incomplete')
        phases = qresult['phases']
        require([p['workers'] for p in phases] == qspec['worker_counts'] and
                phases[0]['workers'] == 1 and len(phases) >= 2, 'Serial/parallel comparison incomplete')
        serial = {r['id']: r['sha256'] for r in phases[0]['rows']}
        require(len(serial) == 64, 'Representative cohort incomplete')
        if common_serial is None:
            common_serial = serial
        require(serial == common_serial, 'Hosts produce different semantic stores')
        for phase in phases:
            require(len(phase['rows']) == 64 and all(r['complete'] for r in phase['rows']) and
                    {r['id']: r['sha256'] for r in phase['rows']} == serial,
                    'Parallel qualification differs from serial')
            require(phase['seconds'] > 0 and
                    abs(phase['matches_per_second'] - 64/phase['seconds']) < 1e-12,
                    'Qualification rate differs from completed work/time')
        fastest = max(phases, key=lambda p: p['matches_per_second'])
        require(allocation['workers'] == fastest['workers'], 'Use fastest qualified worker count')
        rates[name] = fastest['matches_per_second']
        overhead[name] = allocation['startup_transfer_recovery_seconds']
        require(overhead[name] >= 0 and inventory[name]['complete'], 'Ineligible qualified host')
        checked(allocation['toolchain_receipt'])
        require(allocation['runtime_files'] == qspec['runtime_files'] and
                allocation['command_prefix'] == qspec['command_prefix'], 'Qualified runtime changed')
        if name in allocations:
            require(all(allocations[name][key] == value for key, value in allocation.items()),
                    'Selected allocation differs from qualified host')
    # Choices include single hosts and feasible combinations. Measured rates
    # and startup/transfer/recovery overhead must support the selected fleet.
    choices = placement['choices']
    combinations = {tuple(sorted(names)) for size in range(1, len(qualified)+1)
                    for names in itertools.combinations(qualified, size)}
    require({tuple(c['hosts']) for c in choices} == combinations and len(choices) == len(combinations),
            'Compare every qualified host combination')
    for choice in choices:
        _, seconds = cluster_allocation(choice['hosts'], rates, overhead)
        require(abs(choice['projected_seconds'] - seconds) < 1e-6, 'Projection differs from measured rates')
        require(choice['eligible'] or choice['ineligibility_reason'].strip(), 'Explain excluded allocation')
    chosen = [c for c in choices if c['id'] == placement['selected']]
    require(len(chosen) == 1 and chosen[0]['hosts'] == sorted(allocations), 'Allocation choice absent')
    require(chosen[0]['projected_seconds'] == min(c['projected_seconds'] for c in choices if c['eligible']),
            'A faster qualified allocation is available')
    require(chosen[0]['eligible'] and chosen[0]['projected_seconds'] > 0, 'Ineligible allocation')
    owners, _ = cluster_allocation(chosen[0]['hosts'], rates, overhead)
    require(all(assignments[j['id']] == owners[j['cell']*8+j['replica']] for j in expected.values()),
            'Assignments differ from qualified outcome-independent allocation')
    selected = allocations[host]
    for ref in selected['runtime_files']:
        checked(ref)
    require(any(r['path'] == selected['command_prefix'][-1] for r in selected['runtime_files']),
            'Native executable unpinned')
    require(selected['reserve_bytes'] >= (1 if host == 'runpod' else 32) * 2**30, 'Memory reserve too small')
    require(0 < selected['job_timeout_seconds'] <= 1800, 'Whole-match time bound required')
    require(0 < selected['shard_timeout_seconds'] <= 8 * 3600, 'Shard time bound required')
    require(host != 'runpod' or selected['guard_directory'] and selected['lease_name'], 'Paid lease missing')
    return panel, bound, selected, [identifier for identifier in expected if assignments[identifier] == host]


def launch(manifest, host, root):
    panel, bound, allocation, identifiers = validate(manifest, host)
    root = Path(root)
    root.mkdir()  # A completed, failed or partial shard is never overwritten.
    write(root / 'manifest.json', manifest)
    jobs = {j['id']: j for j in bound['jobs']}
    expected = {j['id']: j for j in panel['jobs']}
    guard = Path(allocation['guard_directory']) if host == 'runpod' else None
    pod = os.environ.get('RUNPOD_POD_ID')
    require(not pod or guard is not None, 'Cloud execution cannot bypass lease guard')
    stop = threading.Event()
    lock = threading.Lock()
    active = {}
    started = time.monotonic()
    productive = activity = time.time()
    rows = []

    def admission():
        require(time.time() < DEADLINE, 'Goal deadline reached')
        require(time.monotonic() - started < allocation['shard_timeout_seconds'], 'Shard time bound reached')
        require(free_memory() >= allocation['reserve_bytes'], 'Preserve memory reserve')
        require(shutil.disk_usage(root).free >= (1 if guard else 16) * 2**30, 'Preserve disk reserve')
        if guard:
            state = read(guard / 'guard.json')
            require(pod and state['pod_id'] == pod and state['name'] == allocation['lease_name'], 'Wrong lease')
            require(0 <= time.time() - state['epoch'] <= 75 and state['provider_ok'] and
                    state['allow_new_dispatch'] and not state['latched'], 'Lease disallows dispatch')
            require(not (guard / 'stop-request.json').exists(), 'Lease requested stop')

    def terminate(child):
        if child.poll() is not None:
            return
        if os.name == 'nt':
            child.kill()
        else:
            os.killpg(child.pid, signal.SIGTERM)
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            if os.name == 'nt':
                child.kill()
            else:
                os.killpg(child.pid, signal.SIGKILL)
            child.wait()

    def run(identifier):
        nonlocal productive
        require(not stop.is_set(), 'Shard stopped')
        admission()
        job = jobs[identifier]
        request = checked(job['request'])
        require(not Path(job['native_output_directory']).exists(), 'Native output already exists')
        env = {k: v for k, v in os.environ.items() if os.name == 'nt' and
               k.upper() in ('PATH', 'SYSTEMROOT', 'WINDIR', 'TEMP', 'TMP', 'COMSPEC')}
        env.update(PATH=env.get('PATH', '/usr/local/bin:/usr/bin:/bin'), LANG='C.UTF-8', LC_ALL='C.UTF-8',
                   CUDA_VISIBLE_DEVICES='', OMP_NUM_THREADS='1', MKL_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1')
        argv = allocation['command_prefix'] + [str(request)]
        if os.name == 'nt':
            kwargs = dict(creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        else:
            kwargs = dict(start_new_session=True)
            argv = ['nice', '-n', '10'] + argv
        begin = time.monotonic()
        row = dict(id=identifier, complete=False)
        with (root / (identifier + '.log')).open('xb') as log:
            child = subprocess.Popen(argv, env=env, stdout=log, stderr=subprocess.STDOUT, **kwargs)
            with lock:
                active[child.pid] = dict(child=child, cpu=0., rss=0, io=0)
            try:
                while child.poll() is None:
                    require(not stop.is_set(), 'Shard interrupted')
                    require(time.monotonic() - begin < allocation['job_timeout_seconds'], 'Whole-match time bound reached')
                    time.sleep(.2)
                require(child.returncode == 0, 'Native match failed')
                local = dict(job, output_directory=job['native_output_directory'])
                result = read_match(expected[identifier], local, SOURCE, bound['models'])
                row.update(complete=True, sha256=result['sha256'], games=result['games'], decisions=result['decisions'])
                productive = time.time()
            except Exception as error:
                row.update(error_type=type(error).__name__, error=str(error))
                stop.set()
            finally:
                terminate(child)
                with lock:
                    stats = active.pop(child.pid)
                row.update(seconds=time.monotonic()-begin, cpu_seconds_sampled=stats['cpu'],
                           peak_rss_sampled=stats['rss'], io_bytes_sampled=stats['io'], exit_code=child.returncode)
                write(root / (identifier + '.execution.json'), row)
        return row

    def dispatch(identifier):
        try:
            return run(identifier)
        except Exception:
            stop.set()
            raise

    result = dict(schema='g115-d3-shard-result/v1', complete=False, host=host,
                  formal_measurement=True, formal_verdict=None, assigned_jobs=identifiers)
    try:
        admission()
        with concurrent.futures.ThreadPoolExecutor(allocation['workers']) as pool:
            futures = [pool.submit(dispatch, identifier) for identifier in identifiers]
            while not all(f.done() for f in futures):
                try:
                    admission()
                except Exception:
                    stop.set()
                    raise
                with lock:
                    cpu = rss = io = 0
                    for pid, stats in active.items():
                        current = sample(pid)
                        if current:
                            if current['cpu'] > stats['cpu'] or current['io'] > stats['io']:
                                activity = time.time()
                            stats.update(cpu=current['cpu'], rss=max(stats['rss'], current['rss']), io=current['io'])
                        cpu += stats['cpu']; rss += stats['rss']; io += stats['io']
                    progress = dict(epoch=time.time(), pod_id=pod, last_productive_epoch=productive,
                                    last_activity_epoch=activity, native_alive=bool(active), queued_work=True,
                                    finished=False, workers=allocation['workers'], active=len(active),
                                    cpu_seconds=cpu, rss_bytes=rss, io_bytes=io, completed=sum(f.done() for f in futures))
                write(root / 'progress.json', progress)
                if guard:
                    write(guard / 'progress.json', progress)
                time.sleep(1)
            rows = [future.result() for future in futures]
        require(all(row['complete'] for row in rows), 'Incomplete shard retained')
        result['complete'] = True
    except Exception as error:
        result.update(error_type=type(error).__name__, error=str(error))
    finally:
        stop.set()
        with lock:
            for stats in active.values():
                terminate(stats['child'])
        result['rows'] = [read(root / (i + '.execution.json')) for i in identifiers
                          if (root / (i + '.execution.json')).exists()]
        result['not_started'] = [i for i in identifiers if not (root / (i + '.execution.json')).exists()]
        result['seconds'] = time.monotonic() - started
        write(root / 'completion.json', result)
        if guard:
            write(guard / 'progress.json', dict(pod_id=pod, epoch=time.time(), last_productive_epoch=productive,
                                               last_activity_epoch=time.time(), native_alive=False,
                                               queued_work=False, finished=True))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--host', choices=['jack', 'haleyspc', 'runpod'], required=True)
    parser.add_argument('--root', type=Path)
    parser.add_argument('--check-only', action='store_true')
    args = parser.parse_args()
    manifest = read(args.manifest)
    if args.check_only:
        _, _, _, jobs = validate(manifest, args.host)
        print(json.dumps(dict(checked=True, assigned_jobs=len(jobs), launched=False)))
    else:
        require(args.root is not None, 'Output root required')
        result = launch(manifest, args.host, args.root)
        print(json.dumps({k: v for k, v in result.items() if k not in ('rows', 'assigned_jobs', 'not_started')}))
        raise SystemExit(0 if result['complete'] else 1)


if __name__ == '__main__':
    main()
