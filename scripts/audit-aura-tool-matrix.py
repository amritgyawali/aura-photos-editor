"""Exercise every new capability on each original-bound supplied-photo JPEG proxy."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--shard-index',type=int,required=True)
parser.add_argument('--shards',type=int,default=3)
parser.add_argument('--proxy-root',type=Path,default=ROOT/'output/aura-advanced-2026-10-22')
args=parser.parse_args()
if args.shards<1 or not 0<=args.shard_index<args.shards:parser.error('Invalid shard')
def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
deadline=time.monotonic()+7200;last=0
while True:
    try:
        evidence=json.loads((args.proxy_root/'results.json').read_text(encoding='utf-8'))
        if evidence.get('fatalError'):raise RuntimeError(evidence['fatalError'])
        if len(evidence['photos'])==27 and all(r.get('passed') and r.get('before',{}).get('fileSha256') for r in evidence['photos']):break
    except (FileNotFoundError,json.JSONDecodeError):pass
    if time.monotonic()>deadline:raise TimeoutError('Qualified original-bound proxies did not complete')
    if time.monotonic()-last>60:print('Waiting for all27 hash-bound RAW previews',flush=True);last=time.monotonic()
    time.sleep(2)
inputs={Path(row['file']).name:row for row in evidence['inputManifest']}
photos=sorted(evidence['photos'],key=lambda row:row['file'])
assert len(photos)==27 and all(row.get('passed') for row in photos)
selected=photos[args.shard_index::args.shards]
manifest=[]
for row in selected:
    original=inputs[row['file']]
    assert digest(Path(original['file']))==original['sha256'],'Source no longer matches qualified proxy'
    proxy=args.proxy_root/(row['file']+'-before.jpg')
    assert proxy.resolve()==Path(row['before']['file']).resolve()
    assert digest(proxy)==row['before']['fileSha256'],'Proxy no longer matches its generated RAW preview'
    output=ROOT/'.work-checks/aura-advanced-2026-10-22/photo-tool-matrix'/row['file']
    command=[sys.executable,str(ROOT/'scripts/audit-aura-next100-desktop.py'),'--output',str(output),'--port',str(9410+args.shard_index),'--ledger',str(ROOT/'docs/reports/AURA-advanced-capabilities-2026-10-22.json'),'--expected-tools','100','--expected-diagnostics','95','--photo',str(proxy),'--native-only']
    print('MATRIX',row['file'],flush=True)
    subprocess.run(command,cwd=ROOT,check=True,timeout=1200)
    assert digest(Path(original['file']))==original['sha256']
    assert digest(proxy)==row['before']['fileSha256'],'Proxy changed during qualification'
    manifest.append(dict(original=original,proxy=str(proxy),proxySha256=digest(proxy),results=str(output/'results.json')))
folder=ROOT/'.work-checks/aura-advanced-2026-10-22'
(folder/f'photo-tool-matrix-shard-{args.shard_index}.json').write_text(json.dumps(dict(completed=True,proxies=manifest),indent=2)+'\n',encoding='utf-8')
print('Completed proxy matrix shard',args.shard_index,'photos',len(manifest),flush=True)
