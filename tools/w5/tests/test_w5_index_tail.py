"""A chain index whose last append was interrupted (mtg-kernel#853): under the index lock, `retention.index` and
`retention.IndexCache` keep every complete line, rebuild the torn row and every later one from the updates' own
receipts exactly as a normal append writes them, and refuse, changing nothing, when a receipt is missing or does not
agree with the torn bytes. A line equal to the line before it (an append written twice) is skipped, never rewritten.
A crash at any step of a recovery leaves a file the next recovery repairs. A clean index is left as it was."""
import contextlib
import io
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
sys.path.insert(0, str(HERE))
import chains  # noqa: E402
import retention  # noqa: E402
from test_w5_chains import STEP_C, STEP_X, Node  # noqa: E402


class Crash(BaseException):
    """A process killed at this point (no handler in the code under test catches it)."""


def no_scan(*args, **kwargs):
    return []


class IndexTailTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.mkdtemp()
        node = Node(cls.tmp)
        with mock.patch.dict(os.environ, {'FAKE_CPU_STEP': str(STEP_C), 'FAKE_CPU_STEP_X': str(STEP_X)}), \
                mock.patch.object(chains, 'GUARD_WAIT', 0.0), mock.patch.object(chains, 'GUARD_POLL', 0.0):
            _, plan = node.stage0(c_wall=600.0)
            cls.path = plan['chains']['c1']['manifest']
            node.guard(ceilings={'c1': 3})
            assert node.drive(plan, 'c1') == chains.EXIT_ROOM
        cls.index_file = retention.index_path(cls.path)
        retention.index(cls.path)  # the normal appends write every row
        cls.whole = cls.index_file.read_bytes()
        cls.rows = cls.whole.splitlines(keepends=True)
        cls.records = [json.loads(x) for x in cls.rows]
        assert len(cls.rows) == 12 and [r['boundary'] for r in cls.records] == list(range(1, 13))

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def setUp(self):
        self.err = io.StringIO()
        stack = contextlib.ExitStack()
        stack.enter_context(contextlib.redirect_stderr(self.err))
        self.addCleanup(stack.close)

    def put(self, data):
        self.index_file.write_bytes(data)

    def read_index(self):
        return list(retention.index(self.path)[1])

    def read_cache(self):
        _, records = retention.IndexCache(self.path).refresh()
        handle = records.store.handle
        try:
            return list(records)
        finally:
            if handle is not None:
                handle.close()

    def recovers(self, data, read, expect=None):
        """`data` as the index file, read once: every row is there, and the file is the normally written one."""
        self.put(data)
        self.assertEqual(read(), self.records)
        self.assertEqual(self.index_file.read_bytes(), self.whole if expect is None else expect)

    def receipt(self, u):
        seg = retention.load_chain(self.path)['segments'][u // 4]
        return Path(seg['root'], 'iterations', f'{u % 4:06d}', 'complete.json')

    # ---------------------------------------------------------------- torn last line

    def test_the_rebuilt_row_is_the_normally_appended_row(self):
        """Every cut of the last row: the rebuilt row is byte for byte the row the normal append wrote."""
        last = self.rows[-1]
        for read in (self.read_index, self.read_cache):
            for k in range(1, len(last) - 1):  # a cut before the closing brace (a cut after it parses)
                self.recovers(b''.join(self.rows[:-1]) + last[:k], read)
                self.assertEqual(self.index_file.read_bytes().splitlines(keepends=True)[-1], last)
        self.assertIn('index-tail-recovered', self.err.getvalue())

    def test_a_torn_append_of_several_rows(self):
        """One append writes several rows: torn inside the first, it and every later row are rebuilt."""
        for read in (self.read_index, self.read_cache):
            for back, k in ((2, 1), (3, 30), (5, len(self.rows[-5]) - 2), (12, 7)):
                self.recovers(b''.join(self.rows[:-back]) + self.rows[-back][:k], read)

    def test_a_torn_tail_past_a_running_supervisors_cursor(self):
        """The supervisor's cache holds 10 rows; another writer dies inside its append of row 11: the next refresh
        reads the file again, recovers it and returns every row."""
        self.put(b''.join(self.rows[:10]))
        cache = retention.IndexCache(self.path)
        with mock.patch.object(retention, 'scan', no_scan):
            self.assertEqual(list(cache.refresh()[1]), self.records[:10])
        self.put(b''.join(self.rows[:10]) + self.rows[10][:25])
        self.assertEqual(list(cache.refresh()[1]), self.records)
        self.assertEqual(self.index_file.read_bytes(), self.whole)
        self.assertEqual(list(cache.refresh()[1]), self.records)

    def test_another_writers_recovery_under_a_running_supervisors_read_ahead(self):
        """The supervisor's cache (on POSIX it keeps the index open) reads its last row while a torn tail with NULs
        follows it, so its read-ahead can hold those bytes; another writer then recovers the file. The next refresh
        reads the rebuilt rows from the file, not the stale read-ahead. (Windows opens the file for each read, so
        there this checks the recovery only.)"""
        self.put(b''.join(self.rows[:10]))
        cache = retention.IndexCache(self.path)
        with mock.patch.object(retention, 'scan', no_scan):
            self.assertEqual(list(cache.refresh()[1]), self.records[:10])
        self.put(b''.join(self.rows[:10]) + self.rows[10][:30] + bytes(64))
        if cache.records.handle is not None:  # so the next read starts a fresh read-ahead at row 9's offset
            cache.records.handle.seek(0, 2)
        self.assertEqual(cache.records.check(9), self.records[9])  # reads ahead into the torn bytes
        self.assertEqual(self.read_index(), self.records)  # another writer recovers
        self.assertEqual(self.index_file.read_bytes(), self.whole)
        view = cache.refresh()[1]
        self.assertEqual([view[10], view[11]], self.records[10:])  # the rows right after the read-ahead position
        self.assertEqual(list(view), self.records)

    def test_nul_bytes_left_by_a_power_cut(self):
        """Unwritten data a power cut leaves as NULs, after part of the row or in place of all of it."""
        last = self.rows[-1]
        for read in (self.read_index, self.read_cache):
            for k in (0, 1, 40):
                self.recovers(b''.join(self.rows[:-1]) + last[:k] + bytes(len(last) - k), read)
            self.recovers(b''.join(self.rows[:-3]) + self.rows[-3][:9] + bytes(4096), read)

    def test_a_torn_first_row(self):
        for read in (self.read_index, self.read_cache):
            self.recovers(self.rows[0][:12], read)
            self.recovers(b'\n' + self.rows[0][:12], read, b'\n' + self.whole)  # after an empty line

    def test_other_line_breaks_the_readers_split_on(self):
        """The readers split lines as str.splitlines does: CR-separated rows and a last empty line ending in U+0085
        are clean (nothing recovered); a torn row after a CR is recovered after it."""
        cr = b''.join(r.rstrip(b'\n') + b'\r' for r in self.rows)
        nel = self.whole + b'\xc2\x85'
        for read in (self.read_index, self.read_cache):
            for data in (cr, nel):
                self.put(data)
                with mock.patch.object(retention, 'recover_tail', side_effect=AssertionError('no recovery')):
                    self.assertEqual(read(), self.records)
            cut = b''.join(r.rstrip(b'\n') + b'\r' for r in self.rows[:-1])
            self.recovers(cut + self.rows[-1][:20], read, cut + b'\n' + self.rows[-1])  # terminate's LF, as always

    def test_a_torn_tail_on_a_legacy_index_refuses(self):
        """An index written before S containers were tracked needs a migration that rewrites every line: with a
        torn tail it refuses (nothing rewritten) instead of migrating."""
        legacy = [json.dumps({k: v for k, v in r.items() if k != 'state'}).encode() + b'\n' for r in self.records]
        self.refuses(b''.join(legacy[:-1]) + self.rows[-1][:30], 'S-container migration')

    # ---------------------------------------------------------------- refusals

    def refuses(self, data, why):
        for read in (self.read_index, self.read_cache):
            self.put(data)
            with self.assertRaises(SystemExit) as stop:
                read()
            self.assertIn(why, str(stop.exception))
            self.assertIn('nothing was changed', str(stop.exception))
            self.assertEqual(self.index_file.read_bytes(), data)
        self.assertIn('index-tail-refused', self.err.getvalue())

    def test_a_missing_receipt_refuses(self):
        torn = b''.join(self.rows[:-1]) + self.rows[-1][:30]
        complete = self.receipt(11)
        hidden = complete.with_name('complete.hidden')
        complete.rename(hidden)
        self.addCleanup(hidden.rename, complete)
        self.refuses(torn, f'update 11 has no receipt to rebuild its row from ({complete.as_posix()})')

    def hide(self, path):
        hidden = path.with_name(path.name + '.hidden')
        path.rename(hidden)
        self.addCleanup(hidden.rename, path)

    def test_later_missing_receipt_in_a_completed_carried_segment_refuses(self):
        chain = retention.load_chain(self.path)
        chain['carry'] = {'boundary': 64}
        for seg in chain['segments']:
            seg['offset'] += 64
        self.path = Path(self.path).with_name('carried.json')
        self.path.write_text(json.dumps(chain))
        self.index_file = retention.index_path(self.path)
        self.read_index()
        rows = self.index_file.read_bytes().splitlines(keepends=True)
        self.hide(Path(chain['segments'][-1]['root'], 'iterations', '000003', 'complete.json'))
        self.refuses(b''.join(rows[:6]) + rows[6][:30], 'update 75 has no receipt')

    def test_receipt_after_a_hole_without_segment_completion_refuses(self):
        root = Path(retention.load_chain(self.path)['segments'][-1]['root'])
        self.hide(root / 'completion.json')
        self.hide(self.receipt(10))
        self.refuses(b''.join(self.rows[:8]) + self.rows[8][:30], self.receipt(11).as_posix())

    def test_live_and_mounted_prefixes_recover_up_to_persisted_receipts(self):
        root = Path(retention.load_chain(self.path)['segments'][-1]['root'])
        self.hide(root / 'completion.json')
        self.hide(self.receipt(11))
        torn = b''.join(self.rows[:6]) + self.rows[6][:30]
        for mounted, count in [(False, 11), (True, 8)]:
            with mock.patch.object(os.path, 'ismount', side_effect=lambda p: mounted and Path(p) == root):
                for read in (self.read_index, self.read_cache):
                    self.put(torn)
                    self.assertEqual(read(), self.records[:count])
                    self.assertEqual(self.index_file.read_bytes(), b''.join(self.rows[:count]))

    def test_recovery_reads_a_receipt_published_while_checking_a_gap(self):
        receipt = self.receipt(10)
        hidden = receipt.with_name('complete.hidden')
        evidence = retention.receipt_gap_evidence

        def publish(chain, missing):
            hidden.rename(receipt)
            return evidence(chain, missing)

        with mock.patch.object(retention, 'receipt_gap_evidence', side_effect=publish):
            for read in (self.read_index, self.read_cache):
                receipt.rename(hidden)
                try:
                    self.recovers(b''.join(self.rows[:8]) + self.rows[8][:30], read)
                finally:
                    if hidden.exists():
                        hidden.rename(receipt)

    def test_an_unreadable_receipt_refuses(self):
        torn = b''.join(self.rows[:-1]) + self.rows[-1][:30]
        update = Path(json.loads(self.receipt(11).read_text())['update']['path'])
        hidden = update.with_name('update.hidden')
        update.rename(hidden)
        self.addCleanup(hidden.rename, update)
        self.refuses(torn, 'the receipt of update 11 does not read')

    def test_a_receipt_of_the_wrong_shape_refuses(self):
        torn = b''.join(self.rows[:-1]) + self.rows[-1][:30]
        complete = self.receipt(11)
        good = complete.read_bytes()
        self.addCleanup(complete.write_bytes, good)
        complete.write_text(json.dumps(dict(json.loads(good), chain_ledger=[1])))
        self.refuses(torn, 'the receipt of update 11 does not read (AttributeError')

    def test_torn_bytes_the_receipts_disagree_with_refuse(self):
        last = self.rows[-1]
        self.assertTrue(last.startswith(b'{"update": 11, '))
        self.refuses(b''.join(self.rows[:-1]) + b'{"update": 12, "bou', 'does not begin with the torn bytes')
        self.refuses(b''.join(self.rows[:-1]) + last[:60].replace(b'"', b"'"), 'does not begin with the torn bytes')
        self.refuses(b''.join(self.rows[:-1]) + b'  ', 'does not begin with the torn bytes')  # a line of spaces
        self.refuses(b''.join(self.rows[:-1]) + last[:9] + bytes(5) + b'x' + bytes(5), 'follow a NUL')

    def test_a_torn_row_past_the_last_receipt_refuses(self):
        """Every receipt is indexed and a torn line follows: no receipt holds that row (nothing is guessed)."""
        self.refuses(self.whole + b'{"update": 12', 'update 12 has no receipt')

    # ---------------------------------------------------------------- empty and repeated last lines

    def test_an_empty_last_line(self):
        """The readers skip an empty line; later rows go after it, and nothing before them is rewritten."""
        for read in (self.read_index, self.read_cache):
            self.recovers(self.whole + b'\n', read, self.whole + b'\n')
            self.recovers(b''.join(self.rows[:8]) + b'\n', read, b''.join(self.rows[:8]) + b'\n' + b''.join(
                self.rows[8:]))

    def test_a_repeated_last_line(self):
        """The last append written twice: read once, the file kept as it is, and the next rows found from the
        receipts (the repeat is not counted, so no receipt is skipped)."""
        for read in (self.read_index, self.read_cache):
            self.recovers(self.whole + self.rows[-1], read, self.whole + self.rows[-1])
            self.recovers(b''.join(self.rows[:8]) + self.rows[7], read,
                          b''.join(self.rows[:8]) + self.rows[7] + b''.join(self.rows[8:]))
        self.assertIn('index-repeated-line', self.err.getvalue())

    def test_a_repeated_line_past_a_running_supervisors_cursor(self):
        self.put(b''.join(self.rows[:6]))
        cache = retention.IndexCache(self.path)
        with mock.patch.object(retention, 'scan', no_scan):
            self.assertEqual(list(cache.refresh()[1]), self.records[:6])
            self.put(b''.join(self.rows[:6]) + self.rows[5] + self.rows[6] + self.rows[6])
            self.assertEqual(list(cache.refresh()[1]), self.records[:7])
        self.assertEqual(list(cache.refresh()[1]), self.records)
        expect = b''.join(self.rows[:6]) + self.rows[5] + self.rows[6] + self.rows[6] + b''.join(self.rows[7:])
        self.assertEqual(self.index_file.read_bytes(), expect)
        self.assertEqual(self.read_index(), self.records)

    def test_a_repeated_row_then_a_torn_one(self):
        self.recovers(b''.join(self.rows[:9]) + self.rows[8] + self.rows[9][:20], self.read_index,
                      b''.join(self.rows[:9]) + self.rows[8] + b''.join(self.rows[9:]))

    # ---------------------------------------------------------------- a crash during the recovery

    def crash_states(self, back):
        """The kinds of file a crash during the recovery of a torn row `back` rows from the end can leave: before the
        cut (the torn file), after the cut (the complete lines), inside the append of the rebuilt rows (prefixes of
        them at sampled offsets, including each row boundary and the bytes either side of it, also with the rest as
        NULs), and after it."""
        prefix, rebuilt = b''.join(self.rows[:-back]), b''.join(self.rows[-back:])
        yield prefix + rebuilt[:17]
        yield prefix
        ends = [sum(len(r) for r in self.rows[-back:][:i + 1]) for i in range(back)]
        for j in sorted({1, 2, 16, len(rebuilt) // 2, len(rebuilt) - 2, len(rebuilt) - 1} | {
                e + d for e in ends for d in (-2, -1, 0, 1) if 0 < e + d < len(rebuilt)}):
            yield prefix + rebuilt[:j]
            yield prefix + rebuilt[:j] + bytes(len(rebuilt) - j)
        yield prefix + rebuilt

    def test_every_state_a_crash_in_the_recovery_leaves_is_recovered(self):
        """(A state holding every row with only the final newline missing is clean: index(), with nothing to append,
        leaves it; the cache terminates it, as before.)"""
        for read in (self.read_index, self.read_cache):
            for back in (1, 3):
                for state in self.crash_states(back):
                    clean = state == self.whole[:-1] and read == self.read_index
                    self.recovers(state, read, state if clean else None)

    def crashing_open(self, after):
        """`open` for retention whose append to the index writes only its first `after` bytes, then dies."""
        real, index_file = open, self.index_file

        class Dying:
            def __init__(self, f):
                self.f, self.name = f, f.name

            def __enter__(self):
                return self

            def __exit__(self, *exc):
                self.f.close()

            def fileno(self):
                return self.f.fileno()

            def flush(self):
                self.f.flush()

            def write(self, data):
                if len(data) <= 1:  # terminate's newline
                    return self.f.write(data)
                self.f.write(data[:after])
                self.f.flush()
                raise Crash(after)

        def opener(path, mode='r', *args, **kwargs):
            f = real(path, mode, *args, **kwargs)
            return Dying(f) if mode == 'ab' and Path(path) == index_file else f
        return opener

    def test_a_crash_at_each_step_of_the_recovery(self):
        """An exception injected at each step of the recovery, standing in for a kill there (the file is left as
        the step left it; what a power cut can leave is covered by the prefix and NUL states above): at the scan,
        at the fsync after the truncate, before the append, inside the append (twice in a row), then a clean
        recovery: every row, written as normally."""
        torn = b''.join(self.rows[:-3]) + self.rows[-3][:33]
        steps = [mock.patch.object(retention, 'scan', side_effect=Crash('scan')),
                 mock.patch.object(retention.os, 'fsync', side_effect=Crash('fsync')),
                 mock.patch.object(retention, 'terminate', side_effect=Crash('terminate')),
                 mock.patch.object(retention, 'open', self.crashing_open(5), create=True),
                 mock.patch.object(retention, 'open', self.crashing_open(len(self.rows[-3]) + 9), create=True)]
        for read in (self.read_index, self.read_cache):
            for step in steps:
                self.put(torn)
                with step, self.assertRaises(Crash):
                    read()
                self.assertTrue(self.whole.startswith(self.index_file.read_bytes()[:len(self.whole)]))
                self.assertEqual(read(), self.records)
                self.assertEqual(self.index_file.read_bytes(), self.whole)
            self.put(torn)  # two crashes in a row, inside each recovery's append
            for after in (40, 3):
                with mock.patch.object(retention, 'open', self.crashing_open(after), create=True), \
                        self.assertRaises(Crash):
                    read()
            self.assertEqual(read(), self.records)
            self.assertEqual(self.index_file.read_bytes(), self.whole)

    def test_a_crash_at_the_truncate(self):
        """A crash as the truncate opens the file: nothing was cut, and the next recovery does it."""
        torn = b''.join(self.rows[:-1]) + self.rows[-1][:50]

        def opener(path, mode='r', *args, **kwargs):
            if mode == 'r+b':
                raise Crash('truncate')
            return open(path, mode, *args, **kwargs)
        for read in (self.read_index, self.read_cache):
            self.put(torn)
            with mock.patch.object(retention, 'open', opener, create=True), self.assertRaises(Crash):
                read()
            self.assertEqual(self.index_file.read_bytes(), torn)
            self.assertEqual(read(), self.records)
            self.assertEqual(self.index_file.read_bytes(), self.whole)

    # ---------------------------------------------------------------- a clean index

    def test_a_clean_index_is_left_as_it_is(self):
        """No recovery runs and the writers append exactly as before: index() appends only when there are new rows,
        the cache also terminates a last line left without its newline."""
        unterminated = self.whole.rstrip(b'\n')
        for data in (self.whole, unterminated, b''.join(self.rows[:5])):
            for read, expect in ((self.read_index, unterminated if data == unterminated else self.whole),
                                 (self.read_cache, self.whole)):
                self.put(data)
                with mock.patch.object(retention, 'recover_tail', side_effect=AssertionError('no recovery')):
                    self.assertEqual(read(), self.records)
                self.assertEqual(self.index_file.read_bytes(), expect)
        self.assertNotIn('index-tail', self.err.getvalue())
        self.assertNotIn('index-repeated-line', self.err.getvalue())


if __name__ == '__main__':
    unittest.main()
