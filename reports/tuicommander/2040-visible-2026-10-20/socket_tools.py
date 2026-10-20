"""Instance-scoped Unix-socket observer. No credentials or TCP API access."""
import json
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
EVIDENCE = Path(__file__).resolve().parent
SOCKET = '/Users/stefano.straus/Gits/.tmp/tuic-at/d943b7d93347aefd/tuic-mcp-d943b7d93347aefd.sock'


def api(path, body=None, method=None):
    command = ['curl', '--max-time', '10', '-fsS', '--unix-socket', SOCKET,
               '-H', 'content-type: application/json']
    if method:
        command += ['-X', method]
    if body is not None:
        command += ['--data-binary', json.dumps(body)]
    return json.loads(subprocess.check_output(command + ['http://localhost' + path], text=True, stderr=subprocess.PIPE))


def save(name, data):
    (EVIDENCE / name).write_text(json.dumps(data, indent=2))


def evaluate(script, name):
    rows = api('/logs?source=eval_js&limit=1')
    previous = rows[0]['id'] if rows else -1
    receipt = api('/debug/invoke_js', {'script': script})
    if not receipt.get('ok'):
        raise RuntimeError(receipt)
    deadline = time.monotonic() + 60
    while time.monotonic() < deadline:
        rows = api('/logs?source=eval_js&limit=1')
        if rows and rows[0]['id'] != previous:
            save(name + '-eval.json', rows[0])
            result = json.loads(rows[0]['message'].split('\n---\n')[-1])
            save(name + '.json', result)
            return result
        time.sleep(.05)
    raise RuntimeError('WebView evaluation exceeded 60 seconds')
