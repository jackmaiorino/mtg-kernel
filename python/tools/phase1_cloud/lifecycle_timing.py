"""Assembles the phase1-complete-work-timing/v1 lifecycle envelope that
throughput.qualify() requires, from explicit timestamped receipts.

No provider call, no native process, no key handling. This tool only reads
already-produced JSON receipts (or accepts a bare ISO-8601 timestamp for a
stage that has no receipt of its own, such as "staging complete") and asserts
their timestamps are monotonic before writing the envelope throughput.py's
complete_work_seconds() independently re-validates.

Nine phases (throughput.TIMING_PHASES): input_preparation, allocation,
image_startup, input_transfer, stage_verification, native_invocation,
checkpoint_export, output_transfer, release_confirmation.

For --environment cloud, every phase is measured from these markers, taken in
this chronological order: funding snapshot, network-volume create (created-
volume.json from prepare_volume.py --execute), Pod create (created-pod.json
from prepare_lease.py --execute), guard armed (guard.json from lease_guard.py),
staging complete (cloud_worker.py stage), native run start and end (worker-
start.json / worker-result.json from cloud_worker.py run, or an explicit
--run-end), checkpoint export complete (recovery.json from cloud_worker.py's
export), and an independently confirmed provider release lookup (a receipt
showing provider_absent: true).

For --environment local there is no Pod: allocation, image_startup and
release_confirmation are recorded not_applicable (with a reason and evidence,
never silently zero), and the remaining six phases are built from an explicit
--input-preparation-start plus the same staging/run/export markers.

Phase intervals are allowed to be identical or overlapping (mirrors
throughput.py's own docstring: "Phase intervals need not be disjoint and are
not summed"); this tool does not invent boundaries no receipt supports.
"""
from __future__ import annotations
import argparse
from datetime import datetime
from pathlib import Path
import tempfile
import time
from common import encoded, pin, read, require, write
from throughput import TIMING_PHASES, complete_work_seconds, finite

TIMING_SCHEMA = 'phase1-complete-work-timing/v1'
OBSERVED_SCHEMA = 'phase1-lifecycle-observed-timestamp/v1'
NOT_APPLICABLE_SCHEMA = 'phase1-lifecycle-not-applicable/v1'
CLOUD_ONLY_MARKERS = ('funding_snapshot', 'volume_create', 'pod_create', 'guard_armed', 'release_confirmed')
NOT_APPLICABLE_PHASES = ('allocation', 'image_startup', 'release_confirmation')
# Priority order for extracting an absolute epoch from an existing receipt.
# Covers every receipt schema in this package that carries one: funding
# snapshots (observed_utc), created-volume.json/created-pod.json
# (created_epoch), verify-*.json (verified_epoch), guard.json/worker-
# start.json/recovery.json/released.json (epoch).
EPOCH_FIELDS = ('epoch', 'created_epoch', 'verified_epoch', 'prepared_epoch')


def receipt_epoch(document, marker_name):
    for field in EPOCH_FIELDS:
        value = document.get(field)
        if finite(value):
            return float(value)
    value = document.get('observed_utc')
    if isinstance(value, str):
        parsed = datetime.fromisoformat(value)
        require(parsed.tzinfo is not None, marker_name + ' receipt observed_utc must be timezone-aware')
        return parsed.timestamp()
    raise ValueError(marker_name + ' receipt has no recognized timestamp field '
                      '(epoch/created_epoch/verified_epoch/prepared_epoch/observed_utc)')


def resolve_marker(value, name, receipts_dir):
    """Resolve one marker to (epoch, [pin], document).

    `value` is either the path to an existing JSON receipt (its epoch taken
    via receipt_epoch) or an explicit ISO-8601 timestamp. A bare timestamp is
    synthesized into its own small pinned receipt in `receipts_dir`, so every
    phase always has real, pinned evidence on disk: throughput.py requires
    non-empty evidence for every phase, not_applicable phases included.
    """
    require(isinstance(value, str) and value, name + ' marker value is required')
    path = Path(value)
    if path.is_file():
        document = read(path)
        epoch = receipt_epoch(document, name)
        return epoch, [pin(path)], document
    try:
        parsed = datetime.fromisoformat(value)
    except ValueError:
        raise ValueError(name + ' is neither an existing receipt file nor a valid '
                          'timezone-aware ISO-8601 timestamp: ' + value) from None
    require(parsed.tzinfo is not None, name + ' timestamp must be timezone-aware: ' + value)
    epoch = parsed.timestamp()
    document = {'schema': OBSERVED_SCHEMA, 'marker': name, 'observed_iso': value, 'epoch': epoch}
    target = Path(receipts_dir) / (name + '.json')
    write(target, document, replace=True)
    return epoch, [pin(target)], document


