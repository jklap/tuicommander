"""Same nested 40x120 PTY on both paths; final-help-byte timing, not paint."""
import base64
import fcntl
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import struct
import sys
import termios
import time
import tty

HERE = Path(__file__).resolve().parent
MODE = sys.argv[1]
old = termios.tcgetattr(0) if MODE == 'tuic' else None
master, slave = pty.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 40, 120, 0, 0))
pid = os.fork()
if pid == 0:
    os.close(master)
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    for descriptor in (0, 1, 2):
        os.dup2(slave, descriptor)
    if slave > 2:
        os.close(slave)
    environment = {k: v for k, v in os.environ.items() if not k.startswith('TUIC_')}
    environment.update(EGO_HOME=str(Path.home() / ('Gits/.tmp/desk-2040d-ego-' + MODE)), TERM='xterm-256color', PAGER='cat')
    os.chdir(HERE)
    os.execvpe('/Users/stefano.straus/.local/bin/ego', ['ego', '--bare', '--offline', '--thinking', 'off', '-m', 'ollama/gemma4:e4b-mlx'], environment)
os.close(slave)
os.set_blocking(master, False)
(HERE / (MODE + '-help-pid.txt')).write_text(str(pid))
start = last = time.monotonic()
submitted = finished = None
ready = False
blob = b''
events = []
try:
    if old:
        tty.setraw(0)
    while time.monotonic() - start < 30:
        now = time.monotonic()
        if ready and submitted is None and now - last > .3:
            submitted = now
            os.write(master, b'/help\r')
        descriptors = [master, 0] if MODE == 'tuic' else [master]
        for descriptor in select.select(descriptors, [], [], .01)[0]:
            try:
                data = os.read(descriptor, 65536)
            except OSError:
                data = b''
            if not data:
                raise EOFError()
            if descriptor == 0:
                os.write(master, data)
                continue
            last = time.monotonic()
            events.append({'ms': (last-start)*1000, 'data': base64.b64encode(data).decode()})
            (HERE / (MODE + '-help-progress.json')).write_text(json.dumps(events))
            ready = ready or b'\x1b[?25h' in data
            blob += data
            if MODE == 'tuic':
                os.write(1, data)
            else:
                for _ in re.finditer(rb'\x1b\[6n', data):
                    os.write(master, b'\x1b[1;1R')
                for number in (10, 11):
                    if f'\x1b]{number};?'.encode() in data:
                        os.write(master, f'\x1b]{number};rgb:0000/0000/0000\x1b\\'.encode())
            if submitted and b'twice in a row, exit' in blob:
                finished = last
                break
        if finished:
            break
except EOFError:
    pass
finally:
    if old:
        termios.tcsetattr(0, termios.TCSADRAIN, old)
    (HERE / (MODE + '-help-events.json')).write_text(json.dumps(events))
    (HERE / (MODE + '-help.json')).write_text(json.dumps({'mode': MODE, 'ego': '0.1.2', 'nested_rows': 40, 'nested_columns': 120, 'submit_ms': (submitted-start)*1000 if submitted else None, 'final_ms': (finished-start)*1000 if finished else None, 'help_ms': (finished-submitted)*1000 if submitted and finished else None, 'complete': bool(finished)}, indent=2))

    os.close(master)
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
