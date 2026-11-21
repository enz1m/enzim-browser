#!/usr/bin/env python3
"""Drive the browser through the enzUI agent center, as an agent would.

Starts a private `enzui-center` (its own socket, database and credentials in a
scratch folder), enrolls the browser and a test agent, starts the browser with
the browser's credential, and calls every browser function through
`enzui-mcp`'s four MCP tools against local pages. Each function's first call
asks for consent, which this check grants for the session through the center's
control socket, as the desktop's consent prompt would.

It checks what an agent relies on: pages are listed and keep their ids across
navigation; a background tab is unloaded until an agent uses it and then
loads; snapshots name elements that click, fill, type and hover reach; key and
pointer input arrive as real input; dialogs are answered; screenshots decode;
windows open and close; a uid from an earlier document is refused; a page
opened in a container keeps apart from the default container's cookies.

It needs the center's binaries (`--center`, `--mcp`; build them in the
enzui-mcp-center repository) and the graphical session of
`tools/benchmark_sites.py`, which it shares.
"""
import argparse
import asyncio
import base64
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import hmac
import json
import os
from pathlib import Path
import re
import secrets
import struct
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_sites  # noqa: E402  (the path above is what makes it importable)

APP_ID = 'org.enzim.Browser'
PAGES = {
    '/form': '''<!doctype html><meta charset=utf-8><title>Form</title>
<h1>Order</h1>
<label>Name <input id=name></label>
<label>Note <input id=note></label>
<select id=size><option value=s>Small</option><option value=l>Large</option></select>
<label><input type=checkbox id=gift> Gift</label>
<button id=send onclick="document.getElementById('out').textContent='Sent '+document.getElementById('name').value">Send</button>
<button id=ask onclick="window.answer=confirm('Really?')">Ask</button>
<div id=hover onmouseover="this.textContent='Hovered'">Point here</div>
<p id=out></p>
<a href="/second">Next</a>
<script>window.keys=[];addEventListener('keydown',e=>keys.push(e.key));
document.getElementById('note').addEventListener('keydown',e=>{if(e.key==='Enter')document.title='Submitted '+e.target.value});</script>''',
    '/second': '''<!doctype html><meta charset=utf-8><title>Second</title><h1>Second page</h1><p>Reached.</p>''',
}


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_):
        pass

    def do_GET(self):
        body = PAGES.get(self.path.split('?')[0])
        if body is None:
            self.send_response(404)
            self.end_headers()
            return
        data = body.encode()
        self.send_response(200)
        self.send_header('Content-Type', 'text/html; charset=utf-8')
        self.send_header('Cache-Control', 'no-store')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)


class Failure(Exception):
    pass


def require(condition, message):
    if not condition:
        raise Failure(message)


