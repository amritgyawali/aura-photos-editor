"""Report actual validation coverage without presenting functional checks as Evoto parity."""
import csv
import hashlib
import json
from pathlib import Path
import re
import shutil
import sys
from aura_native_qualification import parse
from aura_qualification import photo_success

ROOT=Path(__file__).resolve().parents[1]
CHECKS=ROOT/'.work-checks/aura-advanced-2026-10-22'
OUT=ROOT/'docs/reports/AURA-advanced-validation-2026-10-22'
PHOTOS=ROOT/'output/aura-advanced-2026-10-22'

def read(path):return json.loads(path.read_text(encoding='utf-8-sig'))
def digest(path):
    with path.open('rb') as stream:return hashlib.file_digest(stream,'sha256').hexdigest()
def source_paths():
    result=[]
    for relative in ['crates','ui/src','ui/src-tauri/src','scripts']:
        result.extend(p for p in (ROOT/relative).rglob('*') if p.is_file() and p.suffix in ['.rs','.ts','.tsx','.css','.py','.ps1','.sh'])
    result.extend(ROOT/p for p in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','ui/package.json','ui/package-lock.json','ui/src-tauri/Cargo.toml','ui/src-tauri/Cargo.lock','ui/src-tauri/tauri.conf.json'])
    result.extend(ROOT/'docs/reports'/p for p in ['AURA-advanced-capabilities-2026-10-22.json','AURA-capabilities-2026-10-22.json','AURA-next-hundred-capabilities-2026-10-21.json','AURA-hundred-capabilities-2026-10-21.json'])
    return sorted(set(result))
def freeze():
    path=CHECKS/'source-manifest.json'
    if path.exists():raise ValueError('Preserve the earlier manifest before re-freezing')
    path.write_text(json.dumps({p.relative_to(ROOT).as_posix():digest(p) for p in source_paths()},indent=2)+'\n',encoding='utf-8')
    print('Source identity recorded')
def main():
    commands={p.stem:read(p) for p in CHECKS.glob('*.json') if not p.name.startswith('source-manifest') and 'command' in read(p)}
    required=['clippy','format-verified','ipc','contracts','python-tests','ui-tests-qualified','ui-build-qualified','desktop-build-qualified','review-launch','native-libraries-final','colour-discipline-verified','colour-discipline-clippy-verified']
    if any(commands[name]['exitCode']!=0 for name in required):raise ValueError('Unresolved validation command')
    frozen=read(CHECKS/'source-manifest.json')
    observed={p.relative_to(ROOT).as_posix():digest(p) for p in source_paths()}
    if frozen!=observed:raise ValueError('Source changed after qualification freeze')
    executable=digest(ROOT/'target/desktop-launch/debug/aura-desktop.exe')
    suites={name:read(CHECKS/name/'results.json') for name in ['new-synthetic','new-real-photo','previous100','second100','first100']}
    photos=read(PHOTOS/'results.json')
    for name,scenario in [*suites.items(),('photos',photos)]:
        if not scenario.get('completed') or not scenario.get('originalsUnchanged') or scenario.get('pageErrors') or scenario['executableSha256']!=executable:
            raise ValueError('Incomplete, stale executable, changed input or UI error: '+name)
    if not photo_success(photos):raise ValueError('Supplied photo qualification failed')
    native=parse((CHECKS/'native-tests.log').read_text(encoding='utf-8'))
    guard=parse((CHECKS/'colour-discipline-verified.log').read_text(encoding='utf-8'))
    failures=[row for row in native['records'] if row['status']=='FAILED']
    expected_guard='no_module_defines_its_own_working_space'
    if commands['native-tests']['exitCode']!=101 or len(failures)!=1 or failures[0]['test']!=expected_guard or 'colour_discipline' not in failures[0]['suite']:
        raise ValueError('The workspace failure does not match the single corrected test-only guard')
    if guard['passed']!=6 or guard['failed'] or not any(row['test']==expected_guard and row['status']=='ok' for row in guard['records']):
        raise ValueError('The complete corrected six-case architecture target did not pass')
    allowed_errors={'error: test failed, to rerun pass `-p aura-render --test colour_discipline`','error: 1 target failed:'}
    errors={line.strip() for line in (CHECKS/'native-tests.log').read_text(encoding='utf-8').splitlines() if line.strip().startswith('error:')}
    if errors!=allowed_errors:raise ValueError('Unresolved compiler or runner error in workspace evidence')
    before_guard=read(CHECKS/'source-manifest-before-colour-guard-repair.json')
    after_guard={name for name,value in observed.items() if before_guard.get(name)!=value}
    if after_guard!={'crates/aura-render/tests/colour_discipline.rs','scripts/report-aura-advanced.py'}:
        raise ValueError('Unexpected production or tooling changes after the test-only guard repair')
    ui=re.sub(r'\x1b\[[0-9;]*m','',(CHECKS/'ui-tests-qualified.log').read_text(encoding='utf-8'))
    frontend=dict(tests=int(re.search(r'Tests\s+(\d+) passed',ui)[1]),files=int(re.search(r'Test Files\s+(\d+) passed',ui)[1]))
    libraries=parse((CHECKS/'native-libraries-final.log').read_text(encoding='utf-8'))
    if libraries['failed'] or not libraries['passed']:raise ValueError('Final libraries failed or ran no tests')
    ledgers={
        'new-synthetic':read(ROOT/'docs/reports/AURA-advanced-capabilities-2026-10-22.json'),
        'previous100':read(ROOT/'docs/reports/AURA-capabilities-2026-10-22.json'),
        'second100':read(ROOT/'docs/reports/AURA-next-hundred-capabilities-2026-10-21.json'),
        'first100':read(ROOT/'docs/reports/AURA-hundred-capabilities-2026-10-21.json'),
    }
    ledger_files={'new-synthetic':'AURA-advanced-capabilities-2026-10-22.json','previous100':'AURA-capabilities-2026-10-22.json','second100':'AURA-next-hundred-capabilities-2026-10-21.json','first100':'AURA-hundred-capabilities-2026-10-21.json'}
    for name,filename in ledger_files.items():
        if suites[name]['ledgerSha256']!=digest(ROOT/'docs/reports'/filename):raise ValueError('Scenario ledger changed: '+name)
    if suites['new-real-photo']['ledgerSha256']!=suites['new-synthetic']['ledgerSha256']:raise ValueError('Real-photo ledger differs')
    first_photo=next(row for row in photos['photos'] if row['file']=='989A2698.CR3')
    if suites['new-real-photo']['realPhotoSha256']!=first_photo['before']['fileSha256']:raise ValueError('Real-photo suite lacks matching RAW preview provenance')
    tools=[];real={c['name']:c for c in suites['new-real-photo']['cases'] if c.get('passed')}
    for name,ledger in ledgers.items():
        cases={c['name']:c for c in suites[name]['cases'] if c.get('passed')}
        for row in ledger['capabilities']:
            if row['id'] not in cases:raise ValueError('Missing invocation: '+row['id'])
            if name=='new-synthetic' and row['id'] not in real:raise ValueError('Missing real-photo invocation: '+row['id'])
            tools.append(dict(row,iteration=name,status='functional_routes_qualified',functional='invocation/history passed; pixel effect recorded separately',synthetic=cases[row['id']],realPhotoProxy=real.get(row['id']) if name=='new-synthetic' else None,evotoParity='unverified',perfection='not established'))
    if len(tools)!=400 or len({(t['group'],t['id']) for t in tools})!=400:raise ValueError('Expected400 unique tracked capabilities')
    matrix=[]
    matrix_observations={row['id']:[] for row in ledgers['new-synthetic']['capabilities']}
    expected_originals={Path(row['file']).resolve():row for row in photos['inputManifest']}
    preview_by_name={row['file']:row['before'] for row in photos['photos']}
    bindings=[]
    for shard in range(3):
        manifest=read(CHECKS/f'photo-tool-matrix-shard-{shard}.json')
        if not manifest.get('completed'):raise ValueError('Incomplete matrix provenance shard')
        bindings.extend(manifest['proxies'])
    if len(bindings)!=27 or {Path(row['original']['file']).resolve() for row in bindings}!=set(expected_originals):raise ValueError('Matrix does not cover27 distinct expected originals')
    binding_by_result={Path(row['results']).resolve():row for row in bindings}
    if len(binding_by_result)!=27:raise ValueError('Duplicated matrix result identity')
    for path in sorted((CHECKS/'photo-tool-matrix').glob('*/results.json')):
        scenario=read(path)
        if not scenario.get('completed') or scenario.get('pageErrors') or not scenario.get('originalsUnchanged') or scenario['executableSha256']!=executable:raise ValueError('Incomplete photo-tool matrix: '+str(path))
        passed={c['name'] for c in scenario['cases'] if c.get('passed')}
        if not all(r['id'] in passed for r in ledgers['new-synthetic']['capabilities']):raise ValueError('Missing matrix method')
        binding=binding_by_result[path.resolve()];original=Path(binding['original']['file']).resolve();proxy=Path(binding['proxy']).resolve()
        preview=preview_by_name[original.name]
        if binding['original']['sha256']!=expected_originals[original]['sha256'] or digest(original)!=binding['original']['sha256']:raise ValueError('Matrix original binding changed')
        if proxy!=Path(preview['file']).resolve() or binding['proxySha256']!=preview['fileSha256'] or digest(proxy)!=binding['proxySha256']:raise ValueError('Matrix proxy differs from the generated RAW preview')
        if Path(scenario['realPhotoSource']).resolve()!=proxy or scenario['realPhotoSha256']!=binding['proxySha256']:raise ValueError('Matrix scenario used another preview')
        if scenario['ledgerSha256']!=suites['new-synthetic']['ledgerSha256']:raise ValueError('Matrix ledger changed')
        for case in scenario['cases']:
            if case['name'] in matrix_observations:matrix_observations[case['name']].append(dict(source=original.name,**case))
        matrix.append(dict(path=str(path),source=str(proxy),original=str(original),originalSha256=binding['original']['sha256'],proxySha256=binding['proxySha256'],capabilities=100))
    if len(matrix)!=27:raise ValueError('Expected completed tool checks on all27 supplied-photo proxies')
    for tool in tools:
        if tool['iteration']!='new-synthetic':continue
        observations=matrix_observations[tool['id']]
        if len(observations)!=27:raise ValueError('Expected27 observations for '+tool['id'])
        tool['photoMatrix']=dict(invocations=27,changedPhotos=sum(case.get('pixelsChanged') is True for case in observations) if tool['group']!='inspection' else None,
                                 maxScenarioSeconds=max(case.get('scenarioSeconds',0) for case in observations),observations=observations)
    tree=read(ROOT/'.work-checks/evoto-advanced-2026-10-22/file-inventory.json')
    configs=read(ROOT/'.work-checks/evoto-advanced-2026-10-22/configuration-classification.json')
    result=dict(date='2026-10-22',iteration='advanced',newCapabilities=100,trackedCapabilities=400,tools=tools,frontend=frontend,native={k:v for k,v in native.items() if k!='records'},finalLibraries={k:v for k,v in libraries.items() if k!='records'},commands=commands,
                nativeVerification=dict(verified=True,unresolvedFailures=0,initialFailures=failures,correctedGuard={k:v for k,v in guard.items() if k!='records'},scope='Complete workspace run plus one explicit six-case test-only architecture target retest; not a single clean full-run log.'),
                executableSha256=executable,sourceFiles=len(frozen),photoToolMatrix=matrix,
                photos={k:photos.get(k) for k in ['source','completed','originalsUnchanged','labTool','expectedDiagnostics','export','exportFiles','screenshotGuidedVariant']},
                evoto=dict(files=tree['fileCount'],bytes=tree['bytes'],completeInventory=tree['completeInventory'],treeManifestSha256=tree['treeManifestSha256'],vendorChecks=tree['manifest']['counts'],configurationCounts=configs['counts']),exactEvotoParity=False,proprietaryModelsRecovered=False)
    OUT.with_suffix('.json').write_text(json.dumps(result,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    with Path(str(OUT)+'-tools.csv').open('w',encoding='utf-8',newline='') as stream:
        writer=csv.writer(stream);writer.writerow(['iteration','group','id','name','functional','syntheticPixelChange','realProxyPixelChange','photoMatrixInvocations','photoMatrixPixelChanges','maxPhotoScenarioSeconds','evotoParity','perfection'])
        for t in tools:
            matrix_case=t.get('photoMatrix',{})
            writer.writerow([t['iteration'],t['group'],t['id'],t['name'],'invocation/history passed',t['synthetic'].get('pixelsChanged','read-only/workflow'),(t['realPhotoProxy'] or {}).get('pixelsChanged','not individually checked'),matrix_case.get('invocations','not individually checked'),matrix_case.get('changedPhotos','read-only/not individually checked'),matrix_case.get('maxScenarioSeconds','not individually checked'),'unverified','not established'])
    with Path(str(OUT)+'-native-tests.csv').open('w',encoding='utf-8',newline='') as stream:
        writer=csv.DictWriter(stream,fieldnames=['suite','test','status','reason']);writer.writeheader();writer.writerows(native['records'])
    assets=ROOT/'docs/reports/assets';assets.mkdir(exist_ok=True)
    for p in (CHECKS/'new-real-photo').glob('*.png'):
        if not p.stem.startswith('qualification-'):shutil.copyfile(p,assets/('aura-advanced-'+p.name))
    lines=['# AURA advanced implementation and validation - 2026-10-22','',
           'Added 100 further independent capabilities: 25 colour methods, 25 spatial methods, 25 selections and 25 read-only diagnostics. Each editing catalogue now contains 100 methods; inspection contains 95 measurements. Texture fidelity, Colour stability and Delivery integrity add three review screens.','',
           '**Complete Evoto reconstruction, proprietary model recovery, exact tone/output parity and per-tool perfection have not been achieved.** Functional passes below do not measure equivalence to Evoto.','',
           '| Check | Actual result |','|---|---|',
           f'| Initial complete native suite | {native["passed"]} passed, one architecture-guard failure, {native["ignored"]} ignored; corrected below |',
           '| Corrected architecture target | 6 passed, 0 failed; canonical matrix imports accepted, local definitions rejected; no unresolved native failures |',
           f'| Final changed-library retest | {libraries["passed"]} passed, {libraries["failed"]} failed, {libraries["ignored"]} ignored |',
           f'| Frontend | {frontend["tests"]} tests in {frontend["files"]} files passed |',
           '| Strict Clippy, formatting, contracts, IPC, Python tooling | Passed |',
           '| New capability desktop tests | 100/100 synthetic and 100/100 real-photo proxy invocations; exact Undo/Redo and stale-revision guards where applicable |',
           '| Existing capability regression | 300 previously tracked capabilities invoked in the rebuilt desktop |',
           f'| Additional photo-tool matrix | {len(matrix)} supplied-photo proxies, 100 additions invoked on each |',
           f'| Supplied originals and export | 27 RAWs edited and inspected; {photos["export"]["verified"]} full-resolution JPEGs verified; original SHA-256 values unchanged |',
           f'| Source and executable identity | {len(frozen)} source files match the frozen manifest; all desktop suites used the current executable |','',
           '[Edited-photo gallery](../../output/aura-advanced-2026-10-22/index.html) | [Per-tool CSV](AURA-advanced-validation-2026-10-22-tools.csv) | [Machine report](AURA-advanced-validation-2026-10-22.json) | [Native cases](AURA-advanced-validation-2026-10-22-native-tests.csv)','',
           '## Existing-tool improvements','',
           'Colour methods now evaluate selected local stencil targets while blending, avoiding the retained full-frame RGB target plane. The relevant implementation ledger and native fixtures identify exact applicability and allocation limits. Spatial repair adds a bounded native-resolution path for supported local operators, preserving small selections that disappeared on the former working grid; global/geometric methods retain an explicitly documented reduced grid. Selection controls validate only parameters used by the active method, and resource preflight happens before source loading. Existing horizontal autocorrelation now centres each valid lagged support separately, making an affine ramp correlate correctly. Renderer identity advanced to labs.selected-v5 so cached older output is invalidated.','',
           'See [implementation and limits](../advanced-editing-methods-2026-10-22.md) and the three mechanism ledgers for the exact changed operators. Improvements apply to documented mechanisms; every existing tool was not individually redesigned.','',
           '## Evoto investigation','',
           f'REA MCP inspected the current worker section layout and retained evidence ev_3f7a60e930868de6e68156879bfc76e697535b837a336fb0be273ea659b61f54. Its worker SHA-256 remains add7588f94aed190b97076f949709ba9622aa14f046b661783135d838d3d3bb2. The refreshed inventory hashed all {tree["fileCount"]} regular files ({tree["bytes"]} bytes); vendor consistency results: {tree["manifest"]["counts"]}.','',
           f'All 621 resources with a .json extension were examined by a strict UTF-8 JSON parser: {configs["counts"]}. Opaque content does not establish its encoding, encryption, tensor shapes, learned weights or algorithm. No inspected code, model, configuration values or image assets were incorporated into AURA. Deep native analysis remains unavailable because no supported decompiler is configured. [REA evidence](../../.work-checks/evoto-advanced-2026-10-22/rea-evidence.json), [provider status](../../.work-checks/evoto-advanced-2026-10-22/provider-status.json), [file inventory](../../.work-checks/evoto-advanced-2026-10-22/file-inventory.json), [configuration classification](../../.work-checks/evoto-advanced-2026-10-22/configuration-classification.json).','',
           '## Limits of the checks','',
           'Automated tests establish selected functional mechanisms, serialization, finite outputs and the exercised history/control paths. They do not establish subjective superiority or exhaustive parameter coverage. The photo-tool matrix uses rendered proxies, not full-resolution RAW invocations of every method. RAW exports use native automatic enhancement plus the explicit gentle colour method recorded per photo; creative effects were not stacked indiscriminately. Inspection remains limited to a 512-pixel preview. Some global spatial methods use a reduced working grid; new local native-resolution operators have bounded scratch and halo limits. Trained semantic face/body model reconstruction, matching training data and full Evoto algorithm recovery remain unresolved. AURA is open with the qualified catalogue.','',
           '## Per-tool functional results','',
           'An invocation/history pass does not require a visible change. Conditional operators can be neutral on a fixture; the following pixel-effect observation is separate from the route check and from numerical mechanism tests. No-op routes are not labelled as demonstrated effects.','',
           'The full workspace build started during boundary review. The final changed-library rerun covers the frozen implementation and final regression fixtures. Initial incomplete-module attempts, pre-final build records and source manifests remain retained. The first scenario startup was stopped before qualifying any cases when artifact timestamps exposed a renderer compiled before the last fixes; the qualified executable was then rebuilt and all scenarios restarted. The Windows linker retained its duplicate-resource-manifest warning.','',
           'The complete workspace run found one false positive in the lexical colour-space guard: the new perceptual helper already imports the single canonical matrix from aura-raw, but the guard allowed its symbol only in two historical filenames. The corrected guard checks canonical imports and rejects local const/static/let redefinitions, with positive and negative fixtures. All six architecture cases and scoped strict Clippy passed. Only the integration test and report generator changed afterward; the qualified executable and every production implementation stayed identical. The original failed workspace log is retained, and the report does not describe it as a single clean full-run pass.','',
           '| Iteration | Group | Method | Route/history; synthetic pixel effect | New photo matrix pixel changes | Evoto perfection |','|---|---|---|---|---|---|']
    for t in tools:
        matrix_case=t.get('photoMatrix')
        effect='read-only' if matrix_case and t['group']=='inspection' else str(matrix_case['changedPhotos'])+'/27' if matrix_case else 'not individually checked'
        lines.append(f'| {t["iteration"]} | {t["group"]} | {t["name"]} | passed; {t["synthetic"].get("pixelsChanged","read-only/workflow")} | {effect} | unverified |')
    OUT.with_suffix('.md').write_text('\n'.join(lines)+'\n',encoding='utf-8')
    print(json.dumps(dict(new=100,tracked=400,native=result['native'],frontend=frontend,matrix=len(matrix),photos=27,report=str(OUT.with_suffix('.md')))))

if __name__=='__main__':freeze() if '--freeze' in sys.argv else main()
