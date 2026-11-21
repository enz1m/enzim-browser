#!/usr/bin/env python3
"""Fast real-site speed, memory and CPU comparison against Chromium and Firefox.

Each browser gets a fresh profile inside one shared headless compositor. Every
site opens in a new foreground tab and stays open, so one pass yields paint and
load timings, then settled process memory and total CPU for the same workload.
Timings come from each page's own Performance entries; `open_*` values add the
time between the driver's request and the page's time origin.
"""
import argparse
import json
import math
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import sys
import time
import traceback
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
DEPENDENCIES = json.loads((ROOT / 'DEPENDENCIES.json').read_text())
RUNTIME_DIRECTORY = Path(os.environ.get('XDG_RUNTIME_DIR', '/tmp')) / 'enzim-bench'
DESK_UNIT = 'enzim-bench-desk'
WINDOW = (1400, 900)
# Session helpers that a private bus starts beside the browser; they are not
# browser memory.
HELPERS = re.compile(r'^(dbus-|xdg-|gvfs|dconf|at-spi|systemd|gnome-|fusermount|bwrap$|sh$|sleep$|python3?$)')


def process_role(name, command):
    """One of browser, renderer, gpu, network, other, from a process's name and command line."""
    kind = re.search(r'--type=([\w-]+)', command)
    if kind:  # Chromium
        return {'renderer': 'renderer', 'gpu-process': 'gpu', 'zygote': 'other'}.get(
            kind.group(1), 'network' if 'network.mojom' in command else 'other')
    if name.startswith('WPEWeb'):
        return 'renderer'
    if name.startswith('WPEGPU'):
        return 'gpu'
    if name.startswith('WPENetwork'):
        return 'network'
    if name in ('Isolated Web Co', 'Web Content', 'WebExtensions', 'Privileged Cont', 'Isolated Servic'):
        return 'renderer'
    if name in ('RDD Process', 'Utility Process', 'forkserver', 'chrome_crashpad'):
        return 'other'
    if name == 'Socket Process':
        return 'network'
    return 'browser'


def mapping_groups(pid):
    """PSS of one process by kind of mapping, largest first."""
    groups = {}
    label = None
    try:
        lines = Path(f'/proc/{pid}/smaps').read_text().splitlines()
    except OSError:
        return {}
    for line in lines:
        head = line.split()
        if head and '-' in head[0] and not head[0].endswith(':'):
            path = ' '.join(head[5:]) if len(head) > 5 else ''
            if not path:
                label = 'anonymous'
            elif path.startswith('[anon:'):
                label = path
            elif path.startswith('['):
                label = path
            elif path.startswith('/memfd:') or path.startswith('/dev/shm') or path.startswith('/SYSV'):
                label = 'shared memory: ' + re.sub(r'[\d.]+', '', path.split(' (deleted)')[0])[:40]
            elif '/dri/' in path or path.startswith('/dev/dri') or 'i915' in path or 'dmabuf' in path:
                label = 'gpu mapping'
            elif re.search(r'\.(ttf|otf|ttc|woff2?)$', path) or '/fonts/' in path:
                label = 'fonts'
            elif '.so' in path or '/bin/' in path or '/libexec/' in path:
                label = 'code: ' + path.rsplit('/', 1)[-1].split('.so')[0][:32]
            else:
                label = 'file: ' + path.rsplit('/', 1)[-1][:32]
        elif head and head[0] == 'Pss:' and label is not None:
            groups[label] = groups.get(label, 0) + int(head[1]) * 1024
    return dict(sorted(groups.items(), key=lambda item: -item[1])[:24])


PAGE_METRICS = """(() => {
  const n = performance.getEntriesByType('navigation')[0];
  const paint = name => (performance.getEntriesByType('paint').find(p => p.name === name) || {}).startTime || 0;
  return JSON.stringify({origin: performance.timeOrigin, url: location.href, title: document.title,
    ready: document.readyState, fp: paint('first-paint'), fcp: paint('first-contentful-paint'),
    response: n ? n.responseStart : 0, dcl: n ? n.domContentLoadedEventEnd : 0,
    load: n ? n.loadEventEnd : 0, transfer: n ? n.transferSize : 0,
    nodes: document.getElementsByTagName('*').length, text: (document.body ? document.body.innerText.length : 0),
    viewport: [innerWidth, innerHeight]});
})()"""


RESOURCE_ROWS = """JSON.stringify(performance.getEntriesByType('resource').map(e => [e.name.slice(0, 120),
  e.initiatorType, Math.round(e.startTime), Math.round(e.domainLookupStart), Math.round(e.connectStart),
  Math.round(e.secureConnectionStart), Math.round(e.connectEnd), Math.round(e.requestStart),
  Math.round(e.responseStart), Math.round(e.responseEnd), e.transferSize, e.decodedBodySize, e.nextHopProtocol]))"""
RESOURCE_FIELDS = ('name', 'initiator', 'start', 'dns', 'connect', 'tls', 'connected', 'request', 'response',
                   'end', 'transfer', 'decoded', 'protocol')


def run(command, **options):
    return subprocess.run(command, check=True, capture_output=True, text=True, **options).stdout


def desk():
    """Return the compositor environment, starting the shared headless desk once."""
    RUNTIME_DIRECTORY.mkdir(mode=0o700, exist_ok=True)
    display, socket = RUNTIME_DIRECTORY / 'wl-display', RUNTIME_DIRECTORY / 'swaysock'
    active = subprocess.run(['systemctl', '--user', 'is-active', '--quiet', DESK_UNIT]).returncode == 0
    if not active:
        for stale in (display, socket):
            stale.unlink(missing_ok=True)
        config = RUNTIME_DIRECTORY / 'sway.conf'
        config.write_text(
            f'output HEADLESS-1 resolution {WINDOW[0]}x{WINDOW[1]} position 0,0 scale 1\n'
            'default_border none\ndefault_floating_border none\nfocus_follows_mouse no\nxwayland disable\n'
            f"exec_always sh -c 'echo $WAYLAND_DISPLAY > {display}; echo $SWAYSOCK > {socket}'\n")
        run(['systemd-run', '--user', '--collect', '--unit=' + DESK_UNIT,
             '--setenv=WLR_BACKENDS=headless', '--setenv=WLR_LIBINPUT_NO_DEVICES=1',
             '-p', 'UnsetEnvironment=WAYLAND_DISPLAY DISPLAY SWAYSOCK', '--', 'sway', '-c', str(config)])
    deadline = time.monotonic() + 10
    while not (display.exists() and socket.exists() and socket.read_text().strip()):
        if time.monotonic() > deadline:
            raise RuntimeError('Headless compositor did not start')
        time.sleep(0.1)
    return {'WAYLAND_DISPLAY': display.read_text().strip(), 'SWAYSOCK': socket.read_text().strip(),
            'XDG_SESSION_TYPE': 'wayland', 'MOZ_ENABLE_WAYLAND': '1'}


