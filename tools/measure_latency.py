#!/usr/bin/env python3
"""Measure scroll response and frame handoff in an isolated, static QA page.

Use tools/fixtures/scroll-latency.html at 1400 x 900 and 60 Hz.
The instant-scroll fixture must be settled before timing begins.
Times end at first GPU command encoding, not GPU completion or display scanout.
"""
import argparse
import json
import re
import statistics
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--unit', required=True, help='Systemd user unit running the browser')
parser.add_argument('--pidfile', type=Path, required=True, help='File with the browser process ID')
parser.add_argument('--vnc', required=True, help='VNC endpoint for the dedicated compositor')
parser.add_argument('--client', type=Path, default=Path('vncdotool'))
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--repeats', type=int, default=3)
args = parser.parse_args()
pid = int(args.pidfile.read_text())
env = dict(item.split(b'=', 1) for item in Path(f'/proc/{pid}/environ').read_bytes().split(b'\0') if b'=' in item)
assert env.get(b'ENZIM_QA') == b'1', 'Only an isolated QA process may be measured'
assert env.get(b'ENZIM_BENCH_FRAME_TRACE') == b'1', 'Enable frame diagnostics first'
identity = subprocess.check_output(['systemctl', '--user', 'show', args.unit, '-p', 'InvocationID', '--value'], text=True).strip()
group = subprocess.check_output(['systemctl', '--user', 'show', args.unit, '-p', 'ControlGroup', '--value'], text=True).strip()
cpu_file = Path('/sys/fs/cgroup'+group)/'cpu.stat'

def cpu():
    return int(dict(line.split() for line in cpu_file.read_text().splitlines())['usage_usec'])

def drive(steps):
    subprocess.run([str(args.client), '-s', args.vnc, '--timeout', '35', 'pause', '1.5', 'move', '1050', '350', 'pause', '1', *steps, 'pause', '1'], check=True, stdout=subprocess.DEVNULL, timeout=38)

def journal():
    return subprocess.check_output(['journalctl', '--user', '-u', args.unit, '_SYSTEMD_INVOCATION_ID='+identity, '-o', 'cat', '--no-pager'], text=True)

def summary(values):
    if not values:
        return None
    ordered = sorted(values)
    return {'n': len(values), 'median_ms': statistics.median(values)/1000, 'p95_ms': ordered[min(len(ordered)-1, int(len(ordered)*.95))]/1000, 'max_ms': max(values)/1000}

def metrics(log, first_input, isolated):
    events = [(kind, int(stamp), int(serial)) for kind, stamp, serial in re.findall(r'FRAME_TRACE (\w+) (\d+) (\d+)', log)]
    enqueued = [stamp for kind, stamp, _ in events if kind == 'enqueue'][first_input:]
    wheels = [stamp for kind, stamp, _ in events if kind == 'wheel'][first_input:]
    assert len(enqueued) == len(wheels), 'Queued input was lost during the measurement'
    encoded = {int(serial): int(elapsed) for serial, elapsed in re.findall(r'ENCODE_TRACE (\d+) (\d+)', log)}
    frames = [(stamp, encoded[serial]) for kind, stamp, serial in events if kind == 'frame' and serial in encoded and stamp >= wheels[0]]
    queue = [b-a for a, b in zip(enqueued, wheels)]
    responses = []
    if isolated:
        for index, wheel in enumerate(wheels):
            end = wheels[index+1] if index+1 < len(wheels) else wheel+250000
            candidate = next((stamp+cost for stamp, cost in frames if wheel <= stamp < end), None)
            if candidate is not None:
                responses.append(candidate-enqueued[index])
        assert len(responses) == len(wheels), 'A static-page input had no encoded response'
        assert len(frames) == len(wheels), f'Unexpected animation/reload frames in instant-scroll control: {len(frames)} for {len(wheels)} inputs'
    active = [(stamp, cost) for stamp, cost in frames if stamp <= wheels[-1]]
    return {'inputs': len(wheels), 'enqueue_to_dispatch': summary(queue), 'input_to_first_encoding': summary(responses), 'native_to_encoding': summary([cost for _, cost in frames]), 'encoded_frame_interval': summary([b[0]-a[0]+b[1]-a[1] for a,b in zip(active,active[1:])])}

reports = []
for repeat in range(args.repeats):
    for isolated in (True, False):
        prior = len(re.findall(r'FRAME_TRACE wheel ', journal()))
        steps = []
        for direction in (['5']*24 if isolated else ['5']*90+['4']*90):
            steps += ['click', direction, 'pause', '.25' if isolated else '.0167']
        started = time.monotonic()
        before = cpu()
        drive(steps)
        elapsed = time.monotonic()-started
        result = metrics(journal(), prior, isolated)
        result.update(repeat=repeat, mode='isolated' if isolated else 'continuous', duration_seconds=elapsed, all_process_cpu_seconds=(cpu()-before)/1e6)
        reports.append(result)
        print(json.dumps(result), flush=True)
    time.sleep(2)
    before = cpu()
    time.sleep(3)
    reports.append({'repeat':repeat, 'mode':'post_scroll', 'one_core_cpu_percent':(cpu()-before)/30000})
quiet = False
for _ in range(12):
    before_frames = len(re.findall(r'FRAME_TRACE frame ', journal()))
    time.sleep(1)
    if len(re.findall(r'FRAME_TRACE frame ', journal())) == before_frames:
        quiet = True
        break
before = cpu()
time.sleep(3)
reports.append({'mode':'quiescent_idle','native_frames_stopped':quiet,'one_core_cpu_percent':(cpu()-before)/30000})
args.output.parent.mkdir(parents=True, exist_ok=True)
args.output.write_text(json.dumps({'app_pid':pid, 'invocation':identity, 'binary_inode':Path(f'/proc/{pid}/exe').stat().st_ino, 'measurements':reports}, indent=2))
