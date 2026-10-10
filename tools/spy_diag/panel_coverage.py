"""Exact root coverage for full panels and explicitly selected qualifications."""
from collections import Counter


def expected_roots(panel, selected=None):
    rows = panel['roots']
    ids = [row['root_id'] for row in rows]
    if not ids or len(set(ids)) != len(ids):
        raise ValueError('panel must contain unique nonempty root coverage')
    expected = set(ids)
    if selected is not None:
        if not selected or len(set(selected)) != len(selected) or not set(selected) <= expected:
            raise ValueError('qualification roots must be unique members of the panel')
        expected = set(selected)
    return expected


def require_coverage(observed, expected):
    counts = Counter(observed)
    duplicates = sorted(root for root, count in counts.items() if count != 1)
    missing = sorted(expected - counts.keys())
    extra = sorted(counts.keys() - expected)
    if duplicates or missing or extra:
        raise ValueError(f'root coverage differs: missing={missing}, extra={extra}, duplicates={duplicates}')