class Center:
    def __init__(self, options, root):
        self.options = options
        self.root = Path(root)
        self.socket = self.root / 'center.sock'
        self.control = self.root / 'control.json'
        self.app_credential = self.root / 'app.json'
        self.agent_credential = self.root / 'agent.json'
        self.processes = []
        self.mcp = None
        self.rpc_id = 0
        self.approved = set()
        self.rate_limited = 0

    def command(self, *args):
        return [str(self.options.center), '--socket', str(self.socket), '--database', str(self.root / 'db'),
                '--control-credential', str(self.control), *args]

    async def spawn(self, command, stdin=False):
        process = await asyncio.create_subprocess_exec(
            *command, stdin=asyncio.subprocess.PIPE if stdin else asyncio.subprocess.DEVNULL,
            stdout=asyncio.subprocess.PIPE, stderr=open(self.root / 'center.log', 'ab'),
            limit=8 * 1024 * 1024)
        self.processes.append(process)
        return process

    async def start(self):
        for args in [('init',), ('enroll', 'app', '--id', APP_ID, '--name', 'enzim Browser', '--output', str(self.app_credential)),
                     ('enroll', 'agent', '--id', 'browser-check', '--name', 'Browser check agent', '--output', str(self.agent_credential))]:
            process = await self.spawn(self.command(*args))
            require(await asyncio.wait_for(process.wait(), 10) == 0, f'center {args[0]} failed')
        await self.spawn(self.command('run'))
        deadline = time.monotonic() + 10
        while not self.socket.exists():
            require(time.monotonic() < deadline, 'the center did not open its socket')
            await asyncio.sleep(0.05)
        self.mcp = await self.spawn([str(self.options.mcp), '--socket', str(self.socket), '--credential', str(self.agent_credential)], stdin=True)
        await self.rpc('initialize', {'protocolVersion': '2025-11-25', 'capabilities': {}, 'clientInfo': {'name': 'check', 'version': '1'}})
        self.mcp.stdin.write(b'{"jsonrpc":"2.0","method":"notifications/initialized"}\n')
        await self.mcp.stdin.drain()

    async def stop(self):
        for process in reversed(self.processes):
            if process.returncode is None:
                process.terminate()
                try:
                    await asyncio.wait_for(process.wait(), 3)
                except TimeoutError:
                    process.kill()

    async def rpc(self, method, params, timeout=90):
        self.rpc_id += 1
        ident = self.rpc_id
        self.mcp.stdin.write(json.dumps({'jsonrpc': '2.0', 'id': ident, 'method': method, 'params': params}).encode() + b'\n')
        await self.mcp.stdin.drain()

        async def read():
            while True:
                line = await self.mcp.stdout.readline()
                require(line, 'the MCP adapter closed')
                value = json.loads(line)
                if value.get('id') == ident:
                    require('error' not in value, f'MCP protocol error: {value.get("error")}')
                    return value['result']
        return await asyncio.wait_for(read(), timeout)

    async def tool(self, name, arguments, timeout=90):
        return await self.rpc('tools/call', {'name': name, 'arguments': arguments}, timeout)

    async def control_request(self, operation):
        credential = json.loads(self.control.read_text())
        reader, writer = await asyncio.open_unix_connection(str(self.socket))

        async def read_frame():
            size, = struct.unpack('>I', await reader.readexactly(4))
            return json.loads(await reader.readexactly(size))

        async def write_frame(value):
            data = json.dumps(value, separators=(',', ':')).encode()
            writer.write(struct.pack('>I', len(data)) + data)
            await writer.drain()
        try:
            challenge = await read_frame()
            nonce = secrets.token_hex(32)

            def proof(direction):
                data = b'enzui-center-v2\0' + struct.pack('>I', 2)
                for field in [direction, challenge['nonce'], nonce, 'control', credential['id']]:
                    field = field.encode()
                    data += struct.pack('>I', len(field)) + field
                data += struct.pack('>I', os.geteuid())
                return hmac.new(bytes.fromhex(credential['secret']), data, hashlib.sha256).hexdigest()
            await write_frame({'version': 2, 'role': 'control', 'credential_id': credential['id'],
                               'client_nonce': nonce, 'proof': proof('client')})
            accepted = await read_frame()
            require(accepted['status'] == 'accepted' and hmac.compare_digest(accepted['proof'], proof('broker')),
                    'the center did not prove itself')
            await write_frame(operation)
            result = (await read_frame())['result']
            require('Ok' in result, f'control request failed: {result}')
            return result['Ok']
        finally:
            writer.close()

    async def wait_app(self, seconds=40):
        deadline = time.monotonic() + seconds
        while time.monotonic() < deadline:
            result = await self.tool('enzui_apps', {})
            if not result.get('isError') and any(app['id'] == APP_ID for app in result['structuredContent']['apps']):
                return
            await asyncio.sleep(0.25)
        raise Failure('the browser did not register with the center')

    async def call(self, function, arguments, expect_error=None, timeout=90):
        """Calls a browser function, granting consent for the session when asked.

        A session grant covers every call of a function except a destructive
        one, whose grant holds for the same arguments only: closing another
        window asks again."""
        request = {'app_id': APP_ID, 'function_id': function, 'arguments': arguments, 'timeout_ms': 70000}
        attempts = 0
        while attempts < 2:
            result = await self.tool('enzui_call', request, timeout)
            if result.get('isError') and json.loads(result['content'][0]['text'])['code'] == 'rate_limited':
                # The center allows an agent 60 calls a minute; this check makes more.
                self.rate_limited += 1
                await asyncio.sleep(2)
                continue
            attempts += 1
            if not result.get('isError'):
                require(expect_error is None, f'{function} succeeded where {expect_error} was expected')
                return result['structuredContent']['result']
            error = json.loads(result['content'][0]['text'])
            if error['code'] == 'permission_required':
                await self.control_request({'operation': 'decide', 'request_id': error['details']['request_id'], 'decision': 'session'})
                self.approved.add(function)
                continue
            if expect_error is not None:
                require(error['code'] == expect_error, f'{function}: expected {expect_error}, got {error}')
                return error
            raise Failure(f'{function} failed: {error}')
        raise Failure(f'{function} still needed permission after it was granted')


