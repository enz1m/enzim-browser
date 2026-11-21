#!/usr/bin/env python3
"""Compare fixed headed workloads in isolated graphical browser profiles."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

from memory_snapshot import collect

SITES = [('YouTube', 'https://www.youtube.com/'),
         ('CNN', 'https://www.cnn.com/'), ('GitHub', 'https://github.com/')]
TEN_SITES = [('Google', 'https://www.google.com/'), *SITES[:1],
             ('Bing', 'https://www.bing.com/'), ('DuckDuckGo', 'https://duckduckgo.com/'),
             ('Wikipedia', 'https://en.wikipedia.org/wiki/Main_Page'),
             ('BBC', 'https://www.bbc.com/news'), *SITES[1:],
             ('Microsoft', 'https://www.microsoft.com/'), ('Mozilla', 'https://www.mozilla.org/')]
ROOT = Path(__file__).resolve().parents[1]


def command(args, env=None):
    return subprocess.check_output(args, env=env, text=True)


def unit_log(unit, since):
    return command(['journalctl', '--user', '-u', unit,
                    '--since=@' + str(since), '-o', 'cat', '--no-pager'])


def windows(env):
    tree = json.loads(command(['swaymsg', '-t', 'get_tree', '-r'], env))
    result = []

    def visit(node):
        if node.get('pid'):
            result.append({k: node.get(k) for k in ['id', 'pid', 'name', 'app_id', 'rect']})
        for child in node.get('nodes', []) + node.get('floating_nodes', []):
            visit(child)
    visit(tree)
    return result


def key(client, name):
    parts = name.split('-')
    for modifier in parts[:-1]:
        client.keyDown(modifier)
    time.sleep(.08)
    client.keyPress(parts[-1])
    time.sleep(.08)
    for modifier in reversed(parts[:-1]):
        client.keyUp(modifier)
    time.sleep(.35)


def click(client, x, y):
    client.mouseMove(x, y)
    time.sleep(.2)
    client.mouseDown(1)
    time.sleep(.1)
    client.mouseUp(1)
    time.sleep(.4)


def enter_address(client, url, env, browser):
    if browser == 'webkitgtk':
        subprocess.run(['wtype', '-M', 'ctrl', '-k', 'l', '-m', 'ctrl',
                        '-s', '150', '-d', '15', url, '-s', '150', '-k', 'Return'],
                       env=env, check=True)
    else:
        subprocess.run(['wl-copy', '--', url], env=env, check=True)
        key(client, 'ctrl-v')
        key(client, 'enter')


def browser_key(client, name, env, browser):
    if browser != 'webkitgtk':
        key(client, name)
        return
    parts = name.split('-')
    command = ['wtype']
    for modifier in parts[:-1]:
        command += ['-M', modifier]
    command += ['-k', {'enter': 'Return', 'esc': 'Escape', 'tab': 'Tab'}.get(parts[-1], parts[-1])]
    for modifier in reversed(parts[:-1]):
        command += ['-m', modifier]
    subprocess.run(command, env=env, check=True)
    time.sleep(.35)


def capture(env, window, path):
    current = next(w for w in windows(env) if w['id'] == window['id'])
    r = current['rect']
    if r['width'] != 1400 or r['height'] != 900:
        raise RuntimeError(f'Unexpected window geometry: {r}')
    subprocess.run(['grim', '-g', f"{r['x']},{r['y']} {r['width']}x{r['height']}", str(path)],
                   env=env, check=True, stdout=subprocess.DEVNULL, timeout=10)
    from PIL import Image, ImageStat
    with Image.open(path) as image:
        region = image.convert('RGB').crop((25, 120, r['width'] - 25, r['height'] - 25))
        if max(ImageStat.Stat(region).stddev) < 2:
            raise RuntimeError('Content capture is blank; sampling is refused')
    path.with_suffix('.provenance.json').write_text(json.dumps(current, indent=2))


def require_audited(sample):
    for key in ['unreadable_pss', 'vanished', 'new_pids_during_sample', 'contaminated_extra_cgroups']:
        if sample['audit'][key]:
            raise RuntimeError(f'Incomplete ownership/PSS sample: {key}: {sample["audit"][key]}')
    if sample['gpu']['unreadable_pids'] or sample['gpu']['unidentifiable_drm_descriptors']:
        raise RuntimeError('Incomplete GPU accounting')
    if not sample['processes']:
        raise RuntimeError('No browser processes sampled')
    for p in sample['processes']:
        if p['ram']['Private_Hugetlb'] or p['ram']['Shared_Hugetlb']:
            raise RuntimeError('HugeTLB accounting requires a separate ownership view')
    for group in sample['cgroups']:
        if group['swap_bytes']:
            raise RuntimeError('Browser swapped')
        events = dict(line.split() for line in group['events'].splitlines())
        if int(events.get('oom_kill', '0')):
            raise RuntimeError('Browser hit an OOM kill')


def enforce_owned_limits(sample, memory_limit, applied):
    if sample['audit']['contaminated_extra_cgroups']:
        raise RuntimeError('Cannot change limits on a shared cgroup')
    for group in sample['cgroups']:
        path = Path(group['path'])
        name = path.name
        if not name.endswith('.scope') or name in applied:
            continue
        subprocess.run(['systemctl', '--user', 'set-property', '--runtime', name,
                        'MemoryMax=' + memory_limit, 'MemorySwapMax=0'], check=True)
        expected = int(memory_limit.removesuffix('G')) * 2**30
        if int((path / 'memory.max').read_text()) != expected or int((path / 'memory.swap.max').read_text()) != 0:
            raise RuntimeError('Owned scope did not receive the benchmark limits')
        applied[name] = {'memory_max_bytes': expected, 'memory_swap_max_bytes': 0}


def require_healthy_runtime(log):
    if 'dumped core' in log or 'MEMORY_DISCARD' in log:
        raise RuntimeError('Loaded-page comparison refused after a crash or tab discard')


def require_loaded_views(log, sample, expected):
    latest = {}
    for line in log.splitlines():
        if not line.startswith('MEMORY_TRACE '):
            continue
        try:
            row = json.loads(line.removeprefix('MEMORY_TRACE '))
        except json.JSONDecodeError:
            continue
        if row.get('event') == 'policy-observe':
            latest[row['view']] = row
    live = {p['pid'] for p in sample['processes'] if p['name'] == 'WPEWebProcess'}
    newest = max((row['ms'] for row in latest.values()), default=0)
    recent = [row for row in latest.values() if newest - row['ms'] <= 3000]
    if len(recent) != expected or any(row['renderer_pid'] not in live for row in recent):
        raise RuntimeError('Not every expected tab has a live, recently observed renderer')


def cpu_usage(sample):
    groups = [Path(group['path']) for group in sample['cgroups']]
    roots = [group for group in groups if not any(
        other != group and other in group.parents for other in groups)]
    total = 0
    for group in roots:
        fields = dict(line.split() for line in (group / 'cpu.stat').read_text().splitlines())
        total += int(fields['usage_usec'])
    return total


def native_details(sample, folder):
    folder.mkdir(exist_ok=True)
    mappings = []
    counters = {}
    for row in sample['processes']:
        pid = row['pid']
        proc = Path('/proc') / str(pid)
        try:
            text = (proc / 'smaps').read_text()
        except (PermissionError, FileNotFoundError) as error:
            raise RuntimeError(f'Mapping attribution missing for owned process {pid}') from error
        totals = {}
        current = ''
        for line in text.splitlines():
            fields = line.split()
            if fields and '-' in fields[0] and len(fields) >= 5:
                current = ' '.join(fields[5:]) or '[anonymous]'
            elif fields and fields[0] == 'Pss:':
                totals[current] = totals.get(current, 0) + int(fields[1]) * 1024
        mappings.append({'pid': pid, 'name': row['name'], 'pss_by_mapping': totals})
        if row['name'] != 'WPEWebProcess':
            continue
        logs = list((proc / 'root/tmp').glob('WPEWebProcess-*'))
        if not logs:
            raise RuntimeError(f'Native memory sampler missing for web process {pid}')
        for log in logs:
            try:
                text = log.read_text()
            except FileNotFoundError as error:
                raise RuntimeError(f'Native memory log vanished for web process {pid}') from error
            target = folder / f'web-memory-{pid}-{log.name}.txt'
            target.write_text(text)
            counters[str(pid)] = str(target)
    return {'mappings': mappings, 'native_counter_logs': counters}


def browser_launch(args, profile, sites, env, binary):
    program_dirs = []
    source = None
    launcher = None
    if args.browser == 'enzim':
        directory = profile / 'config/enzim-browser'
        directory.mkdir()
        (directory / 'session.json').write_text(json.dumps({'layout': 'Top'}))
        runtime = args.runtime.resolve()
        manifest = json.loads((runtime / 'source-runtime.json').read_text())
        source = manifest['source']['source_commit']
        env.update(ENZIM_WPE_ENCRYPTED_MEDIA='0', ENZIM_WPE_RUNTIME_SOURCE=source)
        launcher = args.runtime_launcher.resolve()
        metadata = json.loads((ROOT / 'DEPENDENCIES.json').read_text())
        runtime_arguments = ['--directory', str(runtime)]
        selected = metadata['native_runtime']
        if source == selected['source_commit'] and selected.get('source_pin'):
            pin = ROOT / selected['source_pin']
            if hashlib.sha256(pin.read_bytes()).hexdigest() != selected['source_pin_sha256']:
                raise ValueError('Reviewed browser source pin changed')
            runtime_arguments += ['--source-pin', str(pin)]
        launch = ['python3', str(launcher),
                  *runtime_arguments, 'run', '--', str(binary), '--browser', sites[0][1]]
    elif args.browser == 'chrome':
        # Chromium's singleton socket must fit the Unix-domain pathname limit.
        env['TMPDIR'] = env['XDG_RUNTIME_DIR']
        launch = [str(binary), '--user-data-dir=' + str(profile / 'chrome'),
                  '--no-first-run', '--no-default-browser-check', '--ozone-platform=wayland',
                  '--new-window', sites[0][1]]
        source = None
        program_dirs = [binary.parent]
    elif args.browser == 'webkitgtk':
        launch = [str(binary), '--profile=' + str(profile / 'epiphany')]
        if getattr(args, 'restore_visits', False):
            from xml.etree import ElementTree as ET
            session = ET.Element('session')
            window = ET.SubElement(session, 'window', {
                'width': '1400', 'height': '900', 'active-tab': '0',
                'is-maximized': '0', 'is-fullscreen': '0'})
            for title, uri in sites:
                ET.SubElement(window, 'embed', {'url': uri, 'title': title})
            path = profile / 'initial-session.xml'
            ET.ElementTree(session).write(path, encoding='utf-8', xml_declaration=True)
            launch += ['--load-session=' + str(path)]
        else:
            launch.append(sites[0][1])
        source = None
    else:
        ff = profile / 'firefox'
        ff.mkdir()
        settings = {'browser.shell.checkDefaultBrowser': False, 'browser.aboutwelcome.enabled': False,
                    'browser.startup.homepage_override.mstone': 'ignore', 'browser.newtabpage.enabled': False,
                    'datareporting.policy.dataSubmissionPolicyBypassNotification': True}
        (ff / 'user.js').write_text('\n'.join(f'user_pref({json.dumps(k)}, {json.dumps(v)});' for k, v in settings.items()))
        launch = [str(binary), '--no-remote', '--profile', str(ff), '--new-window', sites[0][1]]
        source = None
        program_dirs = [binary.parent]
    return launch, source, launcher, program_dirs


def run(args):
    from vncdotool import api
    full = args.workload == 'ten'
    sites = TEN_SITES if full else SITES
    phases = [('ten', 80), ('five', 80)] if full else [('three', 40), ('one', 12)]
    out = args.output.resolve()
    if out.exists():
        raise RuntimeError('Use a fresh output directory for each run')
    out.mkdir(parents=True)
    profile = out / 'profile'
    for name in ['config', 'data', 'cache', 'tmp']:
        (profile / name).mkdir(parents=True)
    runroot = args.desktop.resolve()
    env = os.environ.copy()
    env.pop('WEBKIT_SAMPLE_MEMORY', None)
    env.pop('ENZIM_MEMORY_TRACE', None)
    env.pop('LD_PRELOAD', None)
    env.pop('MALLOC_CONF', None)
    for k in list(env):
        if k.startswith(('JSC_', 'ENZIM_BENCH_', 'ENZUI_PROBE_', 'MALLOC_')):
            env.pop(k)
    env.update(WAYLAND_DISPLAY=(runroot / 'wl-display').read_text().strip(),
               SWAYSOCK=(runroot / 'swaysock').read_text().strip(),
               XDG_CONFIG_HOME=str(profile / 'config'), XDG_DATA_HOME=str(profile / 'data'),
               XDG_CACHE_HOME=str(profile / 'cache'), TMPDIR=str(profile / 'tmp'),
               XDG_SESSION_TYPE='wayland', MOZ_ENABLE_WAYLAND='1')
    if args.native_sampler and args.browser == 'enzim':
        env['WEBKIT_SAMPLE_MEMORY'] = '1'
    if args.trace_native and args.browser == 'enzim':
        env.update(ENZIM_BENCH_FRAME_TRACE='1', ENZIM_MEMORY_TRACE='1')
    elif args.trace_memory and args.browser == 'enzim':
        env['ENZIM_MEMORY_TRACE'] = '1'
    for item in args.setenv:
        k, v = item.split('=', 1)
        if not k.startswith(('JSC_', 'WEBKIT_')) and k not in (
                'ENZIM_BENCH_FULL_FILTER_ENGINES', 'ENZIM_BENCH_NATIVE_COSMETICS',
                'ENZIM_BENCH_SYSTEM_ALLOCATOR',
                'MALLOC_ARENA_MAX', 'MALLOC_MMAP_THRESHOLD_', 'GLIBC_TUNABLES',
                'LD_PRELOAD', 'MALLOC_CONF'):
            raise ValueError('Unsupported diagnostic override')
        if k == 'LD_PRELOAD' and v != '/usr/lib/libjemalloc.so.2':
            raise ValueError('Only the installed jemalloc allocator may be tested')
        env[k] = v
    binary = args.binary.resolve()
    binary_sha256 = hashlib.sha256(binary.read_bytes()).hexdigest()
    launch, source, launcher, program_dirs = browser_launch(args, profile, sites, env, binary)
    unit = 'enzim-short-' + args.case
    selected = ['WAYLAND_DISPLAY', 'XDG_RUNTIME_DIR', 'XDG_CONFIG_HOME', 'XDG_DATA_HOME', 'XDG_CACHE_HOME',
                'TMPDIR', 'XDG_SESSION_TYPE', 'MOZ_ENABLE_WAYLAND', 'PULSE_SINK',
                'ENZIM_WPE_ENCRYPTED_MEDIA', 'ENZIM_WPE_RUNTIME_SOURCE', 'WEBKIT_SAMPLE_MEMORY']
    if args.trace_native:
        selected += ['ENZIM_BENCH_FRAME_TRACE', 'ENZIM_MEMORY_TRACE']
    elif args.trace_memory:
        selected += ['ENZIM_MEMORY_TRACE']
    selected += [k for k, _ in (item.split('=', 1) for item in args.setenv)]
    start = ['systemd-run', '--user', '--collect', '--unit=' + unit,
             '-p', 'MemoryMax=' + args.memory_limit, '-p', 'MemorySwapMax=0']
    start += ['--setenv=' + k + '=' + env[k] for k in dict.fromkeys(selected) if k in env]
    harness_source = Path(__file__).read_bytes()
    (out / 'benchmark_memory_short.py').write_bytes(harness_source)
    started_utc = time.time()
    env.pop('LD_PRELOAD', None)
    env.pop('MALLOC_CONF', None)
    subprocess.run(start + ['--'] + launch, env=env, check=True)
    invocation = command(['systemctl', '--user', 'show', unit, '-p', 'InvocationID', '--value']).strip()
    source_patch = subprocess.check_output(
        ['git', 'diff', '--binary', 'HEAD', '--', 'Cargo.toml', 'Cargo.lock', 'src', 'resources'],
        cwd=ROOT)
    untracked = subprocess.check_output(
        ['git', 'ls-files', '--others', '--exclude-standard', '--',
         'Cargo.toml', 'Cargo.lock', 'src', 'resources'], cwd=ROOT, text=True).splitlines()
    for path in untracked:
        diff = subprocess.run(['git', 'diff', '--no-index', '--binary', '--', '/dev/null', path],
                              cwd=ROOT, capture_output=True, check=False)
        if diff.returncode not in (0, 1):
            raise RuntimeError('Could not snapshot untracked browser source')
        source_patch += diff.stdout
    (out / 'browser-source.patch').write_bytes(source_patch)
    result = {'case': args.case, 'browser': args.browser,
              'protocol': ('ten-five-v2-fixed-visits' if args.visit_window else 'ten-five-v1') if full else ('short-three-v2-fixed-review' if args.review_window else 'short-three-v1'),
              'restore_visits': args.restore_visits,
              'invocation': invocation,
              'started_utc': started_utc,
              'harness_sha256': hashlib.sha256(harness_source).hexdigest(),
              'browser_source_patch_sha256': hashlib.sha256(source_patch).hexdigest(),
              'browser_commit': command(['git', 'rev-parse', 'HEAD']).strip(),
              'sites': sites, 'runtime_source': source, 'binary_sha256': binary_sha256,
              'executable_prewarmed': True,
              'memory_limit': args.memory_limit,
              'review_each_visit': args.review_each_visit,
              'review_window_seconds': args.review_window,
              'visit_window_seconds': args.visit_window,
              'phase_inspection_seconds': args.phase_inspection_window,
              'settling_seconds': dict(phases),
              'sample_count': 6, 'sample_interval_seconds': 2 if full else 1,
              'native_sampler': args.native_sampler, 'trace_native': args.trace_native,
              'trace_memory': args.trace_memory,
              'runtime_launcher': str(launcher) if args.browser == 'enzim' else None,
              'overrides': args.setenv, 'stages': {}, 'visits': []}
    if args.restore_visits:
        result['protocol'] += '-gnome-delayed-session'
    additional_limits = {}
    result['additional_owned_scope_limits'] = additional_limits

    def sample():
        error = None
        for _ in range(4):
            try:
                s = collect(unit + '.service', profile, program_dirs)
                enforce_owned_limits(s, args.memory_limit, additional_limits)
                require_audited(s)
                return s
            except (RuntimeError, FileNotFoundError) as e:
                error = e
                time.sleep(.3)
        raise error

    try:
        deadline = time.monotonic() + 30
        while True:
            owned = {p['pid'] for p in sample()['processes']}
            matches = [w for w in windows(env) if w['pid'] in owned]
            if args.restore_visits and len(matches) == 2:
                blank = [w for w in matches if w['name'] == 'New Tab']
                if len(blank) == 1:
                    command(['swaymsg', f'[con_id={blank[0]["id"]}] kill'], env)
                    result.setdefault('closed_initial_empty_windows', []).append(blank[0])
                    time.sleep(.5)
                    continue
            if len(matches) == 1:
                window = matches[0]
                break
            if time.monotonic() > deadline:
                raise RuntimeError('No unique owned rendered window')
            time.sleep(.5)
        command(['swaymsg', f'[con_id={window["id"]}] focus'], env)
        time.sleep(9)
        with api.connect(f'127.0.0.1::{args.port}', timeout=20) as client:
            client.keyUp('ctrl')
            client.keyUp('shift')
            click(client, 350, 28)
            key(client, 'esc')
            for i, (site, url) in enumerate(sites):
                visit_started = time.monotonic()
                if i:
                    if args.restore_visits:
                        browser_key(client, 'ctrl-Page_Down', env, args.browser)
                    else:
                        browser_key(client, 'ctrl-t', env, args.browser)
                        time.sleep(.5)
                        enter_address(client, url, env, args.browser)
                time.sleep(15 if full else 8)
                capture(env, window, out / f'{i+1}-{site}.png')
                result['visits'].append(next(w for w in windows(env) if w['id'] == window['id']))
                result['visits'][-1]['started_monotonic'] = visit_started
                print(args.case, 'VISIT', site, result['visits'][-1]['name'], flush=True)
                if args.review_each_visit:
                    (out / f'visit-{i+1}-ready').touch()
                    while not (out / f'visit-{i+1}-done').exists():
                        if args.visit_window and time.monotonic() - visit_started > args.visit_window:
                            raise RuntimeError('Visit review exceeded its fixed time window')
                        time.sleep(.5)
                result['visits'][-1]['reviewed_elapsed_seconds'] = time.monotonic() - visit_started
                if args.visit_window:
                    remaining = args.visit_window - (time.monotonic() - visit_started)
                    if remaining < 0:
                        raise RuntimeError('Visit review exceeded its fixed time window')
                    time.sleep(remaining)
                if full:
                    runtime_log = unit_log(unit, started_utc)
                    require_healthy_runtime(runtime_log)
                    if args.browser == 'enzim':
                        require_loaded_views(runtime_log, sample(), i+1)
            review_started = time.monotonic()
            (out / 'review-ready').touch()
            print(args.case, 'REVIEW READY', flush=True)
            while not (out / 'review-done').exists():
                if args.review_window and time.monotonic() - review_started > args.review_window:
                    raise RuntimeError('Review exceeded its fixed time window')
                time.sleep(.5)
            result['review_elapsed_seconds'] = time.monotonic() - review_started
            if args.review_window:
                remaining = args.review_window - result['review_elapsed_seconds']
                if remaining < 0:
                    raise RuntimeError('Review exceeded its fixed time window')
                time.sleep(remaining)
            if args.browser != 'enzim':
                modifier = 'ctrl' if args.browser == 'chrome' else 'alt'
                browser_key(client, modifier + ('-5' if full else '-1'), env, args.browser)
            else:
                click(client, 600 if full else 90, 70)
            if full:
                current = next(w for w in windows(env) if w['id'] == window['id'])
                if 'Wikipedia' not in current['name']:
                    raise RuntimeError('The active retained page must be Wikipedia')
            client.mouseMove(1399, 899)
            for phase, settling in phases:
                if phase in ('one', 'five'):
                    if args.browser == 'webkitgtk':
                        for _ in range(5 if full else 2):
                            browser_key(client, 'ctrl-Page_Down', env, args.browser)
                    elif args.browser != 'enzim':
                        browser_key(client, ('ctrl' if args.browser == 'chrome' else 'alt') + '-9', env, args.browser)
                    elif full:
                        for _ in range(5):
                            key(client, 'ctrl-tab')
                    else:
                        click(client, 480, 70)
                    for _ in range(5 if full else 2):
                        browser_key(client, 'ctrl-w', env, args.browser)
                    client.mouseMove(1399, 899)
                cpu_start = cpu_usage(sample())
                phase_start = time.monotonic()
                time.sleep(settling)
                stage = {'samples': []}
                for _ in range(6):
                    stage['samples'].append(sample())
                    time.sleep(2 if full else 1)
                runtime_log = unit_log(unit, started_utc)
                require_healthy_runtime(runtime_log)
                if args.browser == 'enzim' and (full or args.trace_memory):
                    require_loaded_views(runtime_log, stage['samples'][-1],
                                         {'ten': 10, 'five': 5, 'three': 3, 'one': 1}[phase])
                if args.native_sampler and args.browser == 'enzim':
                    stage['allocation_details'] = native_details(stage['samples'][-1], out / phase)
                stage['elapsed_seconds'] = time.monotonic() - phase_start
                stage['all_process_cpu_seconds'] = (
                    cpu_usage(stage['samples'][-1]) - cpu_start) / 1e6
                result['stages'][phase] = stage
                (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
                capture(env, window, out / f'{phase}-settled.png')
                stage['window'] = next(w for w in windows(env) if w['id'] == window['id'])
                (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
                import statistics
                print(args.case, phase, 'PSS MiB', round(statistics.median(s['ram_pss_bytes'] for s in stage['samples']) / 2**20, 1), flush=True)
                if args.phase_inspection_window:
                    inspection_started = time.monotonic()
                    (out / f'{phase}-inspection-ready').touch()
                    while not (out / f'{phase}-inspection-done').exists():
                        if time.monotonic() - inspection_started > args.phase_inspection_window:
                            raise RuntimeError('Phase inspection exceeded its fixed time window')
                        time.sleep(.2)
                    time.sleep(max(0, args.phase_inspection_window - (time.monotonic() - inspection_started)))
            click(client, 700, 500)
            client.mousePress(5)
            client.mousePress(5)
            time.sleep(1)
            capture(env, window, out / 'retained-scroll.png')
            (out / 'final-review-ready').touch()
            while not (out / 'final-review-done').exists():
                time.sleep(.5)
    except Exception as error:
        result['failure'] = str(error)
        (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
        if args.keep_on_failure:
            (out / 'failed-review-ready').touch()
            print(args.case, 'FAILED REVIEW', str(error), flush=True)
            while not (out / 'failed-review-done').exists():
                time.sleep(.5)
        raise
    finally:
        subprocess.run(['systemctl', '--user', 'stop', unit], check=True)
        for owned_scope in additional_limits:
            subprocess.run(['systemctl', '--user', 'stop', owned_scope], check=False,
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        journal = unit_log(unit, started_utc)
        (out / 'journal.log').write_text(journal)
        api.shutdown()
    result['completed_utc'] = time.time()
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--browser', choices=['enzim', 'firefox', 'chrome', 'webkitgtk'], required=True)
    parser.add_argument('--workload', choices=['short', 'ten'], default='short')
    parser.add_argument('--review-each-visit', action='store_true')
    parser.add_argument('--visit-window', type=float, default=0,
                        help='Fixed seconds per visit including navigation and review; overrun refuses comparison')
    parser.add_argument('--case', required=True)
    parser.add_argument('--review-window', type=float, default=0,
                        help='Fixed seconds for the post-navigation review, including idle time')
    parser.add_argument('--phase-inspection-window', type=float, default=0,
                        help='Optional fixed post-sample diagnostic pause; recorded separately from normal runs')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--runtime', type=Path)
    parser.add_argument('--runtime-launcher', type=Path,
                        default=ROOT / json.loads((ROOT / 'DEPENDENCIES.json').read_text())['webkit']['path'] / 'tools/source_runtime.py')
    parser.add_argument('--desktop', type=Path, required=True,
                        help='Dedicated compositor directory containing wl-display and swaysock')
    parser.add_argument('--restore-visits', action='store_true',
                        help='GNOME Web: activate initially delayed session tabs in visit order')
    parser.add_argument('--port', type=int, default=5925)
    parser.add_argument('--memory-limit', choices=['3G', '8G'], default='3G')
    parser.add_argument('--native-sampler', action='store_true')
    parser.add_argument('--trace-native', action='store_true')
    parser.add_argument('--trace-memory', action='store_true')
    parser.add_argument('--keep-on-failure', action='store_true')
    parser.add_argument('--setenv', action='append', default=[])
    arguments = parser.parse_args()
    if arguments.restore_visits and arguments.browser != 'webkitgtk':
        parser.error('--restore-visits is specific to GNOME Web')
    if arguments.visit_window and (arguments.visit_window < 20 or not arguments.review_each_visit):
        parser.error('--visit-window requires per-visit review and at least 20 seconds')
    if arguments.review_window and (arguments.workload != 'short' or arguments.review_window < 30):
        parser.error('--review-window applies to short runs and requires at least 30 seconds')
    if arguments.phase_inspection_window < 0:
        parser.error('--phase-inspection-window cannot be negative')
    if arguments.browser == 'enzim' and arguments.runtime is None:
        parser.error('--runtime is required for Enzim')
    if arguments.browser != 'enzim' and (
            arguments.native_sampler or arguments.trace_native or arguments.trace_memory):
        parser.error('Native sampler and tracing options apply to Enzim')
    if arguments.browser == 'enzim' and arguments.workload == 'ten' and not (
            arguments.trace_native or arguments.trace_memory):
        parser.error('The loaded-tab milestone requires memory-state tracing')
    run(arguments)
