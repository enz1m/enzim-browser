"""Disposable localhost pages for native browser interaction checks."""

import argparse
import base64
import json
import select
import socket
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--port', type=int, default=0)
parser.add_argument('--ready', type=Path, required=True)
parser.add_argument('--log', type=Path, required=True)
parser.add_argument('--pdf', type=Path)
parser.add_argument('--video', type=Path)
args = parser.parse_args()
lock = threading.Lock()

INTERACTIONS = '''<!doctype html><title>enzim interaction fixture</title>
<style>body{font:18px system-ui;max-width:700px;margin:50px auto;padding:20px}input,button{padding:10px;margin:8px}a{display:block;margin:20px 0}</style>
<h1>Browser interaction fixture</h1><p id=message>Ready</p>
<label>Name<input id=name placeholder="Your name"></label>
<button onclick="document.getElementById('message').textContent='Hello '+document.getElementById('name').value">Greet</button>
<form action=/upload method=post enctype=multipart/form-data>
<input type=file name=files multiple onchange="document.title='Selected: '+Array.from(this.files,f=>f.name).join(', ')"><button>Upload</button></form>
<a href=/long>Long page</a><a href=/download>Download text file</a><a href=/cookies>Set test cookies</a><a href=/pdf>Open PDF</a><a href=/media>Silent video</a>
<button onclick="navigator.geolocation.getCurrentPosition(()=>document.getElementById('message').textContent='Location received',e=>document.getElementById('message').textContent=e.message)">Request location</button>'''
LONG_PAGE = '''<!doctype html><title>Long page fixture</title>
<style>body{margin:0;font:18px sans-serif;background:#fafafa}section{height:1600px;padding:20px;box-sizing:border-box}.bottom{background:#27ae60}</style>
<section><h1>Full-page screenshot control</h1>enzim find marker</section>
<section>Middle marker enzim</section>
<section class=bottom>Bottom scroll marker enzim<img src=/image></section>'''
MEDIA = '''<!doctype html><title>Silent media control</title>
<style>body{font:18px system-ui;padding:30px}video{max-width:100%}button{padding:12px}</style>
<h1>Silent media control</h1><video id=v src=/video width=320 height=180 controls muted loop></video>
<p><button onclick="document.getElementById('v').play()">Play silent video</button></p>'''


def record(entry):
    with lock, args.log.open('a') as output:
        output.write(json.dumps(entry) + '\n')


class Site(BaseHTTPRequestHandler):
    def log_message(self, *unused):
        pass

    def send(self, body, content='text/html; charset=utf-8', headers=(), status=200):
        if isinstance(body, str):
            body = body.encode()
        self.send_response(status)
        self.send_header('Content-Type', content)
        self.send_header('Content-Length', str(len(body)))
        self.send_header('Cache-Control', 'no-store')
        for name, value in headers:
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(body)

    def request_lifetime(self):
        readable = select.select([self.connection], [], [], .1)[0]
        closed = bool(readable) and self.connection.recv(1, socket.MSG_PEEK) == b''
        record({'request_lifetime': self.command, 'premature_eof': closed})
        self.send('Early peer EOF' if closed else '<title>Request lifetime</title>Upstream stays open', status=500 if closed else 200)

    def do_GET(self):
        record({'path': self.path, 'user_agent': self.headers.get('User-Agent'), 'time': time.time()})
        if self.path.startswith('/request-lifetime'):
            return self.request_lifetime()
        if self.path.startswith('/ua'):
            return self.send(json.dumps({'user_agent': self.headers.get('User-Agent')}), 'application/json')
        if self.path.startswith('/echo'):
            return self.send(json.dumps({'cookie': self.headers.get('Cookie', '')}), 'application/json')
        if self.path.startswith('/cookies'):
            return self.send('<title>Cookie fixture</title><h1>Cookie fixture</h1>', headers=[
                ('Set-Cookie', 'http_only_fixture=present; HttpOnly; SameSite=Lax; Path=/'),
                ('Set-Cookie', 'visible_fixture=present; SameSite=Lax; Path=/')])
        if self.path.startswith('/download'):
            return self.send('enzim download control\n', 'text/plain', [('Content-Disposition', 'attachment; filename="enzim-download.txt"')])
        if self.path.startswith('/pdf') and args.pdf:
            return self.send(args.pdf.read_bytes(), 'application/pdf', [('Content-Disposition', 'inline; filename="browser-control.pdf"')])
        if self.path.startswith('/video') and args.video:
            body = args.video.read_bytes()
            requested = self.headers.get('Range', '')
            if requested.startswith('bytes='):
                first, last = requested[6:].split('-', 1)
                first, last = int(first or 0), min(int(last) if last else len(body)-1, len(body)-1)
                if first > last:
                    return self.send_error(416)
                return self.send(body[first:last+1], 'video/webm', [('Accept-Ranges', 'bytes'), ('Content-Range', f'bytes {first}-{last}/{len(body)}')], 206)
            return self.send(body, 'video/webm', [('Accept-Ranges', 'bytes')])
        if self.path.startswith('/media'):
            return self.send(MEDIA)
        if self.path.startswith('/image'):
            return self.send(base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aMZkAAAAASUVORK5CYII='), 'image/png')
        if self.path.startswith('/long'):
            return self.send(LONG_PAGE)
        if self.path == '/favicon.ico':
            return self.send_error(404)
        self.send(INTERACTIONS)

    def do_POST(self):
        length = int(self.headers.get('Content-Length', '0'))
        if length > 2 * 1024 * 1024:
            return self.send_error(413)
        body = self.rfile.read(length)
        if self.path.startswith('/request-lifetime'):
            if body != b'enzim request body':
                return self.send_error(400)
            return self.request_lifetime()
        record({'path': self.path, 'bytes': len(body), 'has_fixture': b'enzim upload control' in body})
        self.send(f'<title>Upload received</title><h1>Upload received</h1><p>{len(body)} bytes</p>')


args.log.parent.mkdir(parents=True, exist_ok=True)
server = ThreadingHTTPServer(('127.0.0.1', args.port), Site)
args.ready.write_text(f'http://127.0.0.1:{server.server_port}')
server.serve_forever()
