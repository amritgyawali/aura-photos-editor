#!/usr/bin/env python3
"""Read-only installation metadata audit; never executes Evoto or copies its assets.

Only named installation binaries and resources/effect/config are examined. User profiles,
credentials, projects and network traffic are outside this script's scope. A resource name
or binary string is an observation, not proof of the underlying algorithm or model.
"""
import argparse
from collections import Counter
import hashlib
import json
import math
import mmap
from pathlib import Path
import struct


def sha256(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def inspect_binary(path):
    result = {'name': path.name, 'bytes': path.stat().st_size, 'sha256': sha256(path)}
    with path.open('rb') as source, mmap.mmap(source.fileno(), 0, access=mmap.ACCESS_READ) as data:
        if data[:2] != b'MZ':
            return result
        pe = struct.unpack_from('<I', data, 0x3c)[0]
        if data[pe:pe + 4] != b'PE\0\0':
            return result
        machine, sections = struct.unpack_from('<HH', data, pe + 4)
        optional_size = struct.unpack_from('<H', data, pe + 20)[0]
        section_start = pe + 24 + optional_size
        result.update(machine=hex(machine), section_count=sections, sections=[])
        for index in range(sections):
            offset = section_start + 40 * index
            name = data[offset:offset + 8].rstrip(b'\0').decode('ascii', 'replace')
            virtual_size, address, size, raw = struct.unpack_from('<IIII', data, offset + 8)
            result['sections'].append({'name': name, 'virtual_address': hex(address),
                'virtual_bytes': virtual_size, 'file_offset': raw, 'file_bytes': size})
        # Literal occurrence counts are deliberately not called imports or active backends.
        result['literal_occurrences'] = {word: len(list(iter_offsets(data, word.encode()))) for word in
            ['onnxruntime', 'ncnn', 'MNN', 'tensorflow', 'tflite', 'cudnn', 'DirectML',
             'opencv', 'FaceAcne', 'MatteRefinement', 'MakeupProtect', 'RemoveEyeBag']}
    return result


def iter_offsets(data, needle):
    start = 0
    while (offset := data.find(needle, start)) >= 0:
        yield offset
        start = offset + len(needle)


def inspect_config(path, root):
    data = path.read_bytes()
    sample = data[:65536]
    counts = Counter(sample)
    entropy = -sum((n / len(sample)) * math.log2(n / len(sample)) for n in counts.values()) if sample else 0
    plain_json = False
    try:
        json.loads(data.decode('utf-8-sig'))
        plain_json = True
    except (ValueError, UnicodeError):
        pass
    return {'path': path.relative_to(root).as_posix(), 'bytes': len(data),
        'sha256': hashlib.sha256(data).hexdigest(), 'prefix_hex': data[:16].hex(),
        'sample_entropy_bits_per_byte': round(entropy, 4), 'plain_utf8_json': plain_json}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    config_root = root / 'resources' / 'effect' / 'config'
    configs = [inspect_config(path, root) for path in sorted(config_root.glob('*/effect.json'))]
    package = root / 'resources' / 'pixcook'
    report = {'scope': 'installation metadata only; no private algorithm reconstruction',
        'root': str(root), 'binaries': [inspect_binary(root / name) for name in
            ['Evoto.exe', 'Evoto-worker.exe'] if (root / name).is_file()],
        'effect_modules': sorted(path.name for path in config_root.iterdir() if path.is_dir()),
        'effect_configs': configs, 'opaque_resource_package': {
            'name': 'resources/pixcook', 'bytes': package.stat().st_size,
            'header_hex': package.open('rb').read(32).hex()},
        'limitations': ['A file or literal name does not establish runtime execution.',
            'Opaque configuration carriers have not been decoded; entropy does not prove encryption.',
            'No model weights, training data, loss functions or exact processing order recovered.',
            'No Evoto source, weights or resources added to AURA.']}
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({'binaries': len(report['binaries']), 'modules': len(report['effect_modules']),
        'configs': len(configs), 'plain_json_configs': sum(c['plain_utf8_json'] for c in configs),
        'output': str(args.output)}))


if __name__ == '__main__':
    main()