def free_port():
    import socket
    with socket.socket() as probe:
        probe.bind(('127.0.0.1', 0))
        return probe.getsockname()[1]


class Socket:
    """Minimal JSON request/response client for CDP and WebDriver BiDi."""

    def __init__(self, address):
        from websockets.sync.client import connect
        self.connection = connect(address, max_size=None, open_timeout=10)
        self.serial = 0

    def call(self, method, params=None, session=None, timeout=15):
        self.serial += 1
        message = {'id': self.serial, 'method': method, 'params': params or {}}
        if session:
            message['sessionId'] = session
        self.connection.send(json.dumps(message))
        deadline = time.monotonic() + timeout
        while True:
            reply = json.loads(self.connection.recv(timeout=max(0.05, deadline - time.monotonic())))
            if reply.get('id') == self.serial:
                if reply.get('error') or reply.get('type') == 'error':
                    raise RuntimeError(f'{method}: {reply}')
                return reply.get('result', reply)

    def close(self):
        self.connection.close()


class Browser:
    name = ''

    def __init__(self, arguments, folder, environment):
        self.arguments, self.folder = arguments, folder
        self.unit = f'enzim-bench-{self.name}-{os.getpid()}-{int(time.time() * 1000) % 100000}'
        self.log = folder / 'browser.log'
        self.profile = folder / 'profile'
        self.environment = dict(environment)
        for variable, leaf in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache')]:
            path = self.profile / leaf
            path.mkdir(parents=True)
            self.environment[variable] = str(path)

    def launch(self, command):
        variables = [f'--setenv={key}={value}' for key, value in self.environment.items()]
        run(['systemd-run', '--user', '--collect', '--unit=' + self.unit,
             '-p', 'MemoryMax=' + self.arguments.memory_max, '-p', 'MemorySwapMax=0',
             '-p', f'StandardOutput=file:{self.log}', '-p', f'StandardError=file:{self.log}',
             '-p', 'UnsetEnvironment=DISPLAY', '-p', f'WorkingDirectory={self.folder}',
             *variables, '--', *command])
        self.group = Path('/sys/fs/cgroup') / run(
            ['systemctl', '--user', 'show', '-p', 'ControlGroup', '--value', self.unit]).strip().lstrip('/')

    def stop(self):
        subprocess.run(['systemctl', '--user', 'stop', self.unit], capture_output=True)

    def cpu_seconds(self):
        for line in (self.group / 'cpu.stat').read_text().splitlines():
            if line.startswith('usage_usec'):
                return int(line.split()[1]) / 1e6
        return 0.0

    def memory(self, detail=False):
        """Memory of every browser process in the unit, and the unit's own accounting.

        PSS divides shared pages among the processes that map them, so the sum
        counts each page once. It misses memory that no process has mapped:
        buffers handed to the GPU driver, shared memory held only by descriptor
        and page cache. The control group charges those to the unit, so its
        totals are reported beside the PSS sum, and DRM clients report what the
        GPU driver holds for them.
        """
        processes = []
        pids = []
        for pid in (self.group / 'cgroup.procs').read_text().split():
            try:
                name = Path(f'/proc/{pid}/comm').read_text().strip()
                rollup = Path(f'/proc/{pid}/smaps_rollup').read_text()
                command = Path(f'/proc/{pid}/cmdline').read_bytes().replace(b'\0', b' ').decode(errors='replace')
            except OSError:
                continue
            if HELPERS.match(name):
                continue
            pids.append(int(pid))
            fields = {line.split(':')[0]: int(line.split()[1]) * 1024
                      for line in rollup.splitlines()[1:] if line.split()[1].isdigit()}
            try:
                # Fields 14 and 15 of stat, counted after the parenthesized name.
                stat = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()
                cpu = (int(stat[11]) + int(stat[12])) / os.sysconf('SC_CLK_TCK')
            except (OSError, IndexError, ValueError):
                cpu = None
            entry = {'pid': int(pid), 'name': name, 'role': process_role(name, command), 'pss': fields.get('Pss', 0),
                     'pss_anon': fields.get('Pss_Anon', 0), 'pss_file': fields.get('Pss_File', 0),
                     'pss_shmem': fields.get('Pss_Shmem', 0),
                     'private': fields.get('Private_Clean', 0) + fields.get('Private_Dirty', 0),
                     'rss': fields.get('Rss', 0), 'swap': fields.get('SwapPss', 0), 'cpu_seconds': cpu}
            if detail:
                entry['mappings'] = mapping_groups(pid)
            processes.append(entry)
        events = dict(line.split() for line in (self.group / 'memory.events').read_text().splitlines())
        stat = dict(line.split() for line in (self.group / 'memory.stat').read_text().splitlines())
        try:
            from memory_snapshot import gpu_info
            gpu = gpu_info(pids)
            names = {p['pid']: p['name'] for p in processes}

            def resident(client, shared=False, purgeable=False):
                prefix = 'drm-purgeable-' if purgeable else 'drm-shared-' if shared else 'drm-resident-'
                return sum(int(value.split()[0]) * {'KiB': 1024, 'MiB': 2 ** 20}.get(value.split()[-1], 1)
                           for key, value in client['fields'].items() if key.startswith(prefix))

            gpu = {'resident_lower_bytes': gpu['bounds']['resident_lower_bytes'],
                   'resident_upper_bytes': gpu['bounds']['resident_upper_bytes'],
                   # Resident buffers the driver may drop under memory pressure: its cache of freed
                   # buffers. Part of the resident figures, and zero where a driver does not report it.
                   'purgeable_bytes': sum(resident(c, purgeable=True) for c in gpu['clients']),
                   'clients': gpu['unique_client_count'], 'unreadable_pids': gpu['unreadable_pids'],
                   # One row per DRM client: who holds it, what the driver keeps resident for it, how
                   # much of that other clients share and how much is purgeable.
                   'by_client': sorted(({'holders': sorted({names.get(h['pid'], str(h['pid'])) for h in c['holders']}),
                                         'resident_bytes': resident(c), 'shared_bytes': resident(c, True),
                                         'purgeable_bytes': resident(c, purgeable=True)}
                                        for c in gpu['clients']), key=lambda row: -row['resident_bytes'])}
        except Exception as error:  # noqa: BLE001 - GPU accounting is optional detail
            gpu = {'error': repr(error)}
        return {'pss_bytes': sum(p['pss'] for p in processes), 'rss_bytes': sum(p['rss'] for p in processes),
                'private_bytes': sum(p['private'] for p in processes),
                'pss_anon_bytes': sum(p['pss_anon'] for p in processes),
                'pss_file_bytes': sum(p['pss_file'] for p in processes),
                'pss_shmem_bytes': sum(p['pss_shmem'] for p in processes),
                'swap_bytes': sum(p['swap'] for p in processes), 'process_count': len(processes),
                'cgroup_peak_bytes': int((self.group / 'memory.peak').read_text()),
                'cgroup_current_bytes': int((self.group / 'memory.current').read_text()),
                'cgroup_bytes': {key: int(stat.get(key, 0)) for key in
                                 ('anon', 'file', 'shmem', 'file_mapped', 'kernel', 'slab', 'sock', 'pagetables')},
                'gpu': gpu,
                'pressure_events': {key: int(events.get(key, 0)) for key in ('high', 'max', 'oom', 'oom_kill')},
                'processes': sorted(processes, key=lambda p: -p['pss'])}


