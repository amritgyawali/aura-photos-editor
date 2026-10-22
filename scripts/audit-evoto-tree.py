"""Inventory every regular installation file read-only; never execute target code.

SHA256 identifies observed bytes. MD5 only compares the vendor's shipped manifest.
Format classification is header evidence, not recovered processing/model behaviour.
"""
import argparse
from collections import Counter
import csv
import hashlib
import json
import os
from pathlib import Path
import stat
import struct
import time


def category(relative, header):
    name = relative.lower()
    if 'setup' in name or 'installer' in name:
        return 'installer-or-updater'
    if name.endswith(('.exe', '.dll')) or header[:2] == b'MZ':
        if any(token in name for token in ('camera', 'edsdk', 'edsi', 'tscano', 'tsfuji', 'tssony', 'cradapter', 'libgphoto', 'xapi', 'ftlptp')):
            return 'camera-integration'
        if name.startswith(('evoto.exe', 'evoto-worker.exe')):
            return 'primary-native-application'
        return 'native-component'
    if name.startswith('resources/pixcook/'):
        return 'opaque-processing-resource'
    if name.endswith(('.json', '.ini', '.conf', '.txt', '.xml')):
        return 'configuration-or-text'
    if name.endswith(('.png', '.jpg', '.jpeg', '.svg', '.ico', '.webp')):
        return 'image-resource'
    if name.endswith(('.qm', '.pak', '.dat', '.bin', '.rcc')):
        return 'binary-resource'
    return 'other-resource'


def header_format(header):
    signatures = [(b'MZ', 'dos-mz-candidate'), (b'\x89PNG\r\n\x1a\n', 'png'),
                  (b'\xff\xd8\xff', 'jpeg'), (b'PK\x03\x04', 'zip'),
                  (b'\x00asm', 'wasm'), (b'SQLite format 3\x00', 'sqlite'),
                  (b'II*\x00', 'tiff-le'), (b'MM\x00*', 'tiff-be')]
    return next((label for signature, label in signatures if header.startswith(signature)), 'unclassified-header')


def pe_metadata(stream, length):
    """Read PE/COFF headers only; no disassembly or speculative execution."""
    def read(at, count):
        if at < 0 or at + count > length:
            raise ValueError('PE header range outside file')
        stream.seek(at)
        data = stream.read(count)
        if len(data) != count:
            raise ValueError('Truncated PE header')
        return data
    pe = struct.unpack_from('<I', read(0, 64), 60)[0]
    coff = read(pe, 24)
    if coff[:4] != b'PE\0\0':
        raise ValueError('MZ without a PE signature')
    machine, count, timestamp = struct.unpack_from('<HHI', coff, 4)
    optional_length = struct.unpack_from('<H', coff, 20)[0]
    if count > 96:
        raise ValueError('PE section count exceeds96')
    optional = read(pe + 24, optional_length)
    magic = struct.unpack_from('<H', optional)[0]
    sections = []
    for index in range(count):
        data = read(pe + 24 + optional_length + index * 40, 40)
        virtual, rva, size, offset = struct.unpack_from('<IIII', data, 8)
        flags = struct.unpack_from('<I', data, 36)[0]
        sections.append(dict(name=data[:8].rstrip(b'\0').decode('ascii', 'replace'), rva=rva,
                             virtualBytes=virtual, offset=offset, bytes=size, executable=bool(flags & 0x20000000),
                             fileRangeValid=size == 0 or offset + size <= length))
    return dict(format={0x10b: 'pe32', 0x20b: 'pe32-plus'}.get(magic, 'unknown-pe'),
                machine=hex(machine), timestamp=timestamp, sections=sections,
                limitation='Headers only: no imports, functions, model architecture or runtime behaviour inferred.')


