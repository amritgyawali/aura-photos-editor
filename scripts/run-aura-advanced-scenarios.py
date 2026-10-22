"""Run isolated desktop scenarios once the final executable build has succeeded."""
from concurrent.futures import ThreadPoolExecutor
import json
from pathlib import Path
import subprocess
import sys
import time

ROOT=Path(__file__).resolve().parents[1]
CHECKS=ROOT/'.work-checks/aura-advanced-2026-10-22'
RUNNER=ROOT/'scripts/run-aura-advanced-check.py'
deadline=time.monotonic()+7200
last=0
while not (CHECKS/'desktop-build-qualified.json').exists():
    if time.monotonic()>deadline:raise TimeoutError('Final desktop build did not complete')
    if time.monotonic()-last>60:print('Waiting for final desktop executable',flush=True);last=time.monotonic()
    time.sleep(2)
if json.loads((CHECKS/'desktop-build-qualified.json').read_text())['exitCode']!=0:raise RuntimeError('Final executable build failed')
if not (CHECKS/'source-manifest.json').exists():raise RuntimeError('Source identity must be frozen before desktop scenarios')

def run(name,command):
    if name=='new-real-photo-audit':
        source=Path(command[command.index('--photo')+1]);deadline=time.monotonic()+1200
        while not source.is_file():
            if time.monotonic()>deadline:raise TimeoutError('First RAW preview did not complete')
            time.sleep(1)
    print('START',name,flush=True)
    subprocess.run([sys.executable,str(RUNNER),name,*command],cwd=ROOT,check=True,timeout=7200 if name=='photo-audit' or name.startswith('photo-tool-matrix-') else 1800)
    print('COMPLETE',name,flush=True)

def suite(name,port,ledger):
    command=[sys.executable,'scripts/audit-aura-next100-desktop.py','--output',str(CHECKS/name),'--port',str(port),'--ledger','docs/reports/'+ledger,'--expected-tools','100','--expected-diagnostics','95']
    if name=='new-real-photo':command+=['--photo','output/aura-advanced-2026-10-22/989A2698.CR3-before.jpg']
    return name+'-audit',command

photos=[sys.executable,'scripts/audit-aura100-photos.py','H:\\AURA EDIT','output/aura-advanced-2026-10-22','--full-export','--additions20261022','--lab-tool','perceptual_lightness_lift','--expected-diagnostics','95','--preview-reference','.work-checks/evoto-next/runtime-color/Evoto-989A2698-watermarked-preview.png','--port','9506']
scenarios=[suite('new-synthetic',9501,'AURA-advanced-capabilities-2026-10-22.json'),suite('new-real-photo',9502,'AURA-advanced-capabilities-2026-10-22.json'),suite('previous100',9503,'AURA-capabilities-2026-10-22.json'),suite('second100',9504,'AURA-next-hundred-capabilities-2026-10-21.json'),('first100-audit',[sys.executable,'scripts/audit-aura100-desktop.py','--output',str(CHECKS/'first100'),'--port','9505','--selection-count','100'])]
with ThreadPoolExecutor(max_workers=4) as pool:
    photo_future=pool.submit(run,'photo-audit',photos)
    scenario_futures=[pool.submit(run,name,command) for name,command in scenarios]
    for future in scenario_futures:future.result()
    matrix=[pool.submit(run,f'photo-tool-matrix-{index}',[sys.executable,'scripts/audit-aura-tool-matrix.py','--shard-index',str(index)]) for index in range(3)]
    for future in matrix:future.result()
    photo_future.result()
print('All desktop and supplied-photo scenarios passed',flush=True)