class Enzim(Browser):
    name = 'enzim'

    def start(self):
        runtime = Path(self.arguments.runtime).resolve()
        source = json.loads((runtime / 'source-runtime.json').read_text())['source']['source_commit']
        native = DEPENDENCIES['native_runtime']
        launcher = [sys.executable, str(ROOT / DEPENDENCIES['webkit']['path'] / 'tools/source_runtime.py'),
                    '--directory', str(runtime)]
        if source == native['source_commit'] and not self.arguments.source_pin:
            launcher += ['--source-pin', str(ROOT / native['source_pin'])]
        elif self.arguments.source_pin:
            launcher += ['--source-pin', str(Path(self.arguments.source_pin).resolve())]
        settings = self.profile / 'config/enzim-browser'
        settings.mkdir()
        choice = getattr(self.arguments, 'media_registry', 'desktop')
        if choice != 'cold':
            # The media plugin registry is a cache of the desktop, kept under
            # the user's cache folder and shared by every program that plays
            # media. A pass in an empty cache folder would rebuild it inside
            # the first page that asks about media, about 0.3 s that a browser
            # in use does not spend.
            kept = Path(os.environ.get('XDG_CACHE_HOME') or Path.home() / '.cache') / 'gstreamer-1.0'
            if choice != 'desktop':
                kept = Path(choice)
            registries = [kept] if kept.is_file() else sorted(kept.glob('registry.*.bin'))
            target = self.profile / 'cache/gstreamer-1.0'
            target.mkdir(parents=True, exist_ok=True)
            for registry in registries:
                name = registry.name if registry.name.startswith('registry.') else f'registry.{os.uname().machine}.bin'
                shutil.copy2(registry, target / name)
        (settings / 'session.json').write_text(json.dumps({'layout': 'Top', **getattr(self.arguments, 'session', {})}))
        self.environment.update(ENZIM_WPE_RUNTIME_SOURCE=source, ENZIM_WPE_ENCRYPTED_MEDIA='0',
                                ENZIM_BENCH_DOCUMENT_TIMING='1', ENZIM_BENCH_NAVIGATION_TRACE='1')
        if self.arguments.resources:
            self.environment['ENZIM_BENCH_RESOURCE_ENTRIES'] = '1'
        for item in self.arguments.env:
            key, _, value = item.partition('=')
            self.environment[key] = value
        self.bus_file = self.folder / 'bus-address'
        binary = str(Path(self.arguments.enzim_binary).resolve())
        script = f'echo "$DBUS_SESSION_BUS_ADDRESS" > {self.bus_file}; exec "$@"'
        # A private bus keeps the disposable instance from forwarding into a
        # browser the user already has open.
        self.launch(['dbus-run-session', '--', 'sh', '-c', script, 'sh', *launcher, 'run', '--',
                     binary, '--browser', '', '--skip-welcome'])
        from gi.repository import Gio
        deadline = time.monotonic() + 30
        self.bus = self.destination = None
        while time.monotonic() < deadline and not self.destination:
            time.sleep(0.2)
            if not (self.bus_file.exists() and self.bus_file.read_text().strip()):
                continue
            try:
                self.bus = self.bus or Gio.DBusConnection.new_for_address_sync(
                    self.bus_file.read_text().strip(),
                    Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
                    None, None)
                names = self.bus.call_sync('org.freedesktop.DBus', '/org/freedesktop/DBus', 'org.freedesktop.DBus',
                                           'ListNames', None, None, 0, 2000, None).unpack()[0]
                self.destination = next((n for n in names if n.startswith('org.enzim.Browser')), None)
            except Exception:  # noqa: BLE001 - the bus is still starting
                continue
        if not self.destination:
            raise RuntimeError('enzim Browser did not register on its bus')
        self.offset = 0
        self.resource_rows = {}
        self.version = {'binary': binary, 'runtime_source': source, 'env': self.arguments.env}

    def records(self):
        with self.log.open('rb') as handle:
            handle.seek(self.offset)
            data = handle.read()
        end = data.rfind(b'\n') + 1
        self.offset += end
        timings, traces = [], []
        for line in data[:end].decode(errors='replace').splitlines():
            if line.startswith('DOCUMENT_RESOURCE {'):
                try:
                    payload = json.loads(line[len('DOCUMENT_RESOURCE '):]).get('payload', {})
                except ValueError:
                    continue
                if payload.get('event') == 'entries':
                    self.resource_rows.setdefault(payload.get('origin'), {})[payload.get('index')] = payload['entries']
            elif line.startswith('DOCUMENT_TIMING {'):
                try:
                    timings.append(json.loads(line[len('DOCUMENT_TIMING '):]))
                except ValueError:
                    pass
            elif line.startswith('NAVIGATION_TRACE '):
                parts = line.split()
                if len(parts) == 3 and parts[2].isdigit():
                    traces.append((parts[1], int(parts[2]) / 1e6))
        return timings, traces

    def open(self, url):
        from gi.repository import GLib
        self.records()
        self.pending = {'url': url, 'timings': [], 'traces': []}
        request = GLib.Variant('(asa{sv})', ([url], {}))
        started = time.time()
        self.bus.call_sync(self.destination, '/' + self.destination.replace('.', '/'),
                           'org.freedesktop.Application', 'Open', request, None, 0, 5000, None)
        return started

    def poll(self):
        timings, traces = self.records()
        self.pending['timings'] += timings
        self.pending['traces'] += traces
        host = re.sub(r'^www\.', '', self.pending['url'].split('/')[2])
        latest = None
        for record in self.pending['timings']:
            payload = record.get('payload', {})
            if host.split('.')[-2] in payload.get('initialURL', '') + payload.get('url', ''):
                latest = payload
        if not latest:
            return None
        navigation = latest.get('navigation') or {}
        paints = {p['name']: p['start'] for p in latest.get('paints', [])}
        trace = dict(reversed(self.pending['traces']))
        return {'origin': latest['origin'], 'url': latest.get('url'), 'title': self.title(),
                'ready': latest.get('readyState'), 'fp': paints.get('first-paint', 0),
                'fcp': paints.get('first-contentful-paint', 0), 'response': navigation.get('responseStart', 0),
                'dcl': navigation.get('domContentLoadedEventEnd', 0), 'load': navigation.get('loadEventEnd', 0),
                'transfer': navigation.get('transferSize', 0), 'viewport': latest.get('viewport'),
                'submitted_wall': trace.get('navigation-submitted'), 'load_uri_wall': trace.get('load-uri')}

    def resources(self, origin):
        self.records()
        chunks = self.resource_rows.get(origin, {})
        return [row for index in sorted(chunks) for row in chunks[index]]

    def title(self):
        tree = json.loads(run(['swaymsg', '-t', 'get_tree', '-r'], env={**os.environ, **self.environment}))
        names = []

        def visit(node):
            if node.get('app_id') == 'org.enzim.Browser':
                names.append(node.get('name') or '')
            for child in node.get('nodes', []) + node.get('floating_nodes', []):
                visit(child)
        visit(tree)
        return names[0].removesuffix(' - enzim Browser') if names else ''