def scan(root):
    records, directories, gaps = [], [], []
    def walk(folder):
        try:
            with os.scandir(folder) as entries:
                children = sorted(entries, key=lambda item: item.name.casefold())
        except OSError as error:
            gaps.append(dict(path=folder.relative_to(root).as_posix(), error=str(error)))
            return
        for entry in children:
            path = Path(entry.path)
            relative = path.relative_to(root).as_posix()
            try:
                before = path.lstat()
                if stat.S_ISLNK(before.st_mode) or before.st_file_attributes & 0x400:
                    gaps.append(dict(path=relative, error='Reparse point not followed', target=os.readlink(path)))
                elif entry.is_dir(follow_symlinks=False):
                    directories.append(relative)
                    walk(path)
                elif stat.S_ISREG(before.st_mode):
                    record = dict(path=relative, bytes=before.st_size)
                    records.append(record)
                    sha, md5 = hashlib.sha256(), hashlib.md5(usedforsecurity=False)
                    with path.open('rb') as source:
                        header = source.read(64)
                        source.seek(0)
                        for chunk in iter(lambda: source.read(4 * 1024 * 1024), b''):
                            sha.update(chunk)
                            md5.update(chunk)
                        record.update(sha256=sha.hexdigest(), md5=md5.hexdigest(),
                                      header=header[:16].hex(), format=header_format(header), category=category(relative, header))
                        if header.startswith(b'MZ'):
                            try:
                                record['pe'] = pe_metadata(source, before.st_size)
                                record['format'] = record['pe']['format']
                            except (ValueError, struct.error) as error:
                                record['peError'] = str(error)
                    after = path.stat()
                    record['stable'] = (before.st_size, before.st_mtime_ns) == (after.st_size, after.st_mtime_ns)
                    if not record['stable']:
                        gaps.append(dict(path=relative, error='File changed during read'))
                else:
                    gaps.append(dict(path=relative, error='Non-regular entry not read'))
            except OSError as error:
                gaps.append(dict(path=relative, error=str(error)))
    walk(root)
    return records, directories, gaps


def compare_manifest(root, records):
    path = root / 'file_md5_list.json'
    declared = json.loads(path.read_text(encoding='utf-8-sig'))
    observed = {record['path'].casefold(): record for record in records}
    checks = []
    for name, digest in sorted(declared.items()):
        relative = name.replace('\\', '/').lstrip('/')
        record = observed.get(relative.casefold())
        status = 'missing' if record is None else 'match' if record.get('md5') == digest.lower() else 'mismatch'
        checks.append(dict(path=relative, expectedMd5=digest, observedMd5=record.get('md5') if record else None, status=status))
    return dict(checks=checks, counts=dict(Counter(item['status'] for item in checks)),
                limitation='Vendor MD5 is a consistency observation; neither an authenticity signature nor proof of safety.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('root', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    root, output = args.root.resolve(strict=True), args.output.resolve()
    if output.is_relative_to(root):
        raise ValueError('Evidence destination must be outside the inspected installation')
    if output.exists():
        raise ValueError('Evidence destination must be fresh')
    started = time.monotonic()
    files, directories, gaps = scan(root)
    report = dict(root=str(root), files=files, directories=directories, gaps=gaps,
                  completeInventory=not gaps, fileCount=len(files), bytes=sum(item['bytes'] for item in files),
                  formats=dict(Counter(item.get('format', 'unread') for item in files)),
                  categories=dict(Counter(item.get('category', 'unread') for item in files)),
                  manifest=compare_manifest(root, files), seconds=round(time.monotonic() - started, 3),
                  limitations=['Every regular file is hashed, with header classification. This is not complete decompilation.',
                               'Opaque model resources do not establish architecture, weights, training data or numerical parity.',
                               'Installers remain unopened historical artifacts; no setup, uninstall or helper is executed.',
                               'No inspected application code, model or image assets are incorporated into AURA.'])
    canonical = json.dumps([(item['path'], item.get('sha256')) for item in files], separators=(',', ':')).encode()
    report['treeManifestSha256'] = hashlib.sha256(canonical).hexdigest()
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    with output.with_suffix('.csv').open('w', encoding='utf-8', newline='') as stream:
        writer = csv.DictWriter(stream, fieldnames=['path', 'bytes', 'sha256', 'md5', 'format', 'category', 'stable'], extrasaction='ignore')
        writer.writeheader()
        writer.writerows(files)
    print(json.dumps({key: report[key] for key in ('completeInventory', 'fileCount', 'bytes', 'formats', 'categories', 'treeManifestSha256', 'seconds')}))
    print(json.dumps(dict(manifest=report['manifest']['counts'], gaps=gaps)))
    return 0 if not gaps else 1


if __name__ == '__main__':
    raise SystemExit(main())
