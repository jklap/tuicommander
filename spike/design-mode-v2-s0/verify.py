"""Focused framing boundary check: fragmentation, Unicode, repeated frames, bad size."""
import json
from pathlib import Path
import struct
import subprocess
import sys

host = Path(__file__).with_name('host.py')
messages = [{'hello': 'é界'}, {'list': []}]
wire = b''.join(struct.pack('=I', len(b)) + b for b in (json.dumps(m, ensure_ascii=False).encode() for m in messages))
process = subprocess.Popen([sys.executable, str(host), 'chrome-extension://test/'], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
# Splitting input writes catches the common short-read framing bug.
for byte in wire:
    process.stdin.write(bytes([byte]))
    process.stdin.flush()
process.stdin.close()
output = process.stdout.read()
assert process.wait(timeout=5) == 0
for expected in messages:
    size = struct.unpack('=I', output[:4])[0]
    response = json.loads(output[4:4 + size])
    assert response == {'echo': expected, 'origin': 'chrome-extension://test/'}
    output = output[4 + size:]
assert not output
for bad in [struct.pack('=I', 1024 * 1024 + 1), b'\x01\x00', struct.pack('=I', 5) + b'{}']:
    result = subprocess.run([sys.executable, str(host)], input=bad, capture_output=True, timeout=5)
    assert result.returncode != 0, 'Malformed frame must fail rather than silently succeed'
print('PASS: fragmented/multiple Unicode echo frames; oversized/truncated input rejected')