class Chrome(Browser):
    name = 'chrome'

    def start(self):
        binary = shutil.which(self.arguments.chrome_binary) or self.arguments.chrome_binary
        self.port = free_port()
        self.launch([binary, f'--user-data-dir={self.profile / "chrome"}', '--no-first-run',
                     '--no-default-browser-check', '--disable-search-engine-choice-screen',
                     '--ozone-platform=wayland', f'--window-size={WINDOW[0]},{WINDOW[1]}',
                     f'--remote-debugging-port={self.port}', '--remote-allow-origins=*', 'about:blank'])
        deadline = time.monotonic() + 30
        while True:
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{self.port}/json/version', timeout=1) as reply:
                    information = json.load(reply)
                break
            except OSError:
                if time.monotonic() > deadline:
                    raise RuntimeError('Chromium debugging endpoint did not start') from None
                time.sleep(0.2)
        self.socket = Socket(information['webSocketDebuggerUrl'])
        self.version = {'binary': binary, 'version': information.get('Browser')}

    def open(self, url):
        started = time.time()
        target = self.socket.call('Target.createTarget', {'url': url})['targetId']
        self.session = self.socket.call('Target.attachToTarget', {'targetId': target, 'flatten': True})['sessionId']
        return started

    def poll(self):
        try:
            reply = self.socket.call('Runtime.evaluate', {'expression': PAGE_METRICS, 'returnByValue': True},
                                     session=self.session, timeout=5)
            return json.loads(reply['result']['value'])
        except (RuntimeError, KeyError, ValueError, TimeoutError):
            return None

    def resources(self, _origin):
        try:
            reply = self.socket.call('Runtime.evaluate', {'expression': RESOURCE_ROWS, 'returnByValue': True},
                                     session=self.session, timeout=5)
            return json.loads(reply['result']['value'])
        except (RuntimeError, KeyError, ValueError, TimeoutError):
            return []

    def stop(self):
        try:
            self.socket.close()
        except Exception:  # noqa: BLE001 - the browser may already be gone
            pass
        super().stop()


