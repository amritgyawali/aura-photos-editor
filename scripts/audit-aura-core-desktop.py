"""Qualify older native tools against a real-photo proxy in an isolated desktop.

Pixel changes are observations, never a score for quality or Evoto equivalence.
"""
import argparse
import base64
import copy
import hashlib
import json
import math
import os
from pathlib import Path
import re
import subprocess
import time
import traceback
import urllib.request
import uuid

from PIL import Image
from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def tool_names(filename):
    source = (ROOT / filename).read_text(encoding='utf-8')
    body = source.split('pub enum Tool {', 1)[1].split('\n}', 1)[0]
    return [re.sub(r'(?<!^)(?=[A-Z])', '_', name).lower()
            for name in re.findall(r'^    ([A-Z][A-Za-z]+),', body, re.M)]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--photo', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--port', type=int, default=9610)
    args = parser.parse_args()
    out = args.output.resolve()
    if out.exists():
        parser.error('Use a fresh output directory')
    out.mkdir(parents=True)
    fixture = out / 'real-photo-proxy.png'
    with Image.open(args.photo) as source:
        proxy = source.convert('RGB')
        proxy.thumbnail((640, 640))
        proxy.save(fixture)
    report = dict(cases=[], commandCounts={}, pageErrors=[], source=str(args.photo.resolve()),
                  sourceSha256=digest(args.photo), fixtureSha256=digest(fixture),
                  executableSha256=digest(ROOT / 'target/desktop-launch/debug/aura-desktop.exe'))

    def persist():
        (out / 'results.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

    startup = subprocess.STARTUPINFO()
    startup.dwFlags |= subprocess.STARTF_USESHOWWINDOW
    startup.wShowWindow = 0
    env = dict(os.environ, AURA_TEST_CATALOG=str(out / 'catalog/catalog.sqlite'),
               WEBVIEW2_USER_DATA_FOLDER=str(out / 'webview'),
               WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=f'--remote-debugging-port={args.port}')
    (out / 'catalog').mkdir()
    proc = subprocess.Popen([str(ROOT / 'target/desktop-launch/debug/aura-desktop.exe')],
                            cwd=ROOT, env=env, startupinfo=startup)
    try:
        for _ in range(120):
            try:
                urllib.request.urlopen(f'http://127.0.0.1:{args.port}/json/version', timeout=1)
                break
            except OSError:
                time.sleep(1)
        else:
            raise TimeoutError('Desktop did not start')
        with sync_playwright() as pw:
            page = pw.chromium.connect_over_cdp(f'http://127.0.0.1:{args.port}').contexts[0].pages[0]
            page.set_default_timeout(180000)
            page.wait_for_function('() => !!window.__TAURI_INTERNALS__')
            page.on('pageerror', lambda error: report['pageErrors'].append(str(error)))

            def call(command, **values):
                report['commandCounts'][command] = report['commandCounts'].get(command, 0) + 1
                return page.evaluate('''async ([c,input]) => {
                    try { return await window.__TAURI_INTERNALS__.invoke(c,{input}); }
                    catch(e) { throw Error(JSON.stringify(e)); }
                }''', [command, values])

            project = call('create_project', name='Comprehensive core tool qualification',
                           coupleNames=None, eventDate=None)['id']
            call('start_ingest', projectId=project, roots=[str(fixture)])
            for _ in range(120):
                photos = call('list_images', projectId=project, offset=0, limit=10, orderBy=None)
                if photos:
                    break
                time.sleep(.5)
            assert len(photos) == 1 and photos[0]['fileName'] == fixture.name
            photo = photos[0]['id']
            report.update(projectId=project, photoId=photo)
            sources = [dict(type='radial',centre=[.5,.5],radii=[.35,.35],angle=0,feather=.2),
                       dict(type='linear',start=[.2,.2],end=[.8,.8]),
                       dict(type='brush',strokes=[dict(erase=False,opacity=1,radius=.04,
                            points=[[.3,.3,1],[.4,.4,1]])],feather=.2),
                       dict(type='luminance',range=dict(low=.1,high=.8,softness=.1))]
            masks = [dict(id='core', name='Explicit test selection', enabled=True, amount=1,
                          components=[dict(mode='add',source=value) for value in sources],
                          params=dict(exposure=.15)),
                     dict(id='other',name='Combination target',enabled=True,amount=.8,
                          components=[dict(mode='add',source=dict(type='radial',centre=[.3,.5],
                                          radii=[.3,.3],angle=0,feather=.2))],params=dict(exposure=-.2))]
            call('save_local_masks', projectId=project, photoId=photo, masks=masks,
                 label='Core qualification baseline')

            def recipe():
                return call('image_recipe', photoId=photo)

            def render():
                frame = call('render_image', photoId=photo, level='full', purpose='export')
                return (frame['width'], frame['height'], base64.b64decode(frame['rgbBase64']))

            def history(action):
                return call('history_step', projectId=project, photoId=photo, action=action)

            baseline = recipe()
            original = render()

            def run(name, group, action):
                started = time.monotonic()
                row = dict(name=name, group=group)
                try:
                    details = action() or {}
                    edited = render()
                    saved = recipe()
                    changed = saved['recipeHash'] != baseline['recipeHash']
                    if changed:
                        history('undo')
                        assert recipe()['recipeHash'] == baseline['recipeHash'] and render() == original
                        history('redo')
                        assert recipe()['recipeHash'] == saved['recipeHash'] and render() == edited
                        history('undo')
                    else:
                        assert edited == original
                    row.update(passed=True, pixelsChanged=edited != original, recipeChanged=changed,
                               undoRedoExact=True if changed else None,
                               editedPixelsSha256=hashlib.sha256(edited[2]).hexdigest(), **details)
                except Exception as error:
                    row.update(passed=False, error=str(error), traceback=traceback.format_exc())
                    # Only resume after recovering the exact baseline; unrelated failures stay visible.
                    for _ in range(4):
                        if recipe()['recipeHash'] == baseline['recipeHash']:
                            break
                        history('undo')
                    assert recipe()['recipeHash'] == baseline['recipeHash'] and render() == original
                row['seconds'] = round(time.monotonic() - started, 3)
                report['cases'].append(row)
                persist()
                print('PASS' if row['passed'] else 'FAIL', group, name, row.get('error', ''), flush=True)

            selection_actions = ['duplicate','invert','union','intersection','subtraction','freeze',
                                 'feather','expand','contract','edge_contrast','remove_islands','fill_holes',
                                 'edit_linear','edit_radial','edit_brush','edit_luminance','component_mode',
                                 'reorder_component','extract_component','copy_component','transform_selection',
                                 'border_selection','keep_largest','keep_connected','median_cleanup',
                                 'exclusive_union','convex_hull']
            for action in selection_actions:
                values = dict(action=action,otherMaskId='other',value=0,componentIndex=0,
                              targetIndex=1,point=[.5,.5])
                if action in ['feather','expand','contract','remove_islands','fill_holes','border_selection','median_cleanup']:
                    values['value'] = 2
                if action == 'edge_contrast':
                    values['value'] = 40
                if action in ['edit_linear','edit_radial','edit_brush','edit_luminance']:
                    index = {'edit_radial':0,'edit_linear':1,'edit_brush':2,'edit_luminance':3}[action]
                    replacement = copy.deepcopy(sources[index])
                    if index == 0:
                        replacement['centre'] = [.6,.5]
                    elif index == 1:
                        replacement['end'] = [.7,.6]
                    elif index == 2:
                        replacement['strokes'][0]['radius'] = .08
                    else:
                        replacement['range']['low'] = .3
                    values.update(componentIndex=index,source=replacement)
                if action == 'component_mode':
                    values.update(componentIndex=1,mode='subtract')
                if action == 'transform_selection':
                    values['transform'] = dict(offset=[.05,-.03],scale=[.9,1.1],angle=10)

                def selection(values=values):
                    call('mask_workflow',projectId=project,photoId=photo,maskId='core',
                         recipeHash=baseline['recipeHash'],cancelId=str(uuid.uuid4()),**values)
                    return dict(input=values)
                run(action,'selection-management',selection)

            for what in ['subject','background','sky','face_skin','body_skin','hair','clothes',
                         'eyes','iris','sclera','eyebrows','lips','teeth','facial_hair']:
                def semantic(what=what):
                    history_before=call('image_history',photoId=photo)
                    result=call('create_local_mask',projectId=project,photoId=photo,what=what,
                                source=None,into=None,mode=None,invert=False,bounds=None)
                    if result['maskId'] is None:
                        # A content-dependent absence is different from a demonstrated selection.
                        message=result['message']
                        assert message
                        assert recipe()['recipeHash']==baseline['recipeHash']
                        assert call('image_history',photoId=photo)==history_before
                        return dict(selectionAvailable=False,refusal=message,unchangedOnRefusal=True)
                    coverage=call('local_mask_coverage',projectId=project,photoId=photo,maskId=result['maskId'])
                    pixels=base64.b64decode(coverage['rgbBase64'])
                    assert len(pixels)==coverage['width']*coverage['height']*3
                    return dict(selectionAvailable=True,coverageNonzero=any(pixels),coverageSha256=hashlib.sha256(pixels).hexdigest())
                run(what,'semantic-selection',semantic)

            for path,value in dict(exposure=.4,contrast=30,highlights=-30,shadows=30,whites=20,blacks=-20,
                                   temperature=500,tint=20,saturation=30,clarity=25,texture=25).items():
                def local_parameter(path=path,value=value):
                    selected=copy.deepcopy(masks)
                    selected[0]['params'][path]=value
                    call('save_local_masks',projectId=project,photoId=photo,masks=selected,label='Local '+path)
                    return dict(path=path,value=value)
                run(path,'local-parameter',local_parameter)

            for tool in tool_names('crates/aura-recipe/src/retouch_tools.rs'):
                operation = dict(id='core-tool', tool=tool, enabled=True, region=[.5,.5,.22,.22],
                                 source=[.35,.4], amount=.6, feather=.5, radius=.02,
                                 texture=.7, tone=.6, warmth=.25, tint=.1, targetColor=[.5,.25,.15])

                def retouch(op=operation):
                    call('native_retouch_edit', projectId=project, photoId=photo,
                         action='append', edits=[op], id=None)
                    return dict(operation=op)
                run(tool, 'retouch', retouch)

            def guarded_save(command, **values):
                call(command, projectId=project, photoId=photo, recipeHash=baseline['recipeHash'], **values)
                saved = recipe()
                history_before = call('image_history', photoId=photo)
                call(command, projectId=project, photoId=photo, recipeHash=saved['recipeHash'], **values)
                assert call('image_history', photoId=photo) == history_before, 'Repeated save appended history'
                rejected = False
                try:
                    call(command, projectId=project, photoId=photo, recipeHash='stale', **values)
                except Exception:
                    rejected = True
                assert rejected and recipe()['recipeHash'] == saved['recipeHash'], 'Stale save changed recipe or was accepted'
                return dict(staleRejected=True, noOpHistoryExact=True, input=values)

            for tool in tool_names('crates/aura-recipe/src/material_lab.rs'):
                operation = dict(id='material', tool=tool, maskId='core', enabled=True, amount=.7,
                                 radius=.025, angle=30, threshold=.08, detail=.7, coarse=.6)
                run(tool, 'material', lambda op=operation: guarded_save('save_material_lab', operations=[op]))

            neutral_channel = dict(inputBlack=0, inputWhite=1, gamma=1, outputBlack=0, outputWhite=1)
            neutral = dict(levels={**{key:dict(neutral_channel) for key in ['rgb','red','green','blue']},
                                   'luminosity':False}, selective=dict(absolute=False, ranges={}), local={})
            color_cases = {}
            for key in ['rgb','red','green','blue']:
                color = copy.deepcopy(neutral)
                color['levels'][key].update(inputBlack=.01, inputWhite=.93, gamma=1.1)
                color_cases['levels-'+key] = color
            color = copy.deepcopy(color_cases['levels-rgb'])
            color['levels']['luminosity'] = True
            color_cases['luminosity-levels'] = color
            for absolute in [False, True]:
                color = copy.deepcopy(neutral)
                color['selective'] = dict(absolute=absolute, ranges={key:dict(cyan=15,magenta=-10,yellow=5,black=5)
                    for key in ['reds','yellows','greens','cyans','blues','magentas','whites','neutrals','blacks']})
                color_cases['selective-absolute' if absolute else 'selective-relative'] = color
            local = dict(curve=dict(points=[[0,0],[255,255]]), hsl={},
                         grade={**{k:dict(hue=0,saturation=0,luminance=0)
                                  for k in ['shadows','midtones','highlights','global']},
                                'blending':50,'balance':0}, sharpen=0,dehaze=0,vibrance=0)
            for name in ['curve','hsl','grade','sharpen','dehaze','vibrance']:
                color = copy.deepcopy(neutral)
                selected = copy.deepcopy(local)
                if name == 'curve':
                    selected[name]['points'] = [[0,0],[128,150],[255,255]]
                elif name == 'hsl':
                    selected[name] = dict(orange=dict(h=10,s=20,l=5))
                elif name == 'grade':
                    selected[name]['midtones'] = dict(hue=35,saturation=20,luminance=5)
                else:
                    selected[name] = 40
                color['local']['core'] = selected
                color_cases['local-'+name] = color
            for name, color in color_cases.items():
                run(name, 'studio-colour', lambda value=color: guarded_save('save_studio_color', color=value))

            for kind in ['auto','black','white','hsl']:
                def measure(kind=kind):
                    result = call('measure_studio_color', projectId=project, photoId=photo,
                                  recipeHash=baseline['recipeHash'], kind=kind, channel='rgb', x=.5, y=.5)
                    assert result['recipeHash'] == baseline['recipeHash']
                    assert recipe()['recipeHash'] == baseline['recipeHash']
                    return dict(measurement=result)
                run(kind, 'colour-measurement', measure)

            params = {'global.exposure':.5,'global.temperature':6500,'global.tint':20,'global.contrast':25,
                      'global.highlights':-40,'global.shadows':40,'global.whites':20,'global.blacks':-20,
                      'global.texture':30,'global.clarity':30,'global.dehaze':30,'global.vibrance':35,
                      'global.saturation':-30,'global.hsl.red.h':30,'global.hsl.orange.s':-40,
                      'global.hsl.blue.l':30,'global.curve.points':[[0,0],[128,165],[255,255]],
                      'global.channel_curves.red.points':[[0,0],[128,153],[255,255]],
                      'global.parametric.lights':30,'global.sharpen.amount':70,'global.noise.luminance':50,
                      'global.noise.colour':50,'global.effects.vignette.amount':-45,
                      'global.effects.grain.amount':35,'global.calibration.red_hue':35,
                      'global.colour_grade.shadows.saturation':40,'bw':{'mix':{},'grade':None},
                      'geometry.rotate':10,'geometry.crop':[.1,.1,.8,.8],
                      'lens.distortion':True,'lens.ca':True,'lens.vignette':50}
            for path, value in params.items():
                run(path, 'parameter', lambda path=path,value=value: (
                    call('set_param', projectId=project, photoId=photo, path=path, value=value,
                         label='Core qualification ' + path), dict(path=path,value=value))[1])

            before_history = call('image_history', photoId=photo)
            diagnostics = call('inspect_photo_diagnostics', projectId=project, photoId=photo,
                               recipeHash=baseline['recipeHash'])
            assert len(diagnostics['diagnostics']) == 95
            for item in diagnostics['diagnostics']:
                assert len(item['values']) == item['width'] * item['height']
                assert all(math.isfinite(v) for v in item['values'])
                report['cases'].append(dict(name=item['id'],group='inspection',passed=True,
                                            readOnly=True, title=item['title'],samples=item['samples']))
            assert call('image_history', photoId=photo) == before_history and render() == original

            for fmt, depth in [('jpeg',8),('png',8),('tiff',16)]:
                def export(fmt=fmt,depth=depth):
                    destination = out / ('export-' + fmt)
                    result = call('export_run',projectId=project,destination=str(destination),destinationKind='folder',
                                  copyright=None,contact=None,creator=None,keywords=[],stripGps=True,
                                  stripCameraSerial=True,verify=True,sets=[dict(name='core',imageIds=[photo],
                                  format=fmt,quality=95,colour='srgb',bitDepth=depth,resize='full',
                                  sharpen='none',naming='{original}',sidecar=True)])
                    assert result['requested'] == result['written'] == result['verified'] == 1
                    assert result['sidecars'] == 1, 'Requested sidecar was not written'
                    assert result['manifestSealed'] and all(result[k] == 0 for k in ['unverified','corrupt','renderFailed'])
                    paths = [p for p in destination.rglob('*') if p.suffix.lower() in ['.jpg','.png','.tif','.tiff']]
                    assert len(paths) == 1
                    sidecar=paths[0].with_suffix('.xmp')
                    assert sidecar.read_text(encoding='utf-8') == baseline['body'], 'Sidecar differs from rendered recipe'
                    with Image.open(paths[0]) as image:
                        image.load()
                        assert image.size == original[:2]
                        if fmt == 'png':
                            assert image.convert('RGB').tobytes() == original[2]
                        if fmt == 'tiff':
                            assert set(image.tag_v2[258]) == {16}
                    return dict(export=result, file=str(paths[0]), sha256=digest(paths[0]),
                                sidecar=str(sidecar),sidecarSha256=digest(sidecar),sidecarMatchesRecipe=True)
                run(fmt, 'export', export)

            references = [call('import_color_reference',path=str(path.resolve()),cancelId=str(uuid.uuid4()))
                          for path in [fixture,args.photo]]
            reference_ids = [item['id'] for item in references]
            assert len(set(reference_ids)) == 2

            def match_reference():
                prepared = call('prepare_color_match',photoId=photo,referenceId=reference_ids[1],cancelId=str(uuid.uuid4()))
                result = call('apply_color_match',photoId=photo,sessionId=prepared['sessionId'],
                              controls=dict(amount=.25,tone=.4,color=.4,whiteBalance=.2),cancelId=str(uuid.uuid4()))
                return dict(match=result,evotoParity='unverified')
            run('prepare-and-apply-reference','reference-matching',match_reference)

            snapshot = call('color_reference_library_workflow',action='read')

            def library(action, **values):
                nonlocal snapshot
                snapshot = call('color_reference_library_workflow',action=action,
                                expectedRevision=snapshot['revision'],**values)
                return dict(revision=snapshot['revision'],references=len(snapshot['references']),groups=snapshot['groups'])

            for action, values in [
                ('create_group',dict(group='Qualification A')),
                ('create_group',dict(group='Qualification B')),
                ('rename_group',dict(group='Qualification A',name='Renamed')),
                ('reorder_groups',dict(order=['Qualification B','Renamed'])),
                ('move',dict(referenceIds=reference_ids,group='Renamed')),
                ('reorder_references',dict(group='Renamed',order=list(reversed(reference_ids)))),
                ('favorite',dict(referenceIds=reference_ids,favorite=True)),
                ('defaults',dict(referenceIds=reference_ids,controls=dict(amount=.5,tone=.6,color=.6,whiteBalance=.4))),
                ('reset_defaults',dict(referenceIds=reference_ids)),
                ('export',dict(referenceIds=reference_ids)),
            ]:
                run(action+('-'+values['group'] if action=='create_group' else ''),'reference-library',
                    lambda action=action,values=values: library(action,**values))
            document = snapshot['document']
            assert document and len(json.loads(document)['references']) == 2
            run('import','reference-library',lambda: library('import',document=document))
            run('delete_group','reference-library',lambda: library('delete_group',group='Renamed'))
            assert all(item['group'] == '' for item in snapshot['references'])

            def stale_library():
                rejected = False
                try:
                    call('color_reference_library_workflow',action='favorite',expectedRevision='stale',
                         referenceIds=reference_ids,favorite=False)
                except Exception:
                    rejected = True
                assert rejected
                assert call('color_reference_library_workflow',action='read')['revision'] == snapshot['revision']
                return dict(staleRejected=True)
            run('stale-revision','reference-library',stale_library)
            run('remove','reference-library',lambda: library('remove',referenceIds=reference_ids))
            assert not snapshot['references']

            report['originalsUnchanged'] = digest(args.photo) == report['sourceSha256'] and digest(fixture) == report['fixtureSha256']
            report['completed'] = all(row['passed'] for row in report['cases']) and report['originalsUnchanged'] and not report['pageErrors']
            page.screenshot(path=str(out / 'desktop.png'), full_page=True)
            persist()
            if not report['completed']:
                raise RuntimeError('Core qualification contains failed cases')
    except Exception as error:
        report['fatalError'] = str(error)
        raise
    finally:
        persist()
        proc.terminate()
        proc.wait(timeout=30)


if __name__ == '__main__':
    main()
