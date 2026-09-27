"""Hash-pinned manifest of the 14 archival line (a) panel members (identity metadata only).

Recomputes every pin from the stores, the registry namespace comparison from Git blobs, each
member's Store authority route and the registry/training linkage, then folds in the sealed
export, round-trip and smoke receipts where they exist. Membership is the fixed proposal v0.2
roster; no outcome, score, classification or result file is opened.

Usage: python python/tools/line_a_panel_imports_v1.py REVISION OUTPUT.json
"""
import hashlib, json, os, subprocess, sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
REFRESH = Path('E:/mtg-kernel-population-v2-cycle3/refresh-manifests/population-v3-refresh-034.json')
V3B = Path('E:/archive-d/mtg-kernel-exploiter-v3b-20260726')
V3B_RECEIPT = V3B / 'build/build-receipt.json'
CYCLE3_NOTES = Path('E:/mtg-kernel-population-v2-cycle3/lineage/real-attempt-003/EVIDENCE-NOTES.md')
SEALED = Path('E:/mtg-line-a-panel-imports-20260927')
DESTINATION = REPO / 'data/cards_v1.json'
PROMOTED2_RUN = '2c9b7423004428c0e2bb138afafc15ec65957f6bd98c4587bea704fbf9549aae'
SENTINELS = {'8' * 64, '6' * 40, '7' * 64, '4' * 40, '1' * 64, '2' * 64, '3' * 64}
RELOCATIONS = {
    'D:/mtg-kernel-ladder-pilot-20260725': ['E:/archive-d/mtg-kernel-ladder-pilot-20260725'],
    'D:/mtg-kernel-scaled-selfplay-population-v1': ['E:/archive-d/mtg-kernel-scaled-selfplay-population-v1'],
    'D:/mtg-kernel-denovo-campaign-v1': ['E:/archive-d/mtg-kernel-denovo-campaign-v1'],
    'C:/mtg-kernel-population-v2-cycle2': ['E:/archive-d/mtg-kernel-population-v2-cycle2',
                                          'E:/c-evidence-archive-20260825/mtg-kernel-population-v2-cycle2'],
    'E:/mtg-kernel-population-v2-cycle3': ['E:/mtg-kernel-population-v2-cycle3'],
}
V3B_CENSUS = [(1, 0, 'f9a46d0e9f488ffa7cef18f6f82f1477d29a95f72fa38fa07470cdb1f7fcb243', 920013),
              (1, 1, '3dcc2c31d0824ca42d5c5ece11253462156e68475aa65d2fc974fe26ee7d0aa4', 920014),
              (1, 2, '64460bba057a41fd39a660ef35adcef9a7ed5f5980df5796b2f21b3439a9eaa8', 920015),
              (2, 0, '4a9e25476c5eb8dedcc1be5289da06c2265adb009d3f68389633b135256288fd', 920016),
              (2, 1, '96e5be520754e1625374bfd8bbdabe07cbcfa12d26551a9b5646f0465808fe82', 920017),
              (2, 2, 'c74b9b61a8458a5c81fe38620c25eb9848f87f7e8ca33523e744573afd39bc76', 920018)]
# Card-DB hash -> (training revision whose registry is the candidate, evidence of that linkage).
REGISTRY_CANDIDATES = {
    'a06fa9566106f0ea': ('1804e9f9f3bc76dd809b3c4aeddbba0a4ed894ec',
                         'v3b build receipt git_head; the card-DB hash covers colors, keyword tables and engine_capability (build.rs 3987-4057 at 1804e9f9), so every run recording it shares these card definitions'),
    '64c82a261e078f1a': ('18f4ca51419d215370d539cef726edd7f8e7ae0b',
                         'cycle-3 training commits 162b7579 and b01afb52 (EVIDENCE-NOTES.md) and search_authority.engine_commit a7043e30 (refresh manifests) all carry these registry bytes; the commit for generations 0-256 is not recorded'),
}
PARENT_REF_REQUESTED = {'refresh-034/anchor-0', 'refresh-034/anchor-1', 'refresh-034/current-0', 'refresh-034/current-1'}
RECOMPUTATION = {'a06fa9566106f0ea': SEALED / 'receipts/card-db-recomputation/1804e9f9.json',
                 '64c82a261e078f1a': SEALED / 'receipts/card-db-recomputation/18f4ca51.json'}
