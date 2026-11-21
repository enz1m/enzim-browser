#!/usr/bin/env python3
"""Build pinned cosmetic rules and scriptlets from their upstream source modules."""
import argparse,concurrent.futures,hashlib,json,posixpath,re,subprocess,tempfile,urllib.request
from pathlib import Path
from filter_assets import CAPABILITIES, assemble
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('--assets-commit',default='bfb1b38c00cf0b5b9cfad83f32719fd5934c5d32')
p.add_argument('--scriptlets-commit')
p.add_argument('--output',type=Path,default=Path(__file__).resolve().parents[1]/'resources/filters')
a=p.parse_args();a.output.mkdir(parents=True,exist_ok=True)
def read(url):
 with urllib.request.urlopen(url,timeout=30) as r: data=r.read(4*1024*1024+1)
 if len(data)>4*1024*1024:raise RuntimeError('Upstream file exceeds size limit')
 return data
commit=a.scriptlets_commit or json.loads(read('https://api.github.com/repos/gorhill/uBlock/commits/master'))['sha']
for c in [commit,a.assets_commit]:
 if not re.fullmatch('[0-9a-f]{40}',c):raise RuntimeError('A full source commit is required')
metadata=[]
def save(name,data,repo,revision,path):
 (a.output/name).write_bytes(data)
 metadata.append({'file':name,'repository':'https://github.com/'+repo,'commit':revision,'path':path,'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data)})
for name,path in [('ublock.txt','filters/filters.txt'),('ublock-quick-fixes.txt','filters/quick-fixes.txt')]:
 def fetch_asset(path):
  return read(f'https://raw.githubusercontent.com/uBlockOrigin/uAssets/{a.assets_commit}/{path}')
 data,inputs=assemble(path,fetch_asset)
 save(name,data,'uBlockOrigin/uAssets',a.assets_commit,path)
 metadata[-1]['inputs']=inputs
 metadata[-1]['capabilities']=CAPABILITIES
save('uassets-LICENSE',read(f'https://raw.githubusercontent.com/uBlockOrigin/uAssets/{a.assets_commit}/LICENSE'),'uBlockOrigin/uAssets',a.assets_commit,'LICENSE')
base=f'https://raw.githubusercontent.com/gorhill/uBlock/{commit}/src/js/'
with tempfile.TemporaryDirectory(prefix='enzim-scriptlets-') as tmp:
 root=Path(tmp);(root/'package.json').write_text('{"type":"module"}')
 wanted={'resources/scriptlets.js'};seen={}
 for depth in range(12):
  def fetch(name):
   if not re.fullmatch('(?:resources/)?[a-zA-Z0-9-]+[.]js',name):raise RuntimeError('Unexpected module path: '+name)
   return name,read(base+name)
  with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
   for name,data in pool.map(fetch,sorted(wanted)):
    seen[name]=data;(root/name).parent.mkdir(parents=True,exist_ok=True);(root/name).write_bytes(data)
  wanted={posixpath.normpath(posixpath.join(posixpath.dirname(name),x)) for name,data in seen.items() for x in re.findall(r"(?:from\s*|import\s*)['\"]([.][^'\"]+)['\"]",data.decode())}-seen.keys()
  if not wanted:break
  if len(seen)>96:raise RuntimeError('Too many source modules')
 else:raise RuntimeError('Import nesting exceeds limit')
 (root/'build.mjs').write_text('''import { builtinScriptlets } from './resources/scriptlets.js';
const resources = builtinScriptlets.map(s => ({name:s.name,aliases:s.aliases||[],kind:{mime:s.name.endsWith('.fn')?'fn/javascript':'application/javascript'},content:Buffer.from(s.fn.toString()).toString('base64'),dependencies:(s.dependencies||[]).map(d=>typeof d==='function'?d.details.name:d),permission:s.requiresTrust?1:0}));
process.stdout.write(JSON.stringify(resources));''')
 data=subprocess.check_output(['node',str(root/'build.mjs')],timeout=30)
 parsed=json.loads(data)
 if len(parsed)<50:raise RuntimeError('Scriptlet positive control missing')
 save('scriptlets.json',data,'gorhill/uBlock',commit,'src/js/resources/*.js')
 # GPL source modules are distributed alongside the generated function data.
 source=a.output/'scriptlet-source';source.mkdir(exist_ok=True)
 for name,data in seen.items():
  (source/name).parent.mkdir(parents=True,exist_ok=True);(source/name).write_bytes(data)
 (source/'package.json').write_text('{"type":"module"}\n')
 save('ublock-LICENSE',read(f'https://raw.githubusercontent.com/gorhill/uBlock/{commit}/LICENSE.txt'),'gorhill/uBlock',commit,'LICENSE.txt')
(a.output/'COSMETIC_SOURCES.json').write_text(json.dumps(metadata,indent=2)+'\n')
print('Pinned',len(parsed),'scriptlets from',commit,'and uAssets',a.assets_commit)