def native_elapsed_seconds(document):
    """Mirrors reference.py build()'s own fallback chain for the native
    wrapper's elapsed-time field, so --worker-result reads the same figure
    reference.py would bind into the fixed-work reference it validates against."""
    for field in ('native_execution_elapsed_seconds', 'native_active_seconds', 'elapsed_seconds'):
        value = document.get(field)
        if finite(value, positive=True):
            return value
    raise ValueError('worker result has no recognized elapsed-seconds field '
                      '(native_execution_elapsed_seconds/native_active_seconds/elapsed_seconds)')


def not_applicable_receipt(name, reason, receipts_dir):
    require(isinstance(reason, str) and reason, name + ' not-applicable reason is required')
    document = {'schema': NOT_APPLICABLE_SCHEMA, 'phase': name, 'reason': reason}
    target = Path(receipts_dir) / (name + '-not-applicable.json')
    write(target, document, replace=True)
    return pin(target)


def require_chain(order, markers):
    for before, after in zip(order, order[1:]):
        require(markers[before][0] <= markers[after][0],
                'lifecycle markers are not monotonic: ' + before + ' is after ' + after)


def build_phases(environment, markers, not_applicable_evidence):
    def ep(name):
        return markers[name][0]

    def ev(*names):
        result = []
        for name in names:
            result.extend(markers[name][1])
        return result

    phases = {}
    if environment == 'cloud':
        chain = ['funding_snapshot', 'volume_create', 'pod_create', 'guard_armed',
                 'staging_complete', 'run_start', 'run_end', 'export_complete', 'release_confirmed']
        require_chain(chain, markers)
        phases['input_preparation'] = {'start_seconds': ep('funding_snapshot'), 'end_seconds': ep('volume_create'),
                                        'evidence': ev('funding_snapshot', 'volume_create')}
        phases['allocation'] = {'start_seconds': ep('volume_create'), 'end_seconds': ep('pod_create'),
                                 'evidence': ev('volume_create', 'pod_create')}
        phases['image_startup'] = {'start_seconds': ep('pod_create'), 'end_seconds': ep('guard_armed'),
                                    'evidence': ev('pod_create', 'guard_armed')}
        phases['input_transfer'] = {'start_seconds': ep('guard_armed'), 'end_seconds': ep('staging_complete'),
                                     'evidence': ev('guard_armed', 'staging_complete')}
        # cloud_worker.py stage() verifies the archive inline, immediately on
        # transfer completion; no receipt marks a separate verification start.
        phases['stage_verification'] = {'start_seconds': ep('staging_complete'), 'end_seconds': ep('staging_complete'),
                                         'evidence': ev('staging_complete')}
        phases['native_invocation'] = {'start_seconds': ep('run_start'), 'end_seconds': ep('run_end'),
                                        'evidence': ev('run_start', 'run_end')}
        phases['checkpoint_export'] = {'start_seconds': ep('run_end'), 'end_seconds': ep('export_complete'),
                                        'evidence': ev('run_end', 'export_complete')}
        # export_completed() writes straight to the durable network volume, so
        # "output transfer" and "checkpoint export" share the same boundary
        # here: there is no separate copy-back step for this pipeline.
        phases['output_transfer'] = {'start_seconds': ep('run_end'), 'end_seconds': ep('export_complete'),
                                      'evidence': ev('run_end', 'export_complete')}
        phases['release_confirmation'] = {'start_seconds': ep('export_complete'), 'end_seconds': ep('release_confirmed'),
                                           'evidence': ev('export_complete', 'release_confirmed')}
        begin, end = ep('funding_snapshot'), ep('release_confirmed')
    else:
        chain = ['input_preparation_start', 'staging_complete', 'run_start', 'run_end', 'export_complete']
        require_chain(chain, markers)
        phases['input_preparation'] = {'start_seconds': ep('input_preparation_start'), 'end_seconds': ep('staging_complete'),
                                        'evidence': ev('input_preparation_start', 'staging_complete')}
        phases['input_transfer'] = {'start_seconds': ep('input_preparation_start'), 'end_seconds': ep('staging_complete'),
                                     'evidence': ev('input_preparation_start', 'staging_complete')}
        phases['stage_verification'] = {'start_seconds': ep('staging_complete'), 'end_seconds': ep('staging_complete'),
                                         'evidence': ev('staging_complete')}
        phases['native_invocation'] = {'start_seconds': ep('run_start'), 'end_seconds': ep('run_end'),
                                        'evidence': ev('run_start', 'run_end')}
        phases['checkpoint_export'] = {'start_seconds': ep('run_end'), 'end_seconds': ep('export_complete'),
                                        'evidence': ev('run_end', 'export_complete')}
        phases['output_transfer'] = {'start_seconds': ep('run_end'), 'end_seconds': ep('export_complete'),
                                      'evidence': ev('run_end', 'export_complete')}
        for name, (reason, evidence_pin) in not_applicable_evidence.items():
            phases[name] = {'not_applicable': True, 'reason': reason, 'evidence': [evidence_pin]}
        begin, end = ep('input_preparation_start'), ep('export_complete')
    return phases, begin, end


