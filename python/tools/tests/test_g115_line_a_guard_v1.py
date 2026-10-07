"""Refusal tests for the line (a) admission guard; nothing here dispatches native work."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import g115_line_a_guard_v1 as guard

NOW = 1_790_600_000.0
BINDING = dict(launcher_sha256='1' * 64, executable_sha256='2' * 64, data_tree_sha256='3' * 64)
GIB = 2**30


def rows(n, tag='x'):
    return [dict(id='q%02d' % k, sha256=('%s%063d' % (tag, k))[:64]) for k in range(n)]


def evidence():
    """The primary desktop measured at 1, 8 and 24 workers; the compute host and RunPod ineligible with reasons."""
    phases = [dict(workers=w, seconds=s, completed=48, rows=rows(48)) for w, s in ((1, 480.0), (8, 72.0), (24, 36.0))]
    inventory = dict(
        desktop=dict(checked_unix=NOW - 60, eligible=True, reason='idle, 24 logical CPUs, 2 GPUs', competing=[]),
        computehost=dict(checked_unix=NOW - 60, eligible=False, reason='transport not qualified for this workload'),
        runpod=dict(checked_unix=NOW - 60, eligible=False, reason='no Linux path for this workload'))
    value = dict(schema=guard.THROUGHPUT_SCHEMA, binding=dict(BINDING, work_class='bo3-ordinary'),
                 inventory=inventory, hosts=dict(desktop=dict(phases=phases, overhead_seconds=30.0)),
                 placements=[], selected='desktop-24')
    for workers in (1, 8, 24):
        allocation = dict(desktop=workers)
        value['placements'].append(dict(id='desktop-%d' % workers, allocation=allocation, eligible=True,
                                        ineligibility_reason='',
                                        projected_seconds=guard.projected_seconds(value, allocation, 2688)))
    return value


def worksheet(unit_bytes=1_000_000, units=2688):
    items = [dict(category=c, unit_bytes=unit_bytes if c == 'outputs' else 1000, units=units,
                  measured_by=dict(path='E:/q/receipt.json', sha256='4' * 64)) for c in guard.WORKSHEET_CATEGORIES]
    return dict(schema=guard.WORKSHEET_SCHEMA, job_set='calibration', manifest_sha256='5' * 64,
                cap_bytes=guard.JOB_SET_CAPS['calibration'], items=items,
                projected_bytes=guard.projected_bytes(items))


class ThroughputTests(unittest.TestCase):
    def admit(self, value, units=2688, binding=BINDING, work_class='bo3-ordinary'):
        return guard.require_throughput(value, work_class, units, binding, now=NOW)

    def test_fastest_feasible_placement_is_admitted(self):
        self.assertEqual(self.admit(evidence())['id'], 'desktop-24')
        self.assertAlmostEqual(evidence()['placements'][2]['projected_seconds'], 30.0 + 2688 / (48 / 36.0))

    def test_refuses_missing_evidence(self):
        with self.assertRaisesRegex(ValueError, 'Throughput evidence missing'):
            self.admit(None)

    def test_refuses_serial_only_or_parallel_only(self):
        for keep in ([0], [1, 2]):
            value = evidence()
            value['hosts']['desktop']['phases'] = [value['hosts']['desktop']['phases'][k] for k in keep]
            value['placements'] = [p for p in value['placements']
                                   if p['allocation']['desktop'] in [value['hosts']['desktop']['phases'][k]['workers']
                                                                  for k in range(len(keep))]]
            with self.assertRaisesRegex(ValueError, 'Serial and parallel both required'):
                self.admit(value)

    def test_refuses_parallel_output_that_differs_from_serial(self):
        value = evidence()
        value['hosts']['desktop']['phases'][2]['rows'][5]['sha256'] = 'f' * 64
        with self.assertRaisesRegex(ValueError, 'differs from the serial reference'):
            self.admit(value)

    def test_refuses_stale_or_incomplete_inventory(self):
        value = evidence()
        value['inventory']['computehost']['checked_unix'] = NOW - 25 * 3600
        with self.assertRaisesRegex(ValueError, 'Refresh the inventory'):
            self.admit(value)
        value = evidence()
        del value['inventory']['runpod']
        with self.assertRaisesRegex(ValueError, 'the maintainer, the compute host and RunPod'):
            self.admit(value)

    def test_refuses_an_eligible_host_left_unmeasured(self):
        value = evidence()
        value['inventory']['computehost'].update(eligible=True, competing=[])
        with self.assertRaisesRegex(ValueError, 'Eligible host was not measured: computehost'):
            self.admit(value)

    def test_refuses_eligible_host_with_competing_work(self):
        value = evidence()
        value['inventory']['desktop']['competing'] = ['trainer.exe']
        with self.assertRaisesRegex(ValueError, 'competing work'):
            self.admit(value)

    def test_refuses_a_slower_selection(self):
        value = evidence()
        value['selected'] = 'desktop-8'
        with self.assertRaisesRegex(ValueError, 'A faster feasible placement exists'):
            self.admit(value)

    def test_a_slow_best_placement_is_still_admitted(self):
        # 2026-09-25 clarification: a missed earlier bound is recorded, never a veto.
        value = evidence()
        for placement in value['placements']:
            placement['projected_seconds'] = guard.projected_seconds(value, placement['allocation'], 10**7)
        self.assertEqual(self.admit(value, units=10**7)['id'], 'desktop-24')

    def test_a_missed_earlier_bound_is_recorded_as_a_shortfall(self):
        value = evidence()
        value['earlier_bound'] = dict(seconds=100.0, source='CODEX-G115-D3-RESULT-20260924.md:51')
        admitted = self.admit(value)
        self.assertEqual(admitted['id'], 'desktop-24')
        self.assertAlmostEqual(admitted['shortfall']['shortfall_seconds'], admitted['projected_seconds'] - 100.0)
        self.assertIsNone(self.admit(evidence())['shortfall'])
        value['earlier_bound'] = dict(seconds=100.0, source=' ')
        with self.assertRaisesRegex(ValueError, 'Earlier bound needs its source'):
            self.admit(value)

    def test_refuses_a_projection_that_differs_from_measured_rates(self):
        value = evidence()
        value['placements'][2]['projected_seconds'] /= 2
        with self.assertRaisesRegex(ValueError, 'Projection differs'):
            self.admit(value)

    def test_refuses_evidence_bound_to_other_code_or_class(self):
        with self.assertRaisesRegex(ValueError, 'binds a different executable_sha256'):
            self.admit(evidence(), binding=dict(BINDING, executable_sha256='9' * 64))
        with self.assertRaisesRegex(ValueError, 'different work class'):
            self.admit(evidence(), work_class='bo3-search')

    def test_refuses_unexplained_exclusion_and_uncompared_allocations(self):
        value = evidence()
        value['placements'][0].update(eligible=False, ineligibility_reason=' ')
        with self.assertRaisesRegex(ValueError, 'Explain the excluded placement'):
            self.admit(value)
        value = evidence()
        del value['placements'][1]
        with self.assertRaisesRegex(ValueError, 'must be compared'):
            self.admit(value)


class WorksheetTests(unittest.TestCase):
    def admit(self, value, free=500 * GIB, committed=0):
        return guard.require_worksheet(value, 'calibration', '5' * 64, free, committed)

    def test_measured_worksheet_inside_caps_is_admitted(self):
        value = worksheet()
        self.assertEqual(self.admit(value), (2688 * 1_000_000 + 4 * 2688 * 1000) * 115 // 100)

    def test_refuses_missing_or_unmeasured_worksheet(self):
        with self.assertRaisesRegex(ValueError, 'worksheet missing'):
            self.admit(None)
        value = worksheet()
        value['items'][0]['unit_bytes'] = None
        with self.assertRaisesRegex(ValueError, 'measured integers'):
            self.admit(value)
        value = worksheet()
        value['items'][1]['measured_by'] = None
        with self.assertRaisesRegex(ValueError, 'qualification receipt'):
            self.admit(value)

    def test_refuses_over_the_job_set_cap(self):
        with self.assertRaisesRegex(ValueError, 'exceed the calibration cap'):
            self.admit(worksheet(unit_bytes=4_000_000))

    def test_refuses_over_the_joint_60_gb(self):
        with self.assertRaisesRegex(ValueError, 'joint 60 GB'):
            self.admit(worksheet(), committed=57_000_000_000)

    def test_refuses_below_the_60_gib_reserve(self):
        value = worksheet()
        with self.assertRaisesRegex(ValueError, '60 GiB reserve'):
            self.admit(value, free=60 * GIB + value['projected_bytes'] - 1)
        self.admit(value, free=60 * GIB + value['projected_bytes'])

    def test_refuses_missing_category_changed_projection_or_foreign_manifest(self):
        value = worksheet()
        value['items'] = [i for i in value['items'] if i['category'] != 'failed_attempts']
        with self.assertRaisesRegex(ValueError, 'failed attempts'):
            self.admit(value)
        value = worksheet()
        value['projected_bytes'] -= 1
        with self.assertRaisesRegex(ValueError, 'projection differs'):
            self.admit(value)
        with self.assertRaisesRegex(ValueError, 'different manifest'):
            guard.require_worksheet(worksheet(), 'calibration', '6' * 64, 500 * GIB)


class ScratchAndLockTests(unittest.TestCase):
    def scratch(self):
        return dict(schema=guard.SCRATCH_SCHEMA, job='g115-line-a-calibration', owner='opus-line-a-launcher',
                    host='desktop', root='D:/e-scratch/g115-line-a-calibration/',
                    sources=[dict(path='E:/mtg-g115-lineage-20260923/x.json', sha256='a' * 64, bytes=10)],
                    disposable=['native/**'], cap_bytes=1_000_000)

    def test_scratch_manifest_rules(self):
        guard.require_scratch_manifest(self.scratch(), 'g115-line-a-calibration', 'desktop', 12 * 10**9)
        for change, message in ((dict(root='E:/scratch/'), 'D:/e-scratch/<job>/'),
                                (dict(sources=[dict(path='D:/x.json', sha256='a' * 64, bytes=1)]), 'E: path'),
                                (dict(cap_bytes=13 * 10**9), 'inside the job-set cap')):
            with self.assertRaisesRegex(ValueError, message):
                guard.require_scratch_manifest(dict(self.scratch(), **change), 'g115-line-a-calibration', 'desktop',
                                               12 * 10**9)
        with self.assertRaisesRegex(ValueError, 'must precede first use'):
            guard.require_scratch_manifest(None, 'g115-line-a-calibration', 'desktop', 12 * 10**9)

    def test_no_scratch_path_is_an_input_of_record(self):
        guard.require_no_scratch_inputs(dict(inputs=[dict(path='E:/sealed/a.json')]), 'D:/e-scratch/job/')
        with self.assertRaisesRegex(ValueError, 'input of record'):
            guard.require_no_scratch_inputs(dict(inputs=[dict(path='D:\\e-scratch\\job\\a.json')]),
                                            'D:/e-scratch/job/')

    def test_e_io_lock_waits_and_never_breaks(self):
        with tempfile.TemporaryDirectory() as directory:
            lock = Path(directory) / 'LOCKS' / 'e-io.json'
            held = guard.acquire_e_io('lane-a', 'scratch fill', 600, lock_path=lock, clock=lambda: NOW)
            before = lock.read_bytes()
            clock = iter([NOW, NOW, NOW + 1, NOW + 2, NOW + 3])
            with self.assertRaisesRegex(ValueError, 'waiting, never breaking'):
                guard.acquire_e_io('lane-b', 'LZX pass', 60, lock_path=lock, timeout_seconds=2,
                                   clock=lambda: next(clock), sleep=lambda s: None)
            self.assertEqual(lock.read_bytes(), before)
            with self.assertRaisesRegex(ValueError, 'already held by this owner'):
                guard.acquire_e_io('lane-a', 'again', 60, lock_path=lock, clock=lambda: NOW)
            with self.assertRaisesRegex(ValueError, 'Only the holder'):
                guard.release_e_io('lane-b', lock_path=lock)
            guard.release_e_io('lane-a', lock_path=lock)
            self.assertFalse(lock.exists())
            self.assertEqual(held['expected_end_unix'], NOW + 600)


class OrderPinAndYardstickTests(unittest.TestCase):
    def order(self, directory, complete=True, verdict='pass', text='USABILITY: pass (all four conditions)'):
        completion = Path(directory) / 'completion.json'
        completion.write_text(json.dumps(dict(complete=complete)))
        usability = Path(directory) / 'usability.md'
        usability.write_text(text)
        return dict(schema=guard.ORDER_SCHEMA,
                    calibration_completion=dict(path=str(completion), sha256=guard.sha256_file(completion)),
                    usability=dict(path=str(usability), sha256=guard.sha256_file(usability), verdict=verdict,
                                   excerpt='USABILITY: pass'))

    def test_screen_training_waits_for_calibration_and_usability(self):
        with tempfile.TemporaryDirectory() as directory:
            guard.require_calibration_before_training(self.order(directory))
        with self.assertRaisesRegex(ValueError, 'Calibration completion record required first'):
            guard.require_calibration_before_training(None)
        for kwargs, message in ((dict(complete=False), 'not complete'), (dict(verdict='fail'), 'not passed'),
                                (dict(text='USABILITY: fail'), 'not passed')):
            with tempfile.TemporaryDirectory() as directory:
                with self.assertRaisesRegex(ValueError, message):
                    guard.require_calibration_before_training(self.order(directory, **kwargs))

    def test_composition_comes_only_from_the_director_scope_ruling(self):
        with tempfile.TemporaryDirectory() as directory:
            ruling = Path(directory) / 'DIRECTOR-RULINGS.md'
            ruling.write_text('Scope ruling: version 1 freezes as staged B.')
            scope = dict(schema=guard.SCOPE_SCHEMA, composition='B',
                         ruling=dict(path=str(ruling), sha256=guard.sha256_file(ruling),
                                     excerpt='version 1 freezes as staged B'))
            guard.require_scope_ruling(scope, 'B')
            with self.assertRaisesRegex(ValueError, 'differs from the director scope ruling'):
                guard.require_scope_ruling(scope, 'E')
            with self.assertRaisesRegex(ValueError, 'text absent'):
                guard.require_scope_ruling(dict(scope, ruling=dict(scope['ruling'], excerpt='freezes as E')), 'B')
        with self.assertRaisesRegex(ValueError, 'scope ruling record required'):
            guard.require_scope_ruling(None, 'B')

    def test_one_pinned_yardstick_executable(self):
        self.assertEqual(guard.require_one_yardstick([dict(yardstick_executable_sha256='a' * 64)] * 3), 'a' * 64)
        for values in (['a' * 64, 'b' * 64], ['a' * 64, None]):
            with self.assertRaisesRegex(ValueError, 'one pinned yardstick'):
                guard.require_one_yardstick([dict(yardstick_executable_sha256=v) for v in values])

    def test_binaries_run_from_their_pinned_copy(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / 'public_feature_evaluation_v1.exe'
            binary.write_bytes(b'native bytes')
            root = Path(directory) / 'pinned-binaries'
            pinned, digest = guard.pin_binary(binary, root)
            self.assertEqual(pinned, root / digest / binary.name)
            self.assertEqual(guard.pin_binary(binary, root), (pinned, digest))
            guard.require_pinned(pinned, digest, root)
            with self.assertRaisesRegex(ValueError, 'pinned copy'):
                guard.require_pinned(binary, digest, root)
            pinned.write_bytes(b'changed')
            with self.assertRaisesRegex(ValueError, 'differs from its hash'):
                guard.pin_binary(binary, root)


if __name__ == '__main__':
    unittest.main()