LINEAGE = SEALED / 'receipts/lineage-parents-v1.json'
LINKAGE = SEALED / 'receipts/registry-linkage-v1.json'
PINNED = Path('E:/pinned-binaries')
TOOLS = {'exporter': '7bb1bba3490cff7199e6be8dd60283958034c4f4033fecc98b82de4a5b1056d6/native_checkpoint_export_v1.exe',
         'roundtrip': '3956dda1cbb066f90ec20dc7c8ca7f04d5e2bfea7b2900714160b8311001a024/native_inference_export_roundtrip_v1.exe',
         'evaluator': '90457897e9b9a11c80214645e7d301c4b30036b539ad4022207c89ef859b6771/public_feature_evaluation_v1.exe'}
CYCLE3_TRAINING_COMMITS = ['162b7579f899c3df76910be59a9a076d14adfd5e', 'b01afb528d27ffed942a06a4ab9f606edfb6258c',
                           'a7043e3044d38f20dbb95afbc430af74d2f11ec8']


def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def git_blob(commit, path): return subprocess.run(['git', '-C', str(REPO), 'show', f'{commit}:{path}'], capture_output=True, check=True).stdout


def namespace_mismatches(source_bytes):
    source, destination = json.loads(source_bytes)['cards'], json.loads(DESTINATION.read_bytes())['cards']
    changed = []
    for index, (old, new) in enumerate(zip(source, destination)):
        old, new = dict(old), dict(new)
        old.pop('decks', None); new.pop('decks', None)
        if old != new: changed.append({'id': index, 'name': old.get('name'), 'fields': sorted(k for k in set(old) | set(new) if old.get(k) != new.get(k))})
    return len(source), changed


def registry_record(card_db_hash):
    revision, linkage = REGISTRY_CANDIDATES[card_db_hash]
    blob = git_blob(revision, 'data/cards_v1.json')
    count, changed = namespace_mismatches(blob)
    record = {'card_db_hash_u64_hex': card_db_hash, 'candidate_revision': revision, 'candidate_path': 'data/cards_v1.json',
              'candidate_sha256': hashlib.sha256(blob).hexdigest(), 'candidate_card_count': count, 'linkage': linkage,
              'destination_sha256': sha(DESTINATION), 'namespace_mismatch_count': len(changed),
              'namespace_mismatch_fields': sorted({f for c in changed for f in c['fields']}),
              'namespace_mismatch_examples': [c for c in changed if c['fields'] != ['engine_capability']][:5]}
    recomputation = RECOMPUTATION[card_db_hash]
    body = json.loads(recomputation.read_bytes())
    record['card_db_hash_recomputation'] = {'receipt': recomputation.as_posix(), 'sha256': sha(recomputation),
                                            'training_commit': body['training_commit'], 'computed': body['computed_card_db_hash_u64_hex'],
                                            'matches_recorded': body['matches']}
    if changed:
        record['namespace_v1'] = 'fails'
        record['status'] = 'unresolved'
        record['reason'] = ('validate_card_namespace (sideboard_play_policy_v1.rs:1416) rejects this registry against the destination; '
                            'the R14 versioned allowlist route (DIRECTOR-RULINGS-20260927.md R14) is implemented at 3dbe2cb7 with its tests passing; '
                            'admission waits for the R14 design verdict and CODEX #557 item 5')
        return record
    snapshot = REPO / 'data/line_a_source_registries' / (record['candidate_sha256'] + '.json')
    record.update({'namespace_v1': 'passes', 'status': 'passes-v1',
                   'snapshot_repo_path': snapshot.relative_to(REPO).as_posix(), 'snapshot_sha256': sha(snapshot)})
    return record


def sealed_linkage(label):
    """Typed linkage class and evidence digest from the sealed registry-linkage receipt."""
    record = json.loads(LINKAGE.read_bytes())['members'][label]
    return {'class': record['class'], 'receipt': LINKAGE.as_posix(), 'receipt_sha256': sha(LINKAGE),
            'registry_blobs_named': record['registry_blobs_named'], 'all_named_blobs_equal_pinned': record['all_named_blobs_equal_pinned']}