def assemble_envelope(environment, reference_path, clock_session, values, not_applicable_input,
                      receipts_dir, worker_result_path=None, now=None):
    """Build the envelope dict (not yet written). `values` maps marker name to
    either a receipt path or an ISO-8601 timestamp string; `not_applicable_input`
    maps a phase name in NOT_APPLICABLE_PHASES to its reason (local only)."""
    require(environment in ('local', 'cloud'), 'explicit --environment is required')
    require(isinstance(clock_session, str) and clock_session, 'a non-empty --clock-session id is required')
    now = time.time() if now is None else now
    receipts_dir = Path(receipts_dir); receipts_dir.mkdir(parents=True, exist_ok=True)
    reference_pin = pin(reference_path)
    reference_document = read(reference_path)

    markers = {}

    def resolve(name):
        require(values.get(name), '--' + name.replace('_', '-') + ' is required for environment ' + environment)
        epoch, evidence, document = resolve_marker(values[name], name, receipts_dir)
        markers[name] = (epoch, evidence)
        return document

    not_applicable_evidence = {}
    if environment == 'cloud':
        require(not values.get('input_preparation_start'),
                '--input-preparation-start only applies to environment local')
        require(not not_applicable_input, 'not-applicable phases only apply to environment local')
        for name in ('funding_snapshot', 'volume_create', 'pod_create', 'guard_armed', 'staging_complete'):
            resolve(name)
        resolve('run_start')
        if worker_result_path:
            require(not values.get('run_end'), 'pass either --run-end or --worker-result, not both')
            result_document = read(worker_result_path)
            run_start_epoch, run_start_evidence = markers['run_start']
            elapsed = native_elapsed_seconds(result_document)
            markers['run_end'] = (run_start_epoch + elapsed, run_start_evidence + [pin(worker_result_path)])
        else:
            resolve('run_end')
        resolve('export_complete')
        require(values.get('release_confirmed') and Path(values['release_confirmed']).is_file(),
                '--release-confirmed must be a JSON receipt file showing provider_absent: true '
                '(an independent lookup), not a bare timestamp')
        release_document = resolve('release_confirmed')
        require(release_document.get('provider_absent') is True,
                '--release-confirmed receipt must show provider_absent: true (an independently '
                'confirmed absence lookup, not the guard\'s own detection alone)')
    else:
        for name in CLOUD_ONLY_MARKERS:
            require(not values.get(name), '--' + name.replace('_', '-') + ' only applies to environment cloud')
        require(set(not_applicable_input) <= set(NOT_APPLICABLE_PHASES),
                'unknown not-applicable phase(s): ' + ', '.join(sorted(set(not_applicable_input) - set(NOT_APPLICABLE_PHASES))))
        resolve('input_preparation_start')
        resolve('staging_complete')
        resolve('run_start')
        if worker_result_path:
            require(not values.get('run_end'), 'pass either --run-end or --worker-result, not both')
            result_document = read(worker_result_path)
            run_start_epoch, run_start_evidence = markers['run_start']
            elapsed = native_elapsed_seconds(result_document)
            markers['run_end'] = (run_start_epoch + elapsed, run_start_evidence + [pin(worker_result_path)])
        else:
            resolve('run_end')
        resolve('export_complete')
        for name, reason in not_applicable_input.items():
            not_applicable_evidence[name] = (reason, not_applicable_receipt(name, reason, receipts_dir))

    phases, begin, end = build_phases(environment, markers, not_applicable_evidence)
    envelope = {'schema': TIMING_SCHEMA, 'complete': True, 'environment': environment,
                'reference': reference_pin, 'clock': 'single_controller_monotonic',
                'clock_session': clock_session, 'start_seconds': begin, 'end_seconds': end,
                'phases': phases}
    return envelope, reference_pin, reference_document


def write_envelope(envelope, path):
    write(Path(path), envelope, replace=True)
    return pin(path)


