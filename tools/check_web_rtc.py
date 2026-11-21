#!/usr/bin/env python3
"""Place a call between two tabs, as a video call site does.

Two local pages each take the camera and microphone, offer them to the other
over an `RTCPeerConnection` and send a data channel message; a local server
relays their offers, answers and candidates. The browser runs with WebKit's
generated capture devices (`ENZIM_QA_MOCK_CAPTURE=1`, honoured only in QA runs)
and a saved choice that allows both for the test origin, so no prompt waits.

Every sent and received frame passes through an `RTCRtpScriptTransform`
worker, as on a call with end-to-end encryption. It checks that both sides
connect, that each decodes the other's video, that both messages arrive, that
the workers saw the frames, and that the call's UDP sockets belong to the network
process while every web process stays in a network namespace of its own.

Granted capture is what turns off WebKit's candidate filtering, as on a real
call. Without it, WebKit names host candidates with mDNS through Avahi; on a
machine without Avahi two such tabs cannot find each other, which a site's
STUN or TURN server makes up for.

It needs the center's binaries (`--center`, `--mcp`), like
`tools/check_agent_center.py`, and the graphical session of
`tools/benchmark_sites.py`.
"""
import argparse
import asyncio
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import time

sys.path.insert(0, str(Path(__file__).resolve().parent))
import benchmark_sites  # noqa: E402  (the path above is what makes it importable)
from check_agent_center import Center, Failure, require, uid_of  # noqa: E402

PAGE = '''<!doctype html><meta charset=utf-8><title>Call ROLE</title>
<video id=v autoplay playsinline muted width=320 height=240></video>
<button id=share style="font-size:20px">Share screen</button>
<script>
const role = 'ROLE', other = role == 'a' ? 'b' : 'a';
window.state = {connection: 'new', message: null, error: null, local: []};
const send = m => fetch('/signal/' + other, {method: 'POST', body: JSON.stringify(m)});
const pc = new RTCPeerConnection();
pc.onicecandidate = e => { if (e.candidate) send({candidate: e.candidate}); };
pc.onconnectionstatechange = () => { state.connection = pc.connectionState; };
// Every frame passes through a worker, as with end-to-end encryption.
const worker = new Worker(URL.createObjectURL(new Blob([`
  let frames = 0;
  onrtctransform = e => e.transformer.readable.pipeThrough(new TransformStream({transform(frame, out) {
    if (++frames % 10 == 0) postMessage(frames); out.enqueue(frame);
  }})).pipeTo(e.transformer.writable);`])));
worker.onmessage = m => { state.transformed = m.data; };
state.transformed = 0;
const transform = target => { target.transform = new RTCRtpScriptTransform(worker, {}); };
pc.ontrack = e => { transform(e.receiver); document.getElementById('v').srcObject = e.streams[0]; };
pc.ondatachannel = e => e.channel.onmessage = m => { state.message = m.data; };
const ready = navigator.mediaDevices.getUserMedia({video: true, audio: true}).then(stream => {
  state.local = stream.getTracks().map(t => t.kind);
  stream.getTracks().forEach(track => transform(pc.addTrack(track, stream)));
}).catch(e => { state.error = 'capture: ' + e.name; });
const channel = pc.createDataChannel('chat');
channel.onopen = () => channel.send('hello from ' + role);
async function handle(m) {
  try {
    if (m.offer) { await ready; await pc.setRemoteDescription(m.offer); await pc.setLocalDescription(); send({answer: pc.localDescription}); }
    else if (m.answer) await pc.setRemoteDescription(m.answer);
    else if (m.candidate) await pc.addIceCandidate(m.candidate);
  } catch (e) { state.error = String(e); }
}
(async () => { for (;;) { const r = await fetch('/signal/' + role); for (const m of await r.json()) await handle(m); } })();
if (role == 'a') ready.then(async () => { await pc.setLocalDescription(); send({offer: pc.localDescription}); });
window.callStats = async () => {
  const out = {frames: 0, bytes: 0, codec: null, pair: null};
  (await pc.getStats()).forEach((r, _, all) => {
    if (r.type == 'inbound-rtp' && r.kind == 'video') {
      out.frames = r.framesDecoded || 0; out.bytes = r.bytesReceived || 0;
      out.size = [r.frameWidth, r.frameHeight];
      const codec = all.get(r.codecId); out.codec = codec && codec.mimeType;
    }
    if (r.type == 'candidate-pair' && r.nominated && r.state == 'succeeded') {
      const local = all.get(r.localCandidateId), remote = all.get(r.remoteCandidateId);
      out.pair = [local && local.candidateType, local && local.protocol, remote && remote.candidateType];
    }
  });
  return out;
};
// Screen sharing replaces the camera in the call, as a call site does.
document.getElementById('share').onclick = () => navigator.mediaDevices.getDisplayMedia({video: true}).then(stream => {
  const track = stream.getVideoTracks()[0];
  state.screen = track.getSettings();
  return pc.getSenders().find(s => s.track && s.track.kind == 'video').replaceTrack(track);
}).then(() => { state.shared = true; }, e => { state.screenError = e.name; });
// What a's sender saw, for a failure report.
window.sendStats = async () => {
  const out = [];
  (await pc.getStats()).forEach(r => { if (r.type == 'outbound-rtp' || r.type == 'media-source') out.push(r); });
  const track = pc.getSenders().map(s => s.track).filter(t => t && t.kind == 'video')[0];
  return {stats: out, track: track && [track.label, track.readyState, track.muted, track.enabled]};
};
// Speakers are listed to a page with capture access; the call's audio moves to one.
window.speakers = async () => {
  const outputs = (await navigator.mediaDevices.enumerateDevices()).filter(d => d.kind == 'audiooutput');
  const v = document.getElementById('v');
  if (typeof v.setSinkId != 'function') return {api: false, outputs: outputs.length};
  const target = outputs.find(d => d.deviceId && d.deviceId != 'default');
  if (target) await v.setSinkId(target.deviceId);
  return {api: true, outputs: outputs.length, labels: outputs.map(d => d.label), sinkId: v.sinkId, target: target && target.deviceId};
};
</script>'''


