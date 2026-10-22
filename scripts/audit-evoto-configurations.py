"""Classify every supplied .json resource without guessing its opaque encoding."""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path

parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('inventory',type=Path)
parser.add_argument('output',type=Path)
args=parser.parse_args()
inventory=json.loads(args.inventory.read_text(encoding='utf-8'))
root=Path(inventory['root'])
if args.output.exists():
    raise ValueError('Preserve earlier evidence; select a fresh output')
records=[]
for entry in inventory['files']:
    if Path(entry['path']).suffix.lower()!='.json':
        continue
    raw=(root/entry['path']).read_bytes()
    observed=hashlib.sha256(raw).hexdigest()
    if observed!=entry['sha256']:
        raise ValueError('Resource changed after inventory: '+entry['path'])
    record=dict(path=entry['path'],bytes=len(raw),sha256=observed)
    try:
        decoded=json.loads(raw.decode('utf-8-sig'))
        record.update(status='valid_utf8_json',rootType=type(decoded).__name__)
        if isinstance(decoded,dict):
            record['rootKeys']=list(decoded)
        elif isinstance(decoded,list):
            record['items']=len(decoded)
    except (UnicodeError,json.JSONDecodeError) as error:
        record.update(status='opaque_or_non_json',parseFailure=type(error).__name__,prefixHex=raw[:16].hex())
    records.append(record)
result=dict(root=str(root),inventoryTreeSha256=inventory['treeManifestSha256'],records=records,
            counts=dict(Counter(v['status'] for v in records)),
            limitations=['The .json extension does not establish plaintext JSON or an encryption format.',
                         'Opaque content is not decoded; no training weights, architecture, tensor semantics or numerical behavior are established.',
                         'No application assets or configuration values are incorporated into AURA.'])
args.output.parent.mkdir(parents=True,exist_ok=True)
args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(files=len(records),counts=result['counts'],output=str(args.output))))
