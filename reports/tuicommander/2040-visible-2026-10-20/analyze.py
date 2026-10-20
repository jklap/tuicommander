"""Analyze retained clocks and decode transmitted scalar cells, not byte substrings."""
import base64
import json
import math
import statistics
import struct
from socket_tools import EVIDENCE, save


def stats(values):
    ordered = sorted(values)
    if not ordered:
        return {'n': 0}
    return {'n': len(ordered), 'p50_ms': statistics.median(ordered),
            'p95_ms': ordered[max(0, math.ceil(len(ordered)*.95)-1)], 'max_ms': ordered[-1]}


def arrival_cells(event):
    wire = base64.b64decode(event['binary'])
    count = struct.unpack_from('<H', wire)[0]
    offset = 26
    cells = []
    for _ in range(count):
        row, flagged = struct.unpack_from('<HH', wire, offset)
        offset += 4
        columns = flagged & 0x3fff
        start = 0
        if flagged & 0x4000:
            start = struct.unpack_from('<H', wire, offset)[0]
            offset += 2
        for column in range(columns):
            codepoint = struct.unpack_from('<I', wire, offset)[0]
            cells.append((codepoint, row, column + start))
            offset += 11
    return cells


def analyze(label, backend, recording=None):
    data = json.loads((EVIDENCE / (label + '.json')).read_text())
    observed = json.loads((EVIDENCE / backend).read_text())
    events = json.loads((EVIDENCE / recording).read_text())['events'] if recording else []
    samples = []
    for key in data['keys']:
        character = key['key']
        dispatch = key['dispatch_at']
        echo = observed['echo_seen'].get(character)
        write = next((i for i in data['ipc'] if json.loads(i['body'])['data'] == character), None)
        arrivals = [e for e in events if e['kind'] == 'arrival' and e['at'] >= dispatch and any(cp == ord(character) for cp, _, _ in arrival_cells(e))]
        paints = [e for e in events if e['kind'] == 'paint' and e['at'] >= dispatch and any(ord(character) in struct.unpack('<'+'I'*(len(base64.b64decode(row['codepoints']))//4), base64.b64decode(row['codepoints'])) for row in e['rows'])]
        sample = {'key': character, 'dispatch_ms': dispatch, 'dispatch_return_ms': key['returned_at']-dispatch,
            'echo_observation_upper_ms': echo['observed_wall_ms']-data['wall_time_origin_ms']-dispatch if echo else None,
            'ipc_start_ms': write['at']-dispatch if write else None,
            'ipc_response_ms': write.get('completed_at', write['at'])-dispatch if write else None,
            'first_arrival_ms': arrivals[0]['at']-dispatch if arrivals else None,
            'first_paint_state_ms': paints[0]['at']-dispatch if paints else None,
            'fillText_ms': key.get('fillText_ms'), 'two_raf_ms': key.get('two_raf_ms')}
        samples.append(sample)
    return {'window': data['windowState'], 'samples': samples,
        'missing_backend_echoes': [s['key'] for s in samples if s['echo_observation_upper_ms'] is None],
        'not_in_transmitted_cells': [s['key'] for s in samples if s['first_arrival_ms'] is None] if recording else None,
        'no_matching_fillText': [s['key'] for s in samples if s['fillText_ms'] is None],
        'dispatch_to_echo_observation_upper': stats([s['echo_observation_upper_ms'] for s in samples if s['echo_observation_upper_ms'] is not None]),
        'dispatch_to_ipc_response': stats([s['ipc_response_ms'] for s in samples if s['ipc_response_ms'] is not None]),
        'dispatch_to_arrival': stats([s['first_arrival_ms'] for s in samples if s['first_arrival_ms'] is not None]),
        'dispatch_to_fillText': stats([s['fillText_ms'] for s in samples if s['fillText_ms'] is not None]),
        'arrival_to_fillText': stats([s['fillText_ms']-s['first_arrival_ms'] for s in samples if s['fillText_ms'] is not None and s['first_arrival_ms'] is not None]),
        'raf_gaps': stats([e['gap_ms'] for e in data['rafs']]),
        'timer_gaps': stats([e['gap_ms'] for e in data['timers']]),
        'socket_request_duration': stats([e['request_ms'] for e in observed['observations'] if 'request_ms' in e])}


summary = {'initial_visible': analyze('visible-loaded', 'backend.json'),
    'layered_visible': analyze('visible-layered-loaded', 'visible-layered-backend.json', 'visible-layered-frame-workload.json')}
plain = json.loads((EVIDENCE / 'plain-help.json').read_text())
tuic = json.loads((EVIDENCE / 'tuic-help.json').read_text())
summary['help'] = {'plain_ms': plain['help_ms'], 'tuic_ms': tuic['help_ms'], 'ratio': tuic['help_ms']/plain['help_ms'], 'within_2x': tuic['help_ms'] <= plain['help_ms']*2, 'claim': 'nested PTY submit to final Ctrl+C help row, not canvas presentation'}
save('summary.json', summary)
print(json.dumps(summary, indent=2))
