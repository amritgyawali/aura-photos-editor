"""Select a target in one owned AURA Windows picker; never touch another app's dialog."""
import argparse
import ctypes
import time
from ctypes import wintypes
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('reference', type=Path)
parser.add_argument('--pid', type=int, required=True)
args = parser.parse_args()
target = args.reference.resolve(strict=True)
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
        if name.value == '#32770' and title.value == 'Choose the finished version of this same photo':
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
text = ctypes.create_unicode_buffer(str(target))
if not api.SendMessageW(fields[0], 0xC, 0, ctypes.cast(text, ctypes.c_void_p).value):
    raise SystemExit('Cannot fill the owned filename control.')
readback = ctypes.create_unicode_buffer(32768)
api.SendMessageW(fields[0], 0xD, 32768, ctypes.cast(readback, ctypes.c_void_p).value)
if readback.value != str(target):
    raise SystemExit('Native filename readback differs; no Open was requested.')
if not api.PostMessageW(buttons[0], 0xF5, 0, 0):
    raise SystemExit('Cannot activate the owned Open control.')
print('Selected the paired target through the owned native Windows dialog.', flush=True)
