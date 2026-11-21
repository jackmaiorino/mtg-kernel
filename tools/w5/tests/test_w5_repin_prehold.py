"""W5 trainer re-pin with gate (a) before the hold (collab LANES/speed-lead/repin-downtime/DESIGN-DELTA.md, change 1).

The pre-hold receipt (`w5-repin-proof-a/v1`) proves gate (a) on a recent completed segment while the chain still
trains; the in-hold receipt (`w5-repin-proof-c/v1`) proves gate (c) only and cites it; the request names both
(`proof` and `proof_a`). `chains.py repin begin` and the commit plan accept the pair only when the pre-hold receipt
is fresh, for this chain, this segment and this trainer pair, and the chain has not changed since. A request
without `proof_a` takes today's path unchanged. Stand-in trainer under two hashes, as test_w5_repin.py.

These tests fail before the change (no `proof_a` path) and pass once it exists, except the last class, which pins
today's behaviour of the path without `proof_a` and passes before and after."""
import contextlib
import gzip
import hashlib
import io
import json
import os
import sys
import time
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import retention  # noqa: E402
import w5box  # noqa: E402
from test_w5_chains import FAKE, jwrite  # noqa: E402
import test_w5_repin as _repin  # noqa: E402  (a module import: its tests are not collected again here)
from test_w5_repin import sha  # noqa: E402

PREHOLD_SCHEMA = 'w5-repin-proof-a/v1'
PROOF_C_SCHEMA = 'w5-repin-proof-c/v1'
MAX_SEGMENTS = 8
MAX_SECONDS = 3600


def history_sha256(man):
    return hashlib.sha256(json.dumps(man.get('trainer_history') or [], sort_keys=True).encode()).hexdigest()


def config_content_sha256(path):
    path = Path(path)
    if not path.exists():
        path = path.with_name(path.name + '.gz')
    raw = path.read_bytes()
    return hashlib.sha256(gzip.decompress(raw) if path.name.endswith('.gz') else raw).hexdigest()