def uid_of(snapshot, pattern):
    for line in snapshot.splitlines():
        if re.search(pattern, line):
            match = re.search(r'uid=(\S+)', line)
            if match:
                return match.group(1)
    raise Failure(f'no element matching {pattern!r} in the snapshot:\n{snapshot[:2000]}')


async def scenario(center, base, report):
    async def step(name, coroutine):
        started = time.monotonic()
        value = await coroutine
        report.append(f'ok     {name} ({(time.monotonic() - started) * 1000:.0f} ms)')
        return value

    windows = await step('list_windows', center.call('list_windows', {}))
    require(len(windows['windows']) == 1, f'one window expected: {windows}')
    first_window = windows['windows'][0]['window_id']

    opened = await step('new_page loads a page', center.call('new_page', {'url': f'{base}/form'}))
    page = opened['page_id']
    require(opened['title'] == 'Form', f'the form page loaded: {opened}')

    snapshot = (await step('take_snapshot', center.call('take_snapshot', {'page_id': page})))['snapshot']
    name, note = uid_of(snapshot, r'textbox "Name"'), uid_of(snapshot, r'textbox "Note"')
    send, ask = uid_of(snapshot, r'button "Send"'), uid_of(snapshot, r'button "Ask"')
    size, gift = uid_of(snapshot, r'combobox'), uid_of(snapshot, r'checkbox')
    hover, link = uid_of(snapshot, r'Point here'), uid_of(snapshot, r'link "Next"')

    await step('fill', center.call('fill', {'page_id': page, 'uid': name, 'value': 'Ada'}))
    await step('fill_form', center.call('fill_form', {'page_id': page, 'elements': [{'uid': size, 'value': 'Large'}, {'uid': gift, 'value': 'true'}]}))
    form = (await step('evaluate_script', center.call('evaluate_script', {'page_id': page,
        'function': "() => ({name: document.getElementById('name').value, size: document.getElementById('size').value, gift: document.getElementById('gift').checked})"})))['result']
    require(form == {'name': 'Ada', 'size': 'l', 'gift': True}, f'fields filled: {form}')

    await step('click', center.call('click', {'page_id': page, 'uid': send}))
    await step('wait_for', center.call('wait_for', {'page_id': page, 'text': ['Sent Ada'], 'timeout_ms': 5000}))

    await step('type_text into a field with a submit key', center.call('type_text', {'page_id': page, 'uid': note, 'text': 'Hello', 'submit_key': 'Enter'}))
    title = (await center.call('evaluate_script', {'page_id': page, 'function': '() => document.title'}))['result']
    require(title == 'Submitted Hello', f'typed text and Enter reached the field: {title!r}')

    await step('press_key', center.call('press_key', {'page_id': page, 'key': 'Escape'}))
    keys = (await center.call('evaluate_script', {'page_id': page, 'function': '() => window.keys'}))['result']
    require('Escape' in keys, f'the key reached the page: {keys}')

    await step('hover', center.call('hover', {'page_id': page, 'uid': hover}))
    hovered = (await center.call('evaluate_script', {'page_id': page, 'args': [hover], 'function': '(el) => el.textContent'}))['result']
    require(hovered == 'Hovered', f'the pointer reached the element: {hovered!r}')

    shot = await step('take_screenshot', center.call('take_screenshot', {'page_id': page, 'max_width': 800}))
    jpeg = base64.b64decode(shot['data'])
    require(jpeg[:3] == b'\xff\xd8\xff' and shot['width'] <= 800 and shot['height'] > 50, f'a JPEG of the view: {len(jpeg)} bytes {shot["width"]}x{shot["height"]}')
    # A center that returns image results as MCP image blocks (enzui-mcp-center
    # browser-agent-images) lets the agent look at the picture; an older one
    # returns it as text only. Reported, not required.
    raw = await center.tool('enzui_call', {'app_id': APP_ID, 'function_id': 'take_screenshot', 'arguments': {'page_id': page, 'max_width': 400}})
    kinds = [block.get('type') for block in raw.get('content', [])]
    report.append(f'info   the screenshot reached the agent as {"an image block" if "image" in kinds else "text only"} ({kinds})')

    await step('click opens a confirmation', center.call('click', {'page_id': page, 'uid': ask}))
    listed = await center.call('list_pages', {})
    entry = next(p for p in listed['pages'] if p['page_id'] == page)
    require('dialog' in entry, f'the dialog is listed: {entry}')
    await step('handle_dialog', center.call('handle_dialog', {'page_id': page, 'action': 'accept'}))
    answer = (await center.call('evaluate_script', {'page_id': page, 'function': '() => window.answer'}))['result']
    require(answer is True, f'the confirmation was accepted: {answer!r}')

    navigated = await step('click a link navigates', center.call('click', {'page_id': page, 'uid': link}))
    await center.call('wait_for', {'page_id': page, 'text': ['Second page'], 'timeout_ms': 10000})
    pages = (await center.call('list_pages', {}))['pages']
    require(any(p['page_id'] == page and p['title'] == 'Second' for p in pages), f'the page keeps its id across navigation: {pages} {navigated}')
    await step('a stale uid is refused', center.call('click', {'page_id': page, 'uid': send}, expect_error='stale_uid'))

    back = await step('navigate_page back', center.call('navigate_page', {'page_id': page, 'type': 'back'}))
    # The form may come back from the back-forward cache, title and all.
    require(back['url'].endswith('/form'), f'back to the form: {back}')
    await step('navigate_page reload', center.call('navigate_page', {'page_id': page, 'type': 'reload'}))
    to = await step('navigate_page url', center.call('navigate_page', {'page_id': page, 'url': f'{base}/second'}))
    require(to['title'] == 'Second', f'navigated: {to}')

    background = await step('new_page in the background', center.call('new_page', {'url': f'{base}/form', 'background': True}))
    quiet = background['page_id']
    entry = next(p for p in (await center.call('list_pages', {}))['pages'] if p['page_id'] == quiet)
    require(not entry['loaded'] and not entry['selected'], f'a background tab is not loaded: {entry}')
    text = (await step('take_snapshot loads an unloaded tab', center.call('take_snapshot', {'page_id': quiet})))['snapshot']
    require('Order' in text, f'the unloaded tab loaded for the snapshot: {text[:300]}')
    entry = next(p for p in (await center.call('list_pages', {}))['pages'] if p['page_id'] == quiet)
    require(entry['loaded'], f'the tab is loaded now: {entry}')

    selected = await step('select_page', center.call('select_page', {'page_id': page}))
    require(selected['title'] == 'Second', f'selected: {selected}')
    scrolled = await step('scroll', center.call('scroll', {'page_id': page, 'dy': 100}))
    require('scroll_y' in scrolled, f'scroll answers its position: {scrolled}')

    await step('close_page', center.call('close_page', {'page_id': quiet}))
    pages = (await center.call('list_pages', {}))['pages']
    require(all(p['page_id'] != quiet for p in pages), 'the closed tab is gone')

    containers = (await step('list_containers', center.call('list_containers', {})))['containers']
    work = next(c for c in containers if c['name'] == 'Work')
    require(containers[0]['container_id'] == 'default' and work['agents_allowed'] and 'proxied' in work,
            f'containers listed: {containers}')
    await center.call('evaluate_script', {'page_id': page, 'function': "() => { document.cookie = 'who=default; path=/'; return document.cookie }"})
    walled = await step('new_page in a container', center.call('new_page', {'url': f'{base}/second', 'container': 'work'}))
    cookie = (await center.call('evaluate_script', {'page_id': walled['page_id'], 'function': '() => document.cookie'}))['result']
    require(cookie == '', f'the Work tab does not see the default cookie: {cookie!r}')
    entry = next(p for p in (await center.call('list_pages', {}))['pages'] if p['page_id'] == walled['page_id'])
    require(entry['container'] == {'container_id': work['container_id'], 'name': 'Work'}, f'the tab names its container: {entry}')
    only = (await step('list_pages of one container', center.call('list_pages', {'container': work['container_id']})))['pages']
    require([p['page_id'] for p in only] == [walled['page_id']], f'only the Work tab: {only}')
    await step('an unknown container is refused', center.call('new_page', {'url': f'{base}/second', 'container': 'Nowhere'}, expect_error='not_found'))
    await center.call('close_page', {'page_id': walled['page_id']})

    made = await step('new_window in a container', center.call('new_window', {'url': f'{base}/second', 'container': 'Work'}))
    entry = next(p for p in (await center.call('list_pages', {}))['pages'] if p['page_id'] == made['page_id'])
    require(entry['container'] and entry['container']['name'] == 'Work', f'the new window tab is in Work: {entry}')
    windows = (await center.call('list_windows', {}))['windows']
    require(len(windows) == 2, f'two windows: {windows}')
    await step('close_window', center.call('close_window', {'window_id': made['window_id']}))
    windows = (await center.call('list_windows', {}))['windows']
    require(len(windows) == 1 and windows[0]['window_id'] == first_window, f'one window again: {windows}')
    await step('the last window is not closed', center.call('close_window', {'window_id': first_window}, expect_error='refused'))