class Firefox(Browser):
    name = 'firefox'

    def start(self):
        binary = shutil.which(self.arguments.firefox_binary) or self.arguments.firefox_binary
        self.port = free_port()
        profile = self.profile / 'firefox'
        profile.mkdir()
        preferences = {
            'browser.shell.checkDefaultBrowser': False, 'browser.aboutwelcome.enabled': False,
            'browser.startup.homepage_override.mstone': 'ignore', 'browser.newtabpage.enabled': False,
            'datareporting.policy.dataSubmissionPolicyBypassNotification': True,
            'browser.startup.page': 0, 'app.update.auto': False, 'app.normandy.enabled': False,
            'remote.active-protocols': 1, 'browser.tabs.warnOnClose': False,
            'toolkit.telemetry.reportingpolicy.firstRun': False, 'browser.sessionstore.resume_from_crash': False,
        }
        (profile / 'user.js').write_text(
            '\n'.join(f'user_pref({json.dumps(k)},{json.dumps(v)});' for k, v in preferences.items()))
        self.launch([binary, '--no-remote', '--profile', str(profile), f'--remote-debugging-port={self.port}',
                     '--width', str(WINDOW[0]), '--height', str(WINDOW[1]), '--new-window', 'about:blank'])
        deadline = time.monotonic() + 40
        while True:
            try:
                self.socket = Socket(f'ws://127.0.0.1:{self.port}/session')
                break
            except OSError:
                if time.monotonic() > deadline:
                    raise RuntimeError('Firefox remote agent did not start') from None
                time.sleep(0.3)
        session = self.socket.call('session.new', {'capabilities': {}}, timeout=30)
        capabilities = session.get('capabilities', {})
        self.version = {'binary': binary, 'version': capabilities.get('browserVersion')}

    def open(self, url):
        started = time.time()
        self.context = self.socket.call('browsingContext.create', {'type': 'tab'})['context']
        self.socket.call('browsingContext.navigate', {'context': self.context, 'url': url, 'wait': 'none'})
        return started

    def poll(self):
        try:
            reply = self.socket.call('script.evaluate', {
                'expression': PAGE_METRICS, 'target': {'context': self.context}, 'awaitPromise': False}, timeout=5)
            return json.loads(reply['result']['value'])
        except (RuntimeError, KeyError, ValueError, TimeoutError):
            return None

    def resources(self, _origin):
        try:
            reply = self.socket.call('script.evaluate', {
                'expression': RESOURCE_ROWS, 'target': {'context': self.context}, 'awaitPromise': False}, timeout=5)
            return json.loads(reply['result']['value'])
        except (RuntimeError, KeyError, ValueError, TimeoutError):
            return []

    def stop(self):
        try:
            self.socket.close()
        except Exception:  # noqa: BLE001 - the browser may already be gone
            pass
        super().stop()


BROWSERS = {'enzim': Enzim, 'chrome': Chrome, 'firefox': Firefox}


def measure(arguments, name, sites, folder, environment):
    folder.mkdir(parents=True)
    browser = BROWSERS[name](arguments, folder, environment)
    result = {'browser': name, 'started_wall': time.time(), 'host_load': Path('/proc/loadavg').read_text().split()[:3],
              'sites': []}
    try:
        browser.start()
        result['identity'] = browser.version
        time.sleep(arguments.warmup)
        cpu_start = browser.cpu_seconds()
        for site in sites:
            requested = browser.open(site['url'])
            deadline = time.monotonic() + arguments.timeout
            metrics, settled = None, None
            while time.monotonic() < deadline:
                time.sleep(0.1)
                current = browser.poll()
                if current and current.get('origin', 0) / 1000 >= requested - 1:
                    metrics = current
                    if current.get('load') and current.get('fcp'):
                        settled = settled or time.monotonic() + arguments.settle
                        if time.monotonic() >= settled:
                            break
            entry = {'site': site['name'], 'url': site['url'], 'requested_wall': requested, 'metrics': metrics}
            if metrics:
                lead = metrics['origin'] / 1000 - requested
                entry.update(
                    title_ok=site['title'].lower() in (metrics.get('title') or '').lower(),
                    complete=bool(metrics.get('load') and metrics.get('fcp')),
                    open_to_origin_ms=lead * 1000,
                    **{key + '_ms': metrics.get(key) or None for key in ('response', 'fcp', 'dcl', 'load')},
                    open_fcp_ms=(lead * 1000 + metrics['fcp']) if metrics.get('fcp') else None,
                    open_load_ms=(lead * 1000 + metrics['load']) if metrics.get('load') else None)
            else:
                entry.update(title_ok=False, complete=False)
            if arguments.resources and metrics:
                entry['resources'] = [dict(zip(RESOURCE_FIELDS, row)) for row in browser.resources(metrics['origin'])]
            if arguments.screenshots:
                subprocess.run(['grim', str(folder / (re.sub(r'\W+', '-', site['name']).lower() + '.png'))],
                               env={**os.environ, **environment}, capture_output=True)
            result['sites'].append(entry)
        result['workload_seconds'] = time.time() - result['started_wall']
        result['cpu_seconds'] = browser.cpu_seconds() - cpu_start
        # Collectors run on their own timers, so one sample can land on either
        # side of a collection; keep the median of three.
        samples = []
        for _ in range(3):
            time.sleep(arguments.memory_settle / 3)
            samples.append(browser.memory())
        samples.sort(key=lambda sample: sample['pss_bytes'])
        result['memory'] = samples[1]
        result['memory']['pss_samples_bytes'] = [sample['pss_bytes'] for sample in samples]
        # Later samples show what a browser gives back once its tabs sit in the background.
        waited = arguments.memory_settle
        result['memory_curve'] = [{'seconds': waited, **{k: v for k, v in result['memory'].items() if k != 'processes'}}]
        for seconds in arguments.memory_times:
            if seconds <= waited:
                continue
            time.sleep(seconds - waited)
            waited = seconds
            sample = browser.memory(detail=arguments.memory_detail)
            result['memory_curve'].append({'seconds': seconds, **sample})
        result['settled_cpu_seconds'] = browser.cpu_seconds() - cpu_start - result['cpu_seconds']
    except Exception as error:  # noqa: BLE001 - a failed browser is a recorded invalid pass
        result['error'] = repr(error)
        result['error_trace'] = traceback.format_exc()
    finally:
        browser.stop()
        (folder / 'result.json').write_text(json.dumps(result, indent=1))
        if not arguments.keep_profiles:
            shutil.rmtree(folder / 'profile', ignore_errors=True)
    return result


STARTUP_PAGE = (b'<!doctype html><title>ready</title><body style="margin:0;background:rgb(0,200,80)">'
                b'<h1 style="font:80px sans-serif;color:#fff;padding:200px">ready</h1>')
STARTUP_COLOR = (0, 200, 80)