class _Base(unittest.TestCase):
    """RepinTests' fixture and helpers (a carried chain c1c from boundary 8, 4-update segments), without its tests."""
    setUp = _repin.RepinTests.setUp
    tearDown = _repin.RepinTests.tearDown
    build = _repin.RepinTests.build
    inputs = _repin.RepinTests.inputs
    drive = _repin.RepinTests.drive
    start = _repin.RepinTests.start
    cdir = _repin.RepinTests.cdir
    man = _repin.RepinTests.man
    stop = _repin.RepinTests.stop
    request = _repin.RepinTests.request
    repin = _repin.RepinTests.repin
    whole = _repin.RepinTests.whole
    switch = _repin.RepinTests.switch

    # ------------------------------------------------------------ the two receipts and the request

    def prehold(self, old, new, k=None, name='a.json', **over):
        """The pre-hold receipt for segment k (default: the second newest, as while the chain still trains)."""
        man = self.man()
        segs = man['segments']
        k = len(segs) - 2 if k is None else k
        seg = segs[k]
        lo, length = seg['offset'], seg['updates']
        _, records = retention.index(self.cdir() / 'chain.json')  # chain.index.jsonl, as the supervise loop keeps it
        rows = {r['boundary']: r['after_state_sha256'] for r in records}
        source = chains.next_source(segs[k - 1]['root'])['checkpoint']
        doc = {'schema': PREHOLD_SCHEMA, 'chain': 'c1c',
               'manifest_path': (self.cdir() / 'chain.json').resolve().as_posix(),
               'arm_file': man['arm_file'], 'trainer_history_sha256': history_sha256(man),
               'segment': k, 'segment_entry': seg, 'config_sha256': config_content_sha256(seg['config']),
               'source_checkpoint': source, 'source_parts': [source],  # the stand-in state is one file
               'boundary': lo + length, 'old_sha256': sha(old), 'new_sha256': sha(new),
               'old_path': str(old), 'new_path': str(new), 'tools_chains_sha256': sha(chains.__file__),
               'updates': length, 'equal': length, 'rc': 0, 'first_before_equal': True,
               'rows': [{'boundary': b, 'index': rows[b], 'new': rows[b]} for b in range(lo + 1, lo + length + 1)],
               'first_before': rows[lo],  # the replay's first before-state: the index row at the segment start
               'rehearsal': False, 'finished_unix': time.time(), 'pass': True}
        doc.update(over)
        return jwrite(self.t / name, doc)

    def prehold_request(self, plan, old, new, a, rid='p1', name=None, c_over=None, started=None, c_rows=None,
                        **over):
        """The in-hold receipt (gate (c) only, citing the pre-hold receipt `a`) and the request naming both."""
        man = self.man()
        segs = man['segments']
        arm = json.loads(Path(plan['arms']['C']['file']['path']).read_text())
        config_sha, source = chains.repin_next_config_sha256(arm, man)
        mp = (self.cdir() / 'chain.json').resolve()
        boundary = segs[-1]['offset'] + segs[-1]['updates']
        a_doc = json.loads(Path(a).read_text())
        proof = {'schema': PROOF_C_SCHEMA, 'chain': 'c1c', 'manifest': {'path': mp.as_posix(), 'sha256': sha(mp)},
                 'boundary': boundary, 'last_segment': len(segs) - 1, 'next_segment': len(segs),
                 'old_sha256': sha(old), 'new_sha256': sha(new), 'tools_chains_sha256': sha(chains.__file__),
                 'started_unix': a_doc['finished_unix'] + 60 if started is None else started,
                 'gate_a_receipt': chains.pin(a),
                 'gate_c': {'segment': len(segs), 'updates': 4, 'equal': 4, 'config_sha256': config_sha,
                            'source_checkpoint': source['checkpoint']},
                 'rehearsal': False, 'pass': True}
        if c_rows is not None:  # gate (c)'s rows E+1..E+L: what `repin confirm` compares the first new segment with
            proof['gate_c']['rows'] = [{'boundary': boundary + 1 + i, 'old': r, 'new': r} for i, r in enumerate(c_rows)]
        proof.update(c_over or {})
        name = name or f'req-{rid}.json'
        proof_path = jwrite(self.t / f'proof-{name}', proof)
        doc = {'schema': chains.REPIN_SCHEMA, 'chain': 'c1c', 'id': rid, 'from_sha256': sha(old),
               'to_sha256': sha(new), 'boundary': boundary, 'segment': len(segs), 'proof': chains.pin(proof_path),
               'proof_a': chains.pin(a)}
        doc.update(over)
        return jwrite(self.t / name, doc)

    def repin_x(self, plan_path, *args):
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            try:
                code = chains.main(['repin', *args, '--plan', str(plan_path), '--chain', 'c1c'])
            except SystemExit as exc:  # before the change: argparse knows no `confirm` step or `--hold-first`
                code = exc.code
        return code, err.getvalue()

    def begin_refused(self, path, req, why):
        code, err = self.repin(path, 'begin', req)
        self.assertEqual(code, w5box.EXIT_REFUSED, err)
        self.assertIn(why, err)
        self.assertFalse((self.cdir() / chains.REPIN_PENDING).exists())


class PreholdAcceptedTests(_Base):
    def test_a_fresh_bound_prehold_proof_switches_and_the_chain_reaches_the_uninterrupted_states(self):
        path, plan = self.start(3)  # c1c at 20: segments 0..2 on the first trainer
        ref = self.whole(plan, 32)
        new = self.build()
        a = self.prehold(FAKE, new, k=2)  # gate (a) on the newest completed segment, the chain still running
        self.assertEqual(self.drive(plan, 4), chains.EXIT_ROOM)  # one more segment before the hold
        self.stop()
        req = self.prehold_request(plan, FAKE, new, a)
        never = AssertionError('retention.index() takes the index lock and can write: never in these checks')
        with mock.patch.object(retention, 'index', side_effect=never):
            code, err = self.repin(path, 'begin', req)
            self.assertEqual(code, 0, err)
            chains.make_plan(self.inputs(trainer=new, repin=req))  # the commit
        path, plan = chains.make_plan(self.inputs(trainer=new, repin=req, throughput=True))
        self.assertEqual(self.repin(path, 'end')[0], 0)
        entry = self.man()['trainer_history'][-1]
        self.assertEqual((entry['id'], entry['segment'], entry['boundary']), ('p1', 4, 24))
        self.assertEqual(entry['proof_a'], json.loads(req.read_text())['proof_a'])
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 6), chains.EXIT_ROOM)
        _, records = retention.index(self.cdir() / 'chain.json')
        self.assertEqual([r['after_state_sha256'] for r in records], ref[8:32])
        self.assertEqual([chains.trainer_of_segment(self.man(), r['segment']) for r in records],
                         [sha(FAKE)] * 16 + [sha(new)] * 8)

    def test_the_bounds_are_inclusive_and_a_compressed_config_still_matches(self):
        path, plan = self.start(2)
        new = self.build()
        a = self.prehold(FAKE, new, k=1)
        self.assertEqual(self.drive(plan, 1 + 1 + MAX_SEGMENTS), chains.EXIT_ROOM)  # exactly 8 segments after k
        self.stop()
        cfg = Path(self.man()['segments'][1]['config'])  # the driver gzips a finished scratch config: same content
        if cfg.exists():
            gz = cfg.with_name(cfg.name + '.gz')
            gz.write_bytes(gzip.compress(cfg.read_bytes()))
            cfg.unlink()
        started = json.loads(a.read_text())['finished_unix'] + MAX_SECONDS
        req = self.prehold_request(plan, FAKE, new, a, started=started)
        code, err = self.repin(path, 'begin', req)
        self.assertEqual(code, 0, err)


