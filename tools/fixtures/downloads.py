#!/usr/bin/env python3
"""Serve deterministic authenticated downloads with range and failure controls."""
import argparse,json,threading,time,gzip
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from urllib.parse import urlsplit,parse_qs
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--port',type=int,default=0);p.add_argument('--receipts',type=Path,required=True);p.add_argument('--delay',type=float,default=.008);a=p.parse_args()
payload=b'enzim-download-fixture\n'+bytes(range(256))*8192
changed=b'changed-representation\n'+bytes(reversed(range(256)))*8192
receipts=[];lock=threading.Lock();attempts={}
class Handler(BaseHTTPRequestHandler):
 def do_GET(self):
  split=urlsplit(self.path);mode=parse_qs(split.query).get('mode',['normal'])[0];offset=self.headers.get('Range');cookie=self.headers.get('Cookie','');validator=self.headers.get('If-Range')
  with lock:
   receipts.append({'path':self.path,'range':offset,'if_range':validator,'cookie':cookie});a.receipts.write_text(json.dumps(receipts,indent=2));attempts[self.path]=attempts.get(self.path,0)+1;attempt=attempts[self.path]
  if split.path=='/fixture':
   data=b'<title>Download fixture</title><h1>Download fixture</h1><p><a href="/download?mode=normal" download>Download file</a></p><p><a href="/download?mode=normal&long=1" download>Long filename</a></p><p><a href="/download?mode=delayed" download>Slow resume</a></p><p><a href="/download?mode=interrupt" download>Interrupted download</a></p>'
   self.send_response(200);self.send_header('Set-Cookie','download_identity=qualified; Path=/; SameSite=Lax');self.send_header('Content-Length',str(len(data)));self.send_header('Content-Type','text/html');self.end_headers();self.wfile.write(data);return
  if split.path!='/download':self.send_error(404);return
  if 'download_identity=qualified' not in cookie:self.send_error(403);return
  if offset and mode=='delayed':time.sleep(32)
  data=changed if offset and mode=='changed' else payload
  if mode=='gzip':data=gzip.compress(data)
  start=0;status=200
  if offset and mode=='redirect':
   self.send_response(302);self.send_header('Location','/download?mode=normal');self.send_header('Content-Length','0');self.end_headers();return
  if offset and mode not in ('ignore','changed'):
   try:start=int(offset.removeprefix('bytes=').removesuffix('-'))
   except ValueError:self.send_error(400);return
   status=206
  self.send_response(status);self.send_header('Content-Type','application/octet-stream');self.send_header('Content-Disposition','attachment; filename="'+('An unusually long download filename for checking compact download panels.bin' if parse_qs(split.query).get('long') else 'range-example.bin')+'"');self.send_header('Cache-Control','no-store');self.send_header('Accept-Ranges','none' if mode=='gzip' else 'bytes');self.send_header('ETag','"fixture-v2"' if (offset and mode in ('changed','liechanged')) else '"fixture-v1"')
  if mode=='gzip':self.send_header('Content-Encoding','gzip')
  if status==206:self.send_header('Content-Range','bytes invalid' if mode=='malformed' else f'bytes {start+int(mode=="wrong")}-{len(data)-1}/{len(data)}')
  if mode=='chunked':self.send_header('Transfer-Encoding','chunked')
  else:self.send_header('Content-Length',str(len(data)-start))
  self.end_headers();stop=len(data)
  if mode=='interrupt' and not offset:stop=len(data)//3
  if mode=='short' and offset:stop=start+(len(data)-start)//2
  try:
   for i in range(start,stop,16384):
    chunk=data[i:min(i+16384,stop)]
    if mode=='chunked':self.wfile.write(f'{len(chunk):x}\r\n'.encode()+chunk+b'\r\n')
    else:self.wfile.write(chunk)
    self.wfile.flush();time.sleep(a.delay)
   if mode=='chunked':self.wfile.write(b'0\r\n\r\n')
  except (BrokenPipeError,ConnectionResetError):pass
  self.close_connection=True
 def log_message(self,*_):pass
server=ThreadingHTTPServer(('127.0.0.1',a.port),Handler);server.daemon_threads=True
print(f'ENZIM_DOWNLOAD_BASE=http://127.0.0.1:{server.server_port}',flush=True)
try:server.serve_forever()
finally:server.server_close()
