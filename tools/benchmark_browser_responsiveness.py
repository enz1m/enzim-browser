#!/usr/bin/env python3
"""Measure headed page work and compositor-visible responses in isolated profiles."""
import argparse
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import statistics
import subprocess
import threading
import time

from benchmark_memory_short import browser_launch, capture, click, cpu_usage, enforce_owned_limits, enter_address, key, require_audited, unit_log, windows


HTML = r'''<!doctype html><meta charset="utf-8"><title>Response fixture PAGE</title>
<style>body{margin:0;background:#eee;color:#222;font:18px sans-serif}
#marker{position:fixed;right:20px;top:10px;width:160px;height:220px;border:0;
background:COLOR;color:white;font:24px sans-serif;z-index:2}
input{position:fixed;left:30px;top:240px;width:500px;height:60px;z-index:2}
article{padding:12px 30px;border-bottom:1px solid #aaa;max-width:1000px}
</style><button id="marker">PAGE</button><input aria-label="Typing test">
<main></main><script>
const marker=document.querySelector('#marker'), field=document.querySelector('input');
const base='COLOR', alternate='#f29d35';let changed=false;
const send=(type,data)=>fetch('/report',{method:'POST',body:JSON.stringify({
type,page:'PAGE',href:location.href,...data})});
addEventListener('error',event=>send('error',{message:event.message}));
addEventListener('unhandledrejection',event=>send('error',{message:String(event.reason)}));
const frame=()=>new Promise(resolve=>requestAnimationFrame(resolve));
const started=performance.now();let rows='';
for(let i=0;i<5000;i++)rows+=`<article><b>Row ${i}</b> — content and layout fixture <span>${i*i}</span></article>`;
document.querySelector('main').innerHTML=rows;
const constructed=performance.now()-started;
async function response(event){const at=performance.now();changed=!changed;
marker.style.background=changed?alternate:base;
if(event.type==='click')field.focus();
await frame();await frame();send(event.type,{dispatchToTwoFramesMs:performance.now()-at,
eventAgeMs:at-event.timeStamp,scrollY,text:field.value,fieldFocused:document.activeElement===field,color:marker.style.background});}
marker.addEventListener('click',response);field.addEventListener('input',response);
addEventListener('wheel',response,{passive:true});
addEventListener('load',async()=>{await frame();await frame();
const n=performance.getEntriesByType('navigation')[0];
send('ready',{constructedMs:constructed,domContentLoadedMs:n.domContentLoadedEventEnd,
responseStartMs:n.responseStart,twoFramesMs:performance.now(),rowCount:document.querySelectorAll('article').length});
if(!location.search.includes('stress'))return;
while(!(await (await fetch('/begin')).json()).go)await new Promise(resolve=>setTimeout(resolve,25));
const retained=Array.from({length:300000},(_,i)=>({i,value:'kept-'+i,child:{i}}));
const weak=new WeakMap(retained.map(x=>[x,x.child]));let checksum=0;
for(let repeat=0;repeat<3;repeat++){
const intervals=[],allocation=[],layout=[],waiting=[];let previous=performance.now(),begin=previous;
for(let wave=0;wave<120;wave++){
const start=performance.now();
const temporary=Array.from({length:30000},(_,i)=>({i,value:'wave-'+i,child:{wave}}));
checksum+=temporary[wave].child.wave;
const allocated=performance.now();allocation.push(allocated-start);
document.querySelector('article').textContent='Allocation wave '+wave;
const edited=performance.now();layout.push(edited-allocated);
await frame();const now=performance.now();waiting.push(now-edited);intervals.push(now-previous);previous=now;
}
if(retained.some((x,i)=>x.i!==i||weak.get(x)!==x.child))throw Error('Live graph corrupted');
await send('stress',{repeat,intervals,allocation,layout,waiting,elapsedMs:performance.now()-begin,checksum,retained:retained.length});
}
});
</script>'''


