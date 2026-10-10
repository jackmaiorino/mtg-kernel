"""One-time eight-case desktop qualification coordinator; no automatic retries.

Run only after parent review. Each case uses the supported frozen dispatcher,
standalone canonical reservation and explicit shared-core declaration adapter.
No full block, native executable invocation, deletion or raw acquisition.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
from datetime import datetime, timezone

ROOT = Path(__file__).resolve().parent


def pin(path):
    with Path(path).open('rb') as stream:
        digest = hashlib.file_digest(stream, 'sha256').hexdigest()
    return {'path': str(path), 'sha256': digest}


def require(ok, message):
    if not ok:
        raise ValueError(message)


def main():
    require(os.name == 'nt' and platform.node().upper() == 'DESKTOP-DJ1C40R'
            and sys.version_info[:3] == (3, 13, 14), 'pinned desktop Python 3.13.14 required')
    plan_path = ROOT / 'qualification.plan.json'
    plan = json.loads(plan_path.read_bytes())
    require(plan['staging_only'] is True and len(plan['commands']) == 8, 'reviewed eight-case plan required')
    for key in ('controller', 'adapter', 'coordinator'):
        require(pin(plan[key]['path']) == plan[key], 'reviewed source changed: ' + key)
    for entry in plan['input_dependencies']:
        require(pin(entry['original']['path']) == entry['original'], 'read-only native source changed')
    with (ROOT / 'qualification.once').open('x', encoding='utf-8') as marker:
        marker.write(str(os.getpid()) + '\n')
    completed, active = [], None
    receipt = {'schema': 'training-speedup-desktop-qualification-batch/v1', 'complete': False,
               'plan': pin(plan_path), 'started_utc': datetime.now(timezone.utc).isoformat()}
    try:
        for entry in plan['commands']:
            active = entry['label']
            require(pin(entry['request']['path']) == entry['request'], 'qualification request changed')
            argv = entry['argv']
            require(Path(argv[0]).resolve() == Path(sys.executable).resolve()
                    and '--declare-desktop-cores' in argv, 'pinned interpreter/declaration changed')
            log_path = ROOT / 'controllers' / (active + '.log')
            with log_path.open('x', encoding='utf-8') as log:
                result = subprocess.run(argv, stdout=log, stderr=subprocess.STDOUT)
            require(result.returncode == 0, 'case failed; preserve attempt, no retry: ' + active)
            result_path = Path(argv[argv.index('--receipt') + 1])
            document = json.loads(result_path.read_bytes())
            require(document['complete'] is True and document['request'] == entry['request'], 'case completion differs')
            completed.append({'label': active, 'controller_receipt': pin(result_path)})
        receipt['complete'] = True
        return 0
    except Exception as error:
        receipt['error'] = f'{type(error).__name__}: {error}'
        return 2
    finally:
        receipt.update(completed=completed, active_case=active, finished_utc=datetime.now(timezone.utc).isoformat())
        with (ROOT / 'qualification.batch.json').open('x', encoding='utf-8') as stream:
            json.dump(receipt, stream, indent=2, allow_nan=False)
            stream.write('\n')
        print(json.dumps(receipt))


if __name__ == '__main__':
    sys.exit(main())
