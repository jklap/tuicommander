"""Owned agent-test lifecycle: native build, socket-only checks, exact stop."""
import json
import os
import shutil
import subprocess
import sys
import time
from socket_tools import ROOT, EVIDENCE, SOCKET, api, evaluate, save

variables = ['AGENT_TEST_INSTANCE=at-2040c', 'AGENT_TEST_PORT=19891']
launch = json.loads(subprocess.check_output(['make', '--no-print-directory', 'agent-test', 'AGENT_TEST_DRY_RUN=1', *variables], cwd=ROOT, text=True))
save('launch.json', launch)
if os.path.exists(launch['pid_file']):
    raise RuntimeError('Instance already owned; refusing to launch')
freeze = os.path.expanduser('~/Gits/.tmp/BUILD_FREEZE')
if os.path.exists(freeze):
    raise RuntimeError('BUILD_FREEZE present; do not launch')
child = None
cleanup = {}
try:
    with (EVIDENCE / 'repeat-launch.log').open('w') as log:
        child = subprocess.Popen(['make', 'agent-test', *variables], cwd=ROOT, env={**os.environ, 'TMPDIR': os.path.expanduser('~/Gits/.tmp/')}, stdout=log, stderr=subprocess.STDOUT)
        save('supervisor.json', {'pid': child.pid})
        deadline = time.monotonic() + 3600
        while time.monotonic() < deadline:
            if child.poll() is not None:
                raise RuntimeError('Launch exited: ' + str(child.returncode))
            if os.path.exists(SOCKET):
                try:
                    api('/health')
                    break
                except subprocess.CalledProcessError:
                    pass
            time.sleep(.2)
        else:
            raise RuntimeError('Setup budget exceeded')
        for _ in range(100):
            try:
                ready = evaluate('return Boolean(window.__TUIC__ && document.querySelector(\'button[title^="New Tab"]\'));', 'repeat-ready')
                if ready:
                    break
            except (RuntimeError, ValueError):
                pass
            time.sleep(.2)
        if api('/sessions'):
            for row in api('/sessions'):
                api('/sessions/' + row['session_id'], method='DELETE')
        subprocess.run([sys.executable, str(EVIDENCE / 'run_layered.py')], check=True, timeout=55)
        measurement = json.loads((EVIDENCE / 'visible-layered-loaded.json').read_text())
        shutil.copyfile(measurement['recording'], EVIDENCE / 'visible-layered-frame-workload.json')
        if not measurement['windowState']['visible'] or measurement['windowState']['visibility'] != 'visible':
            raise RuntimeError('Repeat was not visible; do not claim paint timing')
        session = api('/sessions', {'rows': 40, 'cols': 120, 'shell': '/usr/bin/python3 ' + str(EVIDENCE / 'help_probe.py') + ' tuic', 'cwd': str(EVIDENCE)})['session_id']
        evaluate('''const delay=ms=>new Promise(r=>setTimeout(r,ms));
const session=SESSION;for(let n=0;n<100;n++){const terminal=window.__TUIC__.terminals().find(t=>t.sessionId===session);if(terminal){document.querySelector(`[data-tab-id="${terminal.id}"]`).click();break;}await delay(50);}
return {visibility:document.visibilityState,visible:await window.__TAURI__.window.getCurrentWindow().isVisible()};'''.replace('SESSION', json.dumps(session)), 'help-window')
        deadline = time.monotonic() + 35
        while time.monotonic() < deadline and not (EVIDENCE / 'tuic-help.json').exists():
            time.sleep(.1)
        if not (EVIDENCE / 'tuic-help.json').exists():
            raise RuntimeError('TUIC help probe did not finish within harness budget')
        save('tuic-help-output.json', api('/sessions/' + session + '/output?limit=2097152'))
        if any(row['session_id'] == session for row in api('/sessions')):
            api('/sessions/' + session, method='DELETE')
        else:
            cleanup['help_session_already_exited'] = True
finally:
    if child is not None:
        try:
            for row in api('/sessions'):
                api('/sessions/' + row['session_id'], method='DELETE')
            cleanup['sessions_remaining'] = api('/sessions')
        except Exception as error:
            cleanup['session_cleanup_error'] = str(error)
        result = subprocess.run(['make', 'agent-test-stop', *variables], cwd=ROOT, capture_output=True, text=True)
        cleanup.update(stop_exit=result.returncode, stop_stdout=result.stdout, stop_stderr=result.stderr)
        try:
            cleanup['supervisor_exit'] = child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            child.terminate()
            cleanup['supervisor_exit'] = child.wait(timeout=10)
        ps = subprocess.check_output(['ps', '-axo', 'pid=,comm='], text=True)
        cleanup['worktree_processes_remaining'] = [line for line in ps.splitlines() if launch['binary'] in line]
        save('cleanup.json', cleanup)
