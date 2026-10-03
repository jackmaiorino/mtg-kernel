"""Compare all shared-panel baseline stores before releasing D3 statistics.

Only declared build provenance and additive terminal-audit telemetry differ
between the archived and current representations. Every other field is exact.
"""
import copy
import hashlib
import json
import re
from pathlib import Path


def read(path):
    return json.loads(Path(path).read_bytes())


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def precheck_index(panel, reference, proof):
    """Re-derive the two tiers from actual pinned stores, never proof booleans."""
    if (not proof['complete'] or proof['source']!='cd41885e0ac05586d89bd4b2b7fb1284248689ef' or
        proof['binary_sha256']!='deb637ac2cb836504aad0dfb68c532c6a7a9a053f286e7c9a7de5ace5f471019' or
        proof['reverse_source']!='e990740a76726bb05f34bd2dee872f89e55f0671' or
        proof['reverse_binary_sha256']!='f1069ab05911dc05e5a1a4ccd3ed7a55448bf250feee9bd79326e51307782517'):
        raise ValueError('Wrong complete baseline precheck or reversal source')
    expected={j['id']:j for j in panel['jobs'] if j['arm']=='baseline'}
    refs={j['id']:j for j in reference['jobs']}
    current={r['id']:r for r in proof['current']['rows']}
    reverse={r['id']:r for r in proof['reversal']['rows']}
    if len(expected)!=1024 or set(current)!=set(expected) or len(proof['current']['rows'])!=1024:
        raise ValueError('Baseline precheck must contain exactly 1024 current stores')
    if len(reverse)!=len(proof['reversal']['rows']):raise ValueError('Duplicate reversal condition')
    explained=set();result={}
    from g115_d3_native_results_v1 import read_match
    spec=proof['spec']
    if sha(spec['path'])!=spec['sha256']:raise ValueError('Precheck specification changed')
    models=read(spec['path'])['models']
    for identifier,job in expected.items():
        def pinned(ref):
            if sha(ref['path'])!=ref['sha256']:raise ValueError('Precheck or archive bytes changed')
            return read(ref['path'])
        def native(row,source):
            store=pinned(row['store']);folder=Path(row['store']['path']).parent
            request=folder.parent/'request.json'
            read_match(job,dict(request=dict(path=str(request),sha256=sha(request)),output_directory=str(folder)),source,models)
            return store
        archived=pinned(refs[identifier]['store']);store=native(current[identifier],proof['source'])
        old,new=normalized_pair(archived,store,job['candidate_seat'],proof['v3_envelopes'])
        if old['match']!=job['base_command']['matches'][0]:raise ValueError('Precheck condition differs')
        fields=sorted(k for k in set(old)|set(new) if old.get(k)!=new.get(k) or (k in old)!=(k in new))
        if fields:
            if identifier not in reverse:raise ValueError('Missing reversal for mismatching precheck')
            rold,rnew=normalized_pair(archived,native(reverse[identifier],proof['reverse_source']),job['candidate_seat'],proof['reverse_envelopes'])
            if rold!=rnew:raise ValueError('Reversal does not explain baseline drift')
            explained.add(identifier)
        result[identifier]=dict(tier='trample_reversal' if fields else 'archive_equal',
            precheck_sha256=current[identifier]['store']['sha256'],precheck_differing_fields=fields,
            matchup=job['base_command']['matches'][0]['deck_ids'] if 'deck_ids' in job['base_command']['matches'][0] else job['cell'])
    if set(reverse)!=explained:raise ValueError('Reversal coverage must exactly match precheck drift')
    return result


def compare_overlap(panel, execution, prior, allowed_envelopes):
    if (prior['source_commit']!='cd41885e0ac05586d89bd4b2b7fb1284248689ef' or
        prior['invalid_attempt_sha256']!='50f562871c589ad59e64d439b1fbc90659ad3de5a19a3b22442a17e55256eee8' or
        len(prior['stores'])!=812 or len({r['id'] for r in prior['stores']})!=812):
        raise ValueError('Overlap must preserve all 812 attempt002 stores')
    expected={j['id']:j for j in panel['jobs']};actual={j['id']:j for j in execution['jobs']}
    rows=[];failures=[]
    for row in prior['stores']:
        identifier=row['id'];job=expected[identifier];old_ref=row['store']
        if sha(old_ref['path'])!=old_ref['sha256']:raise ValueError('Retained attempt002 store changed')
        path=Path(actual[identifier]['output_directory'])/'match-000000.json'
        if not path.exists():rows.append(dict(id=identifier,status='missing_current'));continue
        try:
            old,new=normalized_pair(read(old_ref['path']),read(path),job['candidate_seat'],allowed_envelopes)
            equal=old==new;rows.append(dict(id=identifier,status='equal' if equal else 'mismatch',raw_equal=sha(path)==old_ref['sha256']))
            if not equal:failures.append(dict(id=identifier,error='Same-source overlap differs from retained attempt002'))
        except (OSError,ValueError,KeyError,TypeError) as error:failures.append(dict(id=identifier,error=str(error)))
    return dict(expected=812,compared=sum(r['status']!='missing_current' for r in rows),
                semantic_equal=sum(r['status']=='equal' for r in rows),
                missing_current=sum(r['status']=='missing_current' for r in rows),rows=rows,failures=failures)