async def run(options):
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = f'http://127.0.0.1:{server.server_address[1]}'
    report = []
    with tempfile.TemporaryDirectory(prefix='agent-center-') as scratch:
        root = Path(scratch)
        center_root = root / 'center'
        center_root.mkdir(mode=0o700)
        center = Center(options, center_root)
        browser = None
        try:
            await center.start()
            env = [f'ENZUI_CENTER_SOCKET={center.socket}', f'ENZUI_APP_CREDENTIAL={center.app_credential}', *options.env]
            browser_options = argparse.Namespace(runtime=options.runtime, source_pin=options.source_pin,
                                                 enzim_binary=options.enzim_binary, env=env, resources=False, memory_max='6G')
            browser = benchmark_sites.Enzim(browser_options, root / 'browser', benchmark_sites.desk())
            browser.start()
            await center.wait_app()
            report.append('ok     the browser registered with the center')
            functions = (await center.tool('enzui_functions', {'app_id': APP_ID}))['structuredContent']['functions']
            names = {f['function_id'] if 'function_id' in f else f.get('id') for f in functions}
            require({'list_pages', 'take_snapshot', 'click', 'new_page'} <= names, f'functions listed: {sorted(names)}')
            await scenario(center, base, report)
            if center.rate_limited:
                report.append(f'info   the center held back {center.rate_limited} calls at its rate limit')
            return 0, report
        except Failure as failure:
            report.append(f'FAILED {failure}')
            log = root / 'browser' / 'browser.log'
            if log.exists():
                tail = log.read_text(errors='replace').splitlines()[-30:]
                report.append('browser log tail:\n' + '\n'.join(tail))
            return 1, report
        finally:
            if browser is not None:
                browser.stop()
            await center.stop()
            server.shutdown()


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--enzim-binary', required=True, type=Path)
    parser.add_argument('--center', required=True, type=Path, help='the enzui-center binary')
    parser.add_argument('--mcp', required=True, type=Path, help='the enzui-mcp binary')
    parser.add_argument('--runtime', default=str(benchmark_sites.ROOT / benchmark_sites.DEPENDENCIES['native_runtime']['path']))
    parser.add_argument('--source-pin', type=Path)
    parser.add_argument('--env', action='append', default=[], metavar='K=V')
    status, report = asyncio.run(run(parser.parse_args()))
    print('\n'.join(report))
    return status


if __name__ == '__main__':
    sys.exit(main())
