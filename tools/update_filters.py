#!/usr/bin/env python3
"""Assemble pinned EasyList sources from the authors' public GitHub repository."""
import argparse,concurrent.futures,datetime,hashlib,json,re,urllib.request
from pathlib import Path,PurePosixPath
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--commit');p.add_argument('--output',type=Path,default=Path(__file__).resolve().parents[1]/'resources/filters')
a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
def read(url,cap=4*1024*1024):
 request=urllib.request.Request(url,headers={'User-Agent':'enzim-browser-filter-builder'})
 with urllib.request.urlopen(request,timeout=30) as f:data=f.read(cap+1)
 if len(data)>cap:raise RuntimeError('Filter source exceeds the byte limit')
 return data
commit=a.commit or json.loads(read('https://api.github.com/repos/easylist/easylist/commits/master'))['sha']
if not re.fullmatch('[0-9a-f]{40}',commit):raise RuntimeError('Use a complete source commit SHA')
base=f'https://raw.githubusercontent.com/easylist/easylist/{commit}/'
include=re.compile(r'^%include easylist:([^%]+)%$',re.M)
cache={}
def fetch(path):
 v=PurePosixPath(path)
 if v.is_absolute() or '..' in v.parts:raise RuntimeError('Invalid source path')
 return path,read(base+path).decode('utf-8')
for template in ['easylist.template','easyprivacy.template']:
 k,v=fetch(template);cache[k]=v
for depth in range(8):
 wanted=set(x for v in cache.values() for x in include.findall(v))-cache.keys()
 if not wanted:break
 with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
  for k,v in pool.map(fetch,sorted(wanted)):cache[k]=v
else:raise RuntimeError('Filter template nesting exceeds the limit')
def expand(path,depth=0):
 if depth>8:raise RuntimeError('Cyclic template include')
 return include.sub(lambda m:expand(m.group(1),depth+1),cache[path])
meta=[]
for name in ['easylist','easyprivacy']:
 text=expand(name+'.template').replace('%timestamp%',datetime.datetime.now(datetime.timezone.utc).isoformat())
 if '%include ' in text:raise RuntimeError('Unresolved filter include')
 data=text.encode('utf-8')
 if len(data)>8*1024*1024:raise RuntimeError('Assembled list exceeds the byte limit')
 (a.output/(name+'.txt')).write_bytes(data)
 meta.append({'name':name,'repository':'https://github.com/easylist/easylist','commit':commit,'template':name+'.template','sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'sources':[k for k in sorted(cache) if k in include.findall(cache[name+'.template'])]})
 print(name,len(data),'bytes')
(a.output/'SOURCES.json').write_text(json.dumps(meta,indent=2)+'\n')
print('Source commit:',commit)