def normalized_pair(archived, current, seat, allowed_envelopes):
    old, new = copy.deepcopy(archived), copy.deepcopy(current)
    if 'terminal_audit_v1' not in old and 'terminal_audit_v1' in new:
        # Observational counts/roots added after the archive, not game state.
        del new['terminal_audit_v1']
    before = old['models'][1-seat]['identity']['source_import']['appended_rows']
    after = new['models'][1-seat]['identity']['source_import']['appended_rows']
    pattern = r'envelope_sha256=([0-9a-f]{64})'
    old_hash, new_hash = re.findall(pattern, before), re.findall(pattern, after)
    if len(old_hash) != 1 or len(new_hash) != 1 or new_hash[0] not in allowed_envelopes:
        raise ValueError('Unregistered V3 build provenance')
    replacement = after.replace('envelope_sha256='+new_hash[0], 'envelope_sha256='+old_hash[0])
    if replacement != before:
        raise ValueError('V3 source import changed beyond declared build provenance')
    new['models'][1-seat]['identity']['source_import']['appended_rows'] = before
    return old, new


def compare(panel, execution, reference, allowed_envelopes, precheck=None):
    expected = {j['id']: j for j in panel['jobs'] if j['arm'] == 'baseline'}
    refs = {j['id']: j for j in reference['jobs']}
    actual = {j['id']: j for j in execution['jobs']}
    if len(expected) != 1024 or len(refs) != len(reference['jobs']) or set(refs) != set(expected):
        raise ValueError('Baseline archive must cover the exact 1024 conditions')
    tiers=precheck_index(panel,reference,precheck) if precheck is not None else None
    rows, failures = [], []
    for identifier, job in expected.items():
        try:
            ref = refs[identifier]
            if (ref['archive_job'], ref['archive_match_index']) != (job['archive_job'], job['archive_match_index']):
                raise ValueError('Archive condition mapping changed')
            old_path = Path(ref['store']['path'])
            if sha(old_path) != ref['store']['sha256']:
                raise ValueError('Archived semantic store hash changed')
            new_path = Path(actual[identifier]['output_directory']) / 'match-000000.json'
            new_hash = sha(new_path)
            old, new = normalized_pair(read(old_path), read(new_path), job['candidate_seat'], allowed_envelopes)
            if old['match'] != job['base_command']['matches'][0]:
                raise ValueError('Archive match differs from the fixed panel')
            differences = sorted(k for k in set(old) | set(new) if old.get(k) != new.get(k) or (k in old) != (k in new))
            rows.append(dict(id=identifier, raw_hash_equal=new_hash == ref['store']['sha256'],
                             semantic_equal=not differences, differing_fields=differences,
                             archive_sha256=ref['store']['sha256'], current_sha256=new_hash))
            if tiers is not None:
                rows[-1].update(tiers[identifier])
                if new_hash!=tiers[identifier]['precheck_sha256']:
                    failures.append(dict(id=identifier,error='Formal baseline differs from frozen current-engine precheck'))
            elif differences:
                failures.append(dict(id=identifier, error='Unexplained baseline mismatch', differing_fields=differences))
        except (OSError, ValueError, KeyError, TypeError) as error:
            failures.append(dict(id=identifier, error=str(error)))
    return dict(schema='g115-d3-baseline-parity/v1', complete=not failures,
                expected=1024, compared=len(rows), raw_hash_mismatches=sum(not r['raw_hash_equal'] for r in rows),
                semantic_mismatches=sum(not r['semantic_equal'] for r in rows),
                explained_trample_mismatches=sum(r.get('tier')=='trample_reversal' for r in rows),
                failures=failures, rows=rows,
                normalization='Only additive terminal_audit_v1 and predeclared V3 envelope build provenance; all other fields exact.')