class PreholdRefusedTests(_Base):
    def setUp(self):
        super().setUp()
        self.path, self.plan = self.start(3)
        self.new = self.build()
        self.a = self.prehold(FAKE, self.new, k=2)
        self.assertEqual(self.drive(self.plan, 4), chains.EXIT_ROOM)
        self.stop()

    def test_stale_by_segments(self):
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(self.plan, 4 + MAX_SEGMENTS), chains.EXIT_ROOM)  # 9 segments after k = 2
        self.stop()
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, self.a),
                           'pre-hold proof is stale: 9 completed segments after the proven one, over the bound of 8')

    def test_stale_by_seconds(self):
        started = json.loads(self.a.read_text())['finished_unix'] + MAX_SECONDS + 1
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, self.a, started=started),
                           'pre-hold proof is stale: 3601 s from its end to the in-hold proof, over the bound of 3600 s')

    def test_the_first_before_state_must_be_the_row_at_the_segment_start(self):
        a = self.prehold(FAKE, self.new, k=2, name='a-before.json', first_before='0' * 64)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a),
                           'pre-hold proof rows differ from the index')
        doc = json.loads(self.a.read_text())
        doc.pop('first_before')
        a = jwrite(self.t / 'a-nobefore.json', doc)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid='p2'),
                           'pre-hold proof rows differ from the index')

    def test_another_chain(self):
        a = self.prehold(FAKE, self.new, k=2, name='a-c9.json', chain='c9')
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a),
                           'pre-hold proof is for another chain')
        a = self.prehold(FAKE, self.new, k=2, name='a-elsewhere.json', manifest_path='/elsewhere/chain.json')
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid='p2'),
                           'pre-hold proof is for another chain')

    def test_another_segment(self):
        seg = dict(self.man()['segments'][2], offset=self.man()['segments'][2]['offset'] + 4)
        a = self.prehold(FAKE, self.new, k=2, name='a-seg.json', segment_entry=seg)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a),
                           'pre-hold proof replayed another segment')
        a = self.prehold(FAKE, self.new, k=2, name='a-k.json', segment=99)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid='p2'),
                           'pre-hold proof replayed another segment')

    def test_hashes_differ(self):
        other = self.build('trainer-c.py')
        a = self.prehold(FAKE, other, k=2, name='a-other.json')  # proven for another new build
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a,
                                                           c_over={'new_sha256': sha(self.new)}),
                           'pre-hold proof is for another trainer pair')
        a = self.prehold(other, self.new, k=2, name='a-old.json')  # proven from another old build
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid='p2'),
                           'pre-hold proof is for another trainer pair')

    def test_the_in_hold_proof_cites_another_prehold_proof(self):
        other = self.prehold(FAKE, self.new, k=2, name='a-two.json', finished_unix=time.time() - 1)
        req = self.prehold_request(self.plan, FAKE, self.new, self.a, c_over={'gate_a_receipt': chains.pin(other)})
        self.begin_refused(self.path, req, 'the in-hold proof cites another pre-hold proof')

    def test_not_passed_or_a_rehearsal(self):
        for i, over in enumerate(({'pass': False}, {'rehearsal': True}, {'schema': 'w5-repin-proof/v1'})):
            a = self.prehold(FAKE, self.new, k=2, name=f'a-bad{i}.json', **over)
            self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid=f'b{i}'),
                               'pre-hold proof did not pass')

    def test_gate_a_rows_must_be_whole_and_the_chains_own(self):
        rows = json.loads(self.a.read_text())['rows']
        a = self.prehold(FAKE, self.new, k=2, name='a-short.json', equal=3)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a),
                           'pre-hold proof gate (a) is not 4 of 4')
        bad = [dict(r) for r in rows]
        bad[1] = dict(bad[1], index='0' * 64, new='0' * 64)  # consistent with itself, not with the chain's index
        a = self.prehold(FAKE, self.new, k=2, name='a-rows.json', rows=bad)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid='p2'),
                           'pre-hold proof rows differ from the index')

    def test_the_evidence_must_still_be_retained(self):
        src = Path(json.loads(self.a.read_text())['source_checkpoint']['path'])
        src.rename(src.with_name(src.name + '.pruned'))
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, self.a),
                           "the pre-hold proof's source or config is no longer retained")

    def test_times_out_of_order(self):
        started = json.loads(self.a.read_text())['finished_unix'] - 1
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, self.a, started=started),
                           'pre-hold proof times are out of order')

    def test_gate_a_first_before_and_row_boundaries(self):
        a = self.prehold(FAKE, self.new, k=2, name='a-fb.json', first_before_equal=False)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a),
                           'pre-hold proof gate (a) is not 4 of 4')
        rows = [dict(r, boundary=r['boundary'] + 1) for r in json.loads(self.a.read_text())['rows']]
        a = self.prehold(FAKE, self.new, k=2, name='a-shift.json', rows=rows)
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid='p2'),
                           'pre-hold proof gate (a) is not 4 of 4')

    def test_the_source_must_be_the_selected_predecessors(self):
        older = chains.next_source(self.man()['segments'][0]['root'])['checkpoint']  # a real, pinned, other state
        a = self.prehold(FAKE, self.new, k=2, name='a-src.json', source_checkpoint=older, source_parts=[older])
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a),
                           "the pre-hold proof's source or config is no longer retained")

    def test_a_missing_config(self):
        cfg = Path(self.man()['segments'][2]['config'])
        for p in (cfg, cfg.with_name(cfg.name + '.gz')):
            if p.exists():
                p.rename(p.with_name(p.name + '.gone'))
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, self.a),
                           "the pre-hold proof's source or config is no longer retained")

    def test_the_arm_file_bytes_changed(self):
        arm = Path(self.man()['arm_file']['path'])
        arm.write_bytes(arm.read_bytes() + b' ')
        self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, self.a),
                           'the chain changed since the pre-hold proof')

    def test_evidence_lost_between_begin_and_the_commit(self):
        req = self.prehold_request(self.plan, FAKE, self.new, self.a)
        code, err = self.repin(self.path, 'begin', req)
        self.assertEqual(code, 0, err)
        src = Path(json.loads(self.a.read_text())['source_checkpoint']['path'])
        src.rename(src.with_name(src.name + '.pruned'))
        with self.assertRaisesRegex(chains.Stop, "the pre-hold proof's source or config is no longer retained"):
            chains.make_plan(self.inputs(trainer=self.new, repin=req))
        self.assertNotIn('trainer_history', self.man())

    def extra_part(self):
        """An S-style second state part of the predecessor (segment 1), pinned through `scratch_kept_pins`, the
        function that names a full state's files from its authenticated receipts."""
        pred = Path(self.man()['segments'][1]['root']).resolve()
        extra = pred / 's-train-state.bin'
        extra.write_bytes(b'stand-in S container')
        digest = sha(extra)
        real = chains.scratch_kept_pins

        def kept(newest, path_at=Path):
            out = real(newest, path_at)
            if pred in Path(newest).resolve().parents:
                out[extra] = digest
            return out
        return extra, {'path': extra.as_posix(), 'sha256': digest}, mock.patch.object(chains, 'scratch_kept_pins', kept)

    def test_every_source_state_part_is_named_and_retained(self):
        extra, part, patch = self.extra_part()
        doc = json.loads(self.a.read_text())
        with patch:
            self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, self.a),
                               'pre-hold proof does not name every source state part')  # omitted
            a = self.prehold(FAKE, self.new, k=2, name='a-empty.json', source_parts=[])
            self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, a, rid='p2'),
                               'pre-hold proof does not name every source state part')
            full = self.prehold(FAKE, self.new, k=2, name='a-full.json', source_parts=doc['source_parts'] + [part])
            extra.write_bytes(b'corrupted')
            self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, full, rid='p3'),
                               "the pre-hold proof's source or config is no longer retained")
            extra.unlink()
            self.begin_refused(self.path, self.prehold_request(self.plan, FAKE, self.new, full, rid='p4'),
                               "the pre-hold proof's source or config is no longer retained")

    def test_a_source_state_part_lost_after_begin(self):
        extra, part, patch = self.extra_part()
        doc = json.loads(self.a.read_text())
        with patch:
            full = self.prehold(FAKE, self.new, k=2, name='a-full.json', source_parts=doc['source_parts'] + [part])
            req = self.prehold_request(self.plan, FAKE, self.new, full)
            code, err = self.repin(self.path, 'begin', req)
            self.assertEqual(code, 0, err)
            extra.unlink()
            with self.assertRaisesRegex(chains.Stop, "the pre-hold proof's source or config is no longer retained"):
                chains.make_plan(self.inputs(trainer=self.new, repin=req))
        self.assertNotIn('trainer_history', self.man())

    def test_the_two_proofs_ran_different_tools(self):
        req = self.prehold_request(self.plan, FAKE, self.new, self.a, c_over={'tools_chains_sha256': '0' * 64})
        self.begin_refused(self.path, req, 'the two proofs ran different tools')

    def test_back_and_forth_needs_a_new_prehold_proof(self):
        # A to B with today's combined proof, one segment on B, B back to A, one segment on A; the arm file is the
        # same bytes again (same tools and trainer), but the trainer history moved on
        arm_before = self.man()['arm_file']
        path, plan = self.switch(self.plan, self.path, FAKE, self.new, rid='r1')
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 5), chains.EXIT_ROOM)
        self.stop()
        path, plan = self.switch(plan, path, self.new, FAKE, rid='r2')
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 6), chains.EXIT_ROOM)
        self.stop()
        self.assertEqual(self.man()['arm_file'], arm_before)
        self.begin_refused(path, self.prehold_request(plan, FAKE, self.new, self.a, rid='r3'),
                           'the chain changed since the pre-hold proof')

    def test_a_revert_entry_also_changes_the_chain(self):
        req = self.request(self.plan, FAKE, self.new, rid='r1')  # today's combined proof
        self.assertEqual(self.repin(self.path, 'begin', req)[0], 0)
        chains.make_plan(self.inputs(trainer=self.new, repin=req))
        self.assertEqual(self.repin(self.path, 'abort')[0], 0)  # committed, not run: a revert entry
        path, plan = chains.make_plan(self.inputs(throughput=True))
        self.begin_refused(path, self.prehold_request(plan, FAKE, self.new, self.a, rid='p9'),
                           'the chain changed since the pre-hold proof')