def validate(envelope_pin, reference_pin, reference_document, environment):
    """Run the envelope through throughput.py's own validator; raises
    ValueError on any defect and otherwise returns the validated
    complete-work-seconds duration."""
    return complete_work_seconds(envelope_pin, reference_pin, reference_document, environment)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--environment', required=True, choices=('local', 'cloud'))
    parser.add_argument('--reference', required=True,
        help='path to the phase1-fixed-work-training/v1 reference JSON (reference.py output); '
             'must be the exact same path later passed to throughput.py --local-reference/--cloud-reference')
    parser.add_argument('--clock-session', required=True, help='identifies the single controller clock session')
    parser.add_argument('--funding-snapshot', help='cloud only')
    parser.add_argument('--volume-create', help='cloud only: created-volume.json from prepare_volume.py --execute')
    parser.add_argument('--pod-create', help='cloud only: created-pod.json from prepare_lease.py --execute')
    parser.add_argument('--guard-armed', help='cloud only: a fresh guard.json from lease_guard.py')
    parser.add_argument('--staging-complete', required=True,
        help='receipt or ISO timestamp for cloud_worker.py stage completing (both environments)')
    parser.add_argument('--run-start', required=True,
        help='receipt or ISO timestamp for native invocation start; typically worker-start.json')
    parser.add_argument('--run-end',
        help='receipt or ISO timestamp for native invocation end; omit when using --worker-result')
    parser.add_argument('--worker-result',
        help="cloud_worker.py's worker-result.json; when given, run end is computed as "
             '--run-start plus its native execution elapsed seconds, instead of --run-end')
    parser.add_argument('--export-complete', required=True,
        help='receipt or ISO timestamp for checkpoint export completing; typically recovery.json')
    parser.add_argument('--release-confirmed',
        help='cloud only: a JSON receipt showing provider_absent: true from an independent lookup')
    parser.add_argument('--input-preparation-start', help='local only: start of the measured lifecycle')
    parser.add_argument('--allocation-not-applicable', metavar='REASON', help='local only')
    parser.add_argument('--image-startup-not-applicable', metavar='REASON', help='local only')
    parser.add_argument('--release-confirmation-not-applicable', metavar='REASON', help='local only')
    parser.add_argument('--receipts-dir',
        help='directory for synthesized timestamp/not-applicable receipts; defaults next to --output')
    parser.add_argument('--output', help='envelope output path; required unless --dry-run')
    parser.add_argument('--dry-run', action='store_true',
        help='print and validate the envelope without writing --output or persisting any receipt')
    args = parser.parse_args()
    require(not (args.dry_run and args.output), '--dry-run does not write --output; pass one or the other')
    require(not (args.dry_run and args.receipts_dir), '--dry-run does not persist receipts; omit --receipts-dir')
    require(args.dry_run or args.output, '--output is required unless --dry-run')

    values = {'funding_snapshot': args.funding_snapshot, 'volume_create': args.volume_create,
              'pod_create': args.pod_create, 'guard_armed': args.guard_armed,
              'staging_complete': args.staging_complete, 'run_start': args.run_start,
              'run_end': args.run_end, 'export_complete': args.export_complete,
              'release_confirmed': args.release_confirmed,
              'input_preparation_start': args.input_preparation_start}
    not_applicable = {}
    if args.allocation_not_applicable:
        not_applicable['allocation'] = args.allocation_not_applicable
    if args.image_startup_not_applicable:
        not_applicable['image_startup'] = args.image_startup_not_applicable
    if args.release_confirmation_not_applicable:
        not_applicable['release_confirmation'] = args.release_confirmation_not_applicable

    if args.dry_run:
        with tempfile.TemporaryDirectory() as scratch:
            envelope, reference_pin, reference_document = assemble_envelope(
                args.environment, args.reference, args.clock_session, values, not_applicable,
                Path(scratch) / 'receipts', worker_result_path=args.worker_result)
            envelope_pin = write_envelope(envelope, Path(scratch) / 'envelope.json')
            validate(envelope_pin, reference_pin, reference_document, args.environment)
            print(encoded(envelope).decode())
        return

    receipts_dir = Path(args.receipts_dir) if args.receipts_dir else (
        Path(args.output).parent / (Path(args.output).stem + '.receipts'))
    envelope, reference_pin, reference_document = assemble_envelope(
        args.environment, args.reference, args.clock_session, values, not_applicable,
        receipts_dir, worker_result_path=args.worker_result)
    envelope_pin = write_envelope(envelope, args.output)
    try:
        validate(envelope_pin, reference_pin, reference_document, args.environment)
    except ValueError:
        Path(args.output).unlink(missing_ok=True)
        raise
    print(encoded(envelope).decode())


if __name__ == '__main__':
    main()
