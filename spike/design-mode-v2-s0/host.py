#!/usr/bin/python3
"""Native-endian uint32 JSON echo. Delayed reply measures an idle native port.

Echo only: deliberately does not discover or connect to any TUIC socket.
"""
import json
import struct
import sys
import time


def read_exact(size):
    result = b''
    while len(result) < size:
        chunk = sys.stdin.buffer.read(size - len(result))
        if not chunk:
            if result:
                raise EOFError('Truncated native frame')
            return None
        result += chunk
    return result


def send(value):
    payload = json.dumps(value).encode()
    sys.stdout.buffer.write(struct.pack('=I', len(payload)) + payload)
    sys.stdout.buffer.flush()


while True:
    header = read_exact(4)
    if header is None:
        break
    length = struct.unpack('=I', header)[0]
    if length > 1024 * 1024:
        raise ValueError('Spike frame exceeds 1 MiB')
    payload = read_exact(length)
    if payload is None:
        raise EOFError('Missing native payload')
    message = json.loads(payload)
    send({'echo': message, 'origin': sys.argv[1] if len(sys.argv) > 1 else None})
    if message.get('delayed'):
        time.sleep(75)
        send({'delayed': True, 'nonce': message['nonce'], 'idleSeconds': 75})
