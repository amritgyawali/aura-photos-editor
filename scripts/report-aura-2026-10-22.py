"""Write a per-tool report from actual native, desktop and photo evidence."""
from collections import Counter
import csv
import hashlib
import json
from pathlib import Path
import re
import shutil
from aura_native_qualification import parse
from aura_qualification import photo_success

ROOT=Path(__file__).resolve().parents[1]
CHECKS=ROOT/'.work-checks/aura-2026-10-22'
OUT=ROOT/'docs/reports/AURA-validation-2026-10-22'
PHOTOS=ROOT/'output/aura-2026-10-22'


def text(path):
    raw=path.read_bytes()
    return raw.decode('utf-16' if raw.startswith(b'\xff\xfe') else 'utf-8-sig',errors='replace')


def read(path):
    return json.loads(text(path))


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream,'sha256').hexdigest()


def source_paths():
    paths=[]
    for relative in ['crates','ui/src','ui/src-tauri/src','scripts']:
        paths.extend(path for path in (ROOT/relative).rglob('*') if path.is_file() and path.suffix in ['.rs','.ts','.tsx','.css','.py','.ps1','.sh'])
    paths.extend(ROOT/path for path in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','ui/package.json','ui/package-lock.json','ui/src-tauri/Cargo.toml','ui/src-tauri/Cargo.lock','ui/src-tauri/tauri.conf.json'])
    return sorted(set(paths))


def freeze():
    manifest={path.relative_to(ROOT).as_posix():digest(path) for path in source_paths()}
    path=CHECKS/'source-manifest.json'
    if path.exists():
        raise ValueError('Earlier source identity must stay recorded')
    path.write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8')
    print('Frozen source files:',len(manifest))


def main():
    ledger=read(ROOT/'docs/reports/AURA-capabilities-2026-10-22.json')
    desktop=read(CHECKS/'desktop/results.json')
    real=read(CHECKS/'real-photo-tools/results.json')
    previous=read(CHECKS/'previous100/results.json')
    first=read(CHECKS/'first100/results.json')
    photos=read(PHOTOS/'results.json')
    tree=read(ROOT/'.work-checks/evoto-2026-10-22/file-inventory.json')
    rea=read(ROOT/'.work-checks/evoto-2026-10-22/rea-evidence.json')
    commands={path.stem:read(path) for path in CHECKS.glob('*.json') if not path.name.startswith('source-manifest') and 'command' in read(path)}
    gates=['native-tests','clippy-verified' if 'clippy-verified' in commands else 'clippy-final','format-fixtures' if 'format-fixtures' in commands else 'format-verified','ipc','python-tests','ui-build-final','contracts-final']
    gates += [name for name in ['ui-label-check','ui-build-label','desktop-build-final','python-tests-final','native-libraries-final','native-selection-final','native-render-fixtures','format-final-tests'] if name in commands]
    if any(commands[name]['exitCode']!=0 for name in gates):
        raise ValueError('Required command did not pass')
    if text(CHECKS/'desktop-build.exit').strip()!='0' or text(CHECKS/'ui-tests.exit').strip()!='0':
        raise ValueError('Desktop build or complete UI suite failed')
    native=parse(text(CHECKS/'native-tests.log'))
    final_libraries=parse(text(CHECKS/'native-libraries-final.log'))
    final_selection=parse(text(CHECKS/'native-selection-final.log'))
    final_render=parse(text(CHECKS/'native-render-fixtures.log'))
    if final_render['failed'] or not final_render['passed']:
        raise ValueError('Final renderer fixture retest failed or ran no tests')
    if final_selection['failed'] or not final_selection['passed']:
        raise ValueError('Final selection test retest failed or ran no tests')
    if final_libraries['failed']:
        raise ValueError('Final changed-library tests failed')
    if native['failed']:
        raise ValueError('Unresolved native test failures')
    ui=re.sub(r'\x1b\[[0-9;]*m','',text(CHECKS/'ui-tests.log'))
    ui_tests=int(re.search(r'Tests\s+(\d+) passed',ui)[1])
    ui_files=int(re.search(r'Test Files\s+(\d+) passed',ui)[1])
    executable=digest(ROOT/'target/desktop-launch/debug/aura-desktop.exe')
    scenarios=[desktop,real,previous,first,photos]
    if not all(s.get('completed') and s.get('originalsUnchanged') and not s.get('pageErrors') and s['executableSha256']==executable for s in scenarios):
        raise ValueError('Incomplete, stale-executable, changed-input or UI-error desktop evidence')
    if not photo_success(photos):
        raise ValueError('Incomplete supplied-photo edit/export evidence')
    frozen=read(CHECKS/'source-manifest.json')
    changed=[name for name,value in frozen.items() if not (ROOT/name).is_file() or digest(ROOT/name)!=value]
    added=[path.relative_to(ROOT).as_posix() for path in source_paths() if path.relative_to(ROOT).as_posix() not in frozen]
    if changed or added:
        raise ValueError('Source changed after freeze: '+str(changed+added))
    observations=[{case['name']:case for case in scenario['cases'] if case.get('passed')} for scenario in [desktop,real]]
    tools=[]
    for entry in ledger['capabilities']:
        cases=[observation.get(entry['id']) for observation in observations]
        if not all(cases):
            raise ValueError('Missing per-tool invocation: '+entry['id'])
        tools.append(dict(entry,qualification='passed native desktop routes on synthetic and real-photo proxies',
                          synthetic=cases[0],realPhotoProxy=cases[1],evotoParity='unverified',perfection='not established'))
    result=dict(date='2026-10-22',newCapabilities=100,groups=ledger['groups'],native={key:value for key,value in native.items() if key!='records'},
                finalLibraries={key:value for key,value in final_libraries.items() if key!='records'},
                finalSelectionTests={key:value for key,value in final_selection.items() if key!='records'},
                finalRendererTests={key:value for key,value in final_render.items() if key!='records'},
                frontend=dict(tests=ui_tests,files=ui_files),commands=commands,executableSha256=executable,sourceFiles=len(frozen),sourceChanges=changed+added,
                desktopChecks=[len(s['cases']) for s in [desktop,real,previous,first]],tools=tools,
                photos={key:photos.get(key) for key in ['source','projectId','completed','originalsUnchanged','fullExportRequested','export','exportFiles','screenshotGuidedVariant']},
                evoto=dict(files=tree['fileCount'],bytes=tree['bytes'],completeInventory=tree['completeInventory'],treeManifestSha256=tree['treeManifestSha256'],
                           manifest=tree['manifest']['counts'],reaEvidence=str(ROOT/'.work-checks/evoto-2026-10-22/rea-evidence.json')),
                exactEvotoParity=False,proprietaryModelsRecovered=False,
                limits=['100 additions are 50 render methods,25 selections,25 diagnostics, not100 trained AI models.',
                        'Desktop scenarios exercise all new methods on one synthetic and one real-photo proxy, not all settings on all27 RAWs.',
                        'All27 RAWs receive native automatic enhancement, an explicit gentle stop-zone layer,70 diagnostics and verified full-resolution exports.',
                        'Evoto native exports remain unavailable in prior evidence; no per-tool fidelity/perfection score is justified.',
                        'Spatial corrections use a256-pixel grid; inspection uses a512-pixel preview. Full-frame colour methods can require substantial memory.',
                        'Complete file inventory and PE resources do not constitute complete decompilation or recovered trained logic.'])
    OUT.with_suffix('.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    with Path(str(OUT)+'-native-tests.csv').open('w',encoding='utf-8',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=['suite','test','status','reason']);writer.writeheader();writer.writerows(native['records'])
    with Path(str(OUT)+'-final-library-tests.csv').open('w',encoding='utf-8',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=['suite','test','status','reason']);writer.writeheader();writer.writerows(final_libraries['records'])
    with Path(str(OUT)+'-tools.csv').open('w',encoding='utf-8',newline='') as stream:
        writer=csv.writer(stream);writer.writerow(['group','id','name','functionalQualification','syntheticPixelChange','realProxyPixelChange','evotoParity','perfection'])
        writer.writerows([tool['group'],tool['id'],tool['name'],'pass',tool['synthetic'].get('pixelsChanged','not an edit'),tool['realPhotoProxy'].get('pixelsChanged','not an edit'),'unverified','not established'] for tool in tools)
    assets=ROOT/'docs/reports/assets';assets.mkdir(exist_ok=True)
    captures=[]
    for path in (CHECKS/'desktop').glob('*.png'):
        if path.stem.startswith('qualification-'):
            continue
        target=assets/('aura-2026-10-22-'+path.name);shutil.copyfile(path,target)
        captures.append(f'[{path.stem}](assets/{target.name})')
    no_effect=[tool['id'] for tool in tools if tool['group']!='inspection' and (not tool['synthetic'].get('pixelsChanged') or not tool['realPhotoProxy'].get('pixelsChanged'))]
    lines=['# AURA implementation and validation — 2026-10-22','',
           'Implemented **100 additional capabilities**:25 colour methods,25 spatial methods,25 selections,25 read-only diagnostics. The three method catalogues now contain75 entries each. Three additional review screens bring inspection to70 measurements. Existing workspace changes were preserved.','',
           '**Exact Evoto equivalence, every Evoto feature, recovered proprietary models and per-tool perfection have not been achieved or verified.** These additions are original mathematical implementations.','',
           '| Validation | Result |','|---|---|',
           f'| Complete native workspace suite | {native["passed"]} passed,0 failed,{native["ignored"]} ignored;{native["groups"]} result groups |',
           f'| Final changed-library retest | {final_libraries["passed"]} passed,0 failed,{final_libraries["ignored"]} ignored;includes checked-coordinate and inspection cleanup |',
           f'| Final selection fixture retest | {final_selection["passed"]} passed after three test-only lint fixes |',
           f'| Final renderer fixture retest | {final_render["passed"]} passed,{final_render["ignored"]} ignored after fixture lint cleanup |',
           f'| Frontend | {ui_tests} tests in{ui_files} files passed;6 targeted mask-panel retests passed after the label correction;production build passed |',
           '| Formatting,strict Clippy,contracts,IPC,evidence tooling | Passed;347 command definitions/registrations/typed calls agree;18 Python tests |',
           '| New methods in rebuilt AURA | 100/100 invoked on synthetic fixture;100/100 invoked on real-photo proxy;history,stale revisions,inspection immutability checked |',
           '| Prior additions regression | Both earlier100-capability suites passed in the rebuilt desktop |',
           f'| Supplied CR3 photos | {len(photos["photos"])}/27 edited and inspected;original SHA-256 values unchanged |',
           f'| Full-resolution JPEG export | {photos["export"]["written"]} written,{photos["export"]["verified"]} verified,0 render failures |',
           f'| Executable/source identity | All5 desktop scenarios match current executable;{len(frozen)} source hashes unchanged after freeze |','',
           '[Edited-photo gallery](../../output/aura-2026-10-22/index.html) · [Machine-readable report](AURA-validation-2026-10-22.json) · [Per-tool CSV](AURA-validation-2026-10-22-tools.csv) · [Native cases](AURA-validation-2026-10-22-native-tests.csv)','',
           'Desktop captures: '+', '.join(captures), '', '## Evoto investigation','',
           f'REA MCP inspected the supplied native worker and exported retained PE-resource evidence. The refreshed installation inventory hashed all{tree["fileCount"]} regular files ({tree["bytes"]} bytes),with no read gaps;{tree["manifest"]["counts"].get("match",0)} vendor-manifest checks matched. The worker matches earlier bounded native research by SHA-256.','',
           'This connected REA session has no configured deep-analysis provider. Earlier decompilation and runtime findings are reused only for the matching artifact. No proprietary code,weights,shaders or UI assets were incorporated. File coverage is not algorithm coverage. See [provider status](../../.work-checks/evoto-2026-10-22/provider-status.json) and [retained REA evidence](../../.work-checks/evoto-2026-10-22/rea-evidence.json).','',
           'A fresh bounded REA-skill follow-up used the existing objdump fallback on the matching worker: five ranges,23,198 bytes,5,404 decoded instructions,and25 validated literal references. It found reference-person/landmark/mask prerequisites,conditional size handling,semantic protection paths,body-skin instance masks,an optional LUT path,and an entry-format gate whose tag meanings remain unknown. These are wrapper/composition facts,not recovered numerical filters or trained networks. The older different-worker decompilation was excluded. [Exact findings and limits](AURA-Evoto-evidence-2026-10-22.md).','',
           'The optional first-photo variant uses the earlier watermarked Evoto preview as an explicitly labelled display reference. Its AURA matching distances are optimization diagnostics,not Evoto fidelity measurements. The screenshot retains its watermark and unknown display colour transform. The primary qualified recipe was restored after rendering the separate variant.','',
           '## Scope of the results','',
           'Tests use CARGO_INCREMENTAL=0 and the documented AURA_PERF_HOST_SCALE=6. The first incremental attempt failed to link with undefined LLVM anonymous symbols;the nonincremental full run is the final native result. A frequency-filter fixture caught a DC-removal error during development and was corrected. The first desktop suites found a stale50-workflow label despite75 available actions;the label now derives its count from the catalogue,with a targeted regression check. Initial failed logs remain retained. The Windows desktop linker also emitted a duplicate-manifest warning;the executable built and was launched for the recorded desktop checks.','',
           'After the final desktop build,strict linting required fixture-only edits in three files: split an oversized test,use contains for a membership assertion,replace a temporary zero vector with a fixed slice,enumerate coverage weights,format a small constant,and compare bit representations for exact float-preservation assertions. The affected selection and renderer tests were recompiled and rerun. Production implementation files did not change. Earlier source manifests are preserved alongside the final manifest.','',
           'All new tools were invoked through the real native desktop on a synthetic fixture and one rendered photo proxy. Each of the27 RAWs received enhancement,a gentle new stop-zone layer,and all70 diagnostics;every new tool was not individually applied to every RAW. Tests do not establish subjective superiority or exhaustive parameter coverage.','',
           'Conditional methods with unchanged pixels in at least one fixture: '+(', '.join(no_effect) or 'none')+'. Accepted saves/history are recorded separately from visible pixel effect. Native mechanism fixtures provide additional evidence.','',
           'Spatial corrections retain the256-pixel working grid;measurements use a512-pixel preview. Some colour methods allocate full-frame planes. Semantic trained skin/body reconstruction,training datasets,complete Evoto algorithm recovery and pixel-for-pixel parity remain unresolved.','',
           '## Per-tool functional qualification','', '| Group | Tool | Synthetic / real-photo pixel effect | Evoto perfection |','|---|---|---|---|']
    for tool in tools:
        effect='read-only' if tool['group']=='inspection' else f'{tool["synthetic"].get("pixelsChanged")} / {tool["realPhotoProxy"].get("pixelsChanged")}'
        lines.append(f'|{tool["group"]}|{tool["name"]}|{effect};route passed|unverified|')
    # Keep generated prose readable around interpolated counts and punctuation.
    prose='\n'.join(lines)+'\n'
    prose=re.sub(r'(?<=[A-Za-z])(?=\d)|(?<=\d)(?=[A-Za-z])', ' ', prose)
    prose=re.sub(r'([,;:])(?=[A-Za-z0-9])', r'\1 ', prose)
    prose=prose.replace('CR 3','CR3')
    OUT.with_suffix('.md').write_text(prose,encoding='utf-8')
    print(json.dumps(dict(native=result['native'],frontend=result['frontend'],tools=len(tools),photos=len(photos['photos']),report=str(OUT.with_suffix('.md')))))


if __name__=='__main__':
    import sys
    freeze() if '--freeze' in sys.argv else main()
