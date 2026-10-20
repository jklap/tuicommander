"""Compare retained native proxy runs without hiding censored or failed samples."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
names = ('priority-before', 'priority-after', 'priority-after-repeat')
runs = {}
for name in names:
    summary = json.loads((ROOT / name / 'summary.json').read_text())
    events = json.loads((ROOT / name / 'native-events.json').read_text())
    samples = summary['samples']
    bypasses = [event['mono_us'] for event in events
                if event['event'] == 'tick-decision' and event['details'].get('priority_bypass')]
    runs[name] = {
        'metrics_ms': summary['metrics_ms'],
        'backend_echo_count': summary['backend_echo_count'],
        'matching_fillText_count': sum(sample['fillText_ms'] is not None for sample in samples),
        'missing_from_vt': [sample['key'] for sample in samples if not sample['native']['vt_applied']],
        'applied_without_matching_cut': [sample['key'] for sample in samples
                                        if sample['native']['vt_applied'] and not sample['native']['cut']],
        'arrived_without_matching_fillText': [sample['key'] for sample in samples
                                            if sample['arrival_at'] and sample['fillText_ms'] is None],
        'output_bytes_per_second_observed': summary['output_bytes_per_second_observed'],
        'raf_gap_ms': summary['raf_gap_ms'], 'timer_gap_ms': summary['timer_gap_ms'],
        'recorder_dropped': summary['recorder_dropped'],
        'priority_bypasses': len(bypasses),
        'max_bypasses_in_rolling_100ms': max(
            (sum(start <= at < start + 100_000 for at in bypasses) for start in bypasses), default=0),
    }
baseline = runs['priority-before']['output_bytes_per_second_observed']
for name in names[1:]:
    runs[name]['observed_throughput_change_percent'] = (
        runs[name]['output_bytes_per_second_observed'] / baseline - 1) * 100
raw = (ROOT / 'priority-after-repeat/raw-ring.bin').read_bytes()
result = {
    'runs': runs,
    'repeat_raw_ring_bytes': len(raw),
    'repeat_raw_ring_key_counts': {chr(0x2460 + index): raw.count(chr(0x2460 + index).encode())
                                   for index in range(15)},
    'claim': 'Untrusted JS dispatch to matching fillText only; censored samples remain explicit. '
             'No trusted keyboard, compositor or native frontend-heartbeat non-regression claim.',
}
(ROOT / 'priority-comparison.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps({name: {key: run[key] for key in (
    'backend_echo_count', 'matching_fillText_count', 'missing_from_vt',
    'applied_without_matching_cut', 'arrived_without_matching_fillText',
    'priority_bypasses', 'max_bypasses_in_rolling_100ms')} for name, run in runs.items()}, indent=2))