def process_cpu_ticks(pid):
    try:
        fields = (Path('/proc') / str(pid) / 'stat').read_text().rsplit(')', 1)[1].split()
    except (FileNotFoundError, ProcessLookupError):
        return None
    return int(fields[11]) + int(fields[12])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--browser', choices=['enzim', 'firefox', 'chrome', 'webkitgtk'], required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--runtime', type=Path)
    parser.add_argument('--runtime-launcher', type=Path)
    parser.add_argument('--desktop', type=Path, required=True,
                        help='Dedicated compositor directory containing wl-display and swaysock')
    parser.add_argument('--port', type=int, default=5925, help='VNC port for desktop input')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--setenv', action='append', default=[])
    parser.add_argument('--gc-pressure', action='store_true')
    parser.add_argument('--address-submit', action='store_true')
    args = parser.parse_args()
    out = args.output.resolve()
    out.mkdir(parents=True, exist_ok=False)
    records = []
    begin = threading.Event()

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            if self.path == '/begin':
                body = json.dumps({'go': begin.is_set()}).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)
                return
            page = 'B' if self.path.startswith('/b') else 'A'
            color = '#8d4acb' if page == 'B' else '#159e99'
            body = HTML.replace('PAGE', page).replace('COLOR', color).encode()
            self.send_response(200)
            self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body)))
            self.send_header('Cache-Control', 'no-store')
            self.end_headers()
            self.wfile.write(body)

        def do_POST(self):
            value = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
            records.append(dict(value, received_monotonic=time.monotonic()))
            self.send_response(204)
            self.end_headers()

    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    url = f'http://127.0.0.1:{server.server_port}'
    desktop = args.desktop.resolve()
    env = {k: v for k, v in os.environ.items() if not k.startswith(
        ('MALLOC_', 'JSC_', 'WEBKIT_', 'ENZIM_BENCH_', 'ENZUI_PROBE_'))}
    env.pop('GLIBC_TUNABLES', None)
    env.pop('ENZIM_MEMORY_TRACE', None)
    env.update(WAYLAND_DISPLAY=(desktop / 'wl-display').read_text().strip(),
               SWAYSOCK=(desktop / 'swaysock').read_text().strip(),
               XDG_SESSION_TYPE='wayland', MOZ_ENABLE_WAYLAND='1')
    profile = out / 'profile'
    for variable, name in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'),
                           ('XDG_CACHE_HOME', 'cache'), ('TMPDIR', 'tmp')]:
        folder = profile / name
        folder.mkdir(parents=True)
        env[variable] = str(folder)
    overrides = dict(item.split('=', 1) for item in args.setenv)
    if any(not k.startswith(('JSC_', 'WEBKIT_')) for k in overrides):
        raise ValueError('Only engine option overrides are allowed')
    env.update(overrides)
    binary = args.binary.resolve()
    initial = url + ('/a?stress' if args.gc_pressure else '/a')
    launch, source, _, _ = browser_launch(args, profile, [('fixture', initial)], env, binary)
    unit = 'enzim-response-' + out.name
    command = ['systemd-run', '--user', '--collect', '--unit=' + unit,
               '-p', 'MemoryMax=8G', '-p', 'MemorySwapMax=0']
    variables = ['WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'XDG_SESSION_TYPE', 'MOZ_ENABLE_WAYLAND',
                 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME', 'TMPDIR', 'PULSE_SINK',
                 'ENZIM_WPE_ENCRYPTED_MEDIA', 'ENZIM_WPE_RUNTIME_SOURCE', *overrides]
    command += ['--setenv=' + k + '=' + env[k] for k in variables if k in env]
    (out / 'driver.py').write_bytes(Path(__file__).read_bytes())
    (out / 'fixture.html').write_text(HTML)
    result = {'browser': args.browser, 'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'fixture_sha256': hashlib.sha256(HTML.encode()).hexdigest(),
              'runtime_source': source, 'overrides': overrides, 'samples': [],
              'foreign_compilers_at_start': subprocess.run(
                  ['ps', '-C', 'rustc,cc1plus,clang++,ld.lld', '-o', 'pid,etime,pcpu,comm'],
                  capture_output=True, text=True).stdout,
              'metric': 'Compositor screenshot upper bounds include input transport and capture overhead; '
                        'page timings end after two animation frames, not physical scanout.'}
    client = None
    additional_limits = {}
    result['additional_owned_scope_limits'] = additional_limits

    def wait_record(kind, since, page=None):
        end = time.monotonic() + 20
        while time.monotonic() < end:
            found = [r for r in records[since:] if r['type'] == kind and (page is None or r['page'] == page)]
            if found:
                return found[-1]
            time.sleep(.01)
        raise RuntimeError(f'Missing page response: {kind}, {page}')

    try:
        started_utc = time.time()
        subprocess.run(command + ['--', *launch], env=env, check=True)
        from memory_snapshot import collect
        from vncdotool import api
        from PIL import Image
        end = time.monotonic() + 30
        result['startup_audit_retries'] = []
        while time.monotonic() < end:
            try:
                snapshot = collect(unit + '.service', profile, [])
                enforce_owned_limits(snapshot, '8G', additional_limits)
            except (OSError, RuntimeError, ValueError) as error:
                result['startup_audit_retries'].append(str(error))
                time.sleep(.2)
                continue
            owned = {p['pid'] for p in snapshot['processes']}
            matches = [w for w in windows(env) if w['pid'] in owned]
            if len(matches) == 1:
                window = matches[0]
                break
            time.sleep(.2)
        else:
            raise RuntimeError('Owned window missing')
        subprocess.run(['swaymsg', f'[con_id={window["id"]}] focus'], env=env, check=True,
                       stdout=subprocess.DEVNULL)
        wait_record('ready', 0)
        actual = collect(unit + '.service', profile, [])
        result['process_environment'] = []
        for process in actual['processes']:
            if process['name'] not in ['enzim-browser', 'WPEWebProcess', 'WPEGPUProcess', 'WPENetworkProce']:
                continue
            values = dict(item.split(b'=', 1) for item in
                          (Path('/proc') / str(process['pid']) / 'environ').read_bytes().split(b'\0') if b'=' in item)
            selected = {k: values.get(k.encode(), b'').decode() for k in overrides}
            if selected != overrides:
                raise RuntimeError('Engine options did not reach an owned process')
            result['process_environment'].append({'pid': process['pid'], 'name': process['name'], 'options': selected})
        client = api.connect(f'127.0.0.1::{args.port}', timeout=15)
        for modifier in ['ctrl', 'alt', 'shift']:
            client.keyUp(modifier)
        capture(env, window, out / 'initial.png')
        if args.gc_pressure:
            time.sleep(2)
            since = len(records)
            client.mouseMove(1300, 200)
            client.mouseDown(1)
            client.mouseUp(1)
            focused = wait_record('click', since)
            if not focused['fieldFocused']:
                raise RuntimeError('Typing target did not acquire DOM focus')
            client.keyPress('x')
            if wait_record('input', since)['text'] != 'x':
                raise RuntimeError('Typing target did not accept initial input')
            cpu_start = cpu_usage(collect(unit + '.service', profile, []))
            cpu_before = {}
            for process in collect(unit + '.service', profile, [])['processes']:
                ticks = process_cpu_ticks(process['pid'])
                if ticks is not None:
                    cpu_before[process['pid']] = ticks
            stress_started = time.monotonic()
            begin.set()
            next_input = stress_started + .5
            input_count = 0
            deadline = time.monotonic() + 120
            while len([r for r in records if r['type'] == 'stress']) < 3:
                errors = [r for r in records if r['type'] == 'error']
                if errors:
                    raise RuntimeError('Page benchmark failed: ' + errors[-1]['message'])
                if time.monotonic() > deadline:
                    raise RuntimeError('Allocation-pressure benchmark did not complete')
                if time.monotonic() >= next_input:
                    client.keyPress('x')
                    input_count += 1
                    next_input += 1
                time.sleep(.1)
            result['stress'] = [r for r in records if r['type'] == 'stress']
            if any(r['retained'] != 300000 or len(r['intervals']) != 120 for r in result['stress']):
                raise RuntimeError('Incomplete allocation-pressure workload')
            capture(env, window, out / 'final.png')
            result['snapshot'] = collect(unit + '.service', profile, [])
            require_audited(result['snapshot'])
            result['all_process_cpu_seconds'] = (cpu_usage(result['snapshot']) - cpu_start) / 1e6
            role_cpu = {}
            for process in result['snapshot']['processes']:
                ticks = process_cpu_ticks(process['pid'])
                if ticks is not None and process['pid'] in cpu_before:
                    name = process['name']
                    role_cpu[name] = role_cpu.get(name, 0) + (ticks - cpu_before[process['pid']]) / os.sysconf('SC_CLK_TCK')
            result['surviving_process_cpu_seconds_by_name'] = role_cpu
            result['stress_wall_seconds'] = time.monotonic() - stress_started
            result['stress_input_count'] = input_count
            result['page_records'] = records
            inputs = [r for r in records if r['type'] == 'input']
            if not inputs or inputs[-1]['text'] != 'x' * (input_count + 1):
                raise RuntimeError('Input was lost during allocation pressure')
            (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
            print([(round(r['elapsedMs']), round(max(r['intervals']))) for r in result['stress']], flush=True)
            return
        click(client, 350, 28)
        key(client, 'esc')

        def visible_response(action, expected, label):
            from PIL import ImageStat
            before = out / (label + '-before.png')
            capture(env, window, before)
            with Image.open(before) as im:
                old = im.convert('RGB').getpixel((1300, 200))
            if max(abs(a - b) for a, b in zip(old, expected)) < 8:
                raise RuntimeError('Visible-response control already matches before input')
            start = time.monotonic()
            action()
            attempts = []
            while time.monotonic() - start < 3:
                path = out / (label + f'-{len(attempts)}.png')
                subprocess.run(['grim', '-g', '1250,180 100x50', str(path)], env=env, check=True,
                               stdout=subprocess.DEVNULL, timeout=5)
                with Image.open(path) as im:
                    actual = ImageStat.Stat(im.convert('RGB')).mean
                elapsed = (time.monotonic() - start) * 1000
                matched = max(abs(a - b) for a, b in zip(actual, expected)) < 8
                attempts.append({'finished_ms': elapsed, 'matched': matched})
                if matched:
                    return {'upper_ms': elapsed, 'captures': attempts}
            raise RuntimeError('Input did not produce the expected visible page')

        for repeat in range(5):
            since = len(records)
            if args.address_submit:
                key(client, 'ctrl-l')
                enter_address(client, url + '/a?repeat=' + str(repeat), env, args.browser)
            else:
                key(client, 'ctrl-r')
            ready = wait_record('ready', since)
            if args.address_submit and not ready['href'].endswith('/a?repeat=' + str(repeat)):
                raise RuntimeError('Address submission navigated to the wrong document')
            if ready['rowCount'] != 5000:
                raise RuntimeError('Incomplete page workload')
            time.sleep(.5)
            since = len(records)

            def click_marker():
                client.mouseMove(1300, 200)
                client.mouseDown(1)
                client.mouseUp(1)

            click_result = visible_response(click_marker, (242, 157, 53), f'click-{repeat}')
            click_record = wait_record('click', since)
            since = len(records)
            input_result = visible_response(lambda: client.keyPress('x'), (21, 158, 153), f'input-{repeat}')
            input_record = wait_record('input', since)
            if input_record['text'] != 'x':
                raise RuntimeError('Input was lost or duplicated')
            since = len(records)

            def wheel():
                client.mouseMove(900, 500)
                client.mousePress(5)

            wheel_result = visible_response(wheel, (242, 157, 53), f'wheel-{repeat}')
            wheel_record = wait_record('wheel', since)
            if wheel_record['scrollY'] <= 0:
                raise RuntimeError('Wheel event did not scroll the page')
            result['samples'].append({'repeat': repeat, 'load': ready,
                'click': click_result, 'click_page': click_record,
                'input': input_result, 'input_page': input_record,
                'wheel': wheel_result, 'wheel_page': wheel_record})
            (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
            print(out.name, repeat, 'load', round(ready['twoFramesMs'], 1),
                  'input-visible', round(input_result['upper_ms'], 1), flush=True)
        key(client, 'ctrl-t')
        enter_address(client, url + '/b', env, args.browser)
        wait_record('ready', 0, 'B')
        time.sleep(2)
        result['switches'] = []
        for repeat in range(10):
            expected = (242, 157, 53) if repeat % 2 == 0 else (141, 74, 203)
            def switch():
                client.keyDown('ctrl')
                client.keyPress('tab')
                client.keyUp('ctrl')

            result['switches'].append(visible_response(switch, expected, f'switch-{repeat}'))
            time.sleep(.5)
        capture(env, window, out / 'final.png')
        result['snapshot'] = collect(unit + '.service', profile, [])
        require_audited(result['snapshot'])
        result['page_records'] = records
        result['summary'] = {kind: statistics.median(s[kind]['upper_ms'] for s in result['samples'])
                             for kind in ['click', 'input', 'wheel']}
        result['summary']['switch'] = statistics.median(s['upper_ms'] for s in result['switches'])
        (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        print(result['summary'], flush=True)
    except Exception as error:
        if client:
            try:
                capture(env, window, out / 'failed.png')
            except (RuntimeError, FileNotFoundError) as capture_error:
                result['failed_capture_error'] = str(capture_error)
        result['failure'] = str(error)
        result['page_records'] = records
        (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        raise
    finally:
        if client:
            client.disconnect()
            api.shutdown()
        subprocess.run(['systemctl', '--user', 'stop', unit], check=False)
        (out / 'journal.log').write_text(unit_log(unit, started_utc))
        for owned_scope in additional_limits:
            subprocess.run(['systemctl', '--user', 'stop', owned_scope], check=False,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        server.shutdown()
        result['foreign_compilers_at_end'] = subprocess.run(
            ['ps', '-C', 'rustc,cc1plus,clang++,ld.lld', '-o', 'pid,etime,pcpu,comm'],
            capture_output=True, text=True).stdout
        if 'failure' not in result:
            result['completed_utc'] = time.time()
        (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    main()
