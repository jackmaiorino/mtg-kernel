"""W5 tools: the w5-xact-v1 mirror and the retention, budget and selection rules, on synthetic chain trees."""
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
sys.path.insert(0, str(Path(__file__).resolve().parent.parent.parent / 'ex1'))
import capture0b  # noqa: E402
import qual  # noqa: E402
import retention  # noqa: E402
import xact  # noqa: E402


class MirrorTests(unittest.TestCase):
    def test_hashes_match_the_rust_pins(self):
        # The same values are pinned in mtg-kernel/src/w5_xact_v1.rs.
        h = xact.hash_u64('w5/placement', 'w5/chain1', 7, 42, 3)
        self.assertEqual(h, 5_962_464_029_930_671_119)
        self.assertEqual(xact.placement_index('w5/chain1', 7, 42, 3), 42)
        a = xact.hash_u64('w5/act', 'w5/chain1', 7, 42, 3)
        self.assertEqual(a, 15_846_080_588_647_116_512)
        self.assertEqual(xact.uniform(a), 0.8590177499795791)
        # A non-ASCII chain id hashes its UTF-8 bytes, as serde_json writes them.
        u = xact.hash_u64('w5/placement', 'w5/cha\u00een1', 7, 42, 3)
        self.assertEqual(u, 16_614_139_203_310_030_779)
        self.assertEqual(1 + (u >> 57), 116)

    def test_placement_index_spans_one_to_128(self):
        self.assertEqual(1 + (0 >> 57), 1)
        self.assertEqual(1 + ((2**64 - 1) >> 57), 128)
        for h in (0, 1, 2**57, 2**57 - 1, 2**63, 2**64 - 1):
            self.assertEqual(1 + (h >> 57), 1 + int(128 * xact.uniform(h)))

    def test_inverse_cdf(self):
        q = [0.25, 0.0, 0.5, 0.25]
        self.assertEqual(xact.sample_from_q(q, 0.0), 0)
        self.assertEqual(xact.sample_from_q(q, 0.25), 2)
        self.assertEqual(xact.sample_from_q(q, 0.75), 3)
        self.assertEqual(xact.sample_from_q([0.5, 0.25, 0.0], 0.9), 1)
        with self.assertRaises(ValueError):
            xact.sample_from_q([0.0, 0.0], 0.1)
        with self.assertRaises(ValueError):
            xact.sample_from_q([float('nan'), 1.0], 0.1)

    def test_bits_round_trip(self):
        values = [0.1, 0.25, 1e-300]
        bits = [int.from_bytes(__import__('struct').pack('<d', v), 'little') for v in values]
        self.assertEqual(xact.bits_to_floats(bits), values)


S_RECORD = 'w5-s-learner-checkpoint/v1'


def make_chain(tmp, arm, segments, per_segment, ledger_step=1000, t0=1.0e9, dt=60.0, complete_last=True,
               s_learner=False):
    """Synthetic segments: every update's complete.json, update.json and checkpoint file (with `s_learner`, an S
    checkpoint record and the S container it pins)."""
    tmp = Path(tmp)
    tmp.mkdir(parents=True, exist_ok=True)
    segs = []
    for s in range(segments):
        root = tmp / f'{arm}-seg{s:03d}'
        cfg = {'output_directory': root.as_posix(), 'iterations': [{'episodes': [{'opponent': {'kind': 'current'}, 'tag': [s, i]}]}
                              for i in range(per_segment)],
               'initial_source': {'play_import': {'path': 'import.json', 'sha256': 'i'}, 'checkpoint': None},
               'ex1': {'run': 'w5/chain1', 'update_offset': s * per_segment} if arm == 'X' else None,
               'chain_ledger': {'source': 'ledger.json'}}
        if arm != 'X':
            del cfg['ex1']
        cfg_path = tmp / f'{arm}-seg{s:03d}.json'
        cfg_path.write_text(json.dumps(cfg))
        for i in range(per_segment):
            update = s * per_segment + i
            it = root / 'iterations' / f'{i:06d}'
            att = it / 'attempt-000000'
            (att / 'update').mkdir(parents=True)
            (att / 'collect').mkdir(parents=True)
            (att / 'collect' / 'episode-0000.json').write_text('x' * 10)
            if arm == 'X':  # the EX1/W5 search-target sidecar beside the trajectory
                (att / 'collect' / 'ex1-0000.json').write_text('y' * 10)
            ck = att / 'update' / 'checkpoint.json'
            state = None
            if s_learner:
                container = att / 'update' / 's-train-state.bin'
                container.write_bytes(f'container-{arm}-{update}'.encode() * 4)
                state = {'path': container.as_posix(), 'sha256': retention.sha(container)}
                ck.write_text(json.dumps({'schema': S_RECORD, 'adam_step': update + 1, 'state': state}))
            else:
                ck.write_text(f'state-{arm}-{update}')
            doc = {'after_state_sha256': f'h{update + 1}', 'checkpoint': {'path': ck.as_posix(),
                                                                          'sha256': retention.sha(ck)}}
            if state is not None:
                doc['s_learner'] = {'architecture': 's-object-transformer-v1', 'state': state}
            (att / 'update' / 'update.json').write_text(json.dumps(doc))
            receipt = {'update': {'path': (att / 'update' / 'update.json').as_posix()},
                       'chain_ledger': {'usec': (update + 1) * ledger_step, 'unix_seconds': t0 + (update + 1) * dt,
                                        'instance': 0}}
            (it / 'complete.json').write_text(json.dumps(receipt))
        if s < segments - 1 or complete_last:
            (root / 'completion.json').write_text('{}')
        segs.append({'config': cfg_path.as_posix(), 'offset': s * per_segment})
    manifest = tmp / f'{arm}-chain.json'
    manifest.write_text(json.dumps({'schema': 'w5-chain/v1', 'chain': 'w5/chain1', 'arm': arm,
                                    'launched_unix': t0, 'segments': segs}))
    return manifest


