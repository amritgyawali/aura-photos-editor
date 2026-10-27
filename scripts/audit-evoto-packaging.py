#!/usr/bin/env python3
"""Read-only, bounded resource metadata audit; never exports decoded payloads.

The carrier masks are empirically verified for the inspected Evoto 8.0 build,
not a claimed universal Evoto format or a reconstruction of its renderer.
Only the installation resources tree and named executables are read.
"""
import argparse
from collections import Counter
import hashlib
import json
from pathlib import Path
import struct
import zlib

KEY = bytes.fromhex('eccdadcb')
JSON_MARKER = bytes.fromhex('2a786978742528672a41246c')
IMAGE_MARKER = bytes.fromhex('2a786978742528672a312430')
JSON_TAIL_MASKS = (b'', b'\xcb', b'\xad\xcb', b'\xad\xcb\xcd')
MAX_CONFIG_BYTES = 1024 * 1024
MAX_FILES = 4096
RANGE_FIELDS = {'DefaultDegree', 'DegreeRange', 'ParamRange', 'EffectRange'}
LABEL_FIELDS = {'EffectName', 'Type'}
PATH_FIELDS = {'Path', 'Paths', 'ModelPath', 'ModelFile'}


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def xor_words(body):
    """Decode complete words only; the tail has a distinct carrier rule."""
    length = len(body) - len(body) % 4
    return bytes(value ^ KEY[index % 4] for index, value in enumerate(body[:length]))


def strict_json(data):
    def reject_constant(value):
        raise ValueError('Non-finite JSON constant: ' + value)
    return json.loads(data.decode('utf-8-sig'), parse_constant=reject_constant)


def decode_config(data):
    if len(data) > MAX_CONFIG_BYTES:
        raise ValueError('Configuration exceeds the bounded audit limit')
    try:
        return strict_json(data), 'plain_utf8_json'
    except (ValueError, UnicodeError):
        pass
    if not data.endswith(JSON_MARKER):
        raise ValueError('Unrecognized configuration carrier marker')
    body = data[:-len(JSON_MARKER)]
    remainder = len(body) % 4
    prefix = xor_words(body)
    tail = bytes(value ^ mask for value, mask in
                 zip(body[len(prefix):], JSON_TAIL_MASKS[remainder]))
    try:
        return strict_json(prefix + tail), 'xor_words_json_tail'
    except (ValueError, UnicodeError) as cause:
        raise ValueError('Carrier transform did not produce strict UTF-8 JSON') from cause


def metadata(value, pointer=''):
    """Keep interfaces, ranges and references; omit tuning arrays and source strings."""
    result = {'labels': [], 'ranges': [], 'references': [], 'schema_keys': set()}

    def visit(node, at):
        if isinstance(node, dict):
            for key, child in sorted(node.items()):
                result['schema_keys'].add(key)
                child_at = at + '/' + key.replace('~', '~0').replace('/', '~1')
                if key in LABEL_FIELDS and isinstance(child, str) and len(child) <= 120:
                    result['labels'].append({'pointer': child_at, 'value': child})
                if key in RANGE_FIELDS and (
                    isinstance(child, (int, float)) and not isinstance(child, bool)
                    or isinstance(child, list) and len(child) <= 8
                    and all(isinstance(item, (int, float)) and not isinstance(item, bool) for item in child)
                ):
                    result['ranges'].append({'pointer': child_at, 'value': child})
                if key in PATH_FIELDS:
                    paths = child if isinstance(child, list) else [child]
                    for path in paths:
                        if isinstance(path, str) and len(path) <= 240 and '\n' not in path:
                            result['references'].append({'pointer': child_at, 'path': path})
                visit(child, child_at)
        elif isinstance(node, list):
            for index, child in enumerate(node):
                visit(child, at + '/' + str(index))

    visit(value, pointer)
    result['schema_keys'] = sorted(result['schema_keys'])
    return result