def startup(arguments, name, folder, environment):
    """Time from spawning a browser with a local address to its page colour on screen.

    The first launch creates the profile; later ones reuse it. This runs on the
    session bus: a private bus has to start the desktop portal first, which adds
    a wait that a real session does not have.
    """
    import threading
    from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

    class Page(BaseHTTPRequestHandler):
        def log_message(self, *_):
            pass

        def do_GET(self):
            body = b'' if self.path.startswith('/favicon') else STARTUP_PAGE
            self.send_response(200)
            self.send_header('Content-Type', 'text/html')
            self.send_header('Content-Length', str(len(body)))
            self.end_headers()
            self.wfile.write(body)

    server = ThreadingHTTPServer(('127.0.0.1', 0), Page)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    address = f'http://127.0.0.1:{server.server_address[1]}/'
    desk_environment = {**os.environ, **environment}
    desk_environment.pop('DISPLAY', None)
    profile = folder / 'profile'
    launch_environment = dict(environment)
    for variable, leaf in [('XDG_CONFIG_HOME', 'config'), ('XDG_DATA_HOME', 'data'), ('XDG_CACHE_HOME', 'cache')]:
        (profile / leaf).mkdir(parents=True, exist_ok=True)
        launch_environment[variable] = str(profile / leaf)
    if name == 'enzim':
        runtime = Path(arguments.runtime).resolve()
        source = json.loads((runtime / 'source-runtime.json').read_text())['source']['source_commit']
        native = DEPENDENCIES['native_runtime']
        pin = arguments.source_pin or (ROOT / native['source_pin'] if source == native['source_commit'] else None)
        settings = profile / 'config/enzim-browser'
        settings.mkdir(exist_ok=True)
        (settings / 'session.json').write_text(json.dumps({'layout': 'Top'}))
        launch_environment.update(ENZIM_WPE_RUNTIME_SOURCE=source, ENZIM_WPE_ENCRYPTED_MEDIA='0')
        command = [sys.executable, str(ROOT / DEPENDENCIES['webkit']['path'] / 'tools/source_runtime.py'),
                   '--directory', str(runtime), *(['--source-pin', str(pin)] if pin else []), 'run', '--',
                   str(Path(arguments.enzim_binary).resolve()), '--browser', address, '--skip-welcome']
    elif name == 'chrome':
        command = [shutil.which(arguments.chrome_binary) or arguments.chrome_binary,
                   f'--user-data-dir={profile / "chrome"}', '--no-first-run', '--no-default-browser-check',
                   '--disable-search-engine-choice-screen', '--ozone-platform=wayland',
                   f'--window-size={WINDOW[0]},{WINDOW[1]}', address]
    else:
        firefox = profile / 'firefox'
        firefox.mkdir(exist_ok=True)
        preferences = {'browser.shell.checkDefaultBrowser': False, 'browser.aboutwelcome.enabled': False,
                       'browser.startup.homepage_override.mstone': 'ignore', 'browser.tabs.warnOnClose': False,
                       'datareporting.policy.dataSubmissionPolicyBypassNotification': True,
                       'toolkit.telemetry.reportingpolicy.firstRun': False}
        (firefox / 'user.js').write_text(
            '\n'.join(f'user_pref({json.dumps(k)},{json.dumps(v)});' for k, v in preferences.items()))
        command = [shutil.which(arguments.firefox_binary) or arguments.firefox_binary, '--no-remote',
                   '--profile', str(firefox), '--width', str(WINDOW[0]), '--height', str(WINDOW[1]), address]

    def window_open():
        try:
            tree = json.loads(run(['swaymsg', '-t', 'get_tree', '-r'], env=desk_environment))
        except (subprocess.CalledProcessError, ValueError):
            return False
        pending = [tree]
        while pending:
            node = pending.pop()
            if node.get('pid'):
                return True
            pending += node.get('nodes', []) + node.get('floating_nodes', [])
        return False

    def page_shown():
        capture = subprocess.run(['grim', '-t', 'ppm', '-g', f'{WINDOW[0] // 2},{WINDOW[1] * 2 // 3} 4x4', '-'],
                                 env=desk_environment, capture_output=True).stdout
        return len(capture) >= 3 and all(abs(a - b) < 12 for a, b in zip(capture[-3:], STARTUP_COLOR))

    launches = []
    try:
        for index in range(arguments.startup + 1):
            unit = f'enzim-bench-start-{name}-{os.getpid()}-{index}'
            variables = [f'--setenv={key}={value}' for key, value in launch_environment.items()]
            began = time.time()
            run(['systemd-run', '--user', '--collect', '--unit=' + unit, '-p', 'MemoryMax=' + arguments.memory_max,
                 '-p', 'MemorySwapMax=0', '-p', 'UnsetEnvironment=DISPLAY', '-p', 'StandardOutput=null',
                 '-p', 'StandardError=null', *variables, '--', *command])
            window = page = None
            while time.time() - began < 20:
                if window is None and window_open():
                    window = time.time() - began
                if window is not None and page_shown():
                    page = time.time() - began
                    break
                time.sleep(0.005)
            subprocess.run(['systemctl', '--user', 'stop', unit], capture_output=True)
            launches.append({'window_ms': window and window * 1000, 'page_ms': page and page * 1000})
            time.sleep(1.0)
    finally:
        server.shutdown()
        server.server_close()
        shutil.rmtree(profile, ignore_errors=True)
    return {'browser': name, 'new_profile': launches[0], 'existing_profile': launches[1:]}


def show_startup(results):
    print('\nLaunch to page on screen (ms; window appears / page colour visible)')
    for result in results:
        later = [launch for launch in result['existing_profile'] if launch['page_ms']]
        cell = lambda launch, key: '-' if not launch or not launch[key] else f'{launch[key]:.0f}'  # noqa: E731
        median = {key: statistics.median(launch[key] for launch in later) if later else None
                  for key in ('window_ms', 'page_ms')}
        print(f'{result["browser"]:<9} new profile {cell(result["new_profile"], "window_ms"):>5} / '
              f'{cell(result["new_profile"], "page_ms"):>5}   existing profile, median of {len(later)}: '
              f'{cell(median, "window_ms"):>5} / {cell(median, "page_ms"):>5}')


def geometric(values):
    return math.exp(sum(math.log(v) for v in values) / len(values)) if values else float('nan')


