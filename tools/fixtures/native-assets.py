#!/usr/bin/env python3
"""Serve the JavaScript-disabled favicon/privacy qualification fixture."""
import argparse
import binascii
import json
import struct
import threading
from urllib.parse import urlsplit, parse_qs
import zlib
from http.server import BaseHTTPRequestHandler, HTTPServer
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--port', type=int, default=38791)
parser.add_argument('--receipts', type=Path, required=True)
parser.add_argument('--extra-port', type=int)
args = parser.parse_args()
requests = []
request_lock = threading.Lock()

def chunk(kind, data):
    return (struct.pack('>I', len(data)) + kind + data
            + struct.pack('>I', binascii.crc32(kind + data) & 0xffffffff))

png = (b'\x89PNG\r\n\x1a\n'
       + chunk(b'IHDR', struct.pack('>IIBBBBB', 32, 32, 8, 6, 0, 0, 0))
       + chunk(b'IDAT', zlib.compress((b'\0' + b'\x24\xb8\x62\xff' * 32) * 32))
       + chunk(b'IEND', b''))

class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        with request_lock:
            requests.append(self.path)
            args.receipts.write_text(json.dumps(requests, indent=2))
        if urlsplit(self.path).path == '/privacy':
            phase = parse_qs(urlsplit(self.path).query).get('phase', ['A'])[0]
            phase = phase if phase in ('A', 'B', 'C', 'D', 'E') else 'A'
            data = (f'<title>Privacy fixture {phase}</title><h1>Privacy fixture {phase}</h1>'
                    f'<img id="ad" src="/ads/banners/check.png?phase={phase}">'
                    f'<img id="tracker" src="/piwik.php?phase={phase}">'
                    '<div id="cosmetic" class="ADBAR">Advertisement</div><p>Article</p>').encode()
            kind = 'text/html'
        elif self.path in ('/csp', '/blocked'):
            icon = 'green' if self.path == '/csp' else 'blocked'
            data = (f'<title>Native icon {icon}</title>'
                    f'<link rel="icon" href="/{icon}.png">'
                    '<h1>Native icon test</h1>').encode()
            kind = 'text/html'
        elif urlsplit(self.path).path in ('/green.png', '/blocked.png', '/ads/banners/check.png', '/piwik.php'):
            data, kind = png, 'image/png'
        else:
            self.send_error(404)
            return
        self.send_response(200)
        self.send_header('Content-Type', kind)
        self.send_header('Content-Length', str(len(data)))
        self.send_header('Content-Security-Policy',
                         "default-src 'none'; img-src 'self'; connect-src 'none'; style-src 'unsafe-inline'")
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_args):
        pass

args.receipts.parent.mkdir(parents=True, exist_ok=True)
with HTTPServer(('127.0.0.1', args.port), Handler) as server:
    print(f'ENZIM_ASSET_BASE=http://127.0.0.1:{server.server_port}', flush=True)
    if args.extra_port is not None:
        extra = HTTPServer(('127.0.0.1', args.extra_port), Handler)
        print(f'ENZIM_ASSET_OTHER=http://127.0.0.1:{extra.server_port}', flush=True)
        threading.Thread(target=extra.serve_forever, daemon=True).start()
    server.serve_forever()