def sealed_lineage(label):
    chain = json.loads(LINEAGE.read_bytes())['members'].get(label)
    if not chain:
        return None
    first = chain[0]
    return {'receipt': LINEAGE.as_posix(), 'receipt_sha256': sha(LINEAGE),
            'direct_parent': first.get('parent'), 'relation': first.get('relation'),
            'chain': [{'run_sha256': step['run_sha256'], 'base_seed': step.get('base_seed')} for step in chain]}


def registry_linkage(facts, provenance):
    """Typed evidence class linking a member's training to its registry bytes (FABLE-REVIEW-20260927 change 2)."""
    if provenance.get('status') == 'resolved' and provenance.get('git_head'):
        blob = hashlib.sha256(git_blob(provenance['git_head'], 'data/cards_v1.json')).hexdigest()
        return {'class': 'build-receipt', 'receipt_path': provenance['build_receipt_path'], 'receipt_sha256': provenance['build_receipt_sha256'],
                'clean_commit': provenance['git_head'], 'registry_blob_sha256': blob}
    if facts['card_db_hash_u64_hex'] == '64c82a261e078f1a':
        commits = {c: hashlib.sha256(git_blob(c, 'data/cards_v1.json')).hexdigest() for c in CYCLE3_TRAINING_COMMITS}
        return {'class': 'commit-recorded',
                'scope': 'generations 256-2048; the commit for generations 0-256 and the executable are not recorded; no build receipt',
                'training_commit_registry_sha256': commits, 'evidence': {'path': CYCLE3_NOTES.as_posix(), 'sha256': sha(CYCLE3_NOTES)}}
    return {'class': 'card-db-hash-only',
            'scope': 'no build receipt or execution manifest located yet for this lineage (to search: 971221, 971222, 971223 and the anchors)'}


def store_record(root, generation, pinned_root):
    root = Path(root); ref_path = root / f'refs/update-{generation:08d}.ref.json'
    ref = json.loads(ref_path.read_bytes()); run = json.loads((root / 'run.json').read_bytes())
    env_v2 = 'environment_randomization_v2' in json.dumps(run.get('contracts', {}))
    route = 'population' if env_v2 else ('original' if ref['run_sha256'] == PROMOTED2_RUN else 'none')
    provenance = [run.get('source', {}).get(k) for k in ('binary_sha256', 'git_commit', 'source_tree_sha256')]
    provenance += [run.get('toolchain', {}).get('rustc_commit_hash')]
    provenance += [run.get('package', {}).get(k) for k in ('cargo_lock_sha256', 'crate_manifest_sha256', 'workspace_manifest_sha256')]
    store = {'pinned_store_root': pinned_root, 'resolved_store_root': root.as_posix(), 'generation': generation,
             'ref_path': ref_path.as_posix(), 'ref_sha256': sha(ref_path), 'run_sha256': ref['run_sha256'],
             'run_json_sha256_verified': sha(root / 'run.json') == ref['run_sha256'],
             **{k: ref[k] for k in ('checkpoint_manifest_sha256', 'checkpoint_sidecar_sha256', 'checkpoint_payload_sha256',
                                    'train_state_sha256', 'model_parameter_sha256')}}
    facts = {'card_db_hash_u64_hex': run['environment']['card_db_hash_u64_hex'], 'training_deck_ids': run['environment']['deck_ids'],
             'architecture_identity': run['contracts']['model']['architecture_identity'],
             'feature_contract_digest': run['contracts']['tensorizer']['feature_contract_digest'],
             'feature_encoding_digest': run['contracts']['tensorizer']['feature_encoding_digest'],
             'environment_contract': 'environment-randomization-v2' if env_v2 else 'legacy-v1',
             'store_authority_route': route, 'run_json_provenance_sentinel_fields': sum(v in SENTINELS for v in provenance)}
    return store, facts


def sealed_receipts(label):
    base = SEALED / 'receipts' / label.replace('/', '__'); out = {}
    for name in ('summary', 'roundtrip', 'smoke'):
        path = base / f'{name}.json'
        if path.exists(): out[name] = {'path': path.as_posix(), 'sha256': sha(path), 'body': json.loads(path.read_bytes())}
    return out