class Handler(BaseHTTPRequestHandler):
    queues = {'a': [], 'b': []}
    lock = threading.Condition()

    def log_message(self, *_):
        pass

    def reply(self, body, kind):
        self.send_response(200)
        self.send_header('Content-Type', kind)
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith('/signal/'):
            queue = self.queues[self.path.rsplit('/', 1)[1]]
            with self.lock:
                self.lock.wait_for(lambda: queue, timeout=10)
                items, queue[:] = list(queue), []
            return self.reply(json.dumps(items).encode(), 'application/json')
        role = self.path.strip('/')
        if role not in self.queues:
            self.send_error(404)
            return
        self.reply(PAGE.replace('ROLE', role).encode(), 'text/html; charset=utf-8')

    def do_POST(self):
        queue = self.queues[self.path.rsplit('/', 1)[1]]
        body = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        with self.lock:
            queue.append(body)
            self.lock.notify_all()
        self.reply(b'{}', 'application/json')


def sockets_by_process(group):
    """(name, network namespace, UDP sockets in that namespace) per engine process."""
    rows = []
    for pid in (group / 'cgroup.procs').read_text().split():
        try:
            name = Path(f'/proc/{pid}/comm').read_text().strip()
            namespace = os.readlink(f'/proc/{pid}/ns/net')
            udp = len(Path(f'/proc/{pid}/net/udp').read_text().splitlines()[1:])
            udp += len(Path(f'/proc/{pid}/net/udp6').read_text().splitlines()[1:])
        except OSError:
            continue
        if name.startswith('WPE'):
            rows.append((name, namespace, udp))
    return rows


