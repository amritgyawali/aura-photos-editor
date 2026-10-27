"""Independently validate AURA's self-contained Adobe XMP/RGB-table preset.

Checks XML namespaces, preset/profile identities, Adobe base85, bounded zlib,
MD5 table identity, RGB16 delta reconstruction, colour space and amount range.
This is file-format validation; it does not certify Lightroom rendering parity.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import re
import struct
import xml.etree.ElementTree as ET
import zlib

import numpy as np

CRS = 'http://ns.adobe.com/camera-raw-settings/1.0/'
RDF = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#'
NS = {'c': CRS, 'r': RDF}
ALPHABET = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ.-:+=^!/*?`'|()[]{}@%$#"


def decode85(text):
    result = bytearray()
    for start in range(0, len(text), 5):
        group = text[start:start + 5]
        assert 2 <= len(group) <= 5, 'Invalid Adobe base85 tail'
        number = sum(ALPHABET.index(char) * 85 ** power for power, char in enumerate(group))
        assert number < 1 << (8 * (len(group) - 1)), 'Noncanonical Adobe base85 group'
        result.extend(number.to_bytes(len(group) - 1, 'little'))
    return bytes(result)


def read_preset(path):
    content = Path(path).read_bytes()
    assert len(content) <= 2 * 1024 * 1024, 'Unexpectedly large preset'
    root = ET.fromstring(content)
    description = root.find('r:RDF/r:Description', NS)
    assert description is not None
    attributes = description.attrib
    assert attributes.get('{' + CRS + '}PresetType') == 'Normal'
    title = description.find('c:Name/r:Alt/r:li', NS)
    assert title is not None and title.text and title.text.strip()
    uuid = attributes.get('{' + CRS + '}UUID', '')
    assert re.fullmatch('[0-9A-F]{32}', uuid)
    look = description.find('c:Look/r:Description', NS)
    assert look is not None and look.get('{' + CRS + '}UUID') != uuid
    assert look.get('{' + CRS + '}Amount') == '1'
    parameters = look.find('c:Parameters/r:Description', NS)
    assert parameters is not None, 'A profile stub cannot carry a standalone look'
    fingerprint = parameters.get('{' + CRS + '}RGBTable', '')
    assert re.fullmatch('[0-9A-F]{32}', fingerprint)
    encoded = attributes.get('{' + CRS + '}Table_' + fingerprint)
    assert encoded, 'Missing embedded table; an external profile is not sufficient'
    packet = decode85(encoded)
    declared = int.from_bytes(packet[:4], 'little')
    assert 44 <= declared <= 32 ** 3 * 6 + 44, 'Unsafe or unsupported table size'
    inflater = zlib.decompressobj()
    payload = inflater.decompress(packet[4:], declared + 1)
    assert len(payload) == declared and inflater.eof and not inflater.unused_data and not inflater.unconsumed_tail
    assert hashlib.md5(payload).hexdigest().upper() == fingerprint, 'Adobe table fingerprint mismatch'
    kind, version, dimensions, edge = struct.unpack_from('<4I', payload)
    assert (kind, version, dimensions) == (1, 1, 3) and 2 <= edge <= 32
    tail = 16 + edge ** 3 * 6
    assert len(payload) in (tail + 28, tail + 32)
    primaries, gamma, gamut, low, high = struct.unpack_from('<3Idd', payload, tail)
    assert (primaries, gamma, gamut) == (0, 1, 0), 'Expected encoded sRGB table'
    assert math.isfinite(low) and math.isfinite(high) and 0 <= low <= 1 <= high
    delta = np.frombuffer(payload, '<u2', count=edge ** 3 * 3, offset=16).reshape(edge, edge, edge, 3).astype(np.uint32)
    coordinates = np.moveaxis(np.indices((edge, edge, edge), dtype=np.uint32), 0, -1)
    identity = (coordinates * 65535 + edge // 2) // (edge - 1)
    grid = ((delta + identity) & 65535).astype(np.float32) / 65535
    assert not any(key.rsplit('}', 1)[-1].startswith(('Crop', 'HasCrop')) for key in attributes)
    assert 'aura:Sidecar' not in content.decode('utf-8')
    report = dict(path=str(Path(path).resolve()), sha256=hashlib.sha256(content).hexdigest(), name=title.text,
                  uuid=uuid, tableFingerprint=fingerprint, tableEdge=edge, tableBytes=len(payload),
                  selfContained=True, xmlValid=True, fingerprintValid=True, colourSpace='encoded sRGB',
                  meaning='Protocol validation only; Lightroom rendering accuracy must be assessed separately.')
    return report, grid


def apply_table(grid, image):
    """Apply decoded tetrahedral RGB-table interpolation, independently of Rust."""
    edge = grid.shape[0]
    source = np.asarray(image, dtype=np.float32) / 255
    scaled = source.clip(0, 1) * (edge - 1)
    base = np.minimum(scaled.astype(np.int32), edge - 2)
    fraction = scaled - base
    order = np.argsort(-fraction, axis=-1)
    position = base.copy()
    previous = grid[position[..., 0], position[..., 1], position[..., 2]]
    output = previous.copy()
    for step in range(3):
        axis = order[..., step:step + 1]
        np.put_along_axis(position, axis, np.take_along_axis(position, axis, axis=-1) + 1, axis=-1)
        value = grid[position[..., 0], position[..., 1], position[..., 2]]
        output += (value - previous) * np.take_along_axis(fraction, axis, axis=-1)
        previous = value
    return np.rint(output.clip(0, 1) * 255).astype(np.uint8)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('files', type=Path, nargs='+')
    parser.add_argument('--report', type=Path)
    args = parser.parse_args()
    reports = [read_preset(path)[0] for path in args.files]
    result = dict(presets=reports, allValid=True)
    if args.report:
        args.report.write_text(json.dumps(result, indent=2), encoding='utf-8')
    print(json.dumps(result, indent=2), flush=True)


if __name__ == '__main__':
    main()