def member(index, label, family, store, facts, pin_check, provenance, registries):
    registry = registries.setdefault(facts['card_db_hash_u64_hex'], registry_record(facts['card_db_hash_u64_hex']))
    open_items = []
    if registry['status'] == 'unresolved': open_items.append('registry: ' + registry['reason'])

    if facts['store_authority_route'] == 'none':
        open_items.append('store authority: legacy-v1 Store that is not the promoted(2) run; no base load_checkpoint_v1 authority admits it')
    if provenance['status'] != 'resolved': open_items.append('training provenance: ' + provenance['reason'])
    receipts = sealed_receipts(label)
    summary, roundtrip, smoke = (receipts.get(k) for k in ('summary', 'roundtrip', 'smoke'))
    version = ('version-1 target (R3: archival roles, Rally-opponent slots only)' if index <= 9
               else 'later-version candidate (R10: after the eight, by 2026-10-04 18:00 EDT)')
    record = {'roster_index': index, 'label': label, 'family': family, 'version_role': version, 'store': store,
              'identity_pin_check': pin_check, 'source_facts': facts, 'registry': registry,
              'registry_linkage': sealed_linkage(label), 'training_provenance': provenance}
    lineage = sealed_lineage(label)
    if lineage:
        record['lineage'] = lineage
    if summary:
        body = summary['body']
        record.update({'export': body['export'], 'import_descriptor': body['import_descriptor'], 'model_source': body['model_source'],
                       'legacy_adapter_flags': {'v3_forced_actions': True, 'v3_spell_target_reference_adapter': True}})
    if roundtrip: record['roundtrip'] = {'receipt': {k: roundtrip[k] for k in ('path', 'sha256')}, 'passed': roundtrip['body'].get('passed'),
                                          'tensor_count': roundtrip['body'].get('tensor_count'), 'checks': roundtrip['body'].get('checks')}
    if smoke: record['smoke'] = {'receipt': {k: smoke[k] for k in ('path', 'sha256')}, 'completed': smoke['body'].get('completed'),
                                  **{k: smoke['body'].get(k) for k in ('command_sha256', 'deck_pair', 'registered_sha256', 'postboard_sha256',
                                                                       'max_physical_games', 'evaluator_sha256', 'evaluator_git_head')}}
    # Provenance gaps are recorded, not a playability gate; registry and authority gaps are.
    routable = registry['status'] != 'unresolved' and facts['store_authority_route'] != 'none'
    done = bool(roundtrip and roundtrip['body'].get('passed') and smoke and smoke['body'].get('completed'))
    record['status'] = 'unresolved' if not routable else ('playable' if done else 'pending')
    if routable and not done: open_items.append('export, round trip or smoke receipt not yet sealed')
    record['open_items'] = open_items
    return record


def toolchain_record(members):
    tools = {}
    for role, relative in TOOLS.items():
        path = PINNED / relative
        tools[role] = {'path': path.as_posix(), 'sha256': sha(path), 'pinned_name_matches': path.parent.name == sha(path)}
    exported = next((m for m in members if m.get('export')), None)
    if exported:
        tools['exporter']['build_identity'] = exported['export']['exporter_build']
        tools['exporter']['clean_tree_attestation'] = ('the production build capture refuses a dirty build: validate_build_capture_constants requires '
                                                       'NATIVE_STORE_BUILD_SOURCE_WORKTREE_CLEAN_V1 and an empty git-status digest '
                                                       '(native_store_production_capture_v2.rs:512-517), and the export succeeded')
    return tools


