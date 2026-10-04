"""Compile focused integration tests against the running desktop's dependency graph.

Fallback for the local cargo/rustc catalog-build crash. This is NOT a replacement
for a clean full-workspace build. Records skipped/unavailable suites explicitly.
"""
import json
import subprocess
import sys
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]
TARGET=ROOT/'target/desktop-launch/debug'
OUT=ROOT/'.work-checks/full-audit/native'
OUT.mkdir(parents=True,exist_ok=True)
results=json.loads((OUT/'results.json').read_text()) if len(sys.argv)>1 and (OUT/'results.json').exists() else []

def library(name):
    return max((TARGET/'deps').glob('lib'+name+'-*.rlib'),key=lambda p:p.stat().st_mtime)

def dependencies(lib):
    name=lib.stem[3:].rsplit('-',1)[0]
    suffix=lib.stem.rsplit('-',1)[1]
    fingerprint=json.loads((TARGET/'.fingerprint'/f'{name.replace("_","-")}-{suffix}'/f'lib-{name}.json').read_text())
    mapping={name:lib}
    for _,dep,_,stamp,*_ in fingerprint['deps']:
        for p in (TARGET/'.fingerprint').glob(f'*/lib-{dep}'):
            if int.from_bytes(bytes.fromhex(p.read_text()),'little')==stamp:
                candidate=TARGET/'deps'/f'lib{dep}-{p.parent.name.rsplit("-",1)[1]}.rlib'
                if candidate.exists():mapping[dep]=candidate;break
    return mapping

for crate,tests in [('aura-raw',['codecs','colour_maths','containers','png_photos','tiers']),
                    ('aura-export',['export_pass','watermark'])]:
    externs=dependencies(library(crate.replace('-','_')))
    for name,path in dependencies(externs['aura_core']).items():
        externs.setdefault(name,path)
    for dev in ['tempfile','time','proptest','serde_json','rusqlite']:
        if dev not in externs:
            candidates=list((TARGET/'deps').glob(f'lib{dev}-*.rlib'))
            if candidates:externs[dev]=max(candidates,key=lambda p:p.stat().st_mtime)
    for test in tests:
        if len(sys.argv)>1 and test not in sys.argv[1:]:
            continue
        source=ROOT/'crates'/crate/'tests'/(test+'.rs')
        binary=OUT/(crate+'-'+test+'.exe')
        args=['rustc','--test','--edition=2021',str(source),'-o',str(binary),'-L',f'dependency={TARGET/"deps"}',
              '-C','debuginfo=0','-C','link-arg=/DEBUG:NONE']
        for name,path in externs.items():args.extend(['--extern',f'{name}={path}'])
        for folder in [*(TARGET/'build').glob('*/out'),*Path.home().glob('.cargo/registry/src/*/windows_x86_64_msvc-*/lib')]:
            args.extend(['-L','native='+str(folder)])
        compiled=subprocess.run(args,capture_output=True,text=True)
        log=compiled.stdout+compiled.stderr
        result=dict(suite=crate+'/'+test,compiled=compiled.returncode==0)
        if compiled.returncode==0:
            executed=subprocess.run([str(binary),'--test-threads=1'],capture_output=True,text=True)
            log+=executed.stdout+executed.stderr
            result.update(exitCode=executed.returncode,summary=[s for s in executed.stdout.splitlines() if 'test result:' in s])
        (OUT/(crate+'-'+test+'.log')).write_text(log,encoding='utf-8')
        results=[r for r in results if r['suite']!=result['suite']]+[result]
        (OUT/'results.json').write_text(json.dumps(results,indent=2))
        print(result,flush=True)