def png_header(path):
    """Validate only the IHDR checksum, never claim full image integrity."""
    if path.stat().st_size < 36 + len(IMAGE_MARKER):
        return {'carrier_marker': False, 'png_signature_ihdr': False}
    with path.open('rb') as source:
        data = source.read(36)
        source.seek(-len(IMAGE_MARKER), 2)
        trailer = source.read(len(IMAGE_MARKER))
    decoded = xor_words(data)
    valid = len(decoded) >= 33 and decoded[:8] == b'\x89PNG\r\n\x1a\n' and decoded[12:16] == b'IHDR'
    result = {'carrier_marker': trailer == IMAGE_MARKER, 'png_signature_ihdr': valid}
    if valid:
        result.update(width=struct.unpack_from('>I', decoded, 16)[0],
                      height=struct.unpack_from('>I', decoded, 20)[0],
                      ihdr_crc_valid=zlib.crc32(decoded[12:29]) == struct.unpack_from('>I', decoded, 29)[0])
    return result


def audit(root, expected_worker_sha256=None):
    root = root.resolve(strict=True)
    worker = root / 'Evoto-worker.exe'
    worker_digest = digest(worker)
    if expected_worker_sha256 and worker_digest != expected_worker_sha256.lower():
        raise ValueError('Worker SHA-256 does not match the requested build')
    resources = root / 'resources'
    files = []
    for path in resources.rglob('*'):
        if path.is_symlink():
            raise ValueError('Symlink resources are outside this audit scope')
        if path.is_file():
            if not path.resolve().is_relative_to(resources.resolve()):
                raise ValueError('Resource resolved outside the installation tree')
            files.append(path)
            if len(files) > MAX_FILES:
                raise ValueError('Resource inventory exceeds the bounded audit limit')
    configs = []
    pngs = []
    for path in sorted(files):
        if path.suffix.lower() == '.json':
            row = {'path': path.relative_to(root).as_posix(), 'bytes': path.stat().st_size,
                   'sha256': digest(path)}
            try:
                if path.stat().st_size > MAX_CONFIG_BYTES:
                    raise ValueError('Configuration exceeds the bounded audit limit')
                value, row['carrier'] = decode_config(path.read_bytes())
                row['metadata'] = metadata(value)
                for reference in row['metadata']['references']:
                    candidate = (path.parent / reference['path']).resolve()
                    inside = candidate.is_relative_to(resources.resolve())
                    reference['within_resources'] = inside
                    reference['exists'] = candidate.is_file() if inside else None
            except ValueError as cause:
                row.update(carrier='unidentified', error=str(cause))
            configs.append(row)
        if path.suffix.lower() == '.png' and path.is_relative_to(resources / 'effect'):
            pngs.append({'path': path.relative_to(root).as_posix(), **png_header(path)})
    return {
        'evidence_id': 'packaging-config-metadata-2026-10-20',
        'method': 'ordinary read-only shell/Python metadata inspection; not REA decompilation',
        'worker_sha256': worker_digest, 'gui_sha256': digest(root / 'Evoto.exe'),
        'resource_count': len(files), 'resource_bytes': sum(path.stat().st_size for path in files),
        'carrier': {'word_xor_hex': KEY.hex(), 'json_marker_hex': JSON_MARKER.hex(),
                    'json_tail_masks_hex': [value.hex() for value in JSON_TAIL_MASKS],
                    'image_marker_hex': IMAGE_MARKER.hex(), 'proof': 'strict JSON parse and PNG IHDR CRC'},
        'config_counts': dict(Counter(row['carrier'] for row in configs)), 'configs': configs,
        'png_header_count': len(pngs), 'png_ihdr_crc_valid': sum(row.get('ihdr_crc_valid', False) for row in pngs),
        'png_headers': pngs,
        'limitations': [
            'Carrier rules are empirically verified for these resources, not a universal renderer format.',
            'Effect names/types/ranges and asset references are static configuration declarations, not observed execution.',
            'Callback bodies, shader source, trained networks, training data and exact pixel behavior remain unknown.',
            'Parameter defaults/ranges do not establish slider normalization, blending, or processing order.',
            'Only metadata is returned: decoded configurations/images/weights are never written.',
            'No accounts, profiles, catalogues, keys or network traffic are read.',
        ],
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expected-worker-sha256')
    args = parser.parse_args()
    result = audit(args.root, args.expected_worker_sha256)
    output = args.output.resolve()
    if output.is_relative_to(args.root.resolve()):
        raise ValueError('Metadata output must stay outside the inspected installation')
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(result, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'output': str(output), 'configs': result['config_counts'],
                      'png_ihdr_crc_valid': result['png_ihdr_crc_valid']}))


if __name__ == '__main__':
    main()
