"""Preserve successive opt-in diagnostic captures from one fixed failed match."""
import json
import os
from pathlib import Path
import subprocess
import time
from prepare_postboard_qualification_v1 import read, write, pin


def run():
    pilot = Path('E:/mtg-postboard-campaign-20260920/pilot-001')
    root = pilot / 'diagnostic-v3-multichoice-002'
    root.mkdir()
    config = read(pilot / 'diagnostic-v3-multichoice-001/config.json')
    config['output_directory'] = (root / 'run').as_posix()
    write(root / 'config.json', config)
    capture = root / 'next-capture.json'
    command = [str(pilot / 'learned_sideboard_v1.exe'), '--config', str(root / 'config.json')]
    start = time.monotonic()
    records = []
    with (root / 'stdout.log').open('xb') as out, (root / 'stderr.log').open('xb') as err:
        child = subprocess.Popen(command, stdout=out, stderr=err,
            env=dict(os.environ, MTG_KERNEL_V3_ACTION_ERROR_CAPTURE=str(capture)),
            creationflags=subprocess.CREATE_NO_WINDOW | subprocess.BELOW_NORMAL_PRIORITY_CLASS)
        while True:
            if capture.exists():
                try:
                    data = json.loads(capture.read_bytes())
                    destination = root / f'capture-{len(records):03}.json'
                    capture.rename(destination)
                    records.append({'file': pin(destination), 'actor': data['actor'],
                        'policy_step': data['policy_step'], 'candidate_count': data['candidate_count']})
                except (PermissionError, json.JSONDecodeError):
                    pass
            if child.poll() is not None:
                break
            if time.monotonic()-start > 30:
                child.kill(); child.wait()
                raise RuntimeError('Bounded diagnostic timed out')
            time.sleep(.001)
    write(root / 'receipt.json', {'exit_code': child.returncode, 'captures': records,
        'elapsed_seconds': time.monotonic()-start, 'config': pin(root / 'config.json'),
        'binary': pin(pilot / 'learned_sideboard_v1.exe'), 'script': pin(__file__),
        'measurement': False, 'gameplay_inputs_changed': False})
    print(records)


if __name__ == '__main__':
    run()
