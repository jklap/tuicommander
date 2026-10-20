"""Correlate real PTY intake, VT snapshots, ticker choices and native delivery."""
import base64
import gzip
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct

HERE = Path(__file__).resolve().parent


def cells(wire):
    if not wire:
        return []
    count = struct.unpack_from('<H', wire)[0]
    offset = 26
    result = []
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
            result.append((codepoint, row, column + start))
            offset += 11
    return result


def locations(state, key):
    return [{'row': index, 'column': row.index(key), 'absolute_row': state['origin']+index}
            for index, row in enumerate(state['screen']) if key in row]


def tail_locations(event, key):
    details = event['details']
    return [{'absolute_row': details['tail_start']+index+details['state']['origin']-details['state']['history'], 'column': row.index(key)}
            for index, row in enumerate(details.get('tail', [])) if key in row]


native = []
for line in gzip.decompress((HERE / 'native.log.gz').read_bytes()).decode().splitlines():
    if 'TUIC2040 ' in line:
        native.append(json.loads(line.split('TUIC2040 ', 1)[1]))
# Preserve native event order as well as the observed epoch timestamps.
for index, event in enumerate(native):
    event['log_sequence'] = index
loaded = json.loads((HERE / 'loaded.json').read_text())
session = loaded['session']
events = sorted([e for e in native if e['session'] == session], key=lambda e: (e['at_us'], e['log_sequence']))
vt_events = [e for e in events if e['event'] == 'vt-applied']
cuts = [e for e in events if e['event'] == 'tick-decision' and e['details']['decision'] == 'cut']
delivered = [e for e in events if e['event'] == 'desktop-submit']
recording = json.loads((HERE / 'frame-workload.json').read_text())
arrivals = [e for e in recording['events'] if e['kind'] == 'arrival' and e['session'] == session]
arrived_hashes = Counter(hashlib.sha256(base64.b64decode(e['binary'])).hexdigest() for e in arrivals)
submitted_hashes = Counter(hashlib.sha256(bytes(e['details']['bytes'])).hexdigest() for e in delivered)
timeline = []
for event in events:
    details = event['details']
    row = {k: event[k] for k in ('log_sequence', 'at_us', 'event')}
    row['decision'] = {k: v for k, v in details.items() if k not in ('bytes', 'frame_bytes', 'state', 'tail', 'processed_bytes')}
    if 'state' in details:
        state = details['state']
        row['geometry'] = {k: v for k, v in state.items() if k != 'screen'}
        row['sentinels'] = {chr(cp): {'screen': locations(state, chr(cp)), 'tail': tail_locations(event, chr(cp))}
                            for cp in range(0x2460, 0x246f)
                            if locations(state, chr(cp)) or tail_locations(event, chr(cp))}
    wire = details.get('frame_bytes', details.get('bytes'))
    if wire is not None and event['event'] in ('grid-offer', 'desktop-submit', 'tick-decision'):
        wire = bytes(wire)
        row['payload_sha256'] = hashlib.sha256(wire).hexdigest()
        row['payload_sentinels'] = [{'key': chr(cp), 'row': r, 'column': c} for cp, r, c in cells(wire) if 0x2460 <= cp < 0x246f]
    if event['event'] in ('pty-read', 'vt-applied'):
        text = bytes(details.get('processed_bytes', details.get('bytes', []))).decode('utf8', errors='replace')
        row['chunk_sentinels'] = [chr(cp) for cp in range(0x2460, 0x246f) if chr(cp) in text]
    timeline.append(row)
(HERE / 'timeline.json').write_text(json.dumps(timeline, indent=2))
samples = []
for sample in loaded['keys']:
    key = sample['key']
    dispatched_us = (loaded['wall_time_origin_ms']+sample['dispatch_at'])*1000
    parsed = [e for e in vt_events if key in bytes(e['details']['processed_bytes']).decode('utf8', errors='replace')]
    if not parsed:
        samples.append({'key': key, 'error': 'sentinel missing from processed PTY chunks'})
        continue
    first = parsed[0]
    after = [e for e in cuts if e['at_us'] >= first['at_us']]
    eligible = after[0] if after else None
    before_cut = [e for e in vt_events if eligible and first['at_us'] <= e['at_us'] <= eligible['at_us']]
    last = before_cut[-1] if before_cut else first
    cut_details = eligible['details'] if eligible else None
    submitted = [e for e in delivered if e['at_us'] >= first['at_us'] and any(cp == ord(key) for cp, _, _ in cells(bytes(e['details']['bytes'])))]
    received = [e for e in arrivals if e['at'] >= sample['dispatch_at'] and any(cp == ord(key) for cp, _, _ in cells(base64.b64decode(e['binary'])))]
    decisions = [e for e in events if e['event'] == 'tick-decision' and first['at_us'] <= e['at_us'] <= (eligible['at_us'] if eligible else first['at_us'])]
    first_tail = tail_locations(first, key)
    last_tail = tail_locations(last, key)
    samples.append({'key': key, 'dispatch_epoch_us': dispatched_us,
        'first_vt_event_sequence': first['log_sequence'], 'first_vt_at_us': first['at_us'],
        'first_vt_screen_locations': locations(first['details']['state'], key),
        'first_vt_tail_locations': first_tail,
        'first_vt_origin': first['details']['state']['origin'],
        'first_vt_history': first['details']['state']['history'],
        'first_vt_geometry': {k:v for k,v in first['details']['state'].items() if k != 'screen'},
        'last_vt_before_cut_sequence': last['log_sequence'], 'last_vt_before_cut_at_us': last['at_us'],
        'last_vt_before_cut_tail_locations': last_tail,
        'last_vt_before_cut_origin': last['details']['state']['origin'],
        'first_eligible_cut_sequence': eligible['log_sequence'] if eligible else None,
        'first_eligible_cut_at_us': eligible['at_us'] if eligible else None,
        'first_eligible_cut_tick': cut_details['tick'] if cut_details else None,
        'first_eligible_cut_screen_locations': locations(cut_details['state'], key) if cut_details else None,
        'first_eligible_cut_origin': cut_details['state']['origin'] if cut_details else None,
        'first_eligible_cut_cells_contain_key': any(cp == ord(key) for cp, _, _ in cells(bytes(cut_details['frame_bytes']))) if cut_details else None,
        'tick_decisions_before_cut': [{'sequence': e['log_sequence'], 'at_us': e['at_us'], **{k:v for k,v in e['details'].items() if k not in ('state','frame_bytes')}} for e in decisions],
        'sentinel_above_viewport_before_cut': bool(last_tail and cut_details and all(location['absolute_row'] < cut_details['state']['origin'] for location in last_tail)),
        'native_submit_count_containing_key': len(submitted), 'frontend_arrival_count_containing_key': len(received),
        'fillText_ms': sample.get('fillText_ms')})
summary = {'session': session, 'window': loaded['windowState'], 'event_counts': dict(Counter(e['event'] for e in events)),
    'ticker_decisions': dict(Counter(e['details']['decision']+':'+e['details']['reason'] for e in events if e['event']=='tick-decision')),
    'native_submit_count': len(delivered), 'frontend_arrival_count': len(arrivals), 'recorder_dropped': recording['dropped'],
    'native_submit_payloads_not_observed': dict(submitted_hashes-arrived_hashes),
    'frontend_arrival_payloads_not_in_native_submit_trace': dict(arrived_hashes-submitted_hashes),
    'samples': samples}
(HERE / 'summary.json').write_text(json.dumps(summary, indent=2))
print(json.dumps(summary, indent=2))
