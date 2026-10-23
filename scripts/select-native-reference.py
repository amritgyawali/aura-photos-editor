"""Select a target in one owned AURA Windows picker; never touch another app's dialog."""
import argparse
import ctypes
import time
from ctypes import wintypes
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('reference', type=Path, nargs='+')
parser.add_argument('--pid', type=int, required=True)
parser.add_argument('--title', default='Choose the finished version of this same photo')
parser.add_argument('--cancel', action='store_true')
args = parser.parse_args()
targets = [path.resolve(strict=True) for path in args.reference]
filename = str(targets[0]) if len(targets) == 1 else ' '.join('"' + str(path) + '"' for path in targets)
api = ctypes.WinDLL('user32', use_last_error=True)
api.SendMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
api.SendMessageW.restype = wintypes.LPARAM
api.PostMessageW.argtypes = [wintypes.HWND, wintypes.UINT, wintypes.WPARAM, wintypes.LPARAM]
callback = ctypes.WINFUNCTYPE(wintypes.BOOL, wintypes.HWND, wintypes.LPARAM)
dialogs = []

@callback
def find_dialog(handle, _):
    owner = wintypes.DWORD()
    api.GetWindowThreadProcessId(handle, ctypes.byref(owner))
    if owner.value == args.pid:
        title = ctypes.create_unicode_buffer(256)
        name = ctypes.create_unicode_buffer(128)
        api.GetWindowTextW(handle, title, 256)
        api.GetClassNameW(handle, name, 128)
        if name.value == '#32770' and title.value == args.title:
            dialogs.append(handle)
    return True

deadline = time.monotonic() + 30
while not dialogs:
    api.EnumWindows(find_dialog, 0)
    if time.monotonic() >= deadline:
        raise SystemExit('The owned AURA reference picker did not appear.')
    time.sleep(.1)
if len(dialogs) != 1:
    raise SystemExit('More than one owned reference picker exists; preserve them.')
if args.cancel:
    if not api.PostMessageW(dialogs[0], 0x10, 0, 0):
        raise SystemExit('Cannot cancel the owned reference dialog.')
    print('Cancelled the owned native Windows reference dialog.', flush=True)
    raise SystemExit(0)
fields, buttons = [], []

@callback
def find_controls(handle, _):
    name = ctypes.create_unicode_buffer(128)
    api.GetClassNameW(handle, name, 128)
    control = api.GetDlgCtrlID(handle)
    if name.value == 'Edit' and control == 1148:
        fields.append(handle)
    if name.value == 'Button' and control == 1:
        buttons.append(handle)
    return True

api.EnumChildWindows(dialogs[0], find_controls, 0)
if len(fields) != 1 or len(buttons) != 1:
    raise SystemExit('Expected native filename and Open controls are missing.')
# Character input sends the edit notifications consumed by modern Windows file dialogs.
# WM_SETTEXT alone can change the visible text while Open still chooses the previous file.
api.SendMessageW(fields[0], 0xB1, 0, -1)  # EM_SETSEL: replace the whole filename.
encoded = filename.encode('utf-16-le')
for offset in range(0, len(encoded), 2):
    api.SendMessageW(fields[0], 0x102, int.from_bytes(encoded[offset:offset + 2], 'little'), 1)
readback = ctypes.create_unicode_buffer(32768)
api.SendMessageW(fields[0], 0xD, 32768, ctypes.cast(readback, ctypes.c_void_p).value)
if readback.value != filename:
    raise SystemExit('Native filename readback differs; no Open was requested.')
if not api.PostMessageW(fields[0], 0x100, 0x0D, 0x001C0001):
    raise SystemExit('Cannot confirm the owned filename control.')
print('Entered and confirmed the reference through the owned native Windows dialog.', flush=True)
