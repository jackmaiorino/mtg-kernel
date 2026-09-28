"""Reservation entry for the shared g115 Windows transport; native guards stay in the owner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import sys
import host_reservation_v1 as reservation


def check_owner():
    token = os.environ.get(reservation.TOKEN_ENV)
    record, _ = reservation.read_lock()
    if not token or not record or record.get('token') != token:
        raise RuntimeError('RunConfig requires the current host reservation token')
    return token


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check-owner', action='store_true')
    parser.add_argument('--config')
    args = parser.parse_args()
    if args.check_owner:
        print(check_owner())
        return
    config_path = Path(args.config).resolve(strict=True)
    config = json.loads(config_path.read_text(encoding='utf-8-sig'))
    transport = Path(__file__).with_name('g115_d3_windows_dispatch.ps1')
    if hashlib.sha256(transport.read_bytes()).hexdigest() != config['transport_sha256']:
        raise RuntimeError('Transport hash differs')
    for path, key in [(Path(__file__), 'reservation_adapter_sha256'),
                      (Path(reservation.__file__), 'reservation_helper_sha256')]:
        if hashlib.sha256(path.read_bytes()).hexdigest() != config[key]:
            raise RuntimeError('Reservation dependency hash differs: ' + key)
    command = [str(Path(os.environ['SystemRoot'])/'System32/WindowsPowerShell/v1.0/powershell.exe'),
               '-NoProfile', '-NonInteractive', '-WindowStyle', 'Hidden', '-File', str(transport),
               '-RunConfig', str(config_path)]
    try:
        result = reservation.dispatch('codex-g115', config_path.parent.name,
            'release after transport owner and all descendants exit', command,
            cwd=str(config_path.parent), python=sys.executable,
            busy_pattern=r'^(cargo|rustc|public_.*|native_.*|expanded_deck_.*|learned_sideboard_v1|trainer|mtg_kernel.*|wsl)\.exe$',
            transport_record={'config_sha256':hashlib.sha256(config_path.read_bytes()).hexdigest()})
    except Exception as error:
        result = {'state': 'refused', 'error': str(error), 'error_type': type(error).__name__}
        (config_path.parent/'dispatch.json').write_text(json.dumps(result, indent=2)+'\n')
        raise
    (config_path.parent/'dispatch.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result))
    if result['state'] not in ('dispatched', 'finished-before-handoff'):
        raise RuntimeError('Dispatch not confirmed; inspect receipt, never retry these roots')


if __name__ == '__main__':
    main()
