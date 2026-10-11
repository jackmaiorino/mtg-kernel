"""Read-only one-shot fleet capture for this implementation qualification."""
import concurrent.futures
import datetime
import importlib.util
import json
from pathlib import Path


def main():
    root = Path(__file__).resolve().parents[1]
    source = root / 'search_throughput_20261010/capture_inventory.py'
    spec = importlib.util.spec_from_file_location('search_audit_inventory', source)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    result = {'schema': 'search-buffer-reuse-fleet-inventory/v1',
              'observed_utc': datetime.datetime.now(datetime.timezone.utc).isoformat()}
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        jobs = {'jacks_pc': pool.submit(module.capture_host, False),
                'haleys_pc': pool.submit(module.capture_host, True),
                'runpod': pool.submit(module.cloud)}
        for name, job in jobs.items():
            try:
                result[name] = job.result()
            except Exception as exc:
                result[name] = {'capture_failed': True, 'error': type(exc).__name__}
                if isinstance(getattr(exc, 'code', None), int):
                    result[name]['http_status'] = exc.code
    output = Path(__file__).with_name('inventory.json')
    output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    compact = {}
    for name in ('jacks_pc', 'haleys_pc'):
        host = result[name]
        compact[name] = {'cpu': host.get('cpu'), 'memory': host.get('memory'),
                         'volumes': host.get('volumes'), 'slots': host.get('slots'),
                         'capture_failed': host.get('capture_failed', False)}
    compact['runpod'] = result['runpod']
    print(json.dumps(compact, indent=2))


if __name__ == '__main__':
    main()