class FirstSegmentHoldTests(_Base):
    """`repin end --hold-first` (a citing chain, DESIGN-DELTA 2.5): the driver commits exactly one segment on the new
    trainer and then holds until `repin confirm` has compared it with gate (c)'s rows."""

    def setUp(self):
        super().setUp()
        self.path, self.plan = self.start(3)  # c1c at 20
        self.ref = self.whole(self.plan, 32)
        self.new = self.build()
        self.a = self.prehold(FAKE, self.new, k=2)
        self.stop()

    def switch_holding(self, rows):
        req = self.prehold_request(self.plan, FAKE, self.new, self.a, c_rows=rows)
        code, err = self.repin(self.path, 'begin', req)
        self.assertEqual(code, 0, err)
        chains.make_plan(self.inputs(trainer=self.new, repin=req))
        path, plan = chains.make_plan(self.inputs(trainer=self.new, repin=req, throughput=True))
        code, err = self.repin_x(path, 'end', '--hold-first')
        self.assertEqual(code, 0, err)
        self.assertTrue(self.man()['trainer_history'][-1]['confirm_required'])
        (self.cdir() / 'STOP').unlink()
        return path, plan

    def test_exactly_one_segment_then_held_until_confirmed(self):
        path, plan = self.switch_holding(self.ref[20:24])
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)  # room for 10: the driver stops after one
        self.assertEqual(len(self.man()['segments']), 4)
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)  # a second start runs nothing
        self.assertEqual(len(self.man()['segments']), 4)
        retention.index(self.cdir() / 'chain.json')  # the supervise loop's index of segment 3
        code, err = self.repin_x(path, 'confirm')
        self.assertEqual(code, 0, err)
        self.assertIsNotNone(self.man()['trainer_history'][-1].get('confirmed'))
        self.assertEqual(self.repin_x(path, 'confirm')[0], 0)  # a rerun reports, rewrites nothing
        self.assertEqual(self.drive(plan, 6), chains.EXIT_ROOM)
        _, records = retention.index(self.cdir() / 'chain.json')
        self.assertEqual([r['after_state_sha256'] for r in records], self.ref[8:32])

    def test_a_mismatch_keeps_the_hold(self):
        path, plan = self.switch_holding(['0' * 64] * 4)
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        retention.index(self.cdir() / 'chain.json')
        code, err = self.repin_x(path, 'confirm')
        self.assertEqual(code, w5box.EXIT_REFUSED, err)
        self.assertIn('the first segment on the new trainer differs from the proven rows', err)
        self.assertIsNone(self.man()['trainer_history'][-1].get('confirmed'))
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        self.assertEqual(len(self.man()['segments']), 4)  # exactly one unproven segment

    def test_the_hold_needs_a_persisted_committed_segment(self):
        """Round 3: a first segment finished in its scratch mount, or without its commit line, is not yet held, so
        `run` still starts the driver that settles and commits it; the driver then holds."""
        path, plan = self.switch_holding(self.ref[20:24])
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        man = self.man()
        self.assertIn('awaits its confirmation', chains.repin_first_hold(man, self.cdir()))
        seg_root = Path(man['segments'][3]['root'])
        with mock.patch.object(chains, 'mounted', lambda p: Path(p) == seg_root):  # not yet persisted
            self.assertIsNone(chains.repin_first_hold(man, self.cdir()))
        log = chains.commit_log(self.cdir())
        lines = log.read_text().splitlines()
        kept = [x for x in lines if not (json.loads(x).get('event') == 'commit' and json.loads(x).get('segment') == 3)]
        self.assertLess(len(kept), len(lines))
        log.write_text(''.join(x + '\n' for x in kept))  # a driver that died between the promotion and its line
        self.assertIsNone(chains.repin_first_hold(man, self.cdir()))
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)  # commit_recovered, then the hold; no new segment
        self.assertEqual(len(self.man()['segments']), 4)
        self.assertIn('awaits its confirmation', chains.repin_first_hold(self.man(), self.cdir()))

    def held_after_one(self):
        path, plan = self.switch_holding(['0' * 64] * 4)
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        self.stop()
        return path, plan

    def test_a_re_pin_back_while_unconfirmed_holds_after_its_own_first_segment(self):
        """The way back after a mismatch is a re-pin to the old trainer with its own proof, allowed while the first
        segment is unconfirmed; with --hold-first it too commits exactly one segment, then holds."""
        path, plan = self.held_after_one()
        req = self.request(plan, self.new, FAKE, rid='back')  # today's combined proof, from the new build back
        code, err = self.repin(path, 'begin', req)
        self.assertEqual(code, 0, err)
        chains.make_plan(self.inputs(trainer=FAKE, repin=req))
        path, plan = chains.make_plan(self.inputs(trainer=FAKE, repin=req, throughput=True))
        code, err = self.repin_x(path, 'end', '--hold-first')
        self.assertEqual(code, 0, err)
        self.assertIsNone(chains.repin_first_hold(self.man(), self.cdir()))  # its own first segment has not run
        (self.cdir() / 'STOP').unlink()
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        self.assertEqual(len(self.man()['segments']), 5)  # exactly one segment back on the old trainer
        self.assertIn('awaits its confirmation', chains.repin_first_hold(self.man(), self.cdir()))

    def test_a_re_pin_forward_while_unconfirmed_is_refused(self):
        path, plan = self.held_after_one()
        third = self.build('trainer-c.py')
        code, err = self.repin(path, 'begin', self.request(plan, self.new, third, rid='fwd'))
        self.assertEqual(code, w5box.EXIT_REFUSED, err)
        self.assertIn('is unconfirmed: only a re-pin back to', err)

    def test_aborting_the_way_back_restores_the_owed_confirmation(self):
        """Round 1 (code): a revert entry must not release the hold the re-pin it restores still owes."""
        path, plan = self.held_after_one()
        req = self.request(plan, self.new, FAKE, rid='back')
        self.assertEqual(self.repin(path, 'begin', req)[0], 0)
        chains.make_plan(self.inputs(trainer=FAKE, repin=req))  # committed
        self.assertEqual(self.repin(path, 'abort')[0], 0)  # not run: a revert entry
        self.assertEqual(self.man()['trainer_history'][-1].get('reverts'), 'back')
        path, plan = chains.make_plan(self.inputs(trainer=self.new, throughput=True))  # the replaced configuration
        (self.cdir() / 'STOP').unlink()
        self.assertIn('awaits its confirmation', chains.repin_first_hold(self.man(), self.cdir()))
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        self.assertEqual(len(self.man()['segments']), 4)  # still exactly one unconfirmed segment

    def test_retention_keeps_the_state_the_unconfirmed_segment_started_from(self):
        """Round 2 (code): while the confirmation is owed, supervise retention keeps the state at the switch boundary
        (the re-pin back proves gate (a) on the first new segment from it); once confirmed it is pruned as usual."""
        path, plan = self.switch_holding(self.ref[20:24])
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        _, records = retention.index(self.cdir() / 'chain.json')
        at_e = [p['path'] for r in records if r['boundary'] == 20 for p in retention.record_parts(r)]
        self.assertTrue(at_e and all(Path(p).exists() for p in at_e))
        with contextlib.redirect_stdout(io.StringIO()):
            chains.cycle(plan, path)
        self.assertTrue(all(Path(p).exists() for p in at_e), 'kept while the confirmation is owed')
        self.assertEqual(self.repin_x(path, 'confirm')[0], 0)
        with contextlib.redirect_stdout(io.StringIO()):
            chains.cycle(plan, path)
        self.assertFalse(any(Path(p).exists() for p in at_e), 'pruned as usual once confirmed (20 is no stride)')

    def test_run_refuses_while_held(self):
        path, plan = self.switch_holding(self.ref[20:24])
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)
        jwrite(self.cont / 'plan' / 'main' / 'LAUNCHED.json', {'opened': [0]})
        err = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(err):
            self.assertEqual(chains.main(['run', '--plan', str(path), '--chain', 'c1c']), w5box.EXIT_REFUSED)
        self.assertIn('awaits its confirmation', err.getvalue())

    def test_an_interrupted_first_segment_resumes_and_still_holds(self):
        path, plan = self.switch_holding(self.ref[20:24])
        with mock.patch.dict(os.environ, {'FAKE_FAIL_AT': '23'}):  # the trainer exits inside segment 3
            self.assertNotEqual(self.drive(plan, 10), chains.EXIT_STOP)
        self.assertFalse((Path(self.man()['segments'][3]['root']) / 'completion.json').exists())
        self.assertEqual(self.drive(plan, 10), chains.EXIT_STOP)  # resumes segment 3, then holds
        self.assertEqual(len(self.man()['segments']), 4)
        self.assertTrue((Path(self.man()['segments'][3]['root']) / 'completion.json').exists())


class TodaysPathUnchangedTests(_Base):
    """Passes before and after the change: a request without `proof_a` keeps today's checks and bytes."""

    def test_combined_proof_history_entry_keys(self):
        path, plan = self.start(2)
        self.stop()
        new = self.build()
        path, plan = self.switch(plan, path, FAKE, new)
        entry = self.man()['trainer_history'][-1]
        self.assertEqual(sorted(entry), sorted(['id', 'segment', 'boundary', 'from_sha256', 'to_sha256', 'to_path',
                                                'request', 'proof', 'unix']))
        self.assertNotIn('proof_a', json.dumps(plan))

    def test_a_gate_c_only_receipt_without_proof_a_is_refused(self):
        path, plan = self.start(3)
        new = self.build()
        a = self.prehold(FAKE, new, k=2)
        self.stop()
        req = self.prehold_request(plan, FAKE, new, a)
        doc = json.loads(req.read_text())
        doc.pop('proof_a')
        code, err = self.repin(path, 'begin', jwrite(self.t / 'req-no-a.json', doc))
        self.assertEqual(code, w5box.EXIT_REFUSED, err)
        self.assertFalse((self.cdir() / chains.REPIN_PENDING).exists())


if __name__ == '__main__':
    unittest.main()
