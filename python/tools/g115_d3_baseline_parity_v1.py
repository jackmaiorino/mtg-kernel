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


def compare(panel, execution, reference, allowed_envelopes):
    expected = {j['id']: j for j in panel['jobs'] if j['arm'] == 'baseline'}
    refs = {j['id']: j for j in reference['jobs']}
    actual = {j['id']: j for j in execution['jobs']}
    if len(expected) != 1024 or len(refs) != len(reference['jobs']) or set(refs) != set(expected):
        raise ValueError('Baseline archive must cover the exact 1024 conditions')
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
            if differences:
                failures.append(dict(id=identifier, error='Unexplained baseline mismatch', differing_fields=differences))
        except (OSError, ValueError, KeyError, TypeError) as error:
            failures.append(dict(id=identifier, error=str(error)))
    return dict(schema='g115-d3-baseline-parity/v1', complete=not failures,
                expected=1024, compared=len(rows), raw_hash_mismatches=sum(not r['raw_hash_equal'] for r in rows),
                semantic_mismatches=sum(not r['semantic_equal'] for r in rows),
                failures=failures, rows=rows,
                normalization='Only additive terminal_audit_v1 and predeclared V3 envelope build provenance; all other fields exact.')
