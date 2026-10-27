"""Regenerate crates/aura-raw/data/camera_matrices.tsv from LibRaw's colour table.

Usage: extract-camera-matrices.py <path to LibRaw src/tables/colordata.cpp> <output .tsv>

The matrices are Adobe DNG Converter's ColorMatrix2 (XYZ under D65 to camera native RGB,
times 10,000) as compiled by dcraw and LibRaw. LibRaw is offered under LGPL-2.1 or
CDDL-1.0; the generated file is distributed under CDDL-1.0, which applies to that file
alone (see crates/aura-raw/data/LICENSE.CDDL and ADR-0124). Order is kept: the table is
matched by model prefix, first match wins, so a longer name must come before its prefix.
"""
import re
import sys

ENTRY = re.compile(
    r'\{\s*LIBRAW_CAMERAMAKER_(\w+)\s*,\s*"([^"]*)"\s*,\s*(-?\w+)\s*,\s*(-?\w+)\s*,\s*\{([-0-9xXa-fA-F\s,]*)\}\s*\}'
)


def number(text: str) -> int:
    text = text.strip()
    return int(text, 16) if text.lower().startswith(('0x', '-0x')) else int(text)


def main() -> int:
    source = open(sys.argv[1], encoding='utf-8', errors='replace').read()
    body = source[source.index('table[] = {'):source.index('int rblack')]
    rows = []
    for maker, prefix, black, maximum, values in ENTRY.findall(body):
        coefficients = [number(v) for v in values.replace('\n', ' ').split(',') if v.strip()]
        # Every row is kept, even one with no usable matrix: the table is matched
        # by first prefix, and dropping a row would let a shorter prefix further
        # down claim a body it was never measured on. Zeros mean "no matrix".
        if len(coefficients) < 9 or (len(coefficients) > 9 and any(coefficients[9:])):
            # Missing, or a four-colour (CMYG) sensor this pipeline cannot use.
            coefficients = [0] * 9
        rows.append((maker, prefix, number(black), number(maximum), coefficients[:9]))
    with open(sys.argv[2], 'w', encoding='utf-8', newline='\n') as out:
        out.write('# Camera colour matrices: Adobe DNG Converter ColorMatrix2 (XYZ D65 -> camera RGB) x 10000.\n')
        out.write('# Compiled by dcraw (Dave Coffin) and LibRaw (Copyright 2019-2025 LibRaw LLC),\n')
        out.write('# src/tables/colordata.cpp. This file is distributed under CDDL-1.0 - see LICENSE.CDDL.\n')
        out.write('# Regenerate with scripts/extract-camera-matrices.py; do not edit by hand.\n')
        out.write('# maker\tmodel prefix\tblack\tmaximum\tmatrix\n')
        for maker, prefix, black, maximum, matrix in rows:
            out.write(f"{maker}\t{prefix}\t{black}\t{maximum}\t{' '.join(str(v) for v in matrix)}\n")
    print(len(rows), 'rows,', sum(1 for r in rows if any(r[4])), 'with a matrix')
    return 0


if __name__ == '__main__':
    sys.exit(main())
