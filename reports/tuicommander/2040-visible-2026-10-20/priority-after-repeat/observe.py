"""Repeat the 15-key synthetic output measurement with per-write IPC observations."""
import json
import subprocess
import os
import threading
import time
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from socket_tools import ROOT, EVIDENCE, SOCKET, api, evaluate, save

TRACE = os.environ.get('TUIC_2040_TRACE_DIR', 'trace')
ids = []
observations = []
echo_seen = {}
stop = threading.Event()


def monitor():
    while not stop.is_set():
        before = time.time() * 1000
        try:
            response = api('/sessions/' + ids[0] + '/output?limit=2097152')
            after = time.time() * 1000
            for i in range(15):
                key = chr(0x2460 + i)
                if key in response.get('data', '') and key not in echo_seen:
                    echo_seen[key] = {'observed_wall_ms': after, 'request_start_wall_ms': before}
            observations.append({'wall_ms': after, 'total_written': response.get('total_written'), 'request_ms': after-before})
        except Exception as error:
            observations.append({'error': str(error)})
        stop.wait(.02)


try:
    save(f'{TRACE}/health.json', api('/health'))
    if api('/sessions'):
        raise RuntimeError('Selected instance is not empty')
    response = api('/sessions', {'rows': 40, 'cols': 120,
        'shell': f'/usr/bin/python3 {ROOT}/scripts/stall-workload/output.py', 'cwd': str(EVIDENCE)})
    ids.append(response['session_id'])
    watch = threading.Thread(target=monitor)
    watch.start()
    source = 'window.__desk2040Config=' + json.dumps({'session': ids[0], 'samples': 15}) + ';\n' + (EVIDENCE / 'layered.js').read_text()
    evaluate(source, f'{TRACE}/loaded')
    save(f'{TRACE}/output.json', api('/sessions/' + ids[0] + '/output?limit=2097152'))
    with (EVIDENCE / TRACE / 'raw-ring.bin').open('wb') as raw:
        subprocess.run(['curl', '--max-time', '10', '-fsS', '--unix-socket', SOCKET, 'http://localhost/sessions/' + ids[0] + '/raw-ring'], stdout=raw, check=True)
finally:
    stop.set()
    if 'watch' in locals():
        watch.join()
    save(f'{TRACE}/backend.json', {'echo_seen': echo_seen, 'observations': observations})
    for session_id in ids:
        api('/sessions/' + session_id, method='DELETE')
    save(f'{TRACE}/sessions-after.json', api('/sessions'))
