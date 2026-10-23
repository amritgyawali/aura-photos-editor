"""Build a fresh per-tool report without converting functional passes into parity scores."""
import argparse
from collections import Counter
import csv
import hashlib
import json
from pathlib import Path
import re

from aura_native_qualification import parse
from aura_qualification import photo_success

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    return json.loads(path.read_text(encoding='utf-8-sig'))


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def enum_names(filename, name='Tool'):
    source=(ROOT/filename).read_text(encoding='utf-8')
    body=source.split('pub enum '+name+' {',1)[1].split('\n}',1)[0]
    return {re.sub(r'(?<!^)(?=[A-Z])','_',variant).lower()
            for variant in re.findall(r'^    ([A-Z][A-Za-z0-9]+),',body,re.M)}


def table_file(path, rows):
    with path.open('w', newline='', encoding='utf-8-sig') as stream:
        writer = csv.DictWriter(stream, fieldnames=list(rows[0]))
        writer.writeheader()
        writer.writerows(rows)


def publication_copy(value):
    if isinstance(value,dict):
        return {key:publication_copy(item) for key,item in value.items()
                if key not in {'thumbnail','rgbBase64','imageBase64'}}
    if isinstance(value,list):
        return [publication_copy(item) for item in value]
    if isinstance(value,str) and value.startswith('data:image/'):
        return '[Photo preview retained only in local qualification evidence]'
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checks',type=Path,default=ROOT/'.work-checks/aura-comprehensive-2026-10-22')
    parser.add_argument('--photos',type=Path,default=ROOT/'output/aura-comprehensive-final-2026-10-22')
    parser.add_argument('--output',type=Path,default=ROOT/'docs/reports/AURA-comprehensive-test-2026-10-22')
    args = parser.parse_args()
    checks, out = args.checks.resolve(), args.output.resolve()
    commands = {p.stem:read(p) for p in checks.glob('*.json') if 'command' in read(p)}
    required = ['ui-tests','ui-build','desktop-build','native-final','clippy-fixed','contracts',
                'format-final','ipc','python-tests','banned','core-final','combined-synthetic-final',
                'combined-photo-final','advanced-final','first100-final','gui-final','photos-final','report-integrity']
    assert all(commands[name]['exitCode'] == 0 for name in required), 'Incomplete or failed qualification command'
    native = parse((checks/'native-final.log').read_text(encoding='utf-8'))
    assert native['passed'] and native['failed'] == 0
    frontend_log = re.sub(r'\x1b\[[0-9;]*m','',(checks/'ui-tests.log').read_text(encoding='utf-8'))
    frontend = dict(tests=int(re.search(r'Tests\s+(\d+) passed',frontend_log)[1]),
                    files=int(re.search(r'Test Files\s+(\d+) passed',frontend_log)[1]))
    executable = digest(ROOT/'target/desktop-launch/debug/aura-desktop.exe')
    suites = {name:read(checks/name/'results.json') for name in [
        'core-final','combined-synthetic-final','combined-photo-final','advanced-final','first100-final']}
    photos = read(args.photos/'results.json')
    for name,suite in [*suites.items(),('photos-final',photos)]:
        assert suite.get('completed') and suite.get('originalsUnchanged') and not suite.get('pageErrors'), name
        assert suite.get('executableSha256') == executable, 'Different executable: '+name
        assert not suite.get('fatalError'), name
    assert photo_success(photos)
    assert photos.get('sidecarsVerified') and len(photos.get('sidecarFiles', [])) == len(photos['photos'])
    assert photos['export']['sidecars'] == len(photos['photos'])
    first_photo=next(photo for photo in photos['photos'] if photo['file']=='989A2698.CR3')
    assert suites['combined-photo-final']['realPhotoSha256']==first_photo['before']['fileSha256']
    assert suites['core-final']['sourceSha256']==first_photo['before']['fileSha256']
    gui = read(checks/'gui-final/results.json')
    assert gui['originals_unchanged'] and not gui['page_errors'] and all(row['passed'] for row in gui['steps'])
    manifest = read(checks/'source-manifest.json')
    assert all((ROOT/name).is_file() and digest(ROOT/name)==value for name,value in manifest.items()), 'Source changed after freeze'
    synthetic = {(row.get('group'),row['name']):row for row in suites['combined-synthetic-final']['cases']}
    real = {(row.get('group'),row['name']):row for row in suites['combined-photo-final']['cases']}
    workflows = {(row.get('group'),row['name']):row for row in suites['first100-final']['cases']}
    ledgers = ['AURA-hundred-capabilities-2026-10-21','AURA-next-hundred-capabilities-2026-10-21',
               'AURA-capabilities-2026-10-22','AURA-advanced-capabilities-2026-10-22']
    tools = []
    for ledger in ledgers:
        for item in read(ROOT/'docs/reports'/(ledger+'.json'))['capabilities']:
            source = workflows if item['group']=='workflow' else synthetic
            key=(item['group'],item['id'])
            case = source[key]
            assert case['passed'], item['id']
            photo_case = real.get(key)
            if item['group']!='workflow':
                assert photo_case and photo_case['passed'], item['id']
            tools.append(dict(group=item['group'],id=item['id'],name=item['name'],
                              status='passed',synthetic=case,realPhotoProxy=photo_case,
                              evidence=ledger,evotoPerfection='unverified'))
    assert len(tools)==400 and len({(row['group'],row['id']) for row in tools})==400
    keyed = {(row['group'],row['id']):row for row in tools}
    for case in suites['core-final']['cases']:
        assert case['passed'], case['name']
        key = (case['group'],case['name'])
        if key in keyed:
            keyed[key]['additionalCoreEvidence'] = case
        else:
            row=dict(group=case['group'],id=case['name'],name=case.get('title',case['name']),status='passed',
                     synthetic=None,realPhotoProxy=case,evidence='core-final',evotoPerfection='unverified')
            tools.append(row)
            keyed[key]=row
    catalogues=[('crates/aura-recipe/src/color_lab.rs','Tool',{'colour'}),
                ('crates/aura-recipe/src/spatial_lab.rs','Tool',{'spatial'}),
                ('crates/aura-recipe/src/retouch_tools.rs','Tool',{'retouch'}),
                ('crates/aura-recipe/src/material_lab.rs','Tool',{'material'}),
                ('crates/aura-app/src/mask_workflows.rs','Action',{'selection','selection-management'})]
    for filename,enum,groups in catalogues:
        expected=enum_names(filename,enum)
        observed={row['id'] for row in tools if row['group'] in groups}
        assert expected==observed, 'Incomplete declared tool coverage: '+filename+' '+str(expected-observed)
    counts=Counter(row['group'] for row in tools)
    rows=[]

    def effect(case):
        if not case:
            return 'not tested in this scenario'
        if case.get('selectionAvailable') is False:
            return 'no selection; safe refusal: '+case['refusal']
        if case.get('selectionAvailable') and not case.get('coverageNonzero'):
            return 'empty coverage on this fixture; selection effect not demonstrated'
        if 'pixelsChanged' in case:
            return 'changed' if case['pixelsChanged'] else 'unchanged; conditional or neutral on this fixture'
        return 'read-only or workflow; see evidence'

    for tool in tools:
        cases=[case for case in [tool['synthetic'],tool['realPhotoProxy']] if case]
        rows.append(dict(group=tool['group'],id=tool['id'],name=tool['name'],route=tool['status'],
                         syntheticPixelEffect=effect(tool['synthetic']),realPhotoPixelEffect=effect(tool['realPhotoProxy']),
                         exactUndoRedo=any(case.get('undoRedoExact') for case in cases),
                         staleWriteRejected=any(case.get('staleRejected') for case in cases),
                         evidence=tool['evidence'],evotoPerfection='unverified; no numeric perfection score'))
    table_file(out.with_name(out.name+'-tools.csv'),rows)
    table_file(out.with_name(out.name+'-native-tests.csv'),native['records'])
    shell=(ROOT/'ui/src-tauri/src/main.rs').read_text(encoding='utf-8')
    defined=set(re.findall(r'#\[tauri::command\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)',shell))
    assert len(defined)==347
    invocations=Counter()
    for suite in [*suites.values(),photos]:
        invocations.update(suite.get('commandCounts',{}))
    inventory=[dict(command=name,desktopInvocations=invocations[name],
                    status='observed in instrumented native calls' if invocations[name] else 'not observed in instrumented calls; GUI/native test coverage may exist',
                    staticRegistration='passed',evotoParity='unverified') for name in sorted(defined)]
    table_file(out.with_name(out.name+'-commands.csv'),inventory)
    attempts={name:dict(exitCode=command['exitCode'],seconds=command['seconds'],command=command['command'])
              for name,command in commands.items()}
    result=dict(date='2026-10-22',timezone='Asia/Katmandu',executableSha256=executable,
                sourceFiles=len(manifest),native={key:value for key,value in native.items() if key!='records'},
                frontend=frontend,tools=tools,groupCounts=dict(counts),commandInventory=inventory,
                commandAttempts=attempts,photos=photos,gui=gui,
                exactEvotoParity=False,perToolPerfection='unverified',
                qualificationProvenance=read(checks/'qualification-provenance.json'),
                fixes=[dict(tool='Levels, selective colour and local colour',
                            problem='An identical save appended an Undo entry despite unchanged canonical settings.',
                            resolution='Validate first, compare decoded controls and canonical recipe hashes before merging.',
                            regression='Repeated and subprecision saves preserve recipe hash, history and reversible pixels.'),
                       dict(tool='Recipe sidecar export',
                            problem='Requested sidecars were omitted; bounded status reasons could also lose their count.',
                            resolution='Carry the exact rendered recipe through output processing and retain delivery facts ahead of optional reasons.',
                            regression='Parallel resized exports and real application exports preserve matching recipes and accurate counts.')],
                limits=['Functional tests do not establish visual perfection or exact Evoto equivalence.',
                        '300 lab editing methods are tested individually on a synthetic fixture and one supplied RAW-derived proxy.',
                        'Every individual method is not rendered at full RAW resolution on every original.',
                        'All 27 saved RAW edits are re-rendered and fully exported by the final executable.',
                        'Cloud providers, external accounts, physical tethering and hardware-dependent paths are not all exercised.',
                        'Ignored native tests remain ignored; see the native case CSV for reasons.',
                        'The executable is an unoptimized Windows debug build; concurrent validation timings are not release performance benchmarks.',
                        'Some global spatial methods use reduced working grids; diagnostic measurements use a 512-pixel preview.',
                        'Several WebView sessions closed unexpectedly; failed attempts are retained. Root cause is unconfirmed.'])
    out.with_suffix('.json').write_text(json.dumps(publication_copy(result),indent=2)+'\n',encoding='utf-8')
    relative_photos=Path('../../output')/args.photos.name/'index.html'
    lines=['# AURA comprehensive desktop and photo test - 2026-10-22','',
           '**AURA was run and tested; exact Evoto parity and per-tool perfection remain unverified.**','',
           '| Check | Fresh result |','|---|---|',
           f'| Full native workspace, including doc tests | {native["passed"]} passed; {native["failed"]} failed; {native["ignored"]} ignored |',
           f'| Frontend | {frontend["tests"]} tests in {frontend["files"]} files passed; production build passed |',
           '| Tracked capability suites | All 400 ledger entries passed in the final desktop |',
           '| Lab editing methods | All 300 exercised individually on synthetic pixels and a supplied RAW-derived photo proxy |',
           f'| Older tools and controls | {len(suites["core-final"]["cases"])} core cases passed; overlap with diagnostics is deduplicated below |',
           f'| Visible workflow | {len(gui["steps"])} GUI steps passed, including import, skin cleanup, batch editing, manual edits, Undo/Redo and verified export |',
           f'| Supplied CR3 photos | {len(photos["photos"])}/{len(photos["inputManifest"])} saved edits reopened with identical rendered pixels; original SHA-256 values unchanged |',
           f'| Final full-resolution JPEG export | {photos["export"]["written"]} written; {photos["export"]["verified"]} verified; {photos["export"]["renderFailed"]} render failures |',
           f'| AURA recipe sidecars | {photos["export"]["sidecars"]} written; each exactly matches its saved edit |',
           '| Other export formats | JPEG, lossless PNG renderer equality, and 16-bit TIFF passed on a real-photo proxy |',
           '| Static IPC surface | 347 definitions, registrations and typed calls agree |',
           f'| Direct desktop command coverage | {sum(bool(invocations[name]) for name in defined)}/347 command names observed; unexercised commands remain explicit |',
           '| Checks | Scoped strict Clippy, formatting, contracts, banned patterns and Python tooling passed |','',
           f'[Before/after gallery and full-resolution exports]({relative_photos.as_posix()}) | '
           f'[Per-tool CSV]({out.name}-tools.csv) | [Per-command coverage]({out.name}-commands.csv) | '
           f'[Native test cases]({out.name}-native-tests.csv) | [Machine-readable evidence]({out.name}.json)','',
           '## Repairs','',
           'Repeatedly applying unchanged Levels, selective colour or local colour controls created redundant Undo entries. '
           'The command now validates the request, compares decoded settings and checks canonical recipe precision before writing. '
           'The regression checks identical and subprecision settings, stale writes, exact Undo/Redo and original-file safety. '
           'All 13 studio-colour desktop cases were rerun in the rebuilt application.','',
           'Requested export sidecars were silently omitted because the export source did not provide recipe data. '
           'Each rendered image now carries its exact canonical recipe through resizing and sharpening. '
           'Delivery facts are retained ahead of optional processing messages so the sidecar count remains accurate. '
           'Parallel exporter tests, the real app workflow and all 27 final photo sidecars verify the repair. '
           'These files retain AURA canonical recipe JSON with the existing .xmp extension; Adobe/Evoto XMP compatibility is unverified.','',
           '## Coverage and quality limits','',
           'The photo edits use AURA automatic enhancement plus the explicit gentle perceptual-lightness layer recorded in each recipe. '
           'Creative effects were tested individually and undone. Their successful invocation does not mean each is suitable for every portrait. '
           'The final executable reopened a backup of the qualified catalogue, reproduced all saved 1600-pixel preview bytes, '
           'ran 95 diagnostics per photo without modifying history, and produced a fresh complete export.','',
           'A functional pass means the exercised operation completed and its recorded invariants held. '
           'Visible effects, history checks and read-only diagnostics are separate columns. '
           'No percentage is assigned to "perfection" because there is no validated ground truth for that score. '
           'The complete parameter space, every camera format, physical tethering, live cloud services and all 347 commands '
           'were not individually exercised. Native ignored tests are listed with their reasons.','',
           'The supplied-photo proxy contains no sky and the sky selection correctly returned an explanatory refusal. '
           'Facial-hair selection coverage was empty on that portrait; a visible beard-selection effect was not demonstrated. '
           'Some global spatial methods retain reduced working grids, and diagnostics use a 512-pixel preview. '
           'The tested executable is an unoptimized Windows debug build; concurrent run times are not release benchmarks.','',
           'Several earlier desktop sessions ended with TargetClosedError, including a concurrent export and method suite. Their logs and incomplete results remain retained; '
           'the cause has not been established. Fresh final scenarios are reported separately. '
           'An initial formatting failure was corrected. A queued duplicate regression command was stopped before execution; '
           'it is not counted as a product failure or a test pass. '
           'The initial native workspace run reproduced the history defect and was stopped after the corrected target passed. '
           'The complete corrected-source run supersedes that partial earlier attempt; only the complete final run supplies the native totals above.','',
           'Report review also corrected method matching to use both group and ID, and retained method names containing digits. '
           'The final source manifest includes both product repairs and all qualification scripts. '
           'Earlier attempts are retained separately; reused checks are identified in qualification provenance. '
           'Final retry scenarios use one desktop session at a time.','',
           '## Evoto comparison','',
           'Evoto documents semantic skin retouching, local and batch colour matching, and a broader 8.0 workflow. '
           'These documents establish the reference feature categories; they do not specify the numerical algorithms or establish output equivalence. '
           '[Skin retouching](https://support.evoto.ai/skin-retouching/), '
           '[AI Color Match](https://support.evoto.ai/feature-introduction-ai-color-match/), '
           '[Colour and tone](https://www.evoto.ai/features/color-and-tone), '
           '[Evoto 8.0](https://www.evoto.ai/launch26).','',
           'AURA retains unresolved differences in trained semantic corrections, model-dependent generative tools, '
           'RAW/colour interpretation and subject-specific retouching. No paired Evoto export with identical settings and '
           'colour management was produced for this run. Exact matching and every Evoto feature remain unfinished. '
           'Existing historical reference investigations remain linked from the earlier reports and are not presented as fresh measurements.','',
           '## Tool-by-tool results','',
           '| Group | Qualified rows |','|---|---|']
    lines.extend(f'| {group} | {count} |' for group,count in sorted(counts.items()))
    lines += ['', 'These rows include individual controls and validation workflows, and are not a count of independent algorithms.','',
              '| Group | Tool/control | Route | Synthetic effect | Real-photo proxy effect | Evoto perfection |','|---|---|---|---|---|---|']
    lines.extend(f'| {row["group"]} | {row["name"]} | {row["route"]} | {row["syntheticPixelEffect"]} | '
                 f'{row["realPhotoPixelEffect"]} | unverified |' for row in rows)
    lines += ['',f'Executable SHA-256: `{executable}`. {len(manifest)} source-file hashes matched the qualification manifest.','']
    out.with_suffix('.md').write_text('\n'.join(lines),encoding='utf-8')
    print(json.dumps(dict(report=str(out.with_suffix('.md')),rows=len(rows),native=result['native'],frontend=frontend,photos=len(photos['photos']))))


if __name__=='__main__':
    main()