def main(revision, output):
    refresh = json.loads(REFRESH.read_bytes()); registries = {}; members = []
    for slot in refresh['slots']:
        generation = slot['source_generation']; pinned = slot['store_root'].replace(chr(92), '/')
        candidates = [pinned] + [new + pinned[len(old):] for old, news in RELOCATIONS.items() if pinned.startswith(old) for new in news]
        root = next(r for r in candidates if (Path(r) / f'refs/update-{generation:08d}.ref.json').is_file())
        store, facts = store_record(root, generation, slot['store_root'])
        check = {'against': f"refresh-034 slot {slot['slot_index']}",
                 'all_match': all(store[k] == slot[k] for k in ('model_parameter_sha256', 'checkpoint_manifest_sha256', 'checkpoint_payload_sha256', 'run_sha256'))}
        if facts['card_db_hash_u64_hex'] == '64c82a261e078f1a':
            provenance = {'status': 'partial', 'source': 'EVIDENCE-NOTES.md and refresh-manifest search_authority.engine_commit (run.json provenance fields are sentinels)',
                          'training_commits': CYCLE3_TRAINING_COMMITS, 'reason': 'engine commits recorded for generations 256-2048 only; executable identity not recorded'}
        else:
            provenance = {'status': 'unresolved', 'reason': 'run.json carries sentinel provenance and no archived build receipt was located for this store'}
        members.append(member(2 + slot['slot_index'], 'refresh-034/' + slot['role'], 'population-v3-refresh-034', store, facts, check, provenance, registries))
    receipt = json.loads(V3B_RECEIPT.read_bytes())
    for arm, run, ref_sha, seed in V3B_CENSUS:
        root = V3B / f'runs-arm{arm}/dev0/run-{run}/store'
        store, facts = store_record(root, 512, root.as_posix())
        provenance = {'status': 'resolved', 'source': 'archived build receipt (run.json provenance fields are sentinels)',
                      'build_receipt_path': V3B_RECEIPT.as_posix(), 'build_receipt_sha256': sha(V3B_RECEIPT), 'git_head': receipt['git_head'],
                      'git_status_clean': receipt['git_status_clean'], 'executable_sha256': receipt['executable_sha256'], 'feature': receipt['feature'],
                      'cargo_lock_sha256': receipt['cargo_lock_sha256'], 'executable_available': Path(receipt['executable']).exists(), 'training_seed': seed}
        members.append(member(10 + run if arm == 1 else 13 + run, f'exploiter-v3b/arm{arm}/run-{run}', 'exploiter-v3b-20260726', store, facts,
                              {'against': 'proposal census 2026-09-26 22:40 EDT (ref SHA256)', 'all_match': store['ref_sha256'] == ref_sha}, provenance, registries))
    members.sort(key=lambda m: m['roster_index'])
    counts = {s: sum(m['status'] == s for m in members) for s in ('playable', 'pending', 'unresolved')}
    document = {'schema': 'mtg-kernel-line-a-panel-imports/v1', 'revision': int(revision),
                'status': f"{counts['playable']} playable, {counts['pending']} pending, {counts['unresolved']} unresolved of 14 archival members",
                'producer': 'Opus lane panel-export (collab/GOALS/opus-panel-export-20260927.md), branch opus/panel-export-v1, python/tools/line_a_panel_imports_v1.py',
                'roster': 'proposal v0.2 (collab/CODEX-G115-OPPONENT-PORTFOLIO-PROPOSAL-20260926.md, 2026-09-27 10:18 EDT): 0 V3 and 1 D3 wrapper are outside this manifest; 2-9 refresh-034 slots 0-7; 10-12 v3b arm1 runs 0-2; 13-15 v3b arm2 runs 0-2',
                'nonclaims': ['No match outcome, score, classification or calibration value was read or is recorded here.',
                              'Membership is the fixed v0.2 roster; no member was chosen, dropped or reweighted by any result.',
                              'Playable means exported, imported bit-exactly and loaded by the public evaluator; it is not a strength claim.'],
                'sources': {'refresh_manifest': {'path': REFRESH.as_posix(), 'sha256': sha(REFRESH)}, 'v3b_archive_root': V3B.as_posix(),
                            'sealed_root': SEALED.as_posix(),
                            'rulings': 'collab/DIRECTOR-RULINGS-20260927.md R3, R10, R11, R14; collab/FABLE-REVIEW-20260927.md (panel-export verdict)'},
                'toolchain': toolchain_record(members),
                'destination': {'registry': 'data/cards_v1.json', 'registry_sha256': sha(DESTINATION),
                                'v3_feature_contract_digest': '9319fbd41e6b42ec90d565c13f3b4f75a3898dafe3816387453c08419ba9cc68',
                                'v3_feature_encoding_digest': 'c4662291ca9a75525b51b51f3b5d512671340c05b69fd33fb0827c0c8af70a2b'},
                'members': members}
    data = (json.dumps(document, indent=2) + '\n').encode('utf-8')
    Path(output).write_bytes(data)
    print(output, hashlib.sha256(data).hexdigest(), document['status'])


if __name__ == '__main__':
    main(*sys.argv[1:3])