# A load event this far past the site's median, and at least three times it, is reported by itself.
LATE_LOAD_MS = 2000


def summarize(results, sites):
    """Median per site across passes, then a geometric mean over sites valid everywhere."""
    names = sorted({r['browser'] + (':' + r['label'] if r.get('label') else '') for r in results})
    keyed = {name: [r for r in results if r['browser'] + (':' + r['label'] if r.get('label') else '') == name
                    and not r.get('error')] for name in names}
    metrics = ('fcp_ms', 'load_ms', 'open_fcp_ms', 'open_load_ms', 'open_to_origin_ms', 'dcl_ms', 'response_ms')
    table = {name: {} for name in names}
    for name, passes in keyed.items():
        for site in sites:
            entries = [s for p in passes for s in p['sites'] if s['site'] == site['name']]
            valid = [s for s in entries if s.get('complete') and s.get('title_ok')]
            table[name][site['name']] = {'valid': len(valid), 'attempts': len(entries), **{
                m: statistics.median(s[m] for s in valid) if valid and all(s.get(m) for s in valid) else None
                for m in metrics}}
    summary = {'sites': table, 'geomean': {}, 'memory': {}, 'cpu': {}}
    # A median hides the rare load that waits seconds for one resource.
    summary['late'] = [
        {'browser': name, 'site': s['site'], 'load_ms': s['load_ms'], 'median_ms': table[name][s['site']]['load_ms']}
        for name, passes in keyed.items() for p in passes for s in p['sites']
        if s.get('complete') and s.get('title_ok') and s.get('load_ms') and table[name][s['site']].get('load_ms')
        and s['load_ms'] > max(3 * table[name][s['site']]['load_ms'], table[name][s['site']]['load_ms'] + LATE_LOAD_MS)]
    for metric in metrics:
        common = [s['name'] for s in sites if all(table[n][s['name']].get(metric) for n in names)]
        for name in names:
            summary['geomean'].setdefault(name, {})[metric] = geometric([table[name][s][metric] for s in common])
        summary.setdefault('common_sites', {})[metric] = common
    summary['pressure'] = sorted({name for name, passes in keyed.items() for p in passes
                                  if p.get('memory') and any(p['memory']['pressure_events'].values())})
    for name, passes in keyed.items():
        memory = [p['memory']['pss_bytes'] / 2 ** 20 for p in passes if p.get('memory')]
        cpu = [p['cpu_seconds'] for p in passes if p.get('cpu_seconds') is not None]
        summary['memory'][name] = statistics.median(memory) if memory else None
        summary['cpu'][name] = statistics.median(cpu) if cpu else None
        # Memory over time, each figure the median over passes: the PSS sum, what the control group
        # charges the browser in total (anonymous, file-backed including shared memory, kernel), and
        # the upper bound of what the GPU driver holds for its DRM clients.
        curve = {}
        for p in passes:
            for sample in p.get('memory_curve', []):
                group = sample['cgroup_bytes']
                curve.setdefault(sample['seconds'], []).append((
                    sample['pss_bytes'], sample.get('private_bytes', 0),
                    group['anon'] + group['file'] + group.get('kernel', 0),
                    sample.get('gpu', {}).get('resident_upper_bytes', 0), sample['process_count'],
                    sample.get('gpu', {}).get('purgeable_bytes', 0)))
        summary.setdefault('memory_curve', {})[name] = [
            {'seconds': seconds, **{key: statistics.median(row[index] for row in rows) / 2 ** 20 for index, key in
                                    enumerate(('pss_mib', 'private_mib', 'cgroup_mib', 'gpu_mib'))},
             'gpu_purgeable_mib': statistics.median(row[5] for row in rows) / 2 ** 20,
             'processes': statistics.median(row[4] for row in rows)}
            for seconds, rows in sorted(curve.items())]
    return summary


