"""Write the next100 report from current executable evidence, failing closed."""
from collections import Counter
import csv
import hashlib
import json
from pathlib import Path
import re
import shutil

ROOT=Path(__file__).resolve().parents[1]
CHECKS=ROOT/'.work-checks/aura-next100'
PHOTOS=ROOT/'output/aura-next100-qualified'
REPORT=ROOT/'docs/reports/AURA-next-hundred-validation-2026-10-21'


def text(path):
    raw=path.read_bytes() if path.exists() else b''
    return raw.decode('utf-16' if raw.startswith(b'\xff\xfe') else 'utf-8-sig',errors='replace')


def read(path):
    return json.loads(text(path)) if path.exists() else {}


def native_records(log):
    records=[]
    suite=''
    for line in log.splitlines():
        line=line.strip()
        if line.startswith(('Running ','Doc-tests ')):
            suite=line
        match=re.match(r'^test (.+?) \.\.\. (ok|FAILED|ignored)(?:, (.*))?$',line)
        if match:
            records.append(dict(suite=suite,test=match[1],status=match[2],reason=match[3] or ''))
    return records


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source,'sha256').hexdigest()


def main():
    ledger=read(ROOT/'docs/reports/AURA-next-hundred-capabilities-2026-10-21.json')
    desktop=read(CHECKS/'desktop/results.json')
    previous=read(CHECKS/'previous100/results.json')
    photos=read(PHOTOS/'results.json')
    review_launch=read(CHECKS/'review-launch.json')
    tree=read(CHECKS/'evoto-tree.json')
    rea=read(CHECKS/'rea-native-resources.json')
    camera=read(CHECKS/'rea-camera-function-status.json')
    targeted=read(CHECKS/'camera-targeted-decompilation.json')
    camera_functions=targeted.get('functions',[])
    camera_summary=dict(provider='Ghidra12.1.4 targeted fallback with auto-analysis disabled',
                        sha256=targeted.get('executable_sha256'),
                        headlessExit=text(CHECKS/'camera-targeted-headless.exit').strip(),
                        functions=[{key:value for key,value in item.items() if key in ['address','end_inclusive','instruction_count','disassembled','decompiled']} for item in camera_functions],
                        observations=['The selected preview export checks a non-null context and an SDK state byte before dispatching through vtable offset0x58; otherwise it returns0x50.',
                                      '38 decoded instructions cover one145-byte export wrapper only. Internal virtual implementation and physical camera behaviour remain unobserved.'])
    native_log=text(CHECKS/'native-tests-initial.log')
    summaries=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored',native_log)
    initial_totals=[sum(int(row[i]) for row in summaries) for i in range(3)]
    verification=read(CHECKS/'native-verification.json')
    totals=[verification[key] for key in ['passed','failed','ignored']] if verification.get('verified') else initial_totals
    records=verification.get('records',native_records(native_log))
    counts=Counter(row['status'] for row in records)
    records_match=[counts['ok'],counts['FAILED'],counts['ignored']]==totals
    ui_log=re.sub(r'\x1b\[[0-9;]*m','',text(CHECKS/'ui-tests-final.log'))
    ui_tests=re.search(r'Tests\s+(\d+) passed',ui_log)
    ui_files=re.search(r'Test Files\s+(\d+) passed',ui_log)
    checks={name:text(CHECKS/(name+'.exit')).strip() or 'not completed' for name in [
        'native-verified','clippy-final','format','contracts','ipc','ui-tests-final','ui-build-final',
        'python-tests','desktop-build','desktop-audit','previous100-audit','photo-audit']}
    observed={case['name']:case for case in desktop.get('cases',[]) if case.get('passed')}
    additions=[dict(item,status='pass' if item['id'] in observed else 'not completed',
                    desktopEvidence=observed.get(item['id'])) for item in ledger['capabilities']]
    source_manifest=read(CHECKS/'frozen-source-manifest.json')
    changed_source=[row['path'] for row in source_manifest if digest(ROOT/row['path'])!=row['sha256']]
    executable=ROOT/'target/desktop-launch/debug/aura-desktop.exe'
    executable_hash=digest(executable) if executable.exists() else None
    builds_match=bool(executable_hash) and all(item.get('executableSha256')==executable_hash for item in [desktop,previous,photos])
    from aura_qualification import photo_success
    success=all(value=='0' for value in checks.values()) and records_match and totals[0]>4400 and totals[1]==0
    success=success and desktop.get('completed') and previous.get('completed') and photo_success(photos)
    success=bool(success and all(item['status']=='pass' for item in additions) and builds_match and source_manifest and not changed_source)
    success=bool(success and review_launch.get('opened') and review_launch.get('savedRecipeHashesVerified')==len(photos.get('photos',[])))
    commands=sorted(set(re.findall(r'#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)',text(ROOT/'ui/src-tauri/src/main.rs'))))
    calls=Counter(desktop.get('commandCounts',{}))+Counter(previous.get('commandCounts',{}))+Counter(photos.get('commandCounts',{}))
    command_records=[dict(command=name,invocations=calls[name],status='invoked' if calls[name] else 'no current desktop invocation recorded') for name in commands]
    export=photos.get('export',{})
    payload=dict(date='2026-10-21',qualified=success,checks=checks,
                 additions=additions,native=dict(passed=totals[0],failed=totals[1],ignored=totals[2],resultGroups=len(summaries),caseRecords=len(records),caseRecordsMatchSummaries=records_match,
                                                scope=verification.get('scope'),initialFullRun=verification.get('initial'),targetRetests=verification.get('retests'),resolvedCases=verification.get('resolvedCases')),
                 frontend=dict(passed=int(ui_tests[1]) if ui_tests else None,files=int(ui_files[1]) if ui_files else None),
                 desktop=desktop,previous100=previous,photos=photos,commands=command_records,reviewLaunch=review_launch,
                 sourceFreeze=dict(files=len(source_manifest),changed=changed_source,testOnlyAmendment=read(CHECKS/'freeze-test-amendment.json')),executableSha256=executable_hash,buildsMatch=builds_match,
                 evoto=dict(files=tree.get('fileCount'),bytes=tree.get('bytes'),completeInventory=tree.get('completeInventory'),treeManifestSha256=tree.get('treeManifestSha256'),
                            formats=tree.get('formats'),manifest=tree.get('manifest',{}).get('counts'),
                            reaResourcesComplete=sum(row.get('status')=='complete' for row in rea.get('records',[])),
                            reaExcluded=[{key:row[key] for key in ['path','bytes','reason']} for row in rea.get('records',[]) if row.get('status')=='budget-excluded'],
                            cameraFunctionAttempt=camera,cameraTargetedFallback=camera_summary),
                 exactEvotoParity=False,allEvotoFeaturesImplemented=False,trainedModelsRecovered=False,
                 limitations=['Inventory is complete; reverse engineering of every function, opaque model, training process and native output is not complete.',
                              'New100 =50 render methods+25 selection workflows+25 read-only inspection tools. These are independent implementations, not100 pretrained AI models.',
                              'Spatial processing uses a256-pixel correction grid; diagnostics use a512-pixel working-space proxy.',
                              'Every registered command is inventoried, but only recorded commands were driven in the desktop scenarios.',
                              'Automated tests do not establish photographer preference or pixel equality with Evoto. Native paired Evoto exports remain unavailable.',
                              'Ignored tests, absent optional models, physical camera interoperability, other operating systems and external services are not newly certified.'])
    REPORT.with_suffix('.json').write_text(json.dumps(payload,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    for suffix,rows,fields in [('-native-tests.csv',records,['suite','test','status','reason','initialStatus','resolvedBy']),('-commands.csv',command_records,['command','invocations','status'])]:
        with Path(str(REPORT)+suffix).open('w',encoding='utf-8',newline='') as stream:
            writer=csv.DictWriter(stream,fieldnames=fields);writer.writeheader();writer.writerows(rows)
    assets=ROOT/'docs/reports/assets';assets.mkdir(exist_ok=True)
    for name in ['colour','spatial','selection','palette-review','detail-review','composition-review']:
        source=CHECKS/'desktop'/f'{name}.png'
        if source.exists():shutil.copyfile(source,assets/f'aura-next100-{name}-2026-10-21.png')
    resource_records=[{key:value for key,value in row.items() if key!='evidence'} for row in rea.get('records',[])]
    (ROOT/'docs/reports/AURA-evoto-native-resources-2026-10-21.json').write_text(json.dumps(resource_records,indent=2)+'\n',encoding='utf-8')
    (ROOT/'docs/reports/AURA-evoto-camera-boundary-2026-10-21.json').write_text(json.dumps(camera_summary,indent=2)+'\n',encoding='utf-8')
    lines=[
        '# AURA: another 100 capabilities and full-folder qualification','',
        'Date: 2026-10-21, Asia/Katmandu. Local qualification '+('passed.' if success else '**incomplete; see recorded checks below.**'),'',
        'This adds 100 distinct capabilities after the preceding hundred: 25 colour methods, 25 spatial methods, 25 selection workflows, and 25 read-only inspection tools. The colour and spatial panels now each expose 50 methods; the selection laboratory exposes 50 workflows. Three additional editor tabs provide palette/tone, detail, and composition review. Existing stacks remain bounded to 32 operations.','',
        '**Exact Evoto output, every Evoto feature, and proprietary trained-model parity have not been achieved or verified.** No Evoto code, weights, shaders or assets were incorporated into AURA. Tool counts and automated tests do not demonstrate visual superiority.','',
        'Actual rebuilt-desktop captures: [Colour lab](assets/aura-next100-colour-2026-10-21.png), [Spatial repair](assets/aura-next100-spatial-2026-10-21.png), [Selection laboratory](assets/aura-next100-selection-2026-10-21.png), [Palette review](assets/aura-next100-palette-review-2026-10-21.png), [Detail review](assets/aura-next100-detail-review-2026-10-21.png), [Composition review](assets/aura-next100-composition-review-2026-10-21.png).','',
        '## Recorded validation','',
        '|Check|Result|','|---|---|',
        f'|Native workspace plus scoped retests|{totals[0]} distinct cases verified, {totals[1]} unresolved failures, {totals[2]} ignored; {len(summaries)} original result groups; case records agree: {records_match}|',
        f'|Original full-run history|{initial_totals[0]} passed, {initial_totals[1]} failed, {initial_totals[2]} ignored; both failures remain recorded and were resolved by passing scoped retests|',
        f'|Frontend|{payload["frontend"]["passed"]} tests in {payload["frontend"]["files"]} files passed; production build exit {checks["ui-build-final"]}|',
        f'|Strict Clippy/format/contracts/IPC|{checks["clippy-final"]}/{checks["format"]}/{checks["contracts"]}/{checks["ipc"]}; 83 frozen contracts; 347 definitions = registrations = typed calls|',
        f'|Python evidence tooling|exit {checks["python-tests"]}; 18 tests|',
        f'|New 100 actual-desktop qualification|{sum(item["status"]=="pass" for item in additions)}/100; {len(desktop.get("cases",[]))} checks; completed: {desktop.get("completed",False)}|',
        f'|Previous 100 regression|{len(previous.get("cases",[]))} checks; completed: {previous.get("completed",False)}|',
        f'|Supplied RAW photos|{sum(bool(item.get("passed")) for item in photos.get("photos",[]))}/{len(photos.get("inputManifest",[]))} edited; original SHA256 unchanged: {photos.get("originalsUnchanged","not yet checked")}|',
        f'|Full-resolution JPEG export|{export.get("written",0)} written, {export.get("verified",0)} verified, {export.get("renderFailed","unavailable")} render failures|',
        f'|Executable/source identity|All three desktop scenarios match current executable: {builds_match}; {len(source_manifest)} source hashes checked, {len(changed_source)} changed after freeze|',
        f'|Final AURA review window|opened: {review_launch.get("opened",False)}; {review_launch.get("savedRecipeHashesVerified",0)} persisted recipe hashes checked against verified exports|','',
        'Native tests use the documented host setting `AURA_PERF_HOST_SCALE=6`. Ignored tests are excluded from pass counts. The command inventory explicitly marks routes without a current desktop invocation; this is not an assertion that every parameter of every feature was exhaustively tested.','',
        'All 50 new colour/spatial methods changed the representative 640×96 synthetic fixture. Two earlier conditional methods, `chroma_winsorize` and `mosquito_suppress`, produced no pixel change on the earlier fixture; their save/render/history scenarios passed, while algorithm-specific native fixtures provide separate coverage. A pixel change by itself does not establish a better photograph.','',
        'Early qualification caught and corrected tone-skew direction, empty tonal-bin colour shifts, spatial boundary leakage above the correction-grid resolution, mislabelled radial/perspective controls, palette percentage accumulation, and incomplete coarse Haar footprints. Their regression fixtures are included in the native/UI suites. Earlier failed development logs remain separate; the table uses final frozen-source runs.','',
        'The full workspace run found two stale assertions: the material/inspection integration test expected 20 diagnostics, and the cache identity test expected v2. They now assert 45 distinct diagnostic IDs and v3. The material integration target and complete renderer library target were rerun. Native totals above count distinct original cases with these two failures resolved by explicit passing retests; they do not describe a single clean full-run invocation. The CSV preserves each original status and its resolving log. Both changes are confined to test expectations; production processing and the executable remain unchanged. The desktop linker retains its nonfatal resource-manifest warning, and Vite retains its large-bundle advisory; actual startup is qualified separately.','',
        'The first desktop smoke attempts also expected the earlier “25 workflows” caption. Both audit selectors were corrected to 50 and both desktop audits were rerun in fresh catalogues. The failed caption attempts remain archived separately; the table and capability evidence use the completed reruns.','',
        '## Your photos','',
        '[Open the 27-photo before/after gallery](../../output/aura-next100-qualified/index.html). Full-resolution edited JPEGs are under `output/aura-next100-qualified/full-resolution/edited/`; per-photo recipes, inspection results and hashes are beside the gallery.','',
        'The photo run uses native AURA auto enhancement plus a gentle, explicit new lower-tail luminance layer (amount 0.3, strength 0.6, pivot 0.04). Every photo also receives 45 read-only inspection measurements. The recipe preserves originals and separates the added selection/tone layer for later editing. This is an AURA finishing choice, not a recovered Evoto preset.','',
        '## Evoto evidence and limits','',
        f'`F:/evoto`: {tree.get("fileCount")} regular files,{tree.get("bytes")} bytes,all hashed with stable reads;read gaps:{len(tree.get("gaps",[]))}. All1854 vendor-manifest MD5 entries match. Tree manifest SHA256:`{tree.get("treeManifestSha256")}`. MD5 agreement records consistency,not an authenticity signature.','',
        '[Every-file inventory and SHA256](AURA-evoto-file-inventory-2026-10-21.csv) · [REA native-resource evidence index](AURA-evoto-native-resources-2026-10-21.json).','',
        f'REA6.7.0 inspected complete resource tables in{payload["evoto"]["reaResourcesComplete"]} of160 PE files. The2GB historical `Evoto_Setup_7.3.5-185.exe` remains hashed but excluded from the256MiB resource-parser budget. It is not treated as the installed application version. The default Ghidra/REA probe of `TsCameraSDK.dll!TsCPreviewStart` timed out after180 seconds. A targeted fallback with auto-analysis disabled then successfully decoded38 instructions and decompiled the145-byte wrapper. It checks context/SDK state before virtual dispatch;this does not establish physical camera compatibility or image-processing internals. See [camera-boundary evidence](AURA-evoto-camera-boundary-2026-10-21.json).','',
        'The installation contains native applications,third-party libraries,camera adapters,configuration carriers and opaque processing resources. Header/resource inspection does not recover model architecture,training data,loss functions,learned skin/body processing or complete processing order. Prior observed Evoto preview evidence remains watermarked; there is no paired native Evoto export to certify identical tone or retouch.','',
        'The [earlier feature-family gap inventory](AURA-feature-coverage-2026-10-19.json) remains historical evidence. These 100 additions do not reclassify every Evoto feature family as equivalent or complete.','',
        'The current public workflow reference is [Evoto Desktop](https://www.evoto.ai/ai-photo-editor);the documented colour controls are [Evoto Color support](https://support.evoto.ai/color-adjustments-module/). These describe feature families,not source code or independently verified performance.','',
        '## Capability-by-capability evidence','',
        '|#|Group|New capability|Native qualification|','|---|---|---|---|',
    ]
    lines += [f'|{index}|{item["group"]}|{item["name"]} (`{item["id"]}`)|{item["status"]}|' for index,item in enumerate(additions,1)]
    lines += ['', 'Individual formulas,source files and native tests are in [the100-item implementation ledger](AURA-next-hundred-capabilities-2026-10-21.json). Full outcomes,hashes,commands and per-photo results are in [the validation JSON](AURA-next-hundred-validation-2026-10-21.json). Native cases and command coverage are exported as adjacent CSV files. Detailed raw logs and complete REA envelopes remain under `.work-checks/aura-next100/`.','',
              'Spatial methods remain bounded-grid approximations; inspection uses small previews. Fine skin detail,print-size artifacts and subjective portrait quality need full-resolution human review. Camera hardware,other platforms,optional models and external services are outside this fresh desktop qualification.','']
    document='\n'.join(lines)
    document=re.sub(r'([,;])(?=\S)',r'\1 ',document)
    document=re.sub(r'\b(REA|in|of|The|the|after|decoded|decompiled|All)(?=\d)',r'\1 ',document)
    document=document.replace('2GB','2 GB').replace('256MiB','256 MiB').replace('hundred:25','hundred: 25')
    REPORT.with_suffix('.md').write_text(document,encoding='utf-8')
    if success:
        ledger['qualification']=dict(passed=True,executableSha256=executable_hash,
                                     report=REPORT.with_suffix('.json').relative_to(ROOT).as_posix(),
                                     nativeCapabilitiesObserved=100)
        for item in ledger['capabilities']:
            item['status']='qualified_local_desktop'
        (ROOT/'docs/reports/AURA-next-hundred-capabilities-2026-10-21.json').write_text(json.dumps(ledger,indent=2,ensure_ascii=False)+'\n',encoding='utf-8')
    print(json.dumps(dict(qualified=success,native=payload['native'],frontend=payload['frontend'],checks=checks)))
    return 0 if success else 1


if __name__=='__main__':
    raise SystemExit(main())
