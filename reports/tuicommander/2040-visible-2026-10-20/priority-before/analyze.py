"""Correlate monotonic native stages and frontend frame identities, not socket polling."""
import base64
from collections import Counter
import gzip
import json
import math
from pathlib import Path

HERE = Path(__file__).resolve().parent


def fnv(data):
    value = 2166136261
    for byte in data:
        value = ((value ^ byte) * 16777619) & 0xffffffff
    return value


def distribution(values):
    values = sorted(values)
    return None if not values else {
        'count': len(values), 'p50': values[math.ceil(len(values) * .5) - 1],
        'p95': values[math.ceil(len(values) * .95) - 1], 'max': values[-1]}


log = (HERE / 'native.log')
text = log.read_text() if log.exists() else gzip.decompress((HERE / 'native.log.gz').read_bytes()).decode()
events = [json.loads(line.split('TUIC2040 ', 1)[1]) for line in text.splitlines() if 'TUIC2040 ' in line]
loaded = json.loads((HERE / 'loaded.json').read_text())
events = [event for event in events if event['session'] == loaded['session']]
for sequence, event in enumerate(events):
    event['sequence'] = sequence
arrivals = []
recording = json.loads((HERE / 'frame-workload.json').read_text())
for event in recording['events']:
    if event['kind'] == 'arrival' and event['session'] == loaded['session']:
        data = base64.b64decode(event['binary'])
        arrivals.append({**event, 'len': len(data), 'fnv1a32': fnv(data)})
rows = []
for sample in loaded['keys']:
    key = sample['key']
    def first(kind, predicate=lambda event: True):
        return next((event for event in events if event['event'] == kind and predicate(event)), None)
    written = first('input-write-begin', lambda event: key in event['details']['keys'])
    end = first('input-write-end', lambda event: key in event['details']['keys'])
    read = first('pty-read', lambda event: key in event['details']['bytes']['keys'])
    begin = first('vt-begin', lambda event: key in event['details']['processed_bytes']['keys'])
    applied = first('vt-applied', lambda event: key in event['details']['processed_bytes']['keys'])
    cut = first('tick-decision', lambda event: 'frame_bytes' in event['details'] and key in event['details']['frame_bytes']['keys'])
    sent = first('desktop-submit', lambda event: key in event['details']['bytes']['keys'])
    relevant = [event for event in events if applied and cut and applied['mono_us'] <= event['mono_us'] <= cut['mono_us'] and event['event'] == 'tick-decision']
    floor = [event for event in relevant if event['details'].get('reason') == 'rate-floor']
    received = next((event for event in arrivals if sent and event['at'] >= sample['dispatch_at'] and all(event[field] == sent['details']['bytes'][field] for field in ('len', 'fnv1a32'))), None)
    row = {'key': key, 'fillText_ms': sample.get('fillText_ms'),
           'native': {label: event for label, event in [('write_begin', written), ('write_end', end), ('read', read), ('vt_begin', begin), ('vt_applied', applied), ('cut', cut), ('send', sent)]},
           'decisions_after_vt_before_matching_cut': relevant,
           'arrival_at': received['at'] if received else None}
    def elapsed(start, finish):
        return (finish['mono_us'] - start['mono_us']) / 1000 if start and finish else None
    row['write_to_vt_ms'] = elapsed(written, applied)
    row['vt_processing_ms'] = elapsed(begin, applied)
    row['vt_to_cut_ms'] = elapsed(applied, cut)
    row['vt_to_send_ms'] = elapsed(applied, sent)
    row['floor_deferral_count'] = len(floor)
    row['first_floor_decision_to_cut_ms'] = elapsed(floor[0] if floor else None, cut)
    row['gate_deferral_count'] = sum(event['details'].get('reason', '').startswith('gate-') for event in relevant)
    row['cut_to_send_ms'] = elapsed(cut, sent)
    row['dispatch_to_arrival_ms'] = received['at'] - sample['dispatch_at'] if received else None
    row['arrival_to_fillText_ms'] = sample['paint_at'] - received['at'] if received and 'paint_at' in sample else None
    # Both clocks are monotonic. The native epoch anchor bridges their domains;
    # it is sampled once, so this cross-domain estimate is not a PTY-read clock.
    row['send_to_arrival_estimate_ms'] = loaded['wall_time_origin_ms'] + received['at'] - (sent['anchor_epoch_us'] + sent['mono_us']) / 1000 if received and sent else None
    rows.append(row)
metrics = ('fillText_ms', 'write_to_vt_ms', 'vt_processing_ms', 'vt_to_cut_ms', 'first_floor_decision_to_cut_ms', 'cut_to_send_ms', 'dispatch_to_arrival_ms', 'arrival_to_fillText_ms', 'send_to_arrival_estimate_ms')
backend = json.loads((HERE / 'backend.json').read_text())
observations = [row for row in backend['observations'] if row.get('total_written') is not None]
throughput = None
if len(observations) > 1:
    first, last = observations[0], observations[-1]
    throughput = (last['total_written'] - first['total_written']) / ((last['wall_ms'] - first['wall_ms']) / 1000)
summary = {'session': loaded['session'], 'window': loaded['windowState'], 'recorder_dropped': recording['dropped'],
           'event_counts': dict(Counter(event['event'] for event in events)),
           'ticker_decisions': dict(Counter(event['details']['reason'] for event in events if event['event'] == 'tick-decision')),
           'metrics_ms': {metric: distribution([row[metric] for row in rows if row[metric] is not None]) for metric in metrics},
           'backend_echo_count': len(backend['echo_seen']), 'output_bytes_per_second_observed': throughput,
           'raf_gap_ms': distribution([row['gap_ms'] for row in loaded['rafs']]),
           'timer_gap_ms': distribution([row['gap_ms'] for row in loaded['timers']]), 'samples': rows}
(HERE / 'summary.json').write_text(json.dumps(summary, indent=2))
(HERE / 'native-events.json').write_text(json.dumps(events, indent=2))
print(json.dumps({key: value for key, value in summary.items() if key != 'samples'}, indent=2))