def show(summary, sites):
    names = list(summary['sites'])
    width = max(len(n) for n in names) + 2
    for metric, label in [('fcp_ms', 'First contentful paint (ms, page clock)'),
                          ('load_ms', 'Load event (ms, page clock)'),
                          ('open_to_origin_ms', 'Open request to navigation start (ms)'),
                          ('open_fcp_ms', 'Open request to first contentful paint (ms)')]:
        print(f'\n{label}')
        print(f'{"site":<14}' + ''.join(f'{n:>{width}}' for n in names))
        for site in sites:
            row = [summary['sites'][n][site['name']] for n in names]
            cells = ''.join(f'{(f"{r[metric]:.0f}" if r[metric] else "-") + ("" if r["valid"] == r["attempts"] else "*"):>{width}}'
                            for r in row)
            print(f'{site["name"]:<14}{cells}')
        print(f'{"geomean":<14}' + ''.join(f'{summary["geomean"][n][metric]:>{width}.0f}' for n in names)
              + f'   ({len(summary["common_sites"][metric])} sites valid in all)')
    print('\nSettled process PSS (MiB) / workload CPU (s)')
    for name in names:
        memory, cpu = summary['memory'][name], summary['cpu'][name]
        print(f'{name:<{width}}{"-" if memory is None else round(memory):>8}{"-" if cpu is None else round(cpu, 1):>8}')
    curves = summary.get('memory_curve', {})
    if any(len(curve) > 1 for curve in curves.values()):
        print('\nMemory after the last load (MiB): PSS sum / private / charged to the control group / GPU driver'
              ' (of which purgeable)')
        for name in names:
            for point in curves.get(name, []):
                print(f'{name:<{width}}{point["seconds"]:>5.0f} s{point["pss_mib"]:>8.0f}{point["private_mib"]:>8.0f}'
                      f'{point["cgroup_mib"]:>8.0f}{point["gpu_mib"]:>8.0f} ({point.get("gpu_purgeable_mib", 0):.0f})'
                      f'   {point["processes"]:.0f} processes')
    print('\n* fewer valid passes than attempts (incomplete load or unexpected title)')
    for late in summary.get('late', []):
        print(f'LATE: {late["browser"]} {late["site"]} load event at {late["load_ms"]:.0f} ms '
              f'(median {late["median_ms"]:.0f} ms)')
    if summary.get('pressure'):
        print('WARNING: memory ceiling reached in', ', '.join(summary['pressure']),
              '- those passes are not comparable; raise --memory-max or use fewer sites')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--browsers', default='enzim,chrome,firefox')
    parser.add_argument('--sites', type=Path, default=ROOT / 'tools/benchmark_sites.json')
    parser.add_argument('--only', help='Comma-separated site names')
    parser.add_argument('--rounds', type=int, default=1)
    parser.add_argument('--output', type=Path, help='Result folder (default work/bench/<time>)')
    parser.add_argument('--enzim-binary', default=os.environ.get('ENZIM_BENCH_BINARY'),
                        help='Browser built with --features benchmark')
    parser.add_argument('--runtime', default=str(ROOT / DEPENDENCIES['native_runtime']['path']))
    parser.add_argument('--source-pin', help='Source pin for a runtime other than the selected one')
    parser.add_argument('--variant', action='append', default=[], metavar='LABEL:K=V,K=V',
                        help='Extra enzim pass, repeatable. binary=PATH, runtime=PATH and pin=PATH select '
                             'another build, prepared runtime and its source pin; other pairs are environment')
    parser.add_argument('--env', action='append', default=[], metavar='K=V', help='Environment for every enzim pass')
    parser.add_argument('--media-registry', default='desktop', metavar='desktop|cold|PATH',
                        help='Media plugin registry an enzim pass starts with: the desktop\'s own (default), '
                             'none, or a given file or folder. A variant sets it with media_registry=…')
    parser.add_argument('--chrome-binary', default='chromium')
    parser.add_argument('--firefox-binary', default='firefox')
    parser.add_argument('--timeout', type=float, default=12, help='Seconds to wait for a load event')
    parser.add_argument('--settle', type=float, default=1.0, help='Seconds kept after load before the next site')
    parser.add_argument('--warmup', type=float, default=3.0, help='Seconds after browser start')
    parser.add_argument('--memory-settle', type=float, default=10.0,
                        help='Seconds after the last site before memory is sampled')
    parser.add_argument('--memory-max', default='6G',
                        help='Ceiling for each browser; it must stay out of reach, because a browser that nears '
                             'its limit reclaims caches or sheds tabs and stops being comparable')
    parser.add_argument('--screenshots', action=argparse.BooleanOptionalAction, default=True)
    parser.add_argument('--startup', type=int, default=0, metavar='N',
                        help='Instead of loading sites, time N launches per browser with a local page, '
                             'after one launch that creates the profile; uses the session bus')
    parser.add_argument('--memory-times', type=lambda text: sorted(float(t) for t in text.split(',') if t), default=[],
                        metavar='SECONDS,...', help='also sample memory this long after the last load, e.g. 30,60,120')
    parser.add_argument('--memory-detail', action='store_true',
                        help='keep per-process mapping groups in the later memory samples')
    parser.add_argument('--resources', action='store_true',
                        help='Also record every resource timing entry of each page')
    parser.add_argument('--keep-profiles', action='store_true')
    parser.add_argument('--summarize', type=Path, nargs='+', help='Only summarize existing result folders')
    arguments = parser.parse_args()
    sites = json.loads(arguments.sites.read_text())
    if arguments.only:
        sites = [s for s in sites if s['name'] in arguments.only.split(',')]
    if arguments.summarize:
        results = [json.loads(p.read_text()) for folder in arguments.summarize
                   for p in sorted(folder.glob('*/result.json'))]
        show(summarize(results, sites), sites)
        return
    output = arguments.output or ROOT / 'work/bench' / time.strftime('%Y%m%d-%H%M%S')
    output.mkdir(parents=True, exist_ok=True)
    environment = desk()
    if arguments.startup:
        names = [name for name in arguments.browsers.split(',') if name]
        if 'enzim' in names and not arguments.enzim_binary:
            parser.error('--enzim-binary (or ENZIM_BENCH_BINARY) is required for enzim')
        results = []
        for name in names:
            print('startup', name, flush=True)
            results.append(startup(arguments, name, output / f'startup-{name}', environment))
        (output / 'startup.json').write_text(json.dumps(results, indent=1))
        show_startup(results)
        print('\nResults:', output)
        return
    passes = [(name, '', {}) for name in arguments.browsers.split(',') if name]
    for variant in arguments.variant:
        label, _, pairs = variant.partition(':')
        overrides = {'env': []}
        for pair in filter(None, pairs.split(',')):
            key, _, value = pair.partition('=')
            if key in ('binary', 'runtime', 'pin', 'media_registry'):
                overrides[key] = value
            else:
                overrides['env'].append(pair)
        passes.append(('enzim', label, overrides))
    if any(name == 'enzim' for name, _, _ in passes) and not arguments.enzim_binary:
        parser.error('--enzim-binary (or ENZIM_BENCH_BINARY) is required for enzim passes')
    results = []
    for round_index in range(arguments.rounds):
        # Alternate direction so that no browser always runs first or last.
        order = passes if round_index % 2 == 0 else list(reversed(passes))
        for name, label, overrides in order:
            selected = argparse.Namespace(**vars(arguments))
            selected.env = arguments.env + overrides.get('env', [])
            if overrides.get('binary'):
                selected.enzim_binary = overrides['binary']
            if overrides.get('runtime'):
                selected.runtime, selected.source_pin = overrides['runtime'], overrides.get('pin')
            if overrides.get('media_registry'):
                selected.media_registry = overrides['media_registry']
            folder = output / f'{round_index}-{name}{"-" + label if label else ""}'
            print(f'round {round_index} {name} {label}'.rstrip(), flush=True)
            result = measure(selected, name, sites, folder, environment)
            result['label'] = label
            (folder / 'result.json').write_text(json.dumps(result, indent=1))
            if result.get('error'):
                print('  failed:', result['error'], flush=True)
            results.append(result)
    summary = summarize(results, sites)
    (output / 'summary.json').write_text(json.dumps(summary, indent=1))
    show(summary, sites)
    print('\nResults:', output)


if __name__ == '__main__':
    main()
