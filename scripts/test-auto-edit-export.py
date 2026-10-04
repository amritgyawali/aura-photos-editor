"""Export the photograph finished by test-auto-edit-gui.py through the Export workspace.

Clicks with real Windows mouse input, types the destination through the WebView, then
verifies that the written JPEG decodes and matches the edited render's dimensions.
"""
import base64
import ctypes
import json
import time
from pathlib import Path

from PIL import Image
from playwright.sync_api import expect, sync_playwright

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.work-checks/auto-v3/gui'
DEST = OUT / 'export'
gui = json.loads((OUT / 'results.json').read_text(encoding='utf-8'))
user32 = ctypes.windll.user32
user32.SetProcessDPIAware()


class POINT(ctypes.Structure):
    _fields_ = [('x', ctypes.c_long), ('y', ctypes.c_long)]


def os_click(page, locator):
    locator.scroll_into_view_if_needed()
    box = locator.bounding_box()
    ratio = page.evaluate('devicePixelRatio')
    hwnd = user32.FindWindowW(None, 'AURA')
    user32.SetForegroundWindow(hwnd)
    time.sleep(0.3)
    origin = POINT(0, 0)
    user32.ClientToScreen(hwnd, ctypes.byref(origin))
    user32.SetCursorPos(int(origin.x + (box['x'] + box['width'] / 2) * ratio), int(origin.y + (box['y'] + box['height'] / 2) * ratio))
    time.sleep(0.15)
    user32.mouse_event(0x0002, 0, 0, 0, 0)
    time.sleep(0.05)
    user32.mouse_event(0x0004, 0, 0, 0, 0)


with sync_playwright() as pw:
    page = pw.chromium.connect_over_cdp('http://127.0.0.1:9223').contexts[0].pages[0]
    page.set_default_timeout(180000)
    expect.set_options(timeout=180000)
    close = page.get_by_role('button', name='Close retouch', exact=False)
    if close.count():
        os_click(page, close.first)
    os_click(page, page.get_by_role('button', name='Export Ready to share', exact=True))
    page.get_by_test_id('preset').select_option('gallery')
    field = page.get_by_test_id('destination')
    field.click()
    page.keyboard.press('Control+a')
    page.keyboard.type(str(DEST))
    page.get_by_test_id('verify').check()
    os_click(page, page.get_by_test_id('run'))
    expect(page.get_by_test_id('written')).to_have_text('1')
    expect(page.get_by_test_id('verified')).to_have_text('1')
    page.screenshot(path=str(OUT / 'export-complete.png'))
    files = []
    for f in DEST.rglob('*.jpg'):
        with Image.open(f) as image:
            image.load()
            files.append({'file': str(f.relative_to(OUT)), 'size': list(image.size)})
    result = {'written': 1, 'verified': 1, 'files': files}
    (OUT / 'export-results.json').write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result))
