"""Run a public before image through native AURA and compare its paired reference.

Use an isolated catalogue/WebView. Marketing images are a reference, not a
lossless-source benchmark or a claim of complete feature/quality equivalence.
The source images and labelled comparison remain local, outside application assets.
"""
import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageStat


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def compare_reference(aura, reference):
    """Measure delivered RGB equality; no scaling, alignment or tolerance."""
    if aura.size != reference.size:
        raise ValueError('Reference comparison requires identical delivered dimensions')
    difference = ImageChops.difference(aura.convert('RGB'), reference.convert('RGB'))
    red, green, blue = difference.split()
    maximum = ImageChops.lighter(ImageChops.lighter(red, green), blue)
    matching = maximum.histogram()[0]
    total = aura.width * aura.height
    return {
        'basis': 'decoded RGB at delivered dimensions, without resizing or alignment',
        'exact_match': matching == total,
        'matching_pixels': matching,
        'total_pixels': total,
        'matching_pixel_fraction': matching / total,
        'mean_absolute_channel_error': ImageStat.Stat(difference).mean,
        'max_channel_error': maximum.getextrema()[1],
    }


def match_native_reference(port, collection, reference_path, output, native_pid=None):
    """Use the native command, or the studio and real Windows picker when a PID is supplied."""
    import uuid
    from playwright.sync_api import expect, sync_playwright
    with sync_playwright() as p:
        page = p.chromium.connect_over_cdp(f'http://127.0.0.1:{port}').contexts[0].pages[0]
        page.set_default_timeout(300000)
        expect.set_options(timeout=300000)
        errors = []
        page.on('pageerror', lambda e: errors.append(str(e)))
        def call(command, args=None):
            return page.evaluate('([c,a])=>window.__TAURI_INTERNALS__.invoke(c,a)', [command, args or {}])
        project = next(row for row in call('list_projects') if row['name'] == collection)
        [photo] = call('list_images', {'input': {'projectId': project['id'], 'offset': 0, 'limit': 10, 'orderBy': None}})
        baseline = call('image_recipe', {'input': {'photoId': photo['id']}})
        if native_pid is None:
            report = call('match_paired_reference', {'input': {'photoId': photo['id'],
                'referencePath': str(reference_path.resolve()), 'cancelId': str(uuid.uuid4())}})
        else:
            page.get_by_role('button', name=f'{collection} 1', exact=True).click()
            page.locator('.studio-nav').get_by_role('button', name='Auto edit One click, start to finish', exact=True).click()
            back = page.get_by_role('button', name='Back to Develop', exact=True)
            if back.count():
                back.click()
            button = page.get_by_role('button', name='Choose finished reference', exact=True)
            expect(button).to_be_enabled()
            assert button.count() == 1
            picker = subprocess.Popen([sys.executable, str(Path(__file__).with_name('select-native-reference.py')),
                                       str(reference_path.resolve()), '--pid', str(native_pid)],
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            try:
                button.click()
                stdout, stderr = picker.communicate(timeout=35)
                assert picker.returncode == 0, stderr
                print(stdout.strip(), flush=True)
                result = page.get_by_test_id('paired-reference-result')
                expect(result).to_be_visible()
                report = json.loads(result.get_attribute('data-reference-report'))
                expected_text = 'Pixels match exactly.' if report['exactMatch'] else 'Pixels still differ from the reference.'
                assert expected_text in result.inner_text()
                page.screenshot(path=str(output / 'reference-match-studio.png'))
            finally:
                if picker.poll() is None:
                    picker.terminate()
                    picker.wait(timeout=5)
        assert report['afterError'] <= report['beforeError']
        assert all(after <= before + 1e-6 for before, after in zip(report['subjectErrorsBefore'], report['subjectErrorsAfter']))
        after = call('image_recipe', {'input': {'photoId': photo['id']}})
        before_body, after_body = json.loads(baseline['body']), json.loads(after['body'])
        assert {k: v for k, v in before_body.items() if k not in ('global', 'provenance')} == {
            k: v for k, v in after_body.items() if k not in ('global', 'provenance')}
        assert before_body['provenance']['user_edited_fields'] == after_body['provenance']['user_edited_fields']
        assert not errors
        (output / 'matched-recipe.json').write_text(json.dumps(after, indent=2), encoding='utf8')
        return {'native_report': report, 'native_command_verified': True,
                'studio_button_verified': native_pid is not None,
                'native_file_picker_verified': native_pid is not None,
                'healing_masks_geometry_preserved': True, 'page_errors': errors}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('reference', type=Path)
    parser.add_argument('output', type=Path)
    parser.add_argument('--collection', required=True)
    parser.add_argument('--port', type=int, default=9363)
    parser.add_argument('--portrait-finish', action='store_true', help='compare the measured portrait finishing preset instead of skin-only repair')
    parser.add_argument('--resume', action='store_true', help='reuse an existing verification collection')
    parser.add_argument('--native-pid', type=int, help='exercise the studio with this owned Windows AURA process and its real picker')
    parser.add_argument('--match-reference', action='store_true', help='fit tone/color through the native paired reference studio action')
    args = parser.parse_args()
    before_hash, reference_hash = sha(args.before), sha(args.reference)
    before = Image.open(args.before).convert('RGB')
    reference = Image.open(args.reference).convert('RGB')
    if before.size != reference.size:
        raise SystemExit('The paired images must have identical dimensions; no resizing is applied.')
    args.output.mkdir(parents=True, exist_ok=True)
    source = args.output / 'reference-before.png'
    # Lossless format conversion only: the importer does not accept WebP.
    before.save(source)
    assert Image.open(source).convert('RGB').tobytes() == before.tobytes()
    scripts = Path(__file__).resolve().parent
    common = ['--port', str(args.port), '--collection', args.collection]
    native = args.output / 'native'
    full = args.output / 'native-full'
    subprocess.run([sys.executable, str(scripts / 'verify-professional-retouch.py'),
                    str(source), str(native), '--retouch-only', '--portrait-finish' if args.portrait_finish else '--detailed-skin', *common,
                    *(['--resume'] if args.resume else [])], check=True)
    match = None
    if args.match_reference:
        target = args.output / 'reference-target.png'
        reference.save(target)
        assert Image.open(target).convert('RGB').tobytes() == reference.tobytes()
        match = match_native_reference(args.port, args.collection, target, args.output, args.native_pid)
    subprocess.run([sys.executable, str(scripts / 'verify-saved-retouch-export.py'),
                    str(source), str(full), *common, *(['--under-eye-finishing'] if args.portrait_finish else [])], check=True)
    aura = Image.open(full / 'after-full.png').convert('RGB')
    assert aura.size == before.size
    assert sha(args.before) == before_hash and sha(args.reference) == reference_hash
    width, height = before.size
    comparison = Image.new('RGB', (3 * width, height + 40), '#17191c')
    draw = ImageDraw.Draw(comparison)
    for index, (label, frame) in enumerate([
        ('Public before image', before),
        ('AURA native portrait skin finish' if args.portrait_finish else 'AURA native detailed skin repair', aura),
        ('Published Evoto after image', reference),
    ]):
        draw.text((index * width + 12, 12), label, fill='white')
        comparison.paste(frame, (index * width, 40))
    comparison.save(args.output / 'comparison.png')
    recipe = json.loads((native / 'recipe.json').read_text(encoding='utf8'))
    body = json.loads(recipe['body'])
    faces = body.get('studio_portrait_auto_v1', {}).get('faces', [])
    for index, face in enumerate(faces):
        left, top, right, bottom = face['bounds']
        box = (int(left * width), int(top * height), int(right * width), int(bottom * height))
        crops = [frame.crop(box) for frame in (before, aura, reference)]
        crop_width, crop_height = crops[0].size
        detail = Image.new('RGB', (3 * crop_width, crop_height + 40), '#17191c')
        draw = ImageDraw.Draw(detail)
        for column, (label, crop) in enumerate(zip(('Original', 'AURA', 'Published Evoto'), crops)):
            draw.text((column * crop_width + 12, 12), label, fill='white')
            detail.paste(crop, (column * crop_width, 40))
        detail.save(args.output / f'face-{index + 1}-comparison.png')
    report = {
        'before_sha256': before_hash, 'reference_sha256': reference_hash,
        'dimensions': list(before.size), 'comparison_ready': True, 'detected_faces': len(faces),
        'preset': 'portrait_finish' if args.portrait_finish else 'detailed_skin',
        'paired_reference_match': match,
        'native_verification': json.loads((full / 'verification.json').read_text(encoding='utf8')),
        'reference_pixel_comparison': compare_reference(aura, reference),
        'face_reference_pixel_comparisons': [compare_reference(
            aura.crop((int(f['bounds'][0] * width), int(f['bounds'][1] * height),
                       int(f['bounds'][2] * width), int(f['bounds'][3] * height))),
            reference.crop((int(f['bounds'][0] * width), int(f['bounds'][1] * height),
                            int(f['bounds'][2] * width), int(f['bounds'][3] * height)))) for f in faces],
        'complete_removal_established': False,
        'full_feature_parity_established': False,
        'superior_quality_established': False,
        'limitations': [
            'Published marketing exports are not lossless camera originals or matched editing settings.',
            'Export integrity and protected regions do not measure complete blemish removal.',
            'Visual judgement and an independently annotated, varied dataset are still required.',
        ],
    }
    (args.output / 'comparison-verification.json').write_text(json.dumps(report, indent=2), encoding='utf8')
    print('Native paired comparison exported; quality claims require separate assessment.', flush=True)


if __name__ == '__main__':
    main()
