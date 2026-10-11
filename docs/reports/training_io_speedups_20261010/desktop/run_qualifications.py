"""Run the frozen eight-case qualification plan through its guarded driver."""
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parent


def pin(path):
    with Path(path).open('rb') as stream:
        return {'path': str(path), 'sha256': hashlib.file_digest(stream, 'sha256').hexdigest()}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    plan_path = ROOT / 'qualification.plan.json'
    plan = json.loads(plan_path.read_bytes())
    require(plan['stage'] == 'qualify' and len(plan['commands']) == 8, 'eight staged qualifications required')
    require(pin(sys.executable) == plan['python_pin'], 'pinned Python required')
    sources = list(plan['helpers'].values()) + [plan['helper'], plan['source_config'], plan['runtime_decks']]
    sources += [ref for refs in plan['tools'].values() for ref in refs]
    sources += list(plan['runtimes'].values()) + [row['original'] for row in plan['input_dependencies']]
    sources += plan['supporting_helpers']
    for ref in sources:
        require(pin(ref['path']) == ref, 'frozen source changed: ' + ref['path'])
    out = ROOT / 'qualification-coordinator'
    out.mkdir(exist_ok=False)
    state = {'complete': False, 'plan': pin(plan_path), 'started_utc': datetime.now(timezone.utc).isoformat(),
             'completed': [], 'queue_waits': []}

    def save():
        temporary = out / 'state.pending'
        with temporary.open('w', encoding='utf-8') as stream:
            json.dump(state, stream, indent=2); stream.flush(); os.fsync(stream.fileno())
        os.replace(temporary, out / 'state.json')

    import wait_canonical_free
    save()
    try:
        canonical = None
        for entry in plan['commands']:
            label = entry['label']
            state['active_case'] = label
            def changed(value):
                state['waiting'] = value; save()
            wait = wait_canonical_free.wait_free(changed)
            state['queue_waits'].append({'case': label, **wait}); state.pop('waiting', None); save()
            for ref in sources + [entry['request']]:
                require(pin(ref['path']) == ref, 'frozen source changed: ' + ref['path'])
            argv = entry['argv']
            require(argv[0] == str(sys.executable) and argv[argv.index('--action') + 1] == 'qualify',
                    'guarded qualification command differs')
            with (out / (label + '.log')).open('xb') as log:
                result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT)
            require(result.returncode == 0, 'qualification failed; preserve attempt: ' + label)
            controller_path = ROOT / 'controllers' / (label + '.json')
            controller = json.loads(controller_path.read_bytes())
            require(controller['complete'] and controller['request'] == entry['request'], 'incomplete controller')
            report_ref = controller['report']
            require(pin(report_ref['path']) == report_ref, 'report changed')
            report = json.loads(Path(report_ref['path']).read_bytes())
            require(report['complete'] and report['completed_updates'] == 1 and report['completed_games'] == 10,
                    'incomplete initial update')
            if canonical is None:
                canonical = report['fingerprint']
            else:
                require(report['fingerprint'] == canonical, 'qualification output parity differs')
            state['completed'].append({'label': label, 'controller': pin(controller_path), 'report': report_ref})
            save()
        state['complete'] = True
    except BaseException as error:
        state['error'] = type(error).__name__ + ': ' + str(error)
        raise
    finally:
        state['finished_utc'] = datetime.now(timezone.utc).isoformat(); save()


if __name__ == '__main__':
    main()
