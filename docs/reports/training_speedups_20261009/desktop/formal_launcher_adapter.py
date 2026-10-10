"""Declare desktop P cores inside the frozen dispatcher's own supervisor.

Formal and qualification actions only. Frozen launcher files stay unchanged. Acquisition, busy
refusal, storage admission and the same canonical supervisor remain owned by
the supported native dispatcher. Never invokes a native executable directly.
"""
import argparse
import importlib
import json
from pathlib import Path
import sys


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--launcher-root', type=Path, required=True)
    parser.add_argument('--cores', required=True)
    parser.add_argument('action', choices=['qualify', 'dispatch'])
    parser.add_argument('request', type=Path)
    args = parser.parse_args()
    tools = args.launcher_root / 'python/tools'
    sys.path.insert(0, str(tools))
    reservations = importlib.import_module('host_reservation_v1')
    slots = importlib.import_module('host_slots_v1')
    dispatch = importlib.import_module('native_expanded_dispatch_v1')
    request = json.loads(args.request.read_bytes())
    cores = slots.parse_cores(args.cores)
    if request['placement']['host'] != 'desktop' or request['placement']['cpu_affinity'] != cores:
        raise ValueError('declared cores must match the frozen qualification placement')
    original_dispatch = reservations.dispatch
    def declared_dispatch(*arguments, **keywords):
        command = keywords['command']
        if len(command) < 4 or Path(command[2]).resolve() != (tools / 'native_expanded_dispatch_v1.py').resolve() or command[3] != '_' + args.action:
            raise ValueError('only the supported qualification child may be declared')
        keywords['command'] = [sys.executable, '-B', str(tools / 'host_slots_v1.py'),
                               'timed', '--cores', args.cores, '--', *command]
        return original_dispatch(*arguments, **keywords)
    reservations.dispatch = declared_dispatch
    sys.argv = [str(tools / 'native_expanded_dispatch_v1.py'), args.action, str(args.request)]
    dispatch.main()


if __name__ == '__main__':
    main()
