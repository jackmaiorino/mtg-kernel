"""W5 tools: the w5-xact-v1 mirror and the retention, budget and selection rules, on synthetic chain trees."""
import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import capture0b  # noqa: E402
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


def make_chain(tmp, arm, segments, per_segment, ledger_step=1000, t0=1.0e9, dt=60.0, complete_last=True):
    """Synthetic segments: every update's complete.json, update.json and checkpoint file."""
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
            ck = att / 'update' / 'checkpoint.json'
            ck.write_text(f'state-{arm}-{update}')
            doc = {'after_state_sha256': f'h{update + 1}', 'checkpoint': {'path': ck.as_posix(),
                                                                          'sha256': retention.sha(ck)}}
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
            # The guard reports both kinds and refuses a snapshot directory outside its roots.
            ok = retention.guard([tmp], 10**12, chains=[chain_path])
            self.assertTrue(ok['ok'])
            self.assertEqual(ok['by_chain'][0]['weights_snapshots'], len(have))
            self.assertGreater(ok['by_chain'][0]['full_state_bytes'], 0)
            outside = retention.guard([Path(tmp) / 'X-seg000'], 10**12, chains=[chain_path])
            self.assertFalse(outside['ok'])

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

if __name__ == '__main__':
    unittest.main()