class RetentionTests(unittest.TestCase):
    def test_x_keeps_every_64th_boundary_and_the_live_segments(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain = make_chain(tmp, 'X', segments=4, per_segment=64, complete_last=False)
            doc = retention.plan(chain)
            pruned = {c['boundary'] for c in doc['checkpoints']}
            # Segments 0 and 1 have complete successors; 2 is followed by the live segment 3.
            self.assertEqual(pruned, set(range(1, 128)) - {64})
            self.assertTrue(all(b % 64 for b in pruned))
            self.assertEqual(retention.apply(doc, Path(tmp) / 'log.jsonl'), len(doc['checkpoints']))
            again = retention.plan(chain)
            self.assertEqual(again['checkpoints'], [])
            # The index is cached and grows only.
            _, records = retention.index(chain)
            self.assertEqual(len(records), 256)

    def test_c_keeps_recent_64_stride_and_a_4096_stride(self):
        self.assertTrue(retention.keeps_full_state('C', 4096, 100_000))
        self.assertFalse(retention.keeps_full_state('C', 4096 + 64, 100_000))
        self.assertTrue(retention.keeps_full_state('C', 99_968, 100_000))
        self.assertFalse(retention.keeps_full_state('C', 99_969, 100_000))
        self.assertFalse(retention.keeps_full_state('C', 100_000 - 4096 - 64 + 32, 100_000))
        self.assertTrue(retention.keeps_full_state('X', 64 * 9, 10**6))
        self.assertFalse(retention.keeps_full_state('X', 65, 10**6))

    def test_snapshot_window_is_not_pruned(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain = make_chain(tmp, 'X', segments=3, per_segment=64)
            doc = retention.plan(chain, snapshot_from=100)
            self.assertTrue(all(c['boundary'] < 100 for c in doc['checkpoints']))

    def test_trajectories_roll_over_64_complete_segments(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain = make_chain(tmp, 'C', segments=66, per_segment=1)
            doc = retention.plan(chain)
            self.assertEqual(sorted({t['segment'] for t in doc['trajectories']}), [0, 1])
            # An X chain's ex1 sidecars roll off with their trajectories, and the cached plan agrees.
            x = make_chain(Path(tmp) / 'x', 'X', segments=66, per_segment=1)
            doc = retention.plan(x)
            self.assertEqual(sorted(Path(t['path']).name for t in doc['trajectories']),
                             ['episode-0000.json', 'episode-0000.json', 'ex1-0000.json', 'ex1-0000.json'])
            self.assertEqual(retention.plan(x, None, retention.IndexCache(x)), doc)
            retention.apply(doc, Path(tmp) / 'log.jsonl')
            self.assertEqual(retention.plan(x)['trajectories'], [])

    def test_projection_and_decisions(self):
        with tempfile.TemporaryDirectory() as tmp:
            c = make_chain(Path(tmp) / 'c', 'C', segments=2, per_segment=64, ledger_step=1000, dt=60.0)
            x = make_chain(Path(tmp) / 'x', 'X', segments=2, per_segment=64, ledger_step=1200, dt=60.0)
            c_chain, c_rec = retention.index(c)
            x_chain, x_rec = retention.index(x)
            proj = retention.project(c_rec, c_chain['launched_unix'])
            # 1000 usec per 60 s, launched t0: R-hat = CPU + rate x (7 days - t).
            rate = 1000 / 60.0
            self.assertAlmostEqual(proj['rate_usec_per_s'], rate)
            self.assertAlmostEqual(proj['r_hat_usec'], 128_000 + rate * (7 * 86400 - 128 * 60))
            d = retention.decide(x_chain, x_rec, c_chain, c_rec, c_ended=False, snapshot_bytes=15 * 10**6)
            self.assertEqual(d['action'], 'run')
            # Late in the week: X at 1.12 R-hat pauses; at 0.9 it snapshots when the window fits 30 GB.
            now = c_chain['launched_unix'] + 7 * 86400
            d = retention.decide(x_chain, x_rec, c_chain, c_rec, c_ended=False, now=now)
            self.assertEqual(d['action'], 'pause')
            x_low = [dict(r, ledger_usec=int(r['ledger_usec'] * 0.9 * 128_000 / 153_600)) for r in x_rec]
            d = retention.decide(x_chain, x_low, c_chain, c_rec, c_ended=False, now=now,
                                 snapshot_bytes=15 * 10**6)
            self.assertEqual(d['action'], 'snapshot')
            d = retention.decide(x_chain, x_low, c_chain, c_rec, c_ended=False, now=now,
                                 snapshot_bytes=15 * 10**9)
            self.assertEqual(d['action'], 'run')
            self.assertFalse(d['snapshot_fits'])
            # C ended: R_i = its final ledger; X past it selects, X short resumes to it.
            d = retention.decide(x_chain, x_rec, c_chain, c_rec, c_ended=True)
            self.assertEqual((d['action'], d['endpoint_boundary']), ('select', 107))
            d = retention.decide(x_chain, x_rec[:50], c_chain, c_rec, c_ended=True)
            self.assertEqual((d['action'], d['stop_at_usec']), ('resume_to', 128_000))
            d = retention.decide(x_chain, x_rec[:50], c_chain, c_rec, c_ended=True, x_running_s=11 * 86400)
            self.assertEqual(d['action'], 'stop_under_budget')
            # A cap frozen at Stage 0 above 10.5 days is honoured.
            longer = dict(x_chain, x_cap_seconds=13 * 86400)
            d = retention.decide(longer, x_rec[:50], c_chain, c_rec, c_ended=True, x_running_s=11 * 86400)
            self.assertEqual(d['action'], 'resume_to')

    def test_selection_replay_and_prune_manifest(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain_path = make_chain(tmp, 'X', segments=3, per_segment=64)
            retention.apply(retention.plan(chain_path), Path(tmp) / 'log.jsonl')
            chain, records = retention.index(chain_path)
            spec = retention.select(chain, records, r_usec=100_500)
            self.assertEqual(spec['endpoint_boundary'], 101)
            self.assertEqual(spec['replay']['from']['boundary'], 64)
            self.assertEqual(spec['replay']['updates'], 37)
            self.assertEqual(spec['replay']['expected'][-1]['after_state_sha256'], 'h101')
            cfg = retention.replay_config(retention.load_chain(chain_path), spec, Path(tmp) / 'replay')
            self.assertEqual(len(cfg['iterations']), 37)
            self.assertEqual(cfg['iterations'][0]['episodes'][0]['tag'], [1, 0])
            self.assertEqual(cfg['ex1']['update_offset'], 64)
            self.assertNotIn('chain_ledger', cfg)
            self.assertEqual(cfg['initial_source']['checkpoint']['path'], records[63]['checkpoint']['path'])
            # A replay tree that reproduces every hash passes; one mismatch fails.
            root = Path(tmp) / 'replay'
            for i, item in enumerate(spec['replay']['expected']):
                it = root / 'iterations' / f'{i:06d}'
                it.mkdir(parents=True)
                ck = it / 'checkpoint.json'
                ck.write_text(f'replayed-{i}')
                (it / 'u.json').write_text(json.dumps({'after_state_sha256': item['after_state_sha256'],
                                                       'checkpoint': {'path': ck.as_posix(),
                                                                      'sha256': retention.sha(ck)}}))
                (it / 'complete.json').write_text(json.dumps({'update': {'path': (it / 'u.json').as_posix()}}))
            verified = retention.verify_replay(spec, root)
            self.assertTrue(verified['equal'])
            # The replayed endpoint is kept through its verified receipt.
            manifest = retention.prune_manifest(chain, records, 101, verified)
            self.assertEqual([k['boundary'] for k in manifest['keep']['selected_checkpoint']], [101])
            self.assertTrue(manifest['keep']['selected_checkpoint'][0]['replayed'])
            # Another state at the same boundary is refused.
            with self.assertRaises(SystemExit):
                retention.prune_manifest(chain, records, 101, dict(verified, endpoint_after_state_sha256='other'))
            (root / 'iterations' / '000005' / 'u.json').write_text(json.dumps({'after_state_sha256': 'bad'}))
            receipt = retention.verify_replay(spec, root)
            self.assertFalse(receipt['equal'])
            self.assertEqual(receipt['first_mismatch'], 70)
            # The kept 64 stride: its endpoint needs no replay, and the manifest prunes every other state.
            spec = retention.select(chain, records, r_usec=128_000)
            self.assertIsNone(spec['replay'])
            manifest = retention.prune_manifest(chain, records, 128)
            self.assertEqual([k['boundary'] for k in manifest['keep']['selected_checkpoint']], [128])
            self.assertNotIn(128, [p['boundary'] for p in manifest['prune']])
            self.assertIn(64, [p['boundary'] for p in manifest['prune']])
            with self.assertRaises(SystemExit):
                retention.prune_manifest(chain, records, 101)
            with self.assertRaises(SystemExit):
                retention.prune_manifest(chain, records, 101, receipt)

    def test_replay_refuses_history_relative_opponents(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain_path = make_chain(tmp, 'X', segments=2, per_segment=64)
            chain = retention.load_chain(chain_path)
            cfg = retention.read(chain['segments'][0]['config'])
            for item in cfg['iterations']:
                item['episodes'] = [{'opponent': {'kind': 'initial'}}]
            Path(chain['segments'][0]['config']).write_text(json.dumps(cfg))
            spec = {'endpoint_boundary': 40, 'replay': {'from': {'boundary': 20, 'checkpoint': {'path': 'x',
                                                                                               'sha256': 'y'}}}}
            with self.assertRaises(SystemExit):
                retention.replay_config(retention.load_chain(chain_path), spec, Path(tmp) / 'r')

    def fake_snapshots(self, wrong=None):
        """Stands in for `checkpoint_container_v1 weights-snapshot`: writes a small file and its receipt."""
        def run(args, capture_output, text):
            out = Path(args[args.index('--output') + 1])
            out.write_bytes(b'w' * 10)
            b = int(out.stem[1:])
            receipt = {'output': {'path': out.as_posix(), 'sha256': retention.sha(out)}, 'snapshot_bytes': 10,
                       'cut_from_state_sha256': 'other' if b == wrong else f'h{b}',
                       'parameters_sha256': f'p{b}'}
            return type('R', (), {'returncode': 0, 'stdout': json.dumps(receipt), 'stderr': ''})()
        return run

    def test_weights_snapshots_replace_full_states_in_the_window(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain_path = make_chain(tmp, 'X', segments=3, per_segment=64)
            real = retention.subprocess.run
            retention.subprocess.run = self.fake_snapshots()
            try:
                made = retention.snapshot(chain_path, 'binary', 100)
            finally:
                retention.subprocess.run = real
            boundaries = [m['boundary'] for m in made]
            self.assertEqual(boundaries, [b for b in range(100, 193) if b % 64])
            have = retention.snapshots(chain_path)
            doc = retention.plan(chain_path, snapshot_from=100)
            pruned = {c['boundary'] for c in doc['checkpoints']}
            # Inside the window a full state goes once its snapshot exists; the live tail keeps its full states.
            self.assertTrue(set(range(100, 128)) <= pruned)
            self.assertFalse(pruned & set(range(129, 193)))
            retention.apply(doc, Path(tmp) / 'log.jsonl')
            chain, records = retention.index(chain_path)
            spec = retention.select(chain, records, 100_500, have)
            self.assertIsNone(spec['replay'])
            self.assertEqual(spec['weights_snapshot']['parameters_sha256'], 'p101')
            manifest = retention.prune_manifest(chain, records, 101, weights=have)
            self.assertEqual([(k['boundary'], k['kind']) for k in manifest['keep']['selected_checkpoint']],
                             [(101, 'weights')])
            kinds = {p['kind'] for p in manifest['prune']}
            self.assertEqual(kinds, {'full', 'weights'})
            self.assertNotIn(101, [p['boundary'] for p in manifest['prune'] if p['kind'] == 'weights'])
            self.assertEqual(set(manifest['prune_bytes_by_kind']), {'full', 'weights'})
            self.assertEqual(manifest['prune_bytes_by_kind']['weights'], 10 * (len(have) - 1))
            # A damaged full state at the endpoint is not kept; its verified snapshot is.
            full150 = next(r for r in records if r['boundary'] == 150)['checkpoint']['path']
            Path(full150).write_text('damaged')
            manifest = retention.prune_manifest(chain, records, 150, weights=have)
            self.assertEqual([(k['boundary'], k['kind']) for k in manifest['keep']['selected_checkpoint']],
                             [(150, 'weights')])
            self.assertIn((150, 'full'), [(p['boundary'], p['kind']) for p in manifest['prune']])
            # A snapshot that no longer matches its index never authorizes a prune: a fresh chain, where
            # boundary 110 (segment 1, followed by a complete segment) is eligible until its snapshot changes.
            chain2 = make_chain(Path(tmp) / 'second', 'X', segments=3, per_segment=64)
            real = retention.subprocess.run
            retention.subprocess.run = self.fake_snapshots()
            try:
                retention.snapshot(chain2, 'binary', 100)
            finally:
                retention.subprocess.run = real
            self.assertIn(110, {c['boundary'] for c in retention.plan(chain2, snapshot_from=100)['checkpoints']})
            Path(retention.snapshots(chain2)[110]['path']).write_bytes(b'x' * 10)
            self.assertNotIn(110, {c['boundary'] for c in retention.plan(chain2, snapshot_from=100)['checkpoints']})
            # The guard reports both kinds and refuses a snapshot directory outside its roots.
            ok = retention.guard([tmp], 10**12, chains=[chain_path])
            self.assertTrue(ok['ok'])
            self.assertEqual(ok['by_chain'][0]['weights_snapshots'], len(have))
            self.assertGreater(ok['by_chain'][0]['full_state_bytes'], 0)
            outside = retention.guard([Path(tmp) / 'X-seg000'], 10**12, chains=[chain_path])
            self.assertFalse(outside['ok'])

    def test_the_guard_checks_destinations_before_files_exist(self):
        with tempfile.TemporaryDirectory() as tmp, tempfile.TemporaryDirectory() as elsewhere:
            chain_path = make_chain(tmp, 'X', segments=1, per_segment=2)
            doc = json.loads(Path(chain_path).read_text())
            doc['snapshot_dir'] = elsewhere
            Path(chain_path).write_text(json.dumps(doc))
            out = retention.guard([tmp], 10**12, chains=[chain_path])
            self.assertFalse(out['ok'])
            self.assertIn(Path(elsewhere).as_posix(), [Path(p).as_posix() for p in out['outside_roots']])

    def test_a_snapshot_cut_from_another_state_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain_path = make_chain(tmp, 'X', segments=2, per_segment=64)
            real = retention.subprocess.run
            retention.subprocess.run = self.fake_snapshots(wrong=70)
            try:
                with self.assertRaises(SystemExit):
                    retention.snapshot(chain_path, 'binary', 65)
            finally:
                retention.subprocess.run = real

    def test_s_states_are_pruned_kept_verified_and_counted_as_one(self):
        """An S full state is its record and its container (pinned by the update receipt): both are planned
        (container first), pruned, verified, kept and counted together."""
        with tempfile.TemporaryDirectory() as tmp:
            chain_path = make_chain(tmp, 'X', segments=3, per_segment=64, s_learner=True)
            chain, records = retention.index(chain_path)
            container = {r['boundary']: Path(r['state']['path']) for r in records}
            self.assertTrue(all(p.name == 's-train-state.bin' for p in container.values()))
            doc = retention.plan(chain_path)
            by_boundary = {}
            for item in doc['checkpoints']:
                by_boundary.setdefault(item['boundary'], []).append(item.get('part', 'checkpoint'))
            self.assertEqual(set(by_boundary), set(range(1, 128)) - {64})
            self.assertTrue(all(parts == ['s_state', 'checkpoint'] for parts in by_boundary.values()))
            self.assertEqual(retention.apply(doc, Path(tmp) / 'log.jsonl'), 2 * len(by_boundary))
            self.assertFalse(any(container[b].exists() for b in by_boundary))
            self.assertTrue(container[64].exists() and container[128].exists())
            self.assertEqual(retention.plan(chain_path)['checkpoints'], [])
            # Selection needs both files intact: a damaged container sends the kept boundary to replay.
            self.assertIsNone(retention.select(chain, records, r_usec=128_000)['replay'])
            original = container[128].read_bytes()
            container[128].write_bytes(b'damaged')
            spec = retention.select(chain, records, r_usec=128_000)
            self.assertEqual(spec['replay']['from']['boundary'], 64)
            container[128].write_bytes(original)
            # A missing container is not a replay start.
            moved = container[64].with_suffix('.away')
            container[64].rename(moved)
            spec = retention.select(chain, records, r_usec=100_500)
            self.assertEqual(spec['replay']['from']['boundary'], 0)
            moved.rename(container[64])
            # The PRUNE manifest keeps both files of the endpoint and prunes both files of every other state.
            manifest = retention.prune_manifest(chain, records, 64)
            kept = [(k['boundary'], k.get('part', 'checkpoint')) for k in manifest['keep']['selected_checkpoint']]
            self.assertEqual(kept, [(64, 's_state'), (64, 'checkpoint')])
            pruned = [(p['boundary'], p.get('part', 'checkpoint')) for p in manifest['prune']]
            self.assertIn((128, 's_state'), pruned)
            self.assertIn((128, 'checkpoint'), pruned)
            # A damaged endpoint container is not kept.
            original64 = container[64].read_bytes()
            container[64].write_bytes(b'damaged')
            with self.assertRaises(SystemExit):
                retention.prune_manifest(chain, records, 64)
            container[64].write_bytes(original64)
            # The guard counts both files of every state on disk: 64, 128 and the live segment's 129 to 192.
            out = retention.guard([tmp], 10**12, chains=[chain_path])
            self.assertEqual(out['by_chain'][0]['full_states'], 2 * 66)
            # A replayed S endpoint verifies only with its container intact, and is kept as both files.
            spec = retention.select(chain, records, r_usec=100_500)
            root = Path(tmp) / 'replay'
            for i, item in enumerate(spec['replay']['expected']):
                it = root / 'iterations' / f'{i:06d}'
                it.mkdir(parents=True)
                state = it / 's-train-state.bin'
                state.write_bytes(f'replayed-{i}'.encode())
                state_pin = {'path': state.as_posix(), 'sha256': retention.sha(state)}
                ck = it / 'checkpoint.json'
                ck.write_text(json.dumps({'schema': S_RECORD, 'state': state_pin}))
                (it / 'u.json').write_text(json.dumps({'after_state_sha256': item['after_state_sha256'],
                                                       'checkpoint': {'path': ck.as_posix(),
                                                                      'sha256': retention.sha(ck)},
                                                       's_learner': {'state': state_pin}}))
                (it / 'complete.json').write_text(json.dumps({'update': {'path': (it / 'u.json').as_posix()}}))
            verified = retention.verify_replay(spec, root)
            self.assertTrue(verified['equal'])
            manifest = retention.prune_manifest(chain, records, 101, verified)
            self.assertEqual([k.get('part', 'checkpoint') for k in manifest['keep']['selected_checkpoint']],
                             ['s_state', 'checkpoint'])
            # A receipt without the container's pin (written before containers were tracked) is refused.
            older = {k: v for k, v in verified.items() if k != 'endpoint_state'}
            with self.assertRaises(SystemExit):
                retention.prune_manifest(chain, records, 101, older)
            last = root / 'iterations' / f"{len(spec['replay']['expected']) - 1:06d}" / 's-train-state.bin'
            last.write_bytes(b'damaged')
            self.assertFalse(retention.verify_replay(spec, root)['equal'])

    def test_s_containers_survive_missing_damaged_or_shared_records_and_interrupted_applies(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain_path = make_chain(tmp, 'X', segments=3, per_segment=64, s_learner=True)
            chain, records = retention.index(chain_path)
            by_b = {r['boundary']: r for r in records}
            # A missing record and a record rewritten to point elsewhere: the container still comes from the
            # update receipt, and the record's new contents are never followed.
            Path(by_b[5]['checkpoint']['path']).unlink()
            decoy = Path(tmp) / 'decoy.bin'
            decoy.write_bytes(b'not a chain file')
            Path(by_b[6]['checkpoint']['path']).write_text(json.dumps({'schema': S_RECORD, 'state': {
                'path': decoy.as_posix(), 'sha256': retention.sha(decoy)}}))
            doc = retention.plan(chain_path)
            paths = [c['path'] for c in doc['checkpoints']]
            self.assertIn(Path(by_b[5]['state']['path']).as_posix(), paths)
            self.assertNotIn(Path(by_b[5]['checkpoint']['path']).as_posix(), paths)
            self.assertIn(Path(by_b[6]['state']['path']).as_posix(), paths)
            self.assertNotIn(decoy.as_posix(), paths)
            self.assertEqual(len(paths), len(set(paths)))
            # An interrupted apply (only the first planned file, a container) is completed by the next plan.
            first = dict(doc, checkpoints=doc['checkpoints'][:1], trajectories=[])
            self.assertEqual(first['checkpoints'][0].get('part'), 's_state')
            retention.apply(first, Path(tmp) / 'log.jsonl', hold=Path(tmp) / 'hold')
            again = retention.plan(chain_path)
            boundary = first['checkpoints'][0]['boundary']
            self.assertIn(Path(by_b[boundary]['checkpoint']['path']).as_posix(),
                          [c['path'] for c in again['checkpoints']])
            self.assertNotIn(first['checkpoints'][0]['path'], [c['path'] for c in again['checkpoints']])
            self.assertTrue(decoy.exists())
        with tempfile.TemporaryDirectory() as tmp:
            # A container shared with a surviving state is never planned or pruned.
            chain_path = make_chain(tmp, 'X', segments=3, per_segment=64, s_learner=True)
            chain, records = retention.index(chain_path)
            shared = records[63]['state']  # boundary 64, kept by the stride
            lines = []
            for rec in records:
                if rec['boundary'] == 10:
                    rec = dict(rec, state=shared)
                lines.append(json.dumps(rec))
            retention.index_path(chain_path).write_text('\n'.join(lines) + '\n')
            chain, records = retention.index(chain_path)
            doc = retention.plan(chain_path)
            self.assertNotIn(Path(shared['path']).as_posix(), [c['path'] for c in doc['checkpoints']])
            manifest = retention.prune_manifest(chain, records, 64)
            pruned = [p['path'] for p in manifest['prune']]
            self.assertNotIn(Path(shared['path']).as_posix(), pruned)
            self.assertEqual(len(pruned), len(set(pruned)))

    def test_old_index_caches_gain_the_container_pin(self):
        with tempfile.TemporaryDirectory() as tmp:
            chain_path = make_chain(tmp, 'X', segments=1, per_segment=4, s_learner=True)
            _, records = retention.index(chain_path)
            cache = retention.index_path(chain_path)
            stripped = [{k: v for k, v in r.items() if k != 'state'} for r in records]
            cache.write_text(''.join(json.dumps(r) + '\n' for r in stripped))
            _, again = retention.index(chain_path)
            self.assertEqual([r['state'] for r in again], [r['state'] for r in records])
            self.assertTrue(all('state' in json.loads(line) for line in cache.read_text().splitlines()))
            net8 = make_chain(Path(tmp) / 'n', 'X', segments=1, per_segment=2)
            _, plain = retention.index(net8)
            self.assertTrue(all(r['state'] is None for r in plain))

    def test_guard(self):
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, 'a').write_bytes(b'x' * 1000)
            self.assertTrue(retention.guard([tmp], cap_bytes=2000, next_bytes=500)['ok'])
            self.assertFalse(retention.guard([tmp], cap_bytes=2000, next_bytes=1500)['ok'])
            self.assertEqual(retention.main(['guard', tmp, '--cap-bytes', '10']), 1)



class Capture0bTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = Path(self.tmp.name)
        ck = root / 'b512.json'
        ck.write_text('b512')
        self.source = {'checkpoint': {'path': ck.as_posix(), 'sha256': retention.sha(ck)},
                       'play_import': {'path': 'i', 'sha256': 'i'}}
        self.parent = {'source': {'checkpoint': {'path': 'p', 'sha256': 'p'}}}
        self.base = {'opponents': [{'id': 'parent', 'source': self.parent['source']}],
                     'iterations': [{'episodes': [
                         {'episode': {'id': 'x', 'seed': 1, 'learner_seat': 0}, 'opponent': {'kind': 'current'}},
                         {'episode': {'id': 'y', 'seed': 2, 'learner_seat': 1},
                          'opponent': {'kind': 'fixed', 'id': 'parent'}}]}],
                     'max_non_natural_episode_fraction': 0.2}
        self.root = root

    def tearDown(self):
        self.tmp.cleanup()

    def test_jobs_resolve_opponents_and_key_the_namespace(self):
        made = capture0b.jobs(self.base, {'source': self.source}, self.root / 'out', updates=2)
        self.assertEqual(len(made), 2)
        command = capture0b.read(made[1])
        self.assertEqual(command['mode'], 'collect')
        self.assertEqual(command['ex1']['run'], 'w5/0b')
        self.assertEqual(command['ex1']['update'], 1)
        self.assertEqual(command['ex1']['w5']['capture_0b'],
                         {'budgets': [16000, 64000], 'search_seeds': [0, 1], 'rollouts': 128})
        self.assertEqual(command['episodes'][0]['opponent'], self.source)
        self.assertEqual(command['episodes'][1]['opponent'], self.parent['source'])
        self.assertTrue(command['episodes'][0]['id'].startswith('w5/0b-w5/0b-u1-j0'))
        self.assertNotIn('training_math', command)
        portable = dict(self.base, training_math='portable-math-v1')
        made = capture0b.jobs(portable, {'source': self.source}, self.root / 'portable', updates=1)
        self.assertEqual(capture0b.read(made[0])['training_math'], 'portable-math-v1')
        bad = dict(self.base, iterations=[{'episodes': [{'episode': {'id': 'z', 'seed': 3, 'learner_seat': 0},
                                                         'opponent': {'kind': 'completed_iteration', 'index': 0}}]}])
        with self.assertRaises(SystemExit):
            capture0b.jobs(bad, {'source': self.source}, self.root / 'bad', updates=1)

    def test_convert_writes_records_in_episode_order(self):
        made = capture0b.jobs(self.base, {'source': self.source}, self.root / 'out', updates=1)
        command = capture0b.read(made[0])
        out = Path(command['output_directory'])
        out.mkdir(parents=True)
        pins = []
        for slot, reached in enumerate((True, False)):
            capture = {'reached': reached}
            if reached:
                capture.update({'step': 7, 'policy': {'logit_bits': [1, 2]},
                                'candidates': [{'budget': 16000, 'search_seed': 0, 'seed_root': 'r', 'status': 'ok',
                                                'cpu_s': 1.5, 'n': [8, 8], 'wins': [1, 2], 'draws': [0, 0],
                                                'search': {'big': True}}],
                                'qref': {'rollouts': 128, 'seed_root': 'q', 'actions': [0, 1], 'wins': [1, 2],
                                         'draws': [0, 0], 'n': [128, 128]}, 'qref_nonnatural': [0, 0]})
            sidecar = out / f'ex1-{slot:04d}.json'
            sidecar.write_text(json.dumps({'slot': slot, 'attempts': [
                {'natural': True, 'episode_id': f'g{slot}', 'seed': 10 + slot,
                 'options': {'run': 'w5/0b', 'update': 0}, 'w5': {'placement_index': 5 + slot},
                 'capture_0b': capture}]}))
            pins.append({'path': sidecar.as_posix(), 'sha256': ''})
        (out / 'collection.json').write_text(json.dumps({'ex1': {'episodes': pins}}))
        records = capture0b.convert(self.root / 'out')
        self.assertEqual([r['episode'] for r in records], [0, 1])
        self.assertEqual(records[0]['schema'], 'w5-0b-capture/v1')
        self.assertEqual((records[0]['k'], records[0]['seed'], records[0]['game']), (5, 10, 'g0'))
        self.assertNotIn('search', records[0]['candidates'][0])
        self.assertEqual(records[0]['source_checkpoint_sha256'], self.source['checkpoint']['sha256'])
        self.assertFalse(records[1]['reached'])
        self.assertNotIn('candidates', records[1])

    def _sidecars(self, out_dir, value_leaf, extra_action=False, moved=False):
        command = capture0b.read(sorted((out_dir / 'jobs').glob('u*.json'))[0])
        out = Path(command['output_directory'])
        out.mkdir(parents=True)
        actions = [0, 1]
        capture = {'reached': True, 'step': 7, 'policy': {'logit_bits': [1, 2, 3]},
                   'candidates': [{'budget': b, 'search_seed': s, 'seed_root': f'r{b}{s}', 'status': 'ok',
                                   'cpu_s': 1.5 + s, 'n': [8, 8, 0], 'wins': [1, 2 + moved, 0], 'draws': [0, 0, 0],
                                   'search': {'big': True}} for b in (16000, 64000) for s in (0, 1)],
                   'qref': {'rollouts': 128, 'seed_root': 'q', 'actions': actions, 'wins': [1, 2, 3][:len(actions)],
                            'draws': [0] * len(actions), 'n': [128] * len(actions)}, 'qref_nonnatural': [0, 0]}
        if value_leaf:
            capture['qref_extra'] = {'rollouts': 128, 'seed_root': 'q2a', 'actions': [2] if extra_action else [],
                                     'wins': [3] if extra_action else [], 'draws': [0] if extra_action else [],
                                     'n': [128] if extra_action else [], 'nonnatural': [0] if extra_action else []}
            capture['value_leaf'] = [{'horizon': h, 'n_sims': n, 'search_seed': s, 'seed_root': f'v{h}{n}{s}',
                                      'status': 'ok', 'cpu_s': 0.25, 'primary_sha256': 'a' * 64,
                                      'n': [4, 0, 8], 'value_units': [1 << 19, 0, 8 << 20], 'draws': [0, 0, 0],
                                      'search': {}} for h in ('h0', 't0') for n in (128, 512) for s in (0, 1)]
        sidecar = out / 'ex1-0000.json'
        sidecar.write_text(json.dumps({'slot': 0, 'attempts': [
            {'natural': True, 'episode_id': 'g0', 'seed': 10, 'options': {'run': 'w5/0b', 'update': 0},
             'w5': {'placement_index': 5}, 'capture_0b': capture}]}))
        (out / 'collection.json').write_text(json.dumps({'ex1': {'episodes': [{'path': sidecar.as_posix()}]}}))

    def test_value_leaf_jobs_convert_and_parity(self):
        vl = {'horizons': ['h0', 't0'], 'sims': [128, 512], 'search_seeds': [0, 1]}
        made = capture0b.jobs(self.base, {'source': self.source}, self.root / 'vl', updates=1, value_leaf=vl)
        self.assertEqual(capture0b.read(made[0])['ex1']['w5']['capture_0b']['value_leaf'], vl)
        self._sidecars(self.root / 'vl', True, extra_action=True)
        records = capture0b.convert_vl(self.root / 'vl', 16000, 'dev-windows-haleyspc', 'b' * 64)
        self.assertEqual(len(records), 1)
        r = records[0]
        self.assertEqual((r['schema'], r['episode'], r['game'], r['key_memo_limit_bytes']),
                         ('w5-2a-vl-capture/v1', 0, 'g0', 512 << 20))
        self.assertEqual(len(r['configs']), 8)
        self.assertNotIn('search', r['configs'][0])
        self.assertEqual([c['search_seed'] for c in r['comparator']], [0, 1])
        self.assertTrue(all(c['budget'] == 16000 and c['key_memo'] == 'on' for c in r['comparator']))
        self.assertEqual((r['qref_extra']['actions'], r['qref_extra']['seed_root']), ([2], 'q2a'))
        self.assertNotIn('nonnatural', r['qref_extra'])
        # Parity: the same jobs without value-leaf searches give the same 0b record; a moved count is caught.
        capture0b.jobs(self.base, {'source': self.source}, self.root / 'plain', updates=1)
        self._sidecars(self.root / 'plain', False)
        self.assertTrue(capture0b.parity(self.root / 'plain', self.root / 'vl')['equal'])
        capture0b.jobs(self.base, {'source': self.source}, self.root / 'moved', updates=1)
        self._sidecars(self.root / 'moved', False, moved=True)
        self.assertEqual(capture0b.parity(self.root / 'moved', self.root / 'vl')['differ'], [0])
        # A run whose jobs are not the other's first jobs is not a parity pair.
        capture0b.jobs(self.base, {'source': self.source}, self.root / 'late', updates=1, first_update=1)
        self._sidecars(self.root / 'late', False)
        self.assertFalse(capture0b.parity(self.root / 'late', self.root / 'vl')['equal'])
        with self.assertRaises(SystemExit):  # 0b-only jobs have no value-leaf records
            capture0b.convert_vl(self.root / 'plain', 16000, 'x', 'b' * 64)


def zstd_frame(data):
    """One zstd frame of `data` with its content size, or None when no zstd is available here."""
    try:
        from compression import zstd
        return zstd.compress(data)
    except ImportError:
        pass
    try:
        import zstandard
        return zstandard.ZstdCompressor().compress(data)
    except ImportError:
        pass
    if shutil.which('zstd'):
        with tempfile.TemporaryDirectory() as tmp:  # a file input, so the frame declares its content size
            source = Path(tmp) / 'record'
            source.write_bytes(data)
            return subprocess.run(['zstd', '-q', '-c', str(source)], stdout=subprocess.PIPE, check=True).stdout
    return None


class RecordEnvelopeTests(unittest.TestCase):
    """qual.open_record fails closed exactly where the trainer's reader (record_envelope_v1::open_v1) does."""

    RECORD = json.dumps({'decisions': [{'step': 1, 'tensor': [0, 1065353216] * 50}]}).encode()

    def frame(self, data=None):
        frame = zstd_frame(self.RECORD if data is None else data)
        if frame is None:
            self.skipTest('no zstd implementation on this host')
        return frame

    def check_backend(self):
        record, frame = self.RECORD, self.frame()
        self.assertTrue(frame.startswith(qual.ZSTD_MAGIC))
        self.assertEqual(qual.open_record(frame), record)
        self.assertEqual(qual.open_record(record), record)  # a plain record is returned as it is
        with tempfile.TemporaryDirectory() as tmp:
            sealed, plain = Path(tmp) / 'sealed.json', Path(tmp) / 'plain.json'
            sealed.write_bytes(frame)
            plain.write_bytes(record)
            self.assertEqual(qual.read(sealed), qual.read(plain))
        refused = {
            'two frames': frame + self.frame(b''),
            'trailing byte': frame + b'\0',
            'truncated': frame[:-1],
            'nested': self.frame(frame),
        }
        for name, data in refused.items():
            with self.subTest(name), self.assertRaises((ValueError, OSError)):
                qual.open_record(data)
        with self.assertRaisesRegex(ValueError, 'size limit'):
            qual.open_record(frame, cap=len(record) - 1)
        self.assertEqual(qual.open_record(frame, cap=len(record)), record)

    def test_default_backend(self):
        self.check_backend()

    def test_command_line_backend(self):
        if not shutil.which('zstd'):
            self.skipTest('no zstd command line tool on this host')
        blocked = {'compression': None, 'compression.zstd': None, 'zstandard': None}
        with mock.patch.dict(sys.modules, blocked):
            self.check_backend()

    def test_a_frame_without_a_declared_size_is_refused(self):
        frame = bytearray(self.frame())
        frame[4] &= 0x3f  # frame content size flag 0
        frame[4] &= ~0x20 & 0xff  # not single segment, so no size field at all
        with self.assertRaisesRegex(ValueError, 'declares its content size'):
            qual.zstd_frame_content_size(bytes(frame))


if __name__ == '__main__':
    unittest.main()
