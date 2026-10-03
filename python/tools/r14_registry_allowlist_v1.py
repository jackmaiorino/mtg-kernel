"""Write the R14 registry-evolution allowlist and the training-era registry snapshot it is keyed to.

The allowlist enumerates every per-card field difference (deck membership excluded, as in
validate_card_namespace) between the training-era source registry and the compiled destination
registry data/cards_v1.json. It is keyed to the exact (source, destination) SHA256 pair; the R14
route admits a source only when its observed difference set equals this list exactly.

Usage: python python/tools/r14_registry_allowlist_v1.py SOURCE_COMMIT
"""
import hashlib, json, subprocess, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / 'data/line_a_source_registries'


def main(source_commit):
    source_bytes = subprocess.run(['git', '-C', str(REPO), 'show', f'{source_commit}:data/cards_v1.json'],
                                  capture_output=True, check=True).stdout
    destination_bytes = (REPO / 'data/cards_v1.json').read_bytes()
    source_sha, destination_sha = (hashlib.sha256(b).hexdigest() for b in (source_bytes, destination_bytes))
    source, destination = json.loads(source_bytes)['cards'], json.loads(destination_bytes)['cards']
    if len(source) > len(destination): raise SystemExit('source registry longer than destination')
    entries = []
    for index, (old, new) in enumerate(zip(source, destination)):
        if old.get('name') != new.get('name'): raise SystemExit(f'name or order differs at id {index}')
        fields = sorted((set(old) | set(new)) - {'decks'})
        differences = [{'field': f, 'source_present': f in old, 'source': old.get(f), 'destination_present': f in new,
                        'destination': new.get(f)} for f in fields if (f in old) != (f in new) or old.get(f) != new.get(f)]
        if differences: entries.append({'id': index, 'name': old['name'], 'differences': differences})
    if not entries: raise SystemExit('no differences: the V1 strict route applies')
    snapshot = OUT / f'{source_sha}.json'
    if snapshot.exists() and snapshot.read_bytes() != source_bytes: raise SystemExit('snapshot bytes differ')
    snapshot.write_bytes(source_bytes)
    allowlist = {'schema': 'mtg-kernel-registry-evolution-allowlist/v1',
                 'source_registry_sha256': source_sha, 'source_registry_revision': source_commit,
                 'source_card_count': len(source),
                 'destination_registry_sha256': destination_sha, 'destination_card_count': len(destination),
                 'excluded_fields': ['decks'],
                 'field_difference_count': sum(len(e['differences']) for e in entries),
                 'entries': entries}
    data = (json.dumps(allowlist, indent=2, sort_keys=True) + '\n').encode('utf-8')
    path = OUT / 'allowlists' / f'r14-{source_sha[:16]}-to-{destination_sha[:16]}.json'
    path.parent.mkdir(exist_ok=True)
    path.write_bytes(data)
    print(path.relative_to(REPO).as_posix(), hashlib.sha256(data).hexdigest(), len(entries), 'cards',
          allowlist['field_difference_count'], 'field differences')


if __name__ == '__main__':
    main(sys.argv[1])