async def scenario(center, browser, base, report):
    pages = {}
    for role in ('b', 'a'):
        pages[role] = (await center.call('new_page', {'url': f'{base}/{role}'}))['page_id']
    require((await center.call('evaluate_script', {'page_id': pages['a'], 'function': '() => typeof RTCPeerConnection'}))['result'] == 'function',
            'RTCPeerConnection is missing: the runtime was built without WebRTC')
    started = time.monotonic()
    while True:
        states = {role: (await center.call('evaluate_script', {'page_id': page, 'function': '() => window.state'}))['result']
                  for role, page in pages.items()}
        stats = {role: (await center.call('evaluate_script', {'page_id': page, 'function': '() => callStats()'}))['result']
                 for role, page in pages.items()}
        if all(s['connection'] == 'connected' and s['message'] and s['transformed'] >= 30 for s in states.values()) \
                and all(s['frames'] >= 30 for s in stats.values()):
            break
        if time.monotonic() - started > 30:
            raise Failure(f'no call after 30 s: {json.dumps(states)} {json.dumps(stats)}')
        await asyncio.sleep(1)
    report.append(f'ok     both tabs connected in {time.monotonic() - started:.1f} s with {states["a"]["local"]} each')
    for role, other in (('a', 'b'), ('b', 'a')):
        require(states[role]['message'] == f'hello from {other}', f'{role} received {states[role]["message"]!r}')
        report.append(f'ok     {role} decoded {stats[role]["frames"]} {stats[role]["codec"]} frames over {stats[role]["pair"]} '
                      f'and received "{states[role]["message"]}"; its worker transformed {states[role]["transformed"]} frames')
    rows = sockets_by_process(browser.group)
    host = os.readlink('/proc/self/ns/net')
    network = [r for r in rows if r[0].startswith('WPENetwork')]
    web = [r for r in rows if r[0] == 'WPEWebProcess']
    require(network and all(r[1] == host for r in network), f'network process namespaces: {network}')
    require(web and all(r[1] != host and r[2] == 0 for r in web), f'web processes with network access or sockets: {web}')
    report.append(f'ok     the call runs in the network process; {len(web)} web processes have no network and no sockets')

    before = stats['b']
    snapshot = (await center.call('take_snapshot', {'page_id': pages['a']}))['snapshot']
    await center.call('click', {'page_id': pages['a'], 'uid': uid_of(snapshot, r'button "Share screen')})
    started = time.monotonic()
    while True:
        a = (await center.call('evaluate_script', {'page_id': pages['a'], 'function': '() => window.state'}))['result']
        b = (await center.call('evaluate_script', {'page_id': pages['b'], 'function': '() => callStats()'}))['result']
        require(not a.get('screenError'), f'screen sharing failed: {a.get("screenError")}')
        if a.get('shared') and b['size'] and b['size'] != before['size'] and b['frames'] >= before['frames'] + 30:
            break
        if time.monotonic() - started > 20:
            sending = (await center.call('evaluate_script', {'page_id': pages['a'], 'function': '() => sendStats()'}))['result']
            raise Failure(f'no shared screen after 20 s: {json.dumps(a)} {json.dumps(b)} sending {json.dumps(sending)}')
        await asyncio.sleep(1)
    report.append(f'ok     a shared a {a["screen"].get("width")}x{a["screen"].get("height")} screen; '
                  f'b decodes it at {b["size"][0]}x{b["size"][1]}')

    speakers = (await center.call('evaluate_script', {'page_id': pages['b'], 'function': '() => speakers()'}))['result']
    require(speakers['api'], 'HTMLMediaElement.setSinkId is missing')
    require(speakers['outputs'] > 0, f'no speakers listed to a page with capture access: {speakers}')
    require(speakers['target'] is None or speakers['sinkId'] == speakers['target'], f'setSinkId did not take: {speakers}')
    report.append(f'ok     b lists {speakers["outputs"]} speakers {speakers["labels"]} and plays the call on '
                  f'{"another one" if speakers["target"] else "the default one"}')


async def run(options):
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = f'http://127.0.0.1:{server.server_address[1]}'
    report = []
    with tempfile.TemporaryDirectory(prefix='web-rtc-') as scratch:
        root = Path(scratch)
        center_root = root / 'center'
        center_root.mkdir(mode=0o700)
        center = Center(options, center_root)
        browser = None
        try:
            await center.start()
            env = [f'ENZUI_CENTER_SOCKET={center.socket}', f'ENZUI_APP_CREDENTIAL={center.app_credential}',
                   'ENZIM_QA=1', 'ENZIM_QA_MOCK_CAPTURE=1', 'ENZIM_QA_PERMISSION_ANSWER=allow', *options.env]
            grants = [{'container': None, 'requesting': base, 'top': base, 'kind': kind, 'allowed': True}
                      for kind in ('camera', 'microphone')]
            browser_options = argparse.Namespace(runtime=options.runtime, source_pin=options.source_pin,
                                                 enzim_binary=options.enzim_binary, env=env, resources=False,
                                                 memory_max='6G', session={'site_permissions': grants})
            browser = benchmark_sites.Enzim(browser_options, root / 'browser', benchmark_sites.desk())
            browser.start()
            await center.wait_app()
            await scenario(center, browser, base, report)
            return 0, report
        except Failure as failure:
            report.append(f'FAILED {failure}')
            log = root / 'browser' / 'browser.log'
            if log.exists():
                report.append('browser log tail:\n' + '\n'.join(log.read_text(errors='replace').splitlines()[-30:]))
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
